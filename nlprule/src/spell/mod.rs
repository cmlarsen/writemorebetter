//! Structures and implementations related to spellchecking.
use fst::{
    raw::{Fst, Node},
    IntoStreamer, Map, Streamer,
};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::{BTreeSet, BinaryHeap, HashSet},
    ops::{Deref, DerefMut},
};
use unicode_categories::UnicodeCategories;

use crate::{
    types::*,
    utils::{apply_to_first, has_latin_script, is_title_case, is_uppercase},
    Error,
};

pub(crate) mod levenshtein;
pub(crate) mod multitoken;
use multitoken::MultitokenSpeller;
mod spell_int {
    use std::cmp;

    use serde::{Deserialize, Serialize};

    /// Encodes information about a valid word in a `u64` for storage as value in an FST.
    /// Currently:
    /// - the bottom 8 bits encode the frequency
    /// - the other 56 bits act as flags for the variants e.g. bit 10 and 12 are set if the word exists in the the second and fourth variant.
    #[derive(Debug, Clone, Default, Copy, Serialize, Deserialize)]
    pub(crate) struct SpellInt(pub(super) u64);

    type FreqType = u8;

    const fn freq_size() -> usize {
        std::mem::size_of::<FreqType>() * 8
    }

    #[allow(dead_code)] // some methods are only needed for compilation - kept here for clarity
    impl SpellInt {
        pub fn as_u64(&self) -> u64 {
            self.0
        }

        pub fn update_freq(&mut self, freq: usize) {
            assert!(freq < FreqType::MAX as usize);

            let prev_freq = self.freq();
            // erase previous frequency
            self.0 &= u64::MAX - FreqType::MAX as u64;
            // set new frequency, strictly speaking we would have to store a frequency for each variant
            // but that would need significantly more space, so we just store the highest frequency
            self.0 |= cmp::max(prev_freq, freq) as u64;
        }

        pub fn add_variant(&mut self, index: usize) {
            assert!(index < 64 - freq_size());
            self.0 |= 1 << (freq_size() + index);
        }

        pub fn contains_variant(&self, index: usize) -> bool {
            (self.0 >> (freq_size() + index)) & 1 == 1
        }

        pub fn freq(&self) -> usize {
            (self.0 & FreqType::MAX as u64) as usize
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn can_encode_freq() {
            let mut int = SpellInt::default();
            int.update_freq(100);
            int.add_variant(1);
            int.add_variant(10);

            assert!(int.freq() == 100);
        }

        #[test]
        fn can_encode_variants() {
            let mut int = SpellInt::default();
            int.update_freq(100);
            int.add_variant(1);
            int.add_variant(10);
            int.update_freq(10);

            assert!(int.contains_variant(1));
            assert!(int.contains_variant(10));
            assert!(!int.contains_variant(2));
            assert!(int.freq() == 100);
        }
    }
}

pub(crate) use spell_int::SpellInt;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Candidate {
    distance: usize,
    freq: usize,
    term: String,
}
impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.distance
            .cmp(&other.distance)
            .then_with(|| other.freq.cmp(&self.freq))
            .then_with(|| self.term.cmp(&other.term))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
/// Options to configure the spellchecker.
pub struct SpellOptions {
    /// The language variant to use. Setting this to `None` disables spellchecking.
    pub variant: Option<Variant>,
    /// The maximum edit distance to consider for corrections. Currently Optimal String Alignment distance is used.
    pub max_distance: usize,
    /// A fixed prefix length for which to consider only edits with a distance of 1. This speeds up the search by pruning the tree early.
    pub prefix_length: usize,
    /// The maximum number of correction candidates to return. Candidates are ranked by edit
    /// distance, then by word frequency, and only those at the smallest distance found are kept,
    /// unless the best candidate differs from the word only in case.
    pub top_n: usize,
    /// A set of words to ignore. Can also contain phrases delimited by a space.
    #[serde(serialize_with = "crate::types::sorted::set")]
    pub whitelist: DefaultHashSet<String>,
}

/// A guard around the [SpellOptions]. Makes sure the spellchecker is updated once this is dropped.
/// Implements `Deref` and `DerefMut` to the [SpellOptions].
pub struct SpellOptionsGuard<'a> {
    spell: &'a mut Spell,
}

impl<'a> Deref for SpellOptionsGuard<'a> {
    type Target = SpellOptions;

    fn deref(&self) -> &Self::Target {
        &self.spell.options
    }
}

impl<'a> DerefMut for SpellOptionsGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.spell.options
    }
}

