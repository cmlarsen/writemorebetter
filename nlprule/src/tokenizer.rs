//! A tokenizer to split raw text into tokens.
//! Tokens are assigned lemmas and part-of-speech tags by lookup from a [Tagger][tag::Tagger] and chunks containing
//! information about noun / verb and grammatical case by a statistical [Chunker][chunk::Chunker].
//! Tokens are *disambiguated* (i. e. information from the initial assignment is changed) in a rule-based way by
//! [DisambiguationRule][crate::rule::DisambiguationRule]s.

use crate::{
    rule::id::{Index, Selector},
    rule::MatchSentence,
    types::*,
    utils::{parallelism::MaybeParallelRefIterator, regex::Regex},
    BinaryKind, Error, TOKENIZER_FORMAT_VERSION,
};
use fs_err::File;
use serde::{Deserialize, Serialize};
use std::{
    io::{BufReader, Read, Write},
    ops::Range,
    path::Path,
    sync::Arc,
};

pub mod chunk;
pub mod multiword;
pub mod tag;

use chunk::Chunker;
use multiword::MultiwordTagger;
use tag::Tagger;

use crate::rule::DisambiguationRule;

/// Split a text at the points where the given function is true.
/// Keeps the separators. See https://stackoverflow.com/a/40296745.
fn split<F>(text: &str, split_func: F) -> Vec<&str>
where
    F: Fn(char) -> bool,
{
    let mut result = Vec::new();
    let mut last = 0;
    for (index, matched) in text.match_indices(split_func) {
        if last != index {
            result.push(&text[last..index]);
        }
        result.push(matched);
        last = index + matched.len();
    }
    if last < text.len() {
        result.push(&text[last..]);
    }

    result
}

/// Whether a character is one of the apostrophes LanguageTool treats as part
/// of a contraction: the ASCII apostrophe and the right single quotation mark.
#[inline]
pub(crate) fn is_apostrophe(c: char) -> bool {
    c == '\'' || c == '\u{2019}'
}

