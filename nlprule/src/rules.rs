//! Sets of grammatical error correction rules.

use crate::types::*;
use crate::utils::parallelism::MaybeParallelRefIterator;
use crate::{
    builtin::BuiltinRule,
    rule::id::{Group, Selector},
    rule::MatchSentence,
    rule::Rule,
    spell::Spell,
    tokenizer::Tokenizer,
    BinaryKind, Error, RULES_FORMAT_VERSION,
};
use fs_err::File;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{BufReader, Read, Write},
    path::Path,
};

/// How many sentences to collect before running their rule checks in one parallel region. Large
/// enough to keep every worker busy on a batch, small enough to bound the memory held at once.
/// `wasm32` is single-threaded, so it takes the smaller batch to save memory; native targets keep
/// the larger one unless [`Rules::set_batch_size`] says otherwise.
const PARALLEL_BATCH: usize = if cfg!(target_arch = "wasm32") {
    32
} else {
    128
};

/// Language-dependent options for a rule set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RulesLangOptions {
    /// Whether to allow errors while constructing the rules.
    pub allow_errors: bool,
    /// Grammar Rule selectors to use in this set.
    #[serde(default)]
    pub ids: Vec<Selector>,
    /// Grammar Rule selectors to ignore in this set.
    #[serde(default)]
    pub ignore_ids: Vec<Selector>,
}

impl Default for RulesLangOptions {
    fn default() -> Self {
        RulesLangOptions {
            allow_errors: true,
            ids: Vec::new(),
            ignore_ids: Vec::new(),
        }
    }
}

/// A set of grammatical error correction rules.
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Rules {
    pub(crate) rules: Vec<Rule>,
    pub(crate) spell: Spell,
    pub(crate) builtins: Vec<BuiltinRule>,
    /// The article word lists for LanguageTool's `+DT`/`+INDT` determiner
    /// synthesis, shared by every rule that uses one instead of stored per rule.
    /// `None` when no rule uses determiner synthesis.
    pub(crate) articles: Option<crate::builtin::en::ArticleData>,
    /// The distinct matcher parts of every rule engine, addressed by
    /// [`PartId`][crate::rule::engine::composition::PartId]. See that type for why the parts are
    /// not stored inline in each composition.
    pub(crate) parts: crate::rule::engine::composition::PartPool,
    #[serde(skip)]
    pub(crate) batch_size: Option<usize>,
    #[serde(skip)]
    pub(crate) mother_tongue: Option<String>,
}

impl Rules {
    /// Sets how many sentences are collected before their rule checks run in one parallel region.
    /// A smaller batch holds less memory at once but starts more, shorter parallel regions.
    /// Values below one are treated as one. The default is 128 natively and 32 on `wasm32`.
    pub fn set_batch_size(&mut self, sentences: usize) {
        self.batch_size = Some(sentences.max(1));
    }

    /// Selects the writer's mother tongue as a language code such as "de" or "pt-BR" (only the
    /// language part is used), or `None` for no mother tongue (the default).
    ///
    /// Rules written for one mother tongue, such as the false friends compiled from
    /// LanguageTool's `false-friends.xml`, only run while that mother tongue is selected; the
    /// rules of other mother tongues never run, even if enabled. [Rules::mother_tongues] lists the
    /// mother tongues this rule set has rules for.
    pub fn set_mother_tongue(&mut self, mother_tongue: Option<&str>) {
        self.mother_tongue = mother_tongue.map(|code| {
            code.split(['-', '_'])
                .next()
                .unwrap_or_default()
                .to_lowercase()
        });
    }

    /// The mother tongue selected with [Rules::set_mother_tongue].
    pub fn mother_tongue(&self) -> Option<&str> {
        self.mother_tongue.as_deref()
    }

    /// The mother tongues this rule set has rules for, sorted.
    pub fn mother_tongues(&self) -> Vec<&str> {
        let mut codes: Vec<&str> = self
            .rules
            .iter()
            .filter_map(|rule| rule.mother_tongue())
            .collect();
        codes.sort_unstable();
        codes.dedup();
        codes
    }

