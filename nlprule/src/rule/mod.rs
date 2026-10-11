//! Implementations related to single rules.

use crate::types::*;
use crate::{
    filter::{Filter, Filterable},
    tokenizer::Tokenizer,
    utils,
};
use itertools::Itertools;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

pub(crate) mod disambiguation;
pub(crate) mod engine;
pub(crate) mod filter;
pub(crate) mod grammar;
pub mod id;

use engine::Engine;

pub(crate) use engine::composition::{MatchGraph, MatchSentence};
pub use grammar::{Example, ExampleKind};

use self::{
    disambiguation::PosFilter,
    engine::{composition::GraphId, EngineMatches},
    id::Index,
};

/// A *Unification* makes an otherwise matching pattern invalid if no combination of its filters
/// matches all tokens marked with "unify".
/// Can also be negated.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct Unification {
    pub(crate) mask: Vec<Option<bool>>,
    pub(crate) filters: Vec<Vec<PosFilter>>,
}

impl Unification {
    pub fn keep(&self, graph: &MatchGraph, sentence: &MatchSentence) -> bool {
        let filters: Vec<_> = self.filters.iter().multi_cartesian_product().collect();

        let mut filter_mask: Vec<_> = filters.iter().map(|_| true).collect();
        let negate = self.mask.iter().all(|x| x.map_or(true, |x| !x));

        for (group, maybe_mask_val) in graph.groups()[1..].iter().zip(self.mask.iter()) {
            if maybe_mask_val.is_some() {
                for token in group.tokens(sentence) {
                    for (mask_val, filter) in filter_mask.iter_mut().zip(filters.iter()) {
                        *mask_val = *mask_val && PosFilter::and(filter, token.word());
                    }
                }
            }
        }

        let result = filter_mask.iter().any(|x| *x);
        if negate {
            !result
        } else {
            result
        }
    }
}

/// A disambiguation rule.
/// Changes the information associcated with one or more tokens if it matches.
/// Sourced from LanguageTool. An example of how a simple rule might look in the original XML format:
///
/// ```xml
/// <rule id="NODT_HAVE" name="no determiner + have as verb/noun ->have/verb">
///    <pattern>
///        <token>
///             <exception postag="PRP$"></exception>
///             <exception regexp="yes">the|a</exception>
///        </token>
///        <marker>
///            <token case_sensitive="yes" regexp="yes">[Hh]ave|HAVE</token>
///        </marker>
///    </pattern>
///    <disambig action="replace"><wd lemma="have" pos="VB"></wd></disambig>
/// </rule>
/// ```
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DisambiguationRule {
    pub(crate) id: Index,
    pub(crate) engine: Engine,
    pub(crate) disambiguations: disambiguation::Disambiguation,
    pub(crate) filter: Option<Filter>,
    pub(crate) start: GraphId,
    pub(crate) end: GraphId,
    pub(crate) examples: Vec<disambiguation::DisambiguationExample>,
    pub(crate) unification: Option<Unification>,
}

#[derive(Default)]
pub(crate) struct Changes(Vec<Vec<HashSet<Span>>>);

// This is only used in tests at the moment.
// Could maybe be made generic.
impl Changes {
    fn lshift(self, position: Position) -> Self {
        Changes(
            self.0
                .into_iter()
                .map(|spans| {
                    spans
                        .into_iter()
                        .map(|group_spans| {
                            group_spans
                                .into_iter()
                                .map(|span| span.lshift(position))
                                .collect()
                        })
                        .collect()
                })
                .collect(),
        )
    }
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl DisambiguationRule {
    /// Get a unique identifier of this rule.
    pub fn id(&self) -> &Index {
        &self.id
    }

    pub(crate) fn apply<'t>(&'t self, sentence: &MatchSentence<'t>) -> Changes {
        if matches!(self.disambiguations, disambiguation::Disambiguation::Nop) {
            return Changes::default();
        }

        let mut all_spans = Vec::new();

        for graph in self.engine.get_matches(sentence, self.start, self.end) {
            if let Some(unification) = &self.unification {
                if !unification.keep(&graph, sentence) {
                    continue;
                }
            }

            if let Some(filter) = &self.filter {
                if !filter.keep(sentence, &graph) {
                    continue;
                }
            }

            let mut spans = Vec::new();

            for group_idx in GraphId::range(&self.start, &self.end) {
                let group = graph.by_id(group_idx);

                let group_spans: HashSet<_> =
                    group.tokens(sentence).map(|x| x.span().clone()).collect();

                spans.push(group_spans);
            }

            all_spans.push(spans);
        }

        Changes(all_spans)
    }