impl<'a> Drop for SpellOptionsGuard<'a> {
    fn drop(&mut self) {
        self.spell.ingest_options()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct SpellLangOptions {
    /// Variants of the language (e.g. "en_US", "en_GB") to consider for spellchecking.
    pub variants: Vec<Variant>,
    pub split_hyphens: bool,
    #[serde(default)]
    pub word_chars: Vec<(char, char)>,
    /// The variant [Spell::speller] answers for while no variant is selected, so speller-backed
    /// rule filters work with spellchecking off. `None` leaves them without a speller.
    #[serde(default)]
    pub default_variant: Option<Variant>,
    /// Whether the language is written in the Latin script. If so, a token with no Latin-script
    /// character (a Greek, Cyrillic or CJK word quoted in English prose) is never misspelled, as
    /// in LanguageTool's `isLatinScript()`. A token mixing scripts is still checked.
    #[serde(default)]
    pub latin_script: bool,
}

impl Default for SpellOptions {
    fn default() -> Self {
        SpellOptions {
            variant: None,
            max_distance: 2,
            prefix_length: 2,
            top_n: 10,
            whitelist: DefaultHashSet::default(),
        }
    }
}

/// A valid language variant. Obtained by [Spell::variant].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(transparent)]
pub struct Variant(String);

impl Variant {
    /// Gets the language code of this variant.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// The user dictionary: words a caller adds at runtime so the speller accepts them (character
/// names, invented places, coinages). Kept on [`Spell`] but never serialized into a rules
/// binary; the caller owns persistence and re-adds the words after loading.
///
/// Entries are canonicalized by trimming, collapsing interior whitespace, and mapping curly
/// apostrophes to straight ones, so listing is stable and matching is apostrophe-insensitive.
#[derive(Debug, Clone, Default)]
pub(crate) struct UserWords {
    entries: BTreeSet<String>,
}

impl UserWords {
    /// Canonicalizes one entry. `None` for an entry with no content.
    fn canonicalize(word: &str) -> Option<String> {
        let collapsed = word.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.is_empty() {
            None
        } else {
            Some(collapsed.replace('\u{2019}', "'"))
        }
    }

    fn insert(&mut self, word: &str) -> bool {
        Self::canonicalize(word).is_some_and(|word| self.entries.insert(word))
    }

    fn remove(&mut self, word: &str) -> bool {
        Self::canonicalize(word).is_some_and(|word| self.entries.remove(&word))
    }
}

/// Splits a user entry the way the tokenizer splits an unknown word: on whitespace, the
/// characters the tokenizer splits on, and both apostrophes, keeping every
/// non-whitespace separator because each becomes its own token, e.g. `"Zar'khan"` ->
/// `["Zar", "'", "khan"]` and `"St. Petersburg"` -> `["St", ".", "Petersburg"]`.
fn token_pieces(entry: &str, word_chars: &[(char, char)]) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut last = 0;

    for (index, matched) in entry.match_indices(|c: char| {
        crate::tokenizer::is_split_char(word_chars, c) || crate::tokenizer::is_apostrophe(c)
    }) {
        if last != index {
            pieces.push(entry[last..index].to_owned());
        }
        if !matched.chars().all(|c| c.is_whitespace()) {
            pieces.push(matched.to_owned());
        }
        last = index + matched.len();
    }

    if last < entry.len() {
        pieces.push(entry[last..].to_owned());
    }

    pieces
}

/// Expands a token-piece list into variants where each apostrophe token is straight or curly, so
/// that an entry added with a straight apostrophe also accepts the curly spelling. The expansion
/// is capped so an entry with many apostrophes cannot blow up.
fn continuation_variants(pieces: &[String]) -> Vec<Vec<String>> {
    let mut variants: Vec<Vec<String>> = vec![Vec::new()];

    for piece in pieces {
        if piece == "'" && variants.len() < 4 {
            let mut expanded = Vec::with_capacity(variants.len() * 2);
            for variant in &variants {
                let mut straight = variant.clone();
                straight.push("'".to_owned());
                expanded.push(straight);

                let mut curly = variant.clone();
                curly.push("\u{2019}".to_owned());
                expanded.push(curly);
            }
            variants = expanded;
        } else {
            for variant in &mut variants {
                variant.push(piece.clone());
            }
        }
    }

    variants
}

/// One variant's view of the user dictionary: the accepted single-token forms, the
/// continuations of multi-token entries, and an FST of single-token entries so they can also
/// be suggested. Rebuilt whenever the user dictionary or the selected variant changes.
#[derive(Debug, Clone, Default)]
pub(crate) struct UserMatcher {
    /// Canonical forms a token may equal, including the sentence-initial capitalized form of a
    /// lowercase entry, the all-caps form and possessive forms.
    accepted: HashSet<String>,
    /// Continuations of a multi-token entry, keyed by its first token (same shape as
    /// [`Spell::multiwords`]).
    multiwords: DefaultHashMap<String, Vec<Vec<String>>>,
    /// Serialized `fst::Map` over single-token entries, empty when there are none.
    suggestion_fst: Vec<u8>,
}

impl UserMatcher {
    fn new(
        user: &UserWords,
        max_freq: usize,
        variant_index: usize,
        word_chars: &[(char, char)],
    ) -> Self {
        let mut accepted = HashSet::new();
        let mut multiwords: DefaultHashMap<String, Vec<Vec<String>>> = DefaultHashMap::default();
        let mut suggestion_terms: Vec<(String, u64)> = Vec::new();

        let mut freq = SpellInt::default();
        freq.update_freq(max_freq.clamp(1, 254));
        freq.add_variant(variant_index);
        let freq = freq.as_u64();

        for entry in &user.entries {
            // A lowercase entry also accepts its sentence-initial capitalized form and every
            // entry its all-caps form; a capitalized entry deliberately does not make the
            // lowercase form valid.
            let mut forms = vec![entry.clone()];
            if entry.chars().next().is_some_and(char::is_lowercase) {
                forms.push(apply_to_first(entry, |c| c.to_uppercase().collect()));
            }
            let suggested = forms.len();
            let all_caps = entry.to_uppercase();
            if !forms.contains(&all_caps) {
                forms.push(all_caps);
            }

            for (i, form) in forms.into_iter().enumerate() {
                accepted.insert(form.clone());
                accepted.insert(format!("{form}'s"));
                if is_all_caps(&form) {
                    accepted.insert(format!("{form}'S"));
                }
                if form.ends_with(['s', 'z', 'x', 'S', 'Z', 'X']) {
                    accepted.insert(format!("{form}'"));
                }

                let pieces = token_pieces(&form, word_chars);
                if pieces.len() > 1 {
                    for variant in continuation_variants(&pieces) {
                        multiwords
                            .entry(variant[0].clone())
                            .or_default()
                            .push(variant[1..].to_vec());
                    }
                } else if i < suggested {
                    suggestion_terms.push((form, freq));
                }
            }
        }

        suggestion_terms.sort();
        suggestion_terms.dedup();
        let suggestion_fst = if suggestion_terms.is_empty() {
            Vec::new()
        } else {
            fst::Map::from_iter(suggestion_terms)
                .expect("suggestion terms are sorted")
                .into_fst()
                .to_vec()
        };

        UserMatcher {
            accepted,
            multiwords,
            suggestion_fst,
        }
    }

    /// Whether a token is accepted by the user dictionary. Curly apostrophes match the
    /// canonical straight form.
    fn accepts(&self, word: &str) -> bool {
        self.accepted.contains(word)
            || (word.contains('\u{2019}') && self.accepted.contains(&word.replace('\u{2019}', "'")))
    }
}

fn phrase_len(phrases: &Fst<&[u8]>, tokens: &[Token]) -> usize {
    let step = |node: Node, byte: u8| {
        node.find_input(byte)
            .map(|i| phrases.node(node.transition(i).addr))
    };

    let mut node = phrases.root();
    let mut len = 0;
    for (i, token) in tokens.iter().enumerate() {
        let separator = if i == 0 { None } else { Some(b' ') };
        let next = separator
            .into_iter()
            .chain(token.word().text().as_str().bytes())
            .try_fold(node, step);
        match next {
            Some(next) => node = next,
            None => break,
        }
        if node.is_final() {
            len = i + 1;
        }
    }
    len
}

fn is_all_caps(word: &str) -> bool {
    is_uppercase(word) && word.chars().any(char::is_uppercase)
}

fn recase(term: String, all_caps: bool) -> String {
    let mixed_case = !is_uppercase(&term) && term.chars().skip(1).any(char::is_uppercase);
    if mixed_case {
        term
    } else if all_caps {
        term.to_uppercase()
    } else {
        apply_to_first(&term, |x| x.to_uppercase().collect())
    }
}

/// Spellchecker logic for one variant. Does the actual work.
///
/// Holds no copy of the valid words: lookups go against the shared speller FST (passed in by the caller)
/// and accept a word only if its [`SpellInt`] carries the bit for this variant.
#[derive(Debug, Clone)]
pub(crate) struct VariantChecker {
    variant: Variant,
    variant_index: usize,
    multiwords: DefaultHashMap<String, Vec<Vec<String>>>,
    map: DefaultHashMap<String, String>,
    lang_options: SpellLangOptions,
    options: SpellOptions,
    user: UserMatcher,
}

impl VariantChecker {
    /// Checks the validity of one word.
    /// NB: The ordering of this chain of `||` operators is somewhat nontrivial. Could potentially be improved by benchmarking.
    /// If this is true, the token is always correct. The converse is not true because e.g. multiwords are checked separately.
    fn check_word(&self, fst: &Map<&[u8]>, word: &str, recurse: bool) -> bool {
        word.is_empty()
            || self.is_known(fst, word)
            || self.user.accepts(word)
            || word.chars().all(|x| {
                x.is_symbol() || x.is_punctuation() || x.is_numeric() || x.is_mark() || x.is_other()
            })
            || word.chars().any(char::is_numeric)
            || (recurse
                // for title case words, it is enough if the lowercase variant is known.
                // it is possible that `is_title_case` is still true for word where `.to_lowercase()` was called so we need a `recurse` parameter.
                && is_title_case(word)
                && self.check_word(
                    fst,
                    &apply_to_first(word, |x| x.to_lowercase().collect()),
                    false,
                ))
            || (recurse && is_all_caps(word) && {
                let lower = word.to_lowercase();
                self.is_known(fst, &lower)
                    || self.is_known(fst, &apply_to_first(&lower, |x| x.to_uppercase().collect()))
            })
    }

    fn is_known(&self, fst: &Map<&[u8]>, word: &str) -> bool {
        fst.get(word)
            .is_some_and(|v| SpellInt(v).contains_variant(self.variant_index))
    }

    /// Populates `correct_mask` according to the correctness of the given zeroth token.
    /// - `correct_mask[0]` is `true` if the zeroth token is correct, `false` if it is not correct.
    /// - Indices `1..n` of `correct_mask` are `true` if the `n`th token is also definitely correct.
    ///     If they are `false`, they need to be checked separately.  
    fn check(&self, fst: &Map<&[u8]>, tokens: &[Token], correct_mask: &mut [bool]) {
        let word = tokens[0].word().text().as_str();
        correct_mask[0] = self.check_separated(fst, word);

        if let Some(matching_cont) = self
            .matching_continuation(self.multiwords.get(word), tokens)
            .or_else(|| self.matching_continuation(self.user.multiwords.get(word), tokens))
        {
            correct_mask[..1 + matching_cont.len()]
                .iter_mut()
                .for_each(|x| *x = true);
        }
    }

    /// Checks one word like [`VariantChecker::check_word`], also accepting a word whose
    /// hyphen- or dash-separated parts are all correct, or, in a Latin-script language, a word
    /// with no Latin-script character. That last test looks at the whole word only, as
    /// LanguageTool does: in `Привет-friend` the Cyrillic part alone does not pass.
    fn check_separated(&self, fst: &Map<&[u8]>, word: &str) -> bool {
        let mut word_is_correct = self.check_word(fst, word, true)
            || (self.lang_options.latin_script && !has_latin_script(word));

        if !word_is_correct {
            // there exist multiple valid hyphens, see https://jkorpela.fi/dashes.html
            let hyphens = ['-', '\u{2010}', '\u{2011}'];
            let dashes = ['\u{2012}', '\u{2013}', '\u{2014}', '\u{2015}'];

            let separators: Vec<char> = if self.lang_options.split_hyphens {
                hyphens.iter().chain(dashes.iter()).copied().collect()
            } else {
                dashes.to_vec()
            };

            if word.contains(separators.as_slice())
                && word
                    .split(separators.as_slice())
                    .all(|x| self.check_word(fst, x, true))
            {
                word_is_correct = true;
            }
        }

        word_is_correct
    }

    /// Whether `word`, standing alone, is correct: a known or user word, a word whose
    /// separated parts are all correct, or a phrase list entry with no continuation.
    fn is_correct(&self, fst: &Map<&[u8]>, word: &str) -> bool {
        let standalone = |continuations: Option<&Vec<Vec<String>>>| {
            continuations.is_some_and(|conts| conts.iter().any(Vec::is_empty))
        };
        self.check_separated(fst, word)
            || standalone(self.multiwords.get(word))
            || standalone(self.user.multiwords.get(word))
    }

    /// Finds a continuation of `continuations` that matches the tokens after the first one.
    fn matching_continuation<'m>(
        &self,
        continuations: Option<&'m Vec<Vec<String>>>,
        tokens: &[Token],
    ) -> Option<&'m Vec<String>> {
        continuations.and_then(|continuations| {
            continuations.iter().find(|cont| {
                // important: an empty continuation matches! so single words can also validly be part of `multiwords`
                (tokens.len() - 1) >= cont.len()
                    && cont
                        .iter()
                        .enumerate()
                        .all(|(i, x)| tokens[i + 1].word().text().as_str() == x)
            })
        })
    }