/// Whether `c` falls in one of the sorted, non-overlapping inclusive `ranges`.
fn in_ranges(ranges: &[(char, char)], c: char) -> bool {
    ranges
        .binary_search_by(|&(start, end)| {
            if end < c {
                std::cmp::Ordering::Less
            } else if start > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

pub(crate) fn is_split_char(word_chars: &[(char, char)], c: char) -> bool {
    c.is_whitespace()
        || if word_chars.is_empty() {
            crate::utils::splitting_chars().contains(c)
        } else {
            !in_ranges(word_chars, c)
        }
}

/// Whether `c` is a variation selector (U+FE00..U+FE0F), which LanguageTool
/// 6.6+'s English word tokenizer attaches to the preceding token.
#[inline]
fn is_variation_selector(c: char) -> bool {
    ('\u{FE00}'..='\u{FE0F}').contains(&c)
}

/// The finite set of auxiliaries LanguageTool 6.6+ recognizes before `n't`
/// (its `EnglishWordTokenizer` `patternList`).
const CONTRACTION_NEGATION_STEMS: &[&str] = &[
    "are", "is", "were", "was", "do", "does", "did", "have", "has", "had", "wo", "would", "ca",
    "could", "sha", "should", "must", "ai", "ought", "might", "need", "may", "am", "dare", "das",
    "dass", "hai", "used", "use",
];

/// Words LanguageTool 6.6+ keeps whole even though they contain an apostrophe.
fn is_contraction_whole_word(s: &str) -> bool {
    let lower = s.to_lowercase().replace('\u{2019}', "'");
    matches!(
        lower.as_str(),
        "fo'c'sle"
            | "rec'd"
            | "rec's"
            | "ok'd"
            | "cc'd"
            | "cc's"
            | "dj'd"
            | "pm'd"
            | "pd'd"
            | "rsvp'd"
    )
}

/// LanguageTool 6.6+ `patternList` group 2: `(['’]?)(aux)(n['’]t)`.
/// Returns the non-empty pieces: an optional leading apostrophe, the auxiliary
/// stem, and the `n't` clitic.
fn negation_parts(s: &str) -> Option<Vec<&str>> {
    let starts: Vec<usize> = s.char_indices().map(|(i, _)| i).collect();
    let n = starts.len();
    if n < 4 {
        return None;
    }

    let (i_n, i_apos, i_t) = (starts[n - 3], starts[n - 2], starts[n - 1]);
    let c_n = s[i_n..].chars().next()?;
    let c_apos = s[i_apos..].chars().next()?;
    let c_t = s[i_t..].chars().next()?;

    if !c_n.eq_ignore_ascii_case(&'n') || !is_apostrophe(c_apos) || !c_t.eq_ignore_ascii_case(&'t')
    {
        return None;
    }

    let mut stem = &s[..i_n];
    let mut lead = None;
    if let Some(c) = stem.chars().next() {
        if is_apostrophe(c) {
            lead = Some(&stem[..c.len_utf8()]);
            stem = &stem[c.len_utf8()..];
        }
    }

    if !CONTRACTION_NEGATION_STEMS.contains(&stem.to_ascii_lowercase().as_str()) {
        return None;
    }

    let mut out = Vec::with_capacity(3);
    if let Some(lead) = lead {
        out.push(lead);
    }
    out.push(stem);
    out.push(&s[i_n..]);
    Some(out)
}

/// LanguageTool 6.6+ `patternList` group 3:
/// `(.+)(['’]m|['’]re|['’]ll|['’]ve|['’]d|['’]s)(['’-]?)`.
/// Returns the stem, the apostrophe-initial clitic and an optional trailing
/// apostrophe or hyphen.
fn clitic_parts(s: &str) -> Option<Vec<&str>> {
    // the optional trailing `['’-]`
    let mut body = s;
    let mut tail = "";
    if let Some(last) = s.chars().last() {
        if is_apostrophe(last) || last == '-' {
            let idx = s.len() - last.len_utf8();
            body = &s[..idx];
            tail = &s[idx..];
        }
    }

    // the greedy `.+` makes the clitic start at the rightmost apostrophe that
    // is followed by a valid clitic; the stem must be non-empty.
    let apostrophes: Vec<usize> = body
        .char_indices()
        .filter(|(_, c)| is_apostrophe(*c))
        .map(|(i, _)| i)
        .collect();

    for &i in apostrophes.iter().rev() {
        if i == 0 {
            break;
        }
        let apos_len = body[i..].chars().next()?.len_utf8();
        let after = &body[i + apos_len..];
        if matches!(
            after.to_ascii_lowercase().as_str(),
            "m" | "re" | "ll" | "ve" | "d" | "s"
        ) {
            let mut out = vec![&body[..i], &body[i..]];
            if !tail.is_empty() {
                out.push(tail);
            }
            return Some(out);
        }
    }

    None
}

/// LanguageTool 6.6+ `patternList` group 4: `(['’]t)(was|were|is)`.
fn t_was_parts(s: &str) -> Option<Vec<&str>> {
    let starts: Vec<usize> = s.char_indices().map(|(i, _)| i).collect();
    if starts.len() < 3 {
        return None;
    }

    let first = s[starts[0]..].chars().next()?;
    let second = s[starts[1]..].chars().next()?;
    if !is_apostrophe(first) || !second.eq_ignore_ascii_case(&'t') {
        return None;
    }

    let rest = &s[starts[2]..];
    if matches!(rest.to_ascii_lowercase().as_str(), "was" | "were" | "is") {
        Some(vec![&s[..starts[2]], rest])
    } else {
        None
    }
}

/// The LanguageTool 6.6+ `patternList`: if a word containing an apostrophe
/// matches one of these shapes, its capture groups are emitted as tokens.
fn contraction_parts(s: &str) -> Option<Vec<&str>> {
    if is_contraction_whole_word(s) {
        return Some(vec![s]);
    }
    negation_parts(s)
        .or_else(|| clitic_parts(s))
        .or_else(|| t_was_parts(s))
}

/// Options for a tokenizer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TokenizerLangOptions {
    /// Whether to allow errors while constructing the tokenizer.
    #[serde(default)]
    pub allow_errors: bool,
    /// Disambiguation Rule selectors to use in this tokenizer.
    #[serde(default)]
    pub ids: Vec<Selector>,
    /// Disambiguation Rule selectors to ignore in this tokenizer.
    #[serde(default)]
    pub ignore_ids: Vec<Selector>,
    /// Specific examples in the notation `{id}:{example_index}` which are known to fail.
    #[serde(default)]
    pub known_failures: Vec<String>,
    /// Extra language-specific characters to split text on.
    #[serde(default)]
    pub extra_split_chars: Vec<char>,
    /// The characters LanguageTool 6.6+'s word tokenizer keeps inside a word, as
    /// sorted, non-overlapping inclusive ranges. When set, every other character
    /// (apart from whitespace) is a token of its own; when empty, text is split
    /// on [crate::utils::splitting_chars]. Switched on per build directory (see
    /// `build/make_build_dir.py`).
    #[serde(default)]
    pub word_chars: Vec<(char, char)>,
    /// Extra language-specific Regexes of which the matches will *not* be split into multiple tokens.
    #[serde(default)]
    pub extra_join_regexes: Vec<Regex>,
    /// Split English contractions the way LanguageTool 6.6+ does: a contracted
    /// auxiliary becomes its stem plus an apostrophe-initial clitic (`do` +
    /// `n't`, `it` + `'s`) instead of a stem with the apostrophe on its own
    /// (`don` + `'` + `t`). LanguageTool 5.2 (and nlprule before this option)
    /// use the latter form, so this defaults to `false` and is switched on per
    /// build directory (see `build/make_build_dir.py`).
    #[serde(default)]
    pub split_contractions: bool,
    /// Number a noun phrase the way LanguageTool 5.9+ does: a phrase is plural
    /// only if its *last* token has a plural reading. LanguageTool 5.2 marked a
    /// noun phrase plural if *any* of its tokens did, so `a pens and paper` was
    /// plural there and singular from 5.9 on. Defaults to `false` (5.2
    /// behavior) and is switched on per build directory.
    #[serde(default)]
    pub chunk_number_from_last_token: bool,
}

impl Default for TokenizerLangOptions {
    fn default() -> Self {
        TokenizerLangOptions {
            allow_errors: false,
            ids: Vec::new(),
            ignore_ids: Vec::new(),
            known_failures: Vec::new(),
            extra_split_chars: Vec::new(),
            word_chars: Vec::new(),
            extra_join_regexes: Vec::new(),
            split_contractions: false,
            chunk_number_from_last_token: false,
        }
    }
}

/// An iterator over [IncompleteSentence]s. Has the same properties as [SentenceIter].
pub struct IncompleteSentenceIter<'t> {
    text: &'t str,
    splits: Vec<Range<usize>>,
    tokenizer: &'t Tokenizer,
    index: usize,
    position: Position,
    keep_raw: bool,
}

impl<'t> Iterator for IncompleteSentenceIter<'t> {
    type Item = IncompleteSentence<'t>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.splits.len() {
            return None;
        }

        let mut range = self.splits[self.index].clone();
        self.index += 1;

        // as long as the current sentence contains only whitespace, add the next sentence
        // in practice, this might never happen, but we can not make any assumption about
        // SRX rule behavior here.
        while self.text[range.clone()].trim().is_empty() && self.index < self.splits.len() {
            range.end = self.splits[self.index].end;
            self.index += 1;
        }

        let sentence = self
            .tokenizer
            .tokenize_with_raw(&self.text[range.clone()], self.keep_raw)
            .map(|x| x.rshift(self.position));

        self.position += Position {
            char: self.text[range.clone()].chars().count(),
            byte: range.len(),
        };

        sentence
    }
}