    fn is_active(&self, rule: &Rule) -> bool {
        rule.enabled()
            && match rule.mother_tongue() {
                Some(code) => self.mother_tongue.as_deref() == Some(code),
                None => true,
            }
    }

    /// Creates a new rule set from a path to a binary.
    ///
    /// # Errors
    /// - If the file can not be opened.
    /// - If the file is not a writemorebetter rules binary written by a compatible engine version
    ///   ([Error::Format]).
    /// - If the file content can not be deserialized to a rules set.
    pub fn new<P: AsRef<Path>>(p: P) -> Result<Self, Error> {
        let reader = BufReader::new(File::open(p.as_ref())?);
        Self::from_reader(reader)
    }

    /// Creates a new rules set from a reader.
    ///
    /// # Errors
    /// See [Rules::new].
    pub fn from_reader<R: Read>(reader: R) -> Result<Self, Error> {
        let mut reader = reader;
        crate::read_binary_header(&mut reader, BinaryKind::Rules, RULES_FORMAT_VERSION)?;
        let mut rules: Rules = bincode::deserialize_from(reader)?;
        rules.spell.ingest_options();
        Ok(rules)
    }

    /// Serializes this rules set to a writer, prefixing the payload with the self-describing
    /// header (magic, format version, engine version).
    pub fn to_writer<W: Write>(&self, writer: W) -> Result<(), Error> {
        let mut writer = writer;
        crate::write_binary_header(&mut writer, BinaryKind::Rules, RULES_FORMAT_VERSION)?;
        Ok(bincode::serialize_into(writer, &self)?)
    }

    /// Gets the spellchecker associated with this rules set. The spellchecker always exists, even if spellchecking is disabled (default).
    pub fn spell(&self) -> &Spell {
        &self.spell
    }

    /// Mutably gets the spellchecker.
    pub fn spell_mut(&mut self) -> &mut Spell {
        &mut self.spell
    }