    fn search(&self, fst: &Map<&[u8]>, word: &str) -> Vec<String> {
        if let Some(candidate) = self.map.get(word) {
            return vec![candidate.to_owned()];
        }

        // The finite-state search cost grows sharply with the query length; a pasted
        // run of characters with no word break (a token far longer than any word) is
        // not worth correcting, so skip the search rather than let it dominate a check.
        const MAX_SEARCH_LEN: usize = 64;
        let len = word.chars().count();
        if len > MAX_SEARCH_LEN {
            return Vec::new();
        }
        let search_distance = if len <= self.options.max_distance {
            len.saturating_sub(1)
        } else {
            self.options.max_distance
        };

        let all_caps = is_all_caps(word);
        // As LanguageTool does, a word starting with a capital gets capitalized candidates, also
        // when it is mixed case (`LeBRON` -> `Learn`, not `learn`).
        let convert_case = all_caps || word.chars().next().is_some_and(char::is_uppercase);
        let query =
            levenshtein::Levenshtein::new(word, search_distance, self.options.prefix_length)
                .fold_case(true);
        let restore_case = |term: String| {
            if convert_case {
                recase(term, all_caps)
            } else {
                term
            }
        };

        let mut out = BinaryHeap::with_capacity(self.options.top_n);
        self.push_candidates(&mut out, fst, &query, &restore_case);

        if !self.user.suggestion_fst.is_empty() {
            let user_map = Map::new(self.user.suggestion_fst.as_slice())
                .expect("serialized user suggestion fst must be valid.");
            self.push_candidates(&mut out, &user_map, &query, &restore_case);
        }

        let out = out.into_sorted_vec();
        let max_distance = match out.first() {
            Some(x) if x.distance > 0 => x.distance,
            _ => search_distance,
        };
        out.into_iter()
            .take_while(|x| x.distance <= max_distance)
            .map(|x| x.term)
            .collect()
    }