/// An iterator over [Sentence]s. Has some key properties:
/// - Preceding whitespace is always included so the first sentence always starts at byte and char index zero.
/// - There are no gaps between sentences i.e. `sentence[i - 1].span().end() == sentence[i].span().start()`.
/// - Behavior for trailing whitespace is not defined. Can be included in the last sentence or not be part of any sentence.
pub struct SentenceIter<'t> {
    inner: IncompleteSentenceIter<'t>,
    tokenizer: &'t Tokenizer,
}

impl<'t> Iterator for SentenceIter<'t> {
    type Item = Sentence<'t>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|sentence| self.tokenizer.disambiguate(sentence).into_sentence())
    }
}

/// The complete Tokenizer doing tagging, chunking and disambiguation.
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Tokenizer {
    pub(crate) rules: Vec<DisambiguationRule>,
    pub(crate) chunker: Option<Chunker>,
    pub(crate) sentencizer: srx::Rules,
    pub(crate) multiword_tagger: Option<MultiwordTagger>,
    pub(crate) tagger: Arc<Tagger>,
    pub(crate) lang_options: TokenizerLangOptions,
    /// The distinct matcher parts of every disambiguation rule engine, addressed by
    /// [`PartId`][crate::rule::engine::composition::PartId]. See that type for why the parts are
    /// not stored inline in each composition.
    pub(crate) parts: crate::rule::engine::composition::PartPool,
}

