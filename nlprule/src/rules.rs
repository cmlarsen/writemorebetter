//! Sets of grammatical error correction rules.

use crate::types::*;
use crate::utils::parallelism::MaybeParallelRefIterator;
use crate::{
    builtin::BuiltinRule, rule::id::Selector, rule::MatchSentence, rule::Rule, spell::Spell,
    tokenizer::Tokenizer, BinaryKind, Error, RULES_FORMAT_VERSION,
};
use fs_err::File;
use serde::{Deserialize, Serialize};
use std::{
    io::{BufReader, Read, Write},
    iter::FromIterator,
    path::Path,
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
}

impl Rules {
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
    pub fn apply(&self, sentence: &Sentence) -> Vec<Suggestion> {
        let builtin_suggestions = self.builtin_suggestions(std::slice::from_ref(sentence));
        self.apply_with(sentence, builtin_suggestions)
    }

    fn builtin_suggestions(&self, sentences: &[Sentence]) -> Vec<Suggestion> {
        self.builtins
            .iter()
            .filter(|rule| rule.enabled())
            .flat_map(|rule| rule.builtin().check_text(sentences))
            .collect()
    }

    fn apply_with(
        &self,
        sentence: &Sentence,
        builtin_suggestions: Vec<Suggestion>,
    ) -> Vec<Suggestion> {
        let spell_suggestions = self.spell.suggest(sentence);
        let sentence = MatchSentence::new(sentence);

        let mut output: Vec<(usize, Suggestion)> = self
            .rules
            .maybe_par_iter()
            .enumerate()
            .filter(|(_, rule)| rule.enabled())
            .map(|(i, rule)| {
                let mut output = Vec::new();

                for suggestion in rule.apply(&sentence) {
                    output.push((i, suggestion));
                }

                output
            })
            .flatten()
            .collect();

        output.extend(spell_suggestions.into_iter().map(|x| (0, x)));
        output.extend(builtin_suggestions.into_iter().map(|x| (0, x)));

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
    pub fn suggest(&self, text: &str, tokenizer: &Tokenizer) -> Vec<Suggestion> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut suggestions = Vec::new();
        let mut paragraph = Vec::new();

        for sentence in tokenizer.pipe(text) {
            let ends_paragraph = sentence.text().contains('\n');
            paragraph.push(sentence);

            if ends_paragraph {
                suggestions.extend(self.apply_paragraph(&paragraph));
                paragraph.clear();
            }
        }
        suggestions.extend(self.apply_paragraph(&paragraph));

        suggestions
    }

    fn apply_paragraph(&self, sentences: &[Sentence]) -> Vec<Suggestion> {
        let mut builtin_suggestions = self.builtin_suggestions(sentences);
        let mut suggestions = Vec::new();

        for sentence in sentences {
            let end = sentence.span().char().end;
            let (own, rest) = builtin_suggestions
                .into_iter()
                .partition(|x| x.span().char().start < end);
            builtin_suggestions = rest;

            suggestions.extend(self.apply_with(sentence, own));
        }

        suggestions
    }

    /// Correct a text by first tokenizing, then finding all suggestions and choosing the first replacement of each suggestion.
    pub fn correct(&self, text: &str, tokenizer: &Tokenizer) -> String {
        let suggestions = self.suggest(text, tokenizer);
        apply_suggestions(text, &suggestions)
    }
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

impl<R> FromIterator<R> for Rules
where
    R: Into<Rule>,
{
    fn from_iter<I: IntoIterator<Item = R>>(iter: I) -> Self {
        let rules: Vec<Rule> = iter.into_iter().map(|x| x.into()).collect();
        Self {
            rules,
            spell: Spell::default(),
            builtins: Vec::new(),
        }
    }
}