    /// Pushes the candidates of one speller FST into `out`, capping it at `top_n`.
    fn push_candidates(
        &self,
        out: &mut BinaryHeap<Candidate>,
        fst: &Map<&[u8]>,
        query: &levenshtein::Levenshtein,
        recase: &dyn Fn(String) -> String,
    ) {
        let mut stream = fst.search_with_state(query.clone()).into_stream();
        while let Some((k, v, s)) = stream.next() {
            let id = SpellInt(v);

            // The shared FST holds every variant's words; skip entries for other variants.
            if !id.contains_variant(self.variant_index) {
                continue;
            }

            let state = s.expect("matching levenshtein state is always `Some`.");

            let term =
                recase(String::from_utf8(k.to_vec()).expect("fst keys must be valid utf-8."));
            let candidate = Candidate {
                distance: state.dist(),
                freq: id.freq(),
                term,
            };
            if let Some(existing) = out.iter().find(|x| x.term == candidate.term) {
                if *existing <= candidate {
                    continue;
                }
                out.retain(|x| x.term != candidate.term);
            }
            out.push(candidate);
            if out.len() > self.options.top_n {
                out.pop();
            }
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
/// A spellchecker implementing the algorithm described in [Error-tolerant Finite State Recognition](https://www.aclweb.org/anthology/1995.iwpt-1.24/) with some extensions.
pub struct Spell {
    /// An FST mapping valid words (always single tokens!) to a [SpellInt].
    pub(crate) fst: Vec<u8>,
    /// Known *multiwords* i. e. phrases. Can also validly contain single words if they should not be part of the FST (e.g. words in the whitelist).
    #[serde(serialize_with = "crate::types::sorted::map")]
    pub(crate) multiwords: DefaultHashMap<String, Vec<(Vec<String>, SpellInt)>>,
    pub(crate) phrases: Vec<u8>,
    ///  The maximum occured word frequency. Used to normalize.
    pub(crate) max_freq: usize,
    /// A map of `wrong->right`. `wrong` must always be exactly one token.
    #[serde(serialize_with = "crate::types::sorted::map")]
    pub(crate) map: DefaultHashMap<String, String>,
    pub(crate) lang_options: SpellLangOptions,
    pub(crate) options: SpellOptions,
    /// Known multi-word names and phrases for the multitoken spelling rules.
    pub(crate) multitoken: MultitokenSpeller,
    /// The structure containing the actual spellchecking logic. Computed based on the selected variant.
    #[serde(skip)]
    pub(crate) variant_checker: Option<VariantChecker>,
    /// The checker for [SpellLangOptions::default_variant], built while another variant or none
    /// is selected. Serves [Spell::speller] while none is selected and
    /// [Spell::default_speller], never [Spell::suggest].
    #[serde(skip)]
    pub(crate) default_checker: Option<VariantChecker>,
    /// The runtime user dictionary. Never serialized: the caller persists it and re-adds it.
    #[serde(skip)]
    pub(crate) user: UserWords,
}

impl Spell {
    /// Gets the options.
    pub fn options(&self) -> &SpellOptions {
        &self.options
    }

    /// Mutably gets the options.
    pub fn options_mut(&mut self) -> SpellOptionsGuard {
        SpellOptionsGuard { spell: self }
    }

    /// Returns all known variants.
    pub fn variants(&self) -> &[Variant] {
        self.lang_options.variants.as_slice()
    }

    /// Returns the variant for a language code e.g. `"en_GB"`.
    /// # Errors
    /// - If no variant exists for the language code.
    pub fn variant(&self, variant: &str) -> Result<Variant, Error> {
        self.lang_options
            .variants
            .iter()
            .find(|x| x.as_str() == variant)
            .cloned()
            .ok_or_else(|| {
                Error::UnknownVariant(
                    variant.to_owned(),
                    self.lang_options
                        .variants
                        .iter()
                        .map(|x| x.as_str().to_owned())
                        .collect(),
                )
            })
    }

    pub(crate) fn ingest_options(&mut self) {
        let mut existing: Vec<VariantChecker> = self
            .variant_checker
            .take()
            .into_iter()
            .chain(self.default_checker.take())
            .collect();
        let mut take = |variant: &Variant| {
            existing
                .iter()
                .position(|checker| checker.variant == *variant)
                .map(|i| existing.swap_remove(i))
        };

        let selected = self.options.variant.clone();
        let default = self
            .lang_options
            .default_variant
            .clone()
            .filter(|variant| selected.as_ref() != Some(variant));
        let selected_existing = selected.as_ref().and_then(&mut take);
        let default_existing = default.as_ref().and_then(&mut take);
        self.variant_checker =
            selected.map(|variant| self.build_checker(variant, selected_existing));
        self.default_checker = default.map(|variant| self.build_checker(variant, default_existing));
    }

    fn build_checker(&self, variant: Variant, existing: Option<VariantChecker>) -> VariantChecker {
        let variant_index = self
            .variants()
            .iter()
            .position(|x| *x == variant)
            .expect("only valid variants are created.");

        let mut checker = match existing {
            // if the variant checker exists and uses the correct variant, we don't need to rebuild
            Some(mut checker) if checker.variant == variant => {
                checker.options = self.options.clone();
                checker
            }
            _ => VariantChecker {
                variant,
                variant_index,
                multiwords: DefaultHashMap::default(),
                map: self.map.clone(),
                options: self.options.clone(),
                lang_options: self.lang_options.clone(),
                user: UserMatcher::default(),
            },
        };

        // `multiwords` depend on the whitelist. For convenience we always rebuild this.
        // the whitelist could be separated into a new structure for a speedup.
        // We can revisit this if performance becomes an issue, it should still be quite fast as implemented now.

        // selects only the multiwords which exist for the selected variant
        let mut multiwords: DefaultHashMap<_, _> = self
            .multiwords
            .iter()
            .map(|(key, value)| {
                let value = value
                    .iter()
                    .filter_map(|(continuations, int)| {
                        if int.contains_variant(variant_index) {
                            Some(continuations)
                        } else {
                            None
                        }
                    })
                    .cloned()
                    .collect();
                (key.to_owned(), value)
            })
            .collect();

        // adds words from the user-set whitelist
        // careful: words in the `whitelist` are set by the user, so this must never fail!
        for phrase in self
            .options
            .whitelist
            .iter()
            .map(|x| x.as_str())
            // for some important words we have to manually make sure they are ignored :)
            .chain(vec!["nlprule", "Minixhofer"])
        {
            let mut parts = phrase.trim().split_whitespace();

            let first = if let Some(first) = parts.next() {
                first
            } else {
                // silently ignore empty words
                continue;
            };

            multiwords
                .entry(first.to_owned())
                .or_insert_with(Vec::new)
                .push(parts.map(|x| x.to_owned()).collect());
        }

        checker.multiwords = multiwords;
        checker.user = UserMatcher::new(
            &self.user,
            self.max_freq,
            variant_index,
            &self.lang_options.word_chars,
        );
        checker
    }

    /// Rebuilds only the user-dictionary view of the variant checker, leaving the (much more
    /// expensive) multiword filtering alone. Used when the user dictionary changes.
    fn rebuild_user(&mut self) {
        for checker in self
            .variant_checker
            .iter_mut()
            .chain(self.default_checker.iter_mut())
        {
            checker.user = UserMatcher::new(
                &self.user,
                self.max_freq,
                checker.variant_index,
                &self.lang_options.word_chars,
            );
        }
    }

    /// Adds words to the user dictionary and returns how many were new. An entry is matched the
    /// way the speller tokenizes: a single word, a whitespace-separated phrase (`"Port Royal"`),
    /// or a word with an internal apostrophe. Matching is case-sensitive except that a lowercase
    /// entry also accepts its sentence-initial capitalized form and every entry accepts its
    /// all-caps form (`ZORVATH` for `Zorvath`); a capitalized entry does not make the lowercase
    /// form valid. Entries also accept a possessive (`'s` after a straight or curly apostrophe,
    /// `'S` for the all-caps form) and curly/straight apostrophes interchangeably. Changes apply
    /// to the next [`Spell::suggest`] call; no binary recompilation is needed.
    pub fn add_words<I, S>(&mut self, words: I) -> usize
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let added = words
            .into_iter()
            .filter(|word| self.user.insert(word.as_ref()))
            .count();
        if added > 0 {
            self.rebuild_user();
        }
        added
    }

    /// Removes words from the user dictionary and returns how many were present.
    pub fn remove_words<I, S>(&mut self, words: I) -> usize
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let removed = words
            .into_iter()
            .filter(|word| self.user.remove(word.as_ref()))
            .count();
        if removed > 0 {
            self.rebuild_user();
        }
        removed
    }

    /// Replaces the whole user dictionary and returns the number of entries now set.
    pub fn set_words<I, S>(&mut self, words: I) -> usize
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.user.entries.clear();
        let set = words
            .into_iter()
            .filter(|word| self.user.insert(word.as_ref()))
            .count();
        self.rebuild_user();
        set
    }