impl Tokenizer {
    /// Creates a new tokenizer from a path to a binary.
    ///
    /// # Errors
    /// - If the file can not be opened.
    /// - If the file is not a writemorebetter tokenizer binary written by a compatible engine
    ///   version ([Error::Format]).
    /// - If the file content can not be deserialized to a tokenizer.
    pub fn new<P: AsRef<Path>>(p: P) -> Result<Self, Error> {
        let reader = BufReader::new(File::open(p.as_ref())?);
        Self::from_reader(reader)
    }

    /// Creates a new tokenizer from a reader.
    ///
    /// # Errors
    /// See [Tokenizer::new].
    pub fn from_reader<R: Read>(reader: R) -> Result<Self, Error> {
        let mut reader = reader;
        crate::read_binary_header(&mut reader, BinaryKind::Tokenizer, TOKENIZER_FORMAT_VERSION)?;
        Ok(bincode::deserialize_from(reader)?)
    }

    /// Serializes this tokenizer to a writer, prefixing the payload with the self-describing
    /// header (magic, format version, engine version).
    pub fn to_writer<W: Write>(&self, writer: W) -> Result<(), Error> {
        let mut writer = writer;
        crate::write_binary_header(&mut writer, BinaryKind::Tokenizer, TOKENIZER_FORMAT_VERSION)?;
        Ok(bincode::serialize_into(writer, &self)?)
    }

    /// Gets all disambigation rules in the order they are applied.
    pub fn rules(&self) -> &[DisambiguationRule] {
        &self.rules
    }

    /// Tests one disambiguation rule's examples, supplying this tokenizer's shared part pool.
    pub fn test_rule(&self, rule: &DisambiguationRule, tokenizer: &Tokenizer) -> bool {
        rule.test(tokenizer, &self.parts)
    }

    /// Gets the lexical tagger.
    pub fn tagger(&self) -> &Arc<Tagger> {
        &self.tagger
    }

    /// Gets the chunker if one exists.
    pub fn chunker(&self) -> &Option<Chunker> {
        &self.chunker
    }

    pub(crate) fn lang_options(&self) -> &TokenizerLangOptions {
        &self.lang_options
    }