    /// Adds words to the spellchecker's user dictionary (character names, invented places,
    /// coinages) and returns how many were new. See [`Spell::add_words`] for the matching rules.
    /// Changes apply to the next [`Rules::suggest`] call and are not written into any binary.
    pub fn add_words<I, S>(&mut self, words: I) -> usize
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.spell.add_words(words)
    }

    /// Removes words from the user dictionary and returns how many were present.
    pub fn remove_words<I, S>(&mut self, words: I) -> usize
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.spell.remove_words(words)
    }

    /// Replaces the whole user dictionary and returns the number of entries now set.
    pub fn set_words<I, S>(&mut self, words: I) -> usize
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.spell.set_words(words)
    }

    /// Removes every word from the user dictionary.
    pub fn clear_words(&mut self) {
        self.spell.clear_words()
    }

    /// Lists the user dictionary, canonicalized and sorted.
    pub fn words(&self) -> Vec<String> {
        self.spell.words()
    }

    /// All rules ordered by priority.
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// All rules ordered by priority (mutable).
    pub fn rules_mut(&mut self) -> &mut [Rule] {
        &mut self.rules
    }

    /// All builtin rules.
    pub fn builtins(&self) -> &[BuiltinRule] {
        &self.builtins
    }

    /// All builtin rules (mutable).
    pub fn builtins_mut(&mut self) -> &mut [BuiltinRule] {
        &mut self.builtins
    }

    /// Returns an iterator over all rules matching the selector.
    pub fn select<'a>(&'a self, selector: &'a Selector) -> RulesIter<'a> {
        RulesIter {
            inner: self.rules.iter(),
            selector: Some(selector),
        }
    }

    /// Returns an iterator over all rules matching the selector (mutable).
    pub fn select_mut<'a>(&'a mut self, selector: &'a Selector) -> RulesIterMut<'a> {
        RulesIterMut {
            inner: self.rules.iter_mut(),
            selector: Some(selector),
        }
    }

    /// Compute the suggestions for the given sentence by checking all rules.
    /// Builtin rules that look across sentences only see this sentence; use [Rules::suggest] for full context.
    /// A rule with LanguageTool's `raw_pos` matches [Sentence::raw]; on a sentence that did not keep
    /// it (one not from [Tokenizer::pipe]) such a rule matches the disambiguated tokens instead.
    /// Rules with LanguageTool's `min_prev_matches` only count earlier matches in this sentence.
    pub fn apply(&self, sentence: &Sentence) -> Vec<Suggestion> {
        let builtin_suggestions = self.builtin_suggestions(std::slice::from_ref(sentence), None);
        let matches = self.sentence_matches(sentence, builtin_suggestions, true);
        self.resolve(sentence, matches, &mut Repetitions::default())
    }

    fn builtin_suggestions(
        &self,
        sentences: &[Sentence],
        previous: Option<&Sentence>,
    ) -> Vec<Suggestion> {
        self.builtins
            .iter()
            .filter(|rule| rule.enabled())
            .flat_map(|rule| rule.builtin().check_text_with_previous(sentences, previous))
            .collect()
    }

    /// Matches every active rule, the speller and `builtin_suggestions` against `sentence`, keeping
    /// the matches of rules with a repetition attribute apart since they depend on earlier
    /// sentences. `parallel = false` forces the rule loop serial, for when many sentences
    /// are already processed in parallel and parallelizing the rules of each sentence as well would
    /// only add rayon dispatch overhead per sentence.
    fn sentence_matches(
        &self,
        sentence: &Sentence,
        builtin_suggestions: Vec<Suggestion>,
        parallel: bool,
    ) -> SentenceMatches {
        let spell_suggestions = self.spell.suggest(sentence);
        let speller = self.spell.speller();
        let raw_sentence = sentence.raw().map(MatchSentence::new);
        let sentence = MatchSentence::new(sentence);

        let (repeated, mut output): (Vec<_>, Vec<_>) = self
            .rules
            .maybe_par_iter_cond(parallel)
            .enumerate()
            .filter(|(_, rule)| self.is_active(rule))
            .map(|(i, rule)| {
                let mut output = Vec::new();

                let match_sentence = match &raw_sentence {
                    Some(raw) if rule.raw_pos() => raw,
                    _ => &sentence,
                };

                for suggestion in rule.apply(
                    &self.parts,
                    match_sentence,
                    self.articles.as_ref(),
                    speller.as_ref(),
                ) {
                    output.push((i, suggestion));
                }

                output
            })
            .flatten()
            .collect::<Vec<_>>()
            .into_iter()
            .partition(|(i, _)| self.rules[*i].repetition().is_some());

        output.extend(spell_suggestions.into_iter().map(|x| (0, x)));
        output.extend(builtin_suggestions.into_iter().map(|x| (0, x)));

        SentenceMatches { output, repeated }
    }

    fn resolve(
        &self,
        sentence: &Sentence,
        matches: SentenceMatches,
        repetitions: &mut Repetitions,
    ) -> Vec<Suggestion> {
        let SentenceMatches {
            mut output,
            repeated,
        } = matches;
        repetitions.admit(&self.rules, sentence, repeated, &mut output);

        output.sort_by(|(ia, a), (ib, b)| {
            a.span()
                .char()
                .start
                .cmp(&b.span().char().start)
                .then_with(|| ib.cmp(ia))
        });

        let mut mask = vec![false; sentence.text().chars().count()];

        output
            .into_iter()
            .filter_map(|(_, suggestion)| {
                let span = suggestion.span().clone().lshift(sentence.span().start());
                let range = span.char().start.min(mask.len())..span.char().end.min(mask.len());

                if mask[range.clone()].iter().all(|x| !x) {
                    mask[range.clone()].iter_mut().for_each(|x| *x = true);
                    let original: String = sentence
                        .text()
                        .chars()
                        .skip(range.start)
                        .take(range.len())
                        .collect();
                    Some(suggestion.match_apostrophes(&original))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Compute the suggestions for a text by checking all rules.
    ///
    /// Rules with LanguageTool's `min_prev_matches` (e.g. `REPETITIONS_STYLE`) fire only after
    /// enough earlier matches nearby. They count matches across sentence and paragraph boundaries
    /// within `text` and start from zero on every call, so an app that checks text incrementally
    /// should pass the surrounding paragraphs it wants those rules to see.
    pub fn suggest(&self, text: &str, tokenizer: &Tokenizer) -> Vec<Suggestion> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut suggestions = Vec::new();
        let mut paragraph = Vec::new();
        // Sentences are tokenized/disambiguated sequentially (the sentencizer drives that), but
        // their rule checks are collected into batches and run in parallel: one rayon region per
        // batch instead of one per sentence, which keeps the workers busy instead of repeatedly
        // waking for a single sentence's few thousand rules.
        let mut batch: Vec<(Sentence, Vec<Suggestion>)> = Vec::new();
        let mut repetitions = Repetitions::default();
        // The last sentence of the previous paragraph, kept so that text-level
        // builtins (UppercaseSentenceStart) can use it as context.
        let mut previous: Option<Sentence> = None;

        let keep_raw = self
            .rules
            .iter()
            .any(|rule| rule.enabled() && rule.raw_pos());

        for sentence in tokenizer.pipe_with_raw(text, keep_raw) {
            let ends_paragraph = sentence.text().contains('\n');
            paragraph.push(sentence);

            if ends_paragraph {
                previous = self.extend_paragraph(&mut paragraph, previous.as_ref(), &mut batch);
                if batch.len() >= self.batch_size.unwrap_or(PARALLEL_BATCH) {
                    self.flush_batch(&mut batch, &mut repetitions, &mut suggestions);
                }
            }
        }
        self.extend_paragraph(&mut paragraph, previous.as_ref(), &mut batch);
        self.flush_batch(&mut batch, &mut repetitions, &mut suggestions);

        suggestions
    }

    /// Adds one paragraph's `(sentence, builtin suggestions)` pairs to the pending batch. `previous`
    /// is the last sentence of the preceding paragraph, if any. Returns the paragraph's last
    /// sentence so the caller can pass it on as context.
    fn extend_paragraph<'t>(
        &self,
        paragraph: &mut Vec<Sentence<'t>>,
        previous: Option<&Sentence<'t>>,
        batch: &mut Vec<(Sentence<'t>, Vec<Suggestion>)>,
    ) -> Option<Sentence<'t>> {
        let mut builtin_suggestions = self.builtin_suggestions(paragraph, previous);
        let last = paragraph.last().cloned();

        for sentence in paragraph.drain(..) {
            let end = sentence.span().char().end;
            let (own, rest) = builtin_suggestions
                .into_iter()
                .partition(|x| x.span().char().start < end);
            builtin_suggestions = rest;
            batch.push((sentence, own));
        }

        last
    }

    /// Runs the pending batch in parallel (rules serial within a sentence) and appends the
    /// per-sentence results to `out` in order.
    fn flush_batch<'t>(
        &self,
        batch: &mut Vec<(Sentence<'t>, Vec<Suggestion>)>,
        repetitions: &mut Repetitions,
        out: &mut Vec<Suggestion>,
    ) {
        if batch.is_empty() {
            return;
        }

        let results: Vec<SentenceMatches> = batch
            .maybe_par_iter()
            .map(|(sentence, builtin)| self.sentence_matches(sentence, builtin.clone(), false))
            .collect();

        for ((sentence, _), matches) in batch.iter().zip(results) {
            out.extend(self.resolve(sentence, matches, repetitions));
        }
        batch.clear();
    }

    /// Correct a text by first tokenizing, then finding all suggestions and choosing the first replacement of each suggestion.
    pub fn correct(&self, text: &str, tokenizer: &Tokenizer) -> String {
        let suggestions = self.suggest(text, tokenizer);
        apply_suggestions(text, &suggestions)
    }

    /// Tests one rule's examples against `tokenizer`, supplying this set's shared article word
    /// lists and part pool (the `+DT`/`+INDT` determiner synthesis needs the former) and its
    /// speller (the speller-backed rule filters need it): the selected variant's, or the default
    /// variant's (`en_US` for English) while none is selected.
    pub fn test_rule(&self, rule: &Rule, tokenizer: &Tokenizer) -> bool {
        rule.test(
            tokenizer,
            &self.parts,
            self.articles.as_ref(),
            self.spell.speller().as_ref(),
        )
    }
}

struct SentenceMatches {
    output: Vec<(usize, Suggestion)>,
    repeated: Vec<(usize, Suggestion)>,
}

#[derive(Default)]
struct Repetitions {
    tokens: usize,
    history: HashMap<Group, Vec<usize>>,
}

impl Repetitions {
    fn admit(
        &mut self,
        rules: &[Rule],
        sentence: &Sentence,
        mut repeated: Vec<(usize, Suggestion)>,
        output: &mut Vec<(usize, Suggestion)>,
    ) {
        repeated.sort_by_key(|(i, suggestion)| (suggestion.span().char().start, *i));

        let mut groups: Vec<(&Group, Vec<(usize, Suggestion)>)> = Vec::new();
        for (i, suggestion) in repeated {
            let group = rules[i].id().parent();
            match groups.iter_mut().find(|(x, _)| *x == group) {
                Some((_, matches)) => matches.push((i, suggestion)),
                None => groups.push((group, vec![(i, suggestion)])),
            }
        }

        for (group, matches) in groups {
            let mut matches = matches.into_iter().peekable();
            while let Some((i, suggestion)) = matches.next() {
                while matches
                    .peek()
                    .is_some_and(|(_, next)| overlaps(&suggestion, next))
                {
                    matches.next();
                }

                let repetition = rules[i]
                    .repetition()
                    .expect("only repetition rules are admitted");
                let position = self.tokens + Self::token_position(sentence, &suggestion);
                let history = self.history.entry(group.clone()).or_default();

                if history.len() >= repetition.min_prev_matches
                    && position.saturating_sub(history[history.len() - repetition.min_prev_matches])
                        < repetition.distance_tokens
                {
                    output.push((i, suggestion));
                }

                history.push(position);
            }
        }

        self.tokens += sentence.tokens().len();
    }

    fn token_position(sentence: &Sentence, suggestion: &Suggestion) -> usize {
        let start = suggestion.span().char().start;
        let before = sentence
            .tokens()
            .iter()
            .take_while(|token| token.span().char().start < start)
            .count();
        before + usize::from(start > sentence.span().char().start)
    }
}

fn overlaps(a: &Suggestion, b: &Suggestion) -> bool {
    a.span().char().start <= b.span().char().end && a.span().char().end >= b.span().char().start
}

/// Correct a text by applying suggestions to it.
/// - In case of multiple possible replacements, always chooses the first one.
/// - In case of a suggestion without any replacements, ignores the suggestion.
pub fn apply_suggestions(text: &str, suggestions: &[Suggestion]) -> String {
    let mut offset: isize = 0;
    let mut chars: Vec<_> = text.chars().collect();

    for suggestion in suggestions {
        if let Some(replacement) = suggestion.replacements().get(0) {
            let replacement: Vec<_> = replacement.chars().collect();
            chars.splice(
                (suggestion.span().char().start as isize + offset) as usize
                    ..(suggestion.span().char().end as isize + offset) as usize,
                replacement.iter().cloned(),
            );
            offset = offset + replacement.len() as isize - suggestion.span().char().len() as isize;
        }
    }

    chars.into_iter().collect()
}

/// An iterator over references to rules.
pub struct RulesIter<'a> {
    selector: Option<&'a Selector>,
    inner: std::slice::Iter<'a, Rule>,
}

impl<'a> Iterator for RulesIter<'a> {
    type Item = &'a Rule;
    fn next(&mut self) -> Option<Self::Item> {
        let selector = self.selector.as_ref();

        self.inner
            .find(|rule| selector.map_or(true, |s| s.is_match(rule.id())))
    }
}

/// An iterator over mutable references to rules.
pub struct RulesIterMut<'a> {
    selector: Option<&'a Selector>,
    inner: std::slice::IterMut<'a, Rule>,
}

impl<'a> Iterator for RulesIterMut<'a> {
    type Item = &'a mut Rule;
    fn next(&mut self) -> Option<Self::Item> {
        let selector = self.selector.as_ref();

        self.inner
            .find(|rule| selector.map_or(true, |s| s.is_match(rule.id())))
    }
}

impl IntoIterator for Rules {
    type Item = Rule;
    type IntoIter = std::vec::IntoIter<Rule>;
    fn into_iter(self) -> Self::IntoIter {
        self.rules.into_iter()
    }
}