    /// Removes every word from the user dictionary.
    pub fn clear_words(&mut self) {
        if !self.user.entries.is_empty() {
            self.user.entries.clear();
            self.rebuild_user();
        }
    }

    /// Lists the user dictionary, canonicalized and sorted.
    pub fn words(&self) -> Vec<String> {
        self.user.entries.iter().cloned().collect()
    }

    /// A read-only view of the speller for single-word queries. While no variant is selected
    /// (spellchecking is disabled) it answers for the language's default variant (`en_US` for
    /// English, as LanguageTool uses for plain English), so speller-backed rule filters behave the
    /// same with spellchecking on or off. `None` only when the language has no default variant.
    pub fn speller(&self) -> Option<Speller<'_>> {
        let checker = self
            .variant_checker
            .as_ref()
            .or(self.default_checker.as_ref())?;
        Some(Speller {
            checker,
            fst: Map::new(self.fst.as_slice()).expect("serialized fst must be valid."),
            multitoken: &self.multitoken,
        })
    }

    /// A read-only view of the speller for [SpellLangOptions::default_variant] (`en_US` for
    /// English) whatever variant is selected, for the rule filters LanguageTool pins to it.
    /// `None` only when the language has no default variant.
    pub(crate) fn default_speller(&self) -> Option<Speller<'_>> {
        self.lang_options.default_variant.as_ref()?;
        let checker = self
            .default_checker
            .as_ref()
            .or(self.variant_checker.as_ref())?;
        Some(Speller {
            checker,
            fst: Map::new(self.fst.as_slice()).expect("serialized fst must be valid."),
            multitoken: &self.multitoken,
        })
    }

    /// Runs the spellchecking algorithm on all tokens and returns suggestions.
    pub fn suggest(&self, sentence: &Sentence) -> Vec<Suggestion> {
        let tokens = sentence.tokens();
        let variant_checker = if let Some(checker) = self.variant_checker.as_ref() {
            checker
        } else {
            return Vec::new();
        };

        // Share the FST with the checker instead of materializing a per-variant copy.
        let fst = Map::new(self.fst.as_slice()).expect("serialized fst must be valid.");
        let phrases = (!self.phrases.is_empty()).then(|| {
            Fst::new(self.phrases.as_slice()).expect("serialized phrase fst must be valid.")
        });

        let mut suggestions = Vec::new();
        let mut correct_mask = vec![false; tokens.len()];

        for (i, token) in tokens.iter().enumerate() {
            let text = token.word().text().as_str();

            if let Some(phrases) = &phrases {
                correct_mask[i..i + phrase_len(phrases, &tokens[i..])]
                    .iter_mut()
                    .for_each(|x| *x = true);
            }
            if !correct_mask[i] {
                variant_checker.check(&fst, &tokens[i..], &mut correct_mask[i..]);
            }
            if correct_mask[i] || token.ignore_spelling() {
                continue;
            }

            suggestions.push(Suggestion::new(
                "SPELLCHECK/SINGLE".into(),
                "Possibly misspelled word.".into(),
                token.span().clone(),
                variant_checker.search(&fst, text),
            ));
        }

        suggestions
    }
}