    pub(crate) fn change<'t>(&'t self, sentence: &mut IncompleteSentence<'t>, changes: Changes) {
        log::info!("applying {}", self.id);

        for spans in changes.0 {
            let mut groups = Vec::new();
            let mut refs = sentence.iter_mut().collect::<Vec<_>>();

            for group_spans in spans {
                let mut group = Vec::new();

                while let Some(i) = refs.iter().position(|x| group_spans.contains(&x.span())) {
                    group.push(refs.remove(i));
                }

                groups.push(group);
            }

            self.disambiguations.apply(groups);
        }
    }

    /// Often there are examples associated with a rule.
    /// This method checks whether the correct action is taken in the examples.
    pub fn test(&self, tokenizer: &Tokenizer) -> bool {
        let mut passes = Vec::new();

        for (i, test) in self.examples.iter().enumerate() {
            let text = match test {
                disambiguation::DisambiguationExample::Unchanged(x) => x.as_str(),
                disambiguation::DisambiguationExample::Changed(x) => x.text.as_str(),
            };

            // by convention examples are always considered as one sentence even if the sentencizer would split
            let sentence_before = tokenizer.disambiguate_up_to_id(
                tokenizer
                    .tokenize(text)
                    .expect("test text must not be empty"),
                Some(&self.id),
            );

            // shift the sentence to the right before matching to make sure
            // nothing assumes the sentene starts from absolute index zero
            let shift_delta = Position { byte: 1, char: 1 };
            let sentence_before_complete =
                sentence_before.clone().rshift(shift_delta).into_sentence();
            let changes = self
                .apply(&MatchSentence::new(&sentence_before_complete))
                .lshift(shift_delta);

            let mut sentence_after = sentence_before.clone();

            if !changes.is_empty() {
                self.change(&mut sentence_after, changes);
            }

            info!("Tokens: {:#?}", sentence_before);

            let pass = match test {
                disambiguation::DisambiguationExample::Unchanged(_) => {
                    sentence_before == sentence_after
                }
                disambiguation::DisambiguationExample::Changed(change) => {
                    let before = sentence_before
                        .iter()
                        .find(|x| *x.span().char() == change.char_span);
                    let after = sentence_after
                        .iter()
                        .find(|x| *x.span().char() == change.char_span);

                    // The tokenizer may split the example text differently from
                    // the marker the rule author used (e.g. "ain't" becomes
                    // "ain" + "'" + "t" instead of "ai" + "n't"). LT still runs
                    // the example, so report it as a failure instead of
                    // panicking on the missing token.
                    if let (Some(_before), Some(after)) = (before, after) {
                        let unordered_tags = after
                            .word()
                            .tags()
                            .iter()
                            .map(|x| x.to_owned_word_data())
                            .collect::<HashSet<owned::WordData>>();
                        // need references to compare
                        let unordered_tags: HashSet<_> = unordered_tags.iter().collect();
                        let unordered_tags_change = change
                            .after
                            .tags
                            .iter()
                            .collect::<HashSet<&owned::WordData>>();

                        after.word().as_str() == change.after.text.as_ref_id().as_str()
                            && unordered_tags == unordered_tags_change
                    } else {
                        false
                    }
                }
            };

            if !pass {
                let error_str = format!(
                    "Rule {}: Test \"{:#?}\" failed. Before: {:#?}. After: {:#?}.",
                    self.id, test, sentence_before, sentence_after,
                );

                if tokenizer
                    .lang_options()
                    .known_failures
                    .contains(&format!("{}:{}", self.id, i))
                {
                    warn!("{}", error_str)
                } else {
                    error!("{}", error_str)
                }
            }

            passes.push(pass);
        }

        passes.iter().all(|x| *x)
    }
}

/// An iterator over [Suggestion][crate::types::Suggestion]s.
pub struct Suggestions<'a, 't> {
    rule: &'a Rule,
    matches: EngineMatches<'a, 't>,
    sentence: &'t MatchSentence<'t>,
    /// Overrides "today" for the time-dependent rule filters; the rule test
    /// harness passes LanguageTool's fixed test date.
    now: Option<filter::Date>,
}

impl<'a, 't> Iterator for Suggestions<'a, 't> {
    type Item = Suggestion;