    pub(crate) fn disambiguate_up_to_id<'t>(
        &'t self,
        mut sentence: IncompleteSentence<'t>,
        id: Option<&Index>,
    ) -> IncompleteSentence<'t> {
        let raw = sentence.take_raw();
        let n = id.map_or(self.rules.len(), |id| {
            self.rules.iter().position(|x| x.id == *id).unwrap()
        });
        let mut i = 0;
        let mut start_positions = Default::default();

        while i < n {
            let complete_sentence = sentence.clone().into_sentence();
            let mut match_sentence = MatchSentence::new(&complete_sentence);
            match_sentence.start_positions = start_positions;

            let result = self.rules[i..n]
                .maybe_par_iter()
                .enumerate()
                .filter_map(|(j, rule)| {
                    let changes = rule.apply(&self.parts, &match_sentence);
                    if changes.is_empty() {
                        None
                    } else {
                        Some((j + i, changes))
                    }
                })
                .find_first(|_| true);

            // Disambiguation changes tags and spelling flags, never token texts or positions.
            start_positions = match_sentence.start_positions;
            if let Some((index, changes)) = result {
                self.rules[index].change(&mut sentence, changes);
                i = index + 1;
            } else {
                i = n;
            }
        }

        sentence.set_raw(raw);
        sentence
    }

    /// Apply rule-based disambiguation to the tokens.
    /// This does not change the number of tokens, but can change the content arbitrarily.
    pub fn disambiguate<'t>(&'t self, sentence: IncompleteSentence<'t>) -> IncompleteSentence<'t> {
        self.disambiguate_up_to_id(sentence, None)
    }

    fn get_token_ranges<'t>(
        &self,
        text: &'t str,
    ) -> impl ExactSizeIterator<Item = (Range<usize>, Option<String>)> + 't {
        let mut tokens: Vec<(&'t str, Option<String>)> = Vec::new();

        let word_chars = &self.lang_options.word_chars;
        let split_char = |c: char| is_split_char(word_chars, c);
        let split_text = |text: &'t str| {
            let mut tokens = Vec::new();
            if self.lang_options.split_contractions {
                // Split on everything except apostrophes first so a whole word
                // containing an apostrophe can be handed to the contraction
                // splitter, matching LanguageTool 6.6+'s English word tokenizer.
                for pretoken in split(text, |c| split_char(c) && !is_apostrophe(c)) {
                    self.split_contraction_word(pretoken, &mut tokens);
                }
            } else {
                for pretoken in split(text, split_char) {
                    // if the token is in the dictionary, we add it right away
                    if self.tagger.id_word(pretoken.into()).1.is_some() {
                        tokens.push((pretoken, None));
                    } else {
                        // otherwise, potentially split it again with `extra_split_chars` e. g. "-"
                        tokens.extend(
                            split(pretoken, |c| {
                                split_char(c) || self.lang_options.extra_split_chars.contains(&c)
                            })
                            .into_iter()
                            .map(|x| (x, None)),
                        );
                    }
                }
            }
            tokens
        };

        let mut joined_mask = vec![false; text.len()];
        let mut joins = Vec::new();

        for regex in self.lang_options.extra_join_regexes.iter() {
            for mat in regex.find_iter(text) {
                if !joined_mask[mat.start()..mat.end()].iter().any(|x| *x) {
                    joins.push(mat.start()..mat.end());
                    joined_mask[mat.start()..mat.end()]
                        .iter_mut()
                        .for_each(|x| *x = true);
                }
            }
        }

        joins.sort_by(|a, b| a.start.cmp(&b.start));

        let mut prev = 0;
        for range in joins {
            tokens.extend(split_text(&text[prev..range.start]));
            prev = range.end;
            tokens.push((&text[range], None));
        }

        tokens.extend(split_text(&text[prev..text.len()]));
        let mut ranges: Vec<(Range<usize>, Option<String>)> = Vec::with_capacity(tokens.len());
        for (token, normalized) in tokens {
            let byte_start = (token.as_ptr() as usize)
                .checked_sub(text.as_ptr() as usize)
                .expect("Each token str is a slice of the text str.");

            if !word_chars.is_empty() && token.chars().count() == 1 {
                if let Some((previous, previous_normalized)) = ranges.last_mut() {
                    if previous.end == byte_start && token.chars().all(is_variation_selector) {
                        previous.end = byte_start + token.len();
                        if let Some(previous_normalized) = previous_normalized {
                            previous_normalized.push_str(token);
                        }
                        continue;
                    }
                }
            }

            ranges.push((byte_start..byte_start + token.len(), normalized));
        }
        ranges.into_iter()
    }

    /// Splits a single whitespace/punctuation-delimited piece the way
    /// LanguageTool 6.6+'s `EnglishWordTokenizer` does and appends the tokens
    /// (slices of `s`) to `out`. Only used when `split_contractions` is on.
    /// The second component is the text a token should be represented by when
    /// LanguageTool normalizes it (a recognized contraction is emitted with a
    /// straight apostrophe even if the input used a curly one).
    fn split_contraction_word<'t>(&self, s: &'t str, out: &mut Vec<(&'t str, Option<String>)>) {
        if s.is_empty() {
            return;
        }

        if s.chars().any(is_apostrophe) {
            if let Some(parts) = contraction_parts(s) {
                for part in parts {
                    self.words_to_add(part, out);
                }
                return;
            }
        }

        self.words_to_add(s, out);
    }

    /// Appends the tokens for a single word the way LanguageTool's
    /// `EnglishWordTokenizer.wordsToAdd` does: hyphens at either end of the word
    /// are tokens of their own, a word without an apostrophe (or whose
    /// normalized form is known to the tagger) stays whole, an unknown word
    /// containing apostrophes is split at them.
    fn words_to_add<'t>(&self, s: &'t str, out: &mut Vec<(&'t str, Option<String>)>) {
        let core = s.trim_start_matches('-');
        out.extend((0..s.len() - core.len()).map(|i| (&s[i..i + 1], None)));
        let trimmed = core.trim_end_matches('-');
        self.words_to_add_core(trimmed, out);
        out.extend((trimmed.len()..core.len()).map(|i| (&core[i..i + 1], None)));
    }

    fn words_to_add_core<'t>(&self, s: &'t str, out: &mut Vec<(&'t str, Option<String>)>) {
        if s.is_empty() {
            return;
        }

        if !s.chars().any(is_apostrophe) {
            if self.lang_options.extra_split_chars.is_empty() {
                out.push((s, None));
            } else {
                out.extend(
                    split(s, |c| self.lang_options.extra_split_chars.contains(&c))
                        .into_iter()
                        .map(|x| (x, None)),
                );
            }
            return;
        }

        // LanguageTool tags the word after normalizing curly apostrophes; its
        // tagger also tries the lowercase form, but only for a title-case or
        // all-uppercase word, so mixed case like `n'T` stays untagged. When it is
        // tagged, the normalized form is the token text, so a recognized
        // contraction is emitted with a straight apostrophe.
        let normalized = if s.contains('\u{2019}') {
            s.replace('\u{2019}', "'")
        } else {
            s.to_string()
        };

        let known = !self
            .tagger
            .tags_for_normalized(&normalized, false)
            .is_empty();

        if known {
            if normalized == s {
                out.push((s, None));
            } else {
                out.push((s, Some(normalized)));
            }
        } else {
            out.extend(split(s, is_apostrophe).into_iter().map(|x| (x, None)));
        }
    }

    pub(crate) fn get_token_strs<'t>(&self, text: &'t str) -> Vec<&'t str> {
        self.get_token_ranges(text)
            .map(|(range, _)| text[range].trim())
            .filter(|x| !x.is_empty())
            .collect()
    }

    /// Tokenize the given sentence. This applies chunking and tagging, but does not do disambiguation.
    // NB: this is not public because it could be easily misused by passing a text instead of one sentence.
    pub(crate) fn tokenize<'t>(&'t self, sentence: &'t str) -> Option<IncompleteSentence<'t>> {
        self.tokenize_with_raw(sentence, false)
    }

    /// Like [Tokenizer::tokenize], but with `keep_raw` also keeps the tokens as tagged and
    /// chunked, before multiword tagging, for the `raw_pos` grammar rules (see [Sentence::raw]).
    pub(crate) fn tokenize_with_raw<'t>(
        &'t self,
        sentence: &'t str,
        keep_raw: bool,
    ) -> Option<IncompleteSentence<'t>> {
        let mut sentence = self.tokenize_no_chunk(sentence)?;

        if let Some(chunker) = &self.chunker {
            chunker.apply(
                &mut sentence,
                self.lang_options.chunk_number_from_last_token,
            );
        }

        if keep_raw {
            sentence.keep_raw();
        }

        if let Some(multiword_tagger) = &self.multiword_tagger {
            multiword_tagger.apply(&mut sentence);
        }

        Some(sentence)
    }

    /// Tokenize and tag the given sentence, stopping before chunking and multiword tagging.
    pub(crate) fn tokenize_no_chunk<'t>(
        &'t self,
        sentence: &'t str,
    ) -> Option<IncompleteSentence<'t>> {
        if sentence.trim().is_empty() {
            return None;
        }

        let token_strs = self.get_token_ranges(sentence);
        let n_token_strs = token_strs.len();

        let mut tokens: Vec<_> = token_strs
            .enumerate()
            .filter(|(_, (range, _))| !sentence[range.clone()].trim().is_empty())
            .map(|(i, (range, normalized))| {
                let byte_start = range.start;
                let char_start = sentence[..byte_start].chars().count();
                let raw = &sentence[range.clone()];

                let is_sentence_start = i == 0;
                let is_sentence_end = i == n_token_strs - 1;

                // A normalized token (a recognized contraction whose apostrophe
                // LanguageTool straightens) keeps its original span even though
                // its text is an owned, shorter string.
                let word = match normalized {
                    Some(normalized) => Word::new(
                        self.tagger.id_word(normalized.clone().into()),
                        self.tagger
                            .tags_for_normalized(&normalized, is_sentence_start),
                    ),
                    None => {
                        let token_text = raw.trim();
                        Word::new(
                            self.tagger.id_word(token_text.into()),
                            self.tagger
                                .get_tags_with_options(
                                    token_text,
                                    if is_sentence_start { Some(true) } else { None },
                                    None,
                                )
                                .collect(),
                        )
                    }
                };

                IncompleteToken::new(
                    word,
                    Span::new(
                        byte_start..range.end,
                        char_start..char_start + raw.chars().count(),
                    ),
                    is_sentence_end,
                    sentence[..byte_start].ends_with(char::is_whitespace),
                    Vec::new(),
                )
            })
            .collect();

        let last_idx = tokens.len() - 1;
        *tokens[last_idx].is_sentence_end_mut() = true;

        Some(IncompleteSentence::new(tokens, sentence, &self.tagger))
    }

    /// Splits the text into sentences and tokenizes each sentence.
    pub fn sentencize<'t>(&'t self, text: &'t str) -> IncompleteSentenceIter<'t> {
        self.sentencize_with_raw(text, false)
    }

    fn sentencize_with_raw<'t>(
        &'t self,
        text: &'t str,
        keep_raw: bool,
    ) -> IncompleteSentenceIter<'t> {
        IncompleteSentenceIter {
            text,
            splits: self.sentencizer.split_ranges(text),
            tokenizer: &self,
            index: 0,
            position: Position::default(),
            keep_raw,
        }
    }

    /// Applies the entire tokenization pipeline including sentencization, tagging, chunking and disambiguation.
    /// Each sentence keeps its [raw][Sentence::raw] tokens.
    pub fn pipe<'t>(&'t self, text: &'t str) -> SentenceIter<'t> {
        self.pipe_with_raw(text, true)
    }

    /// Like [Tokenizer::pipe], but keeps each sentence's [raw][Sentence::raw] tokens only with
    /// `keep_raw`.
    pub(crate) fn pipe_with_raw<'t>(&'t self, text: &'t str, keep_raw: bool) -> SentenceIter<'t> {
        SentenceIter {
            inner: self.sentencize_with_raw(text, keep_raw),
            tokenizer: &self,
        }
    }
}