/// Read-only single-word queries against a [Spell] for its selected variant (or its default
/// variant while none is selected), obtained by [Spell::speller]. Rule filters use it to ask the speller while a rule is matching.
#[derive(Debug)]
pub struct Speller<'a> {
    checker: &'a VariantChecker,
    fst: Map<&'a [u8]>,
    multitoken: &'a MultitokenSpeller,
}

impl<'a> Speller<'a> {
    /// Gets the variant this speller answers for.
    pub fn variant(&self) -> &Variant {
        &self.checker.variant
    }

    /// Whether `word` is spelled correctly on its own, by the same checks the spellchecker
    /// applies to a single token (including the user dictionary). Surrounding whitespace is
    /// ignored and a curly apostrophe counts as a straight one, matching the straight-apostrophe
    /// speller and user-dictionary entries.
    ///
    /// `word` must be one token as the tokenizer produces it. The tokenizer splits contractions
    /// (`didn't` is `did` + `n't`), so a whole contraction or a multi-word string is not correct
    /// here; tokenize it and check each token, as LanguageTool's filters do. A token the
    /// disambiguator marked to skip spelling (such as `wo` in `won't`) is skipped by the
    /// spellchecker but not here: check [`Token::ignore_spelling`] first.
    pub fn is_correct(&self, word: &str) -> bool {
        self.checker.is_correct(&self.fst, &Self::normalize(word))
    }