    fn next(&mut self) -> Option<Self::Item> {
        let rule = self.rule;
        let sentence = self.sentence;
        let (start, end) = (self.rule.start, self.rule.end);
        let now = self.now;

        self.matches.find_map(|graph| {
            if let Some(unification) = &rule.unification {
                if !unification.keep(&graph, sentence) {
                    return None;
                }
            }

            let start_group = graph.by_id(start);
            let end_group = graph.by_id(end);

            let replacements: Vec<String> = rule
                .suggesters
                .iter()
                .filter_map(|x| x.alternatives(sentence, &graph, start, end))
                .flatten()
                .collect();

            // A rule without a <suggestion> still reports a match, with its
            // message and no replacement (LanguageTool's behaviour). Such a rule
            // never has replacements, so mark it here to emit the match below.
            let no_suggester = rule.suggesters.is_empty();

            let start = if !replacements.is_empty()
                && replacements
                    .iter()
                    .all(|x| utils::no_space_chars().chars().any(|c| x.starts_with(c)))
            {
                let first_token = graph.groups()[graph.get_index(start)..]
                    .iter()
                    .find_map(|x| x.tokens(sentence).next())
                    .unwrap();

                let idx = sentence
                    .iter()
                    .position(|x| std::ptr::eq(x, first_token))
                    .unwrap_or(0);

                if idx > 0 {
                    sentence.index(idx - 1).span().end()
                } else {
                    start_group.span.start()
                }
            } else {
                start_group.span.start()
            };
            let end = end_group.span.end();

            // this should never happen, but just return None instead of raising an Error
            // `end` COULD be equal to `start` if the suggestion is to insert text at this position
            if end < start {
                return None;
            }

            let text_before = sentence.slice(Span::from_positions(start, end));

            // fix e. g. "Super , dass"
            let replacements: Vec<String> = replacements
                .into_iter()
                .map(|x| utils::fix_nospace_chars(&x))
                .collect();

            let message = rule
                .message
                .apply(sentence, &graph, rule.start, rule.end)
                .expect("Rules must have a message.");

            let match_ = filter::FilterMatch {
                from: start,
                to: end,
                message,
                replacements,
            };

            // A rule filter may veto the match entirely or return it with
            // rewritten suggestions / range. It sees the raw suggestions
            // (before the "suggestion equals the matched text" cleanup below),
            // as LanguageTool filters do.
            let match_ = match &rule.filter {
                Some(rule_filter) => rule_filter.accept(match_, sentence, &graph, now)?,
                None => match_,
            };

            let replacements: Vec<String> = match_
                .replacements
                .into_iter()
                .filter(|suggestion| suggestion.as_str() != text_before)
                .collect();

            if !replacements.is_empty() {
                Some(Suggestion::new(
                    rule.id.to_string(),
                    match_.message,
                    Span::from_positions(match_.from, match_.to),
                    replacements,
                ))
            } else if no_suggester {
                // LanguageTool reports a match for a rule without a <suggestion>
                // with its message and no replacement; emit it so the rule is
                // visible and its examples can be tested.
                Some(Suggestion::new(
                    rule.id.to_string(),
                    rule.message
                        .apply(sentence, &graph, rule.start, rule.end)
                        .expect("Rules must have a message."),
                    Span::from_positions(start, end),
                    Vec::new(),
                ))
            } else {
                None
            }
        })
    }
}