/// Internal pipeline stages exposed for the standalone chunker benchmark harness only.
#[cfg(feature = "bench-harness")]
#[doc(hidden)]
impl Tokenizer {
    /// Splits `text` into sentences and tag-tokenizes each one, stopping before chunking and
    /// multiword tagging. The sentence-merge behavior matches [`Tokenizer::sentencize`].
    pub fn prepared_sentences<'t>(&'t self, text: &'t str) -> Vec<IncompleteSentence<'t>> {
        let splits = self.sentencizer.split_ranges(text);
        let mut out = Vec::new();
        let mut index = 0;
        while index < splits.len() {
            let mut range = splits[index].clone();
            index += 1;
            while text[range.clone()].trim().is_empty() && index < splits.len() {
                range.end = splits[index].end;
                index += 1;
            }
            if let Some(sentence) = self.tokenize_no_chunk(&text[range]) {
                out.push(sentence);
            }
        }
        out
    }

    /// Applies the chunker to `sentence` using this tokenizer's own noun-phrase-number option.
    pub fn chunk_apply(&self, sentence: &mut IncompleteSentence) {
        if let Some(chunker) = &self.chunker {
            chunker.apply(sentence, self.lang_options.chunk_number_from_last_token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_contain_their_bounds_only() {
        let ranges = [
            ('-', '-'),
            ('0', '9'),
            ('\u{300}', '\u{36F}'),
            ('\u{2019}', '\u{2019}'),
        ];
        assert!(in_ranges(&ranges, '-'));
        assert!(in_ranges(&ranges, '0'));
        assert!(in_ranges(&ranges, '9'));
        assert!(in_ranges(&ranges, '\u{301}'));
        assert!(in_ranges(&ranges, '\u{2019}'));
        assert!(!in_ranges(&ranges, '\u{2014}'));
        assert!(!in_ranges(&ranges, '\u{201c}'));
        assert!(!in_ranges(&ranges, '\u{2026}'));
        assert!(!in_ranges(&ranges, '\u{2028}'));
        assert!(!in_ranges(&[], 'a'));
    }

    #[test]
    fn variation_selectors() {
        assert!(is_variation_selector('\u{fe0f}'));
        assert!(is_variation_selector('\u{fe00}'));
        assert!(!is_variation_selector('\u{2014}'));
    }

    #[test]
    fn negation_splits_stem_and_clitic() {
        assert_eq!(negation_parts("didn't"), Some(vec!["did", "n't"]));
        assert_eq!(negation_parts("ain't"), Some(vec!["ai", "n't"]));
        assert_eq!(negation_parts("haven’t"), Some(vec!["have", "n’t"]));
        assert_eq!(negation_parts("isn't"), Some(vec!["is", "n't"]));
        // not a known auxiliary
        assert_eq!(negation_parts("glorbn't"), None);
    }

    #[test]
    fn clitic_splits_stem_and_clitic() {
        assert_eq!(clitic_parts("it's"), Some(vec!["it", "'s"]));
        assert_eq!(clitic_parts("we’re"), Some(vec!["we", "’re"]));
        assert_eq!(clitic_parts("they've"), Some(vec!["they", "'ve"]));
        // the greedy `.+` takes the rightmost valid clitic
        assert_eq!(clitic_parts("he'd've"), Some(vec!["he'd", "'ve"]));
        // a trailing apostrophe or hyphen is kept as a third piece
        assert_eq!(clitic_parts("it's'"), Some(vec!["it", "'s", "'"]));
        assert_eq!(clitic_parts("y'all"), None);
        assert_eq!(clitic_parts("dogs’"), None);
    }

    #[test]
    fn whole_words_and_t_was() {
        assert!(is_contraction_whole_word("fo’c’sle"));
        assert_eq!(t_was_parts("'twas"), Some(vec!["'t", "was"]));
        assert_eq!(t_was_parts("’tis"), Some(vec!["’t", "is"]));
        assert_eq!(t_was_parts("'nope"), None);
    }

    #[test]
    fn contraction_parts_priority() {
        // negation before clitic: "won't" is `wo` + `n't`, not `won` + `'t`
        assert_eq!(contraction_parts("won't"), Some(vec!["wo", "n't"]));
        assert_eq!(contraction_parts("it's"), Some(vec!["it", "'s"]));
        // no apostrophe -> not a contraction shape
        assert_eq!(contraction_parts("plain"), None);
    }
}