    /// At most `n` candidate spellings for `word`, best first, with straight apostrophes. The
    /// search does not check `word` first, so a correct word may be among its own candidates.
    pub fn suggest(&self, word: &str, n: usize) -> Vec<String> {
        let word = Self::normalize(word);
        if word.is_empty() {
            return Vec::new();
        }
        let mut suggestions = self.checker.search(&self.fst, &word);
        suggestions.truncate(n);
        suggestions
    }

    pub(crate) fn multitoken(&self) -> &'a MultitokenSpeller {
        self.multitoken
    }

    fn normalize(word: &str) -> Cow<'_, str> {
        let word = word.trim();
        if word.contains('\u{2019}') {
            Cow::Owned(word.replace('\u{2019}', "'"))
        } else {
            Cow::Borrowed(word)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_trims_collapses_and_straightens() {
        assert_eq!(UserWords::canonicalize("  Zoë  "), Some("Zoë".into()));
        assert_eq!(
            UserWords::canonicalize("Port   Royal\tBay"),
            Some("Port Royal Bay".into())
        );
        assert_eq!(
            UserWords::canonicalize("O\u{2019}Brien"),
            Some("O'Brien".into())
        );
        assert_eq!(UserWords::canonicalize("   "), None);
        assert_eq!(UserWords::canonicalize(""), None);
    }

    #[test]
    fn token_pieces_mirrors_tokenization() {
        assert_eq!(token_pieces("Xylophonix", &[]), vec!["Xylophonix"]);
        assert_eq!(token_pieces("Xyzzy Plugh", &[]), vec!["Xyzzy", "Plugh"]);
        assert_eq!(token_pieces("Zar'khan", &[]), vec!["Zar", "'", "khan"]);
        assert_eq!(
            token_pieces("St. Petersburg", &[]),
            vec!["St", ".", "Petersburg"]
        );
        assert_eq!(token_pieces("Krell-Tarn", &[]), vec!["Krell-Tarn"]);
    }

    #[test]
    fn token_pieces_mirrors_word_char_tokenization() {
        let word_chars = [
            ('-', '-'),
            ('0', '9'),
            ('A', 'Z'),
            ('a', 'z'),
            ('\u{2019}', '\u{2019}'),
        ];
        assert_eq!(
            token_pieces("Ka\u{b7}lel", &word_chars),
            vec!["Ka", "\u{b7}", "lel"]
        );
        assert_eq!(
            token_pieces("Zorvath\u{2014}Quill", &word_chars),
            vec!["Zorvath", "\u{2014}", "Quill"]
        );
        assert_eq!(
            token_pieces("Zar\u{2019}khan", &word_chars),
            vec!["Zar", "\u{2019}", "khan"]
        );
        assert_eq!(
            token_pieces("Vel\u{2026}Dor\u{2028}Ash", &word_chars),
            vec!["Vel", "\u{2026}", "Dor", "Ash"]
        );
        assert_eq!(token_pieces("Krell-Tarn", &word_chars), vec!["Krell-Tarn"]);
    }

    #[test]
    fn continuation_variants_expand_apostrophes() {
        let pieces = token_pieces("Zar'khan", &[]);
        let variants = continuation_variants(&pieces);
        assert!(variants.contains(&vec!["Zar".to_string(), "'".into(), "khan".into()]));
        assert!(variants.contains(&vec!["Zar".to_string(), "\u{2019}".into(), "khan".into()]));
    }
}