/// A grammar rule.
/// Returns a [Suggestion][crate::types::Suggestion] for change if it matches.
/// Sourced from LanguageTool. An example of how a simple rule might look in the original XML format:
///
/// ```xml
/// <rule id="DOSNT" name="he dosn't (doesn't)">
///     <pattern>
///         <token regexp="yes">do[se]n|does|dosan|doasn|dosen</token>
///         <token regexp="yes">['’`´‘]</token>
///         <token>t</token>
///     </pattern>
///     <message>Did you mean <suggestion>doesn\2t</suggestion>?</message>
///     <example correction="doesn't">He <marker>dosn't</marker> know about it.</example>
/// </rule>
/// ```
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Rule {
    pub(crate) id: Index,
    pub(crate) engine: Engine,
    pub(crate) examples: Vec<Example>,
    pub(crate) suggesters: Vec<grammar::Synthesizer>,
    pub(crate) message: grammar::Synthesizer,
    pub(crate) start: GraphId,
    pub(crate) end: GraphId,
    pub(crate) url: Option<String>,
    pub(crate) short: Option<String>,
    pub(crate) name: String,
    pub(crate) category_name: String,
    pub(crate) category_type: Option<String>,
    pub(crate) unification: Option<Unification>,
    pub(crate) filter: Option<filter::RuleFilter>,
    pub(crate) enabled: bool,
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

/// Whether the suggestion a rule reported for an example satisfies what the
/// example expects.
///
/// LanguageTool accepts an empty correction (`correction=""`, i.e. delete the
/// match) when the rule reports a match with no replacement, which is exactly
/// what a rule without a `<suggestion>` does. An all-empty expectation is
/// therefore satisfied by an all-empty result at the same span; otherwise the
/// expectation and the result must share a replacement (LanguageTool compares
/// the lists, but nlprule keeps a set-like comparison for compatibility).
fn example_suggestion_matches(expected: &Suggestion, actual: &Suggestion) -> bool {
    expected.span() == actual.span()
        && if expected.replacements().iter().all(|x| x.is_empty()) {
            actual.replacements().iter().all(|x| x.is_empty())
        } else {
            expected
                .replacements()
                .iter()
                .any(|x| actual.replacements().contains(x))
        }
}

impl Rule {
    /// Hints that this rule should be enabled.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Hints that this rule should be disabled.
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Hints whether the rule should be enabled in a rule set.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Get a unique identifier of this rule.
    pub fn id(&self) -> &Index {
        &self.id
    }

    /// Gets a short text describing this rule e.g. "Possible typo" if there is one.
    pub fn short(&self) -> Option<&str> {
        self.short.as_deref()
    }

    /// Gets an url with more information about this rule if there is one.
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// Gets the examples associated with this rule.
    pub fn examples(&self) -> &[Example] {
        &self.examples
    }

    /// Gets a human-readable name of this rule.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Gets a human-readable name of the category this rule is in.
    pub fn category_name(&self) -> &str {
        &self.category_name
    }

    /// Gets the type of the category this rule is in e. g. "style" or "grammar".
    pub fn category_type(&self) -> Option<&str> {
        self.category_type.as_deref()
    }

    pub(crate) fn apply<'a, 't>(&'a self, sentence: &'t MatchSentence<'t>) -> Suggestions<'a, 't> {
        self.apply_with_date(sentence, None)
    }

    /// Like [`Rule::apply`], but with an explicit "today" for the
    /// time-dependent filters (the rule test harness passes LanguageTool's
    /// fixed test date).
    fn apply_with_date<'a, 't>(
        &'a self,
        sentence: &'t MatchSentence<'t>,
        now: Option<filter::Date>,
    ) -> Suggestions<'a, 't> {
        Suggestions {
            matches: self.engine.get_matches(sentence, self.start, self.end),
            rule: self,
            sentence,
            now,
        }
    }

    /// Grammar rules always have at least one example associated with them.
    /// This method checks whether the correct action is taken in the examples.
    pub fn test(&self, tokenizer: &Tokenizer) -> bool {
        let mut passes = Vec::new();

        // make sure relative position is handled correctly
        // shifting the entire sentence must be a no-op as far as the matcher is concerned
        // if the suggestions are shifted back
        let shift_delta = Position { byte: 1, char: 1 };

        for test in self.examples.iter() {
            // by convention examples are always considered as one sentence even if the sentencizer would split
            let sentence = tokenizer
                .disambiguate(
                    tokenizer
                        .tokenize(&test.text())
                        .expect("test text must not be empty."),
                )
                .rshift(shift_delta)
                .into_sentence();

            info!("Sentence: {:#?}", sentence);
            let suggestions: Vec<_> = self
                .apply_with_date(&MatchSentence::new(&sentence), Some(filter::test_date()))
                .map(|s| s.lshift(shift_delta))
                .collect();

            let pass = match test.kind() {
                // A correct example must not match at all.
                ExampleKind::Correct => suggestions.is_empty(),
                // A triggers_error example must match; LanguageTool does not check
                // the match position or the correction here.
                ExampleKind::TriggersError => !suggestions.is_empty(),
                // An incorrect example must match exactly once. A correction pins
                // the replacement; without one, LanguageTool only requires the
                // match to start at the marker span and accepts an empty
                // replacement (what a rule without a <suggestion> produces).
                ExampleKind::Incorrect => {
                    if suggestions.len() != 1 {
                        false
                    } else if let Some(correct_suggestion) = test.suggestion() {
                        example_suggestion_matches(correct_suggestion, &suggestions[0])
                    } else if let Some(marker) = test.marker() {
                        suggestions[0].span() == marker
                    } else {
                        true
                    }
                }
            };

            if !pass {
                warn!(
                    "Rule {}: test \"{}\" failed. Expected: {:#?}. Found: {:#?}.",
                    self.id,
                    test.text(),
                    test.suggestion(),
                    suggestions
                );
            }

            passes.push(pass);
        }

        passes.iter().all(|x| *x)
    }
}
