use super::engine::composition::{GraphId, Group, MatchGraph, MatchSentence};
use crate::builtin::en::{
    indefinite_article, lowercase_first_char_if_capitalized, Article, ArticleData,
};
use crate::tokenizer::tag::Tagger;
use crate::types::*;
use crate::utils::{self, regex::Regex};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Conversion {
    Nop,
    AllLower,
    StartLower,
    AllUpper,
    StartUpper,
    /// Uppercase like the matched text: all-uppercase if the match was
    /// all-uppercase, otherwise only its first character. Mirrors
    /// LanguageTool's `CaseConversionHelper.convertCase` `PRESERVE`.
    Preserve,
}

impl Conversion {
    pub(crate) fn convert(&self, input: &str, sample: &str) -> String {
        match &self {
            Conversion::Nop => input.to_string(),
            Conversion::AllLower => input.to_lowercase(),
            Conversion::StartLower => utils::apply_to_first(input, |c| c.to_lowercase().collect()),
            Conversion::AllUpper => input.to_uppercase(),
            Conversion::StartUpper => utils::apply_to_first(input, |c| c.to_uppercase().collect()),
            Conversion::Preserve => {
                let starts_upper = sample.chars().next().map_or(false, char::is_uppercase);
                if !starts_upper {
                    input.to_string()
                } else if sample.chars().all(|c| !c.is_lowercase()) {
                    input.to_uppercase()
                } else {
                    utils::apply_to_first(input, |c| c.to_uppercase().collect())
                }
            }
        }
    }
}

/// What LanguageTool expects a rule to do for one `<example>`.
///
/// LanguageTool's `PatternRuleHandler` classifies an example as an incorrect
/// example when it carries a `<marker>` or a `correction` attribute (or
/// `type="incorrect"`), as an error-triggering example when it carries
/// `type="triggers_error"`, and as a correct example otherwise.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum ExampleKind {
    /// The rule must not match (LanguageTool's `CorrectExample`).
    Correct,
    /// The rule must match exactly once; the match must be at the `<marker>`
    /// span, and the correction is checked if the example carries one
    /// (LanguageTool's `IncorrectExample`).
    Incorrect,
    /// The rule must match, but neither the match position nor a correction are
    /// checked (LanguageTool's `ErrorTriggeringExample`, `type="triggers_error"`).
    TriggersError,
}

/// An example associated with a [Rule][crate::rule::Rule].
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Example {
    pub(crate) text: String,
    pub(crate) suggestion: Option<Suggestion>,
    /// What LanguageTool expects of the rule for this example.
    pub(crate) kind: ExampleKind,
    /// The span of the `<marker>` in `text`, if the example has one.
    pub(crate) marker: Option<Span>,
}

impl Example {
    /// Gets the text of this example.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Gets the suggestion for this example.
    /// * If this is `None`, the associated rule should not trigger for this example.
    /// * If it is `Some`, the associated rule should return a suggestion with equivalent range and suggestions.
    pub fn suggestion(&self) -> Option<&Suggestion> {
        self.suggestion.as_ref()
    }

    /// What LanguageTool expects of this rule for this example.
    pub fn kind(&self) -> &ExampleKind {
        &self.kind
    }

    /// The span of the example's `<marker>` in [`Example::text`], if it has one.
    pub(crate) fn marker(&self) -> Option<&Span> {
        self.marker.as_ref()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PosReplacer {
    /// The raw `postag` attribute (used verbatim for the exact-match branch).
    pub(crate) postag: String,
    /// The compiled, anchored POS tag regex. `Some` when `postag_regexp="yes"` or when the match
    /// carries a static lemma (LanguageTool treats the latter as a regexp match too).
    pub(crate) regex: Option<Regex>,
    /// The `postag_replace` regex replacement applied to the matched token's own POS tag.
    pub(crate) postag_replace: Option<String>,
    /// LanguageTool's English determiner extensions (`+DT`/`+INDT`), if this
    /// match uses one. `None` for a normal synthesis.
    pub(crate) determiner: Option<Determiner>,
}

/// LanguageTool's English synthesizer extensions that prepend an article.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Determiner {
    /// `+INDT`: add "a" or "an", chosen for the following word.
    Indefinite,
    /// `+DT`: add "a"/"an" or "the".
    Definite,
}

impl PosReplacer {
    /// Computes the target POS tag(s) from the matched token readings, mirroring
    /// `MatchState.getTargetPosTag`.
    fn target_pos_tag(&self, readings: &[(&str, &str)]) -> String {
        let mut target = self.postag.clone();
        let mut pos_tags: Vec<String> = Vec::new();

        if let Some(regex) = &self.regex {
            for (_, pos) in readings {
                if regex.is_match(pos) {
                    target = (*pos).to_string();
                    pos_tags.push((*pos).to_string());
                }
            }

            if let Some(replace) = &self.postag_replace {
                if pos_tags.is_empty() {
                    pos_tags.push(target.clone());
                }
                target = pos_tags
                    .iter()
                    .map(|pos| regex.replace_all(pos, replace))
                    .collect::<Vec<_>>()
                    .join("|");
            }
        }

        target
    }

    /// Computes the target POS tag(s) for a match with a static lemma. Unlike the normal branch
    /// LanguageTool only applies `postag_replace` to the last matched reading's POS tag.
    fn static_target_pos_tag(&self, readings: &[(&str, &str)]) -> String {
        let mut target = self.postag.clone();

        if let Some(regex) = &self.regex {
            let mut matched = false;
            for (_, pos) in readings {
                if regex.is_match(pos) {
                    target = (*pos).to_string();
                    matched = true;
                }
            }

            if matched {
                if let Some(replace) = &self.postag_replace {
                    target = regex.replace_all(&target, replace);
                }
            }
        }

        target
    }

    /// Synthesizes all word forms of `lemma` whose POS tag matches the `target` tag/regex.
    fn synthesize_target<'t>(&self, tagger: &'t Tagger, lemma: &str, target: &str) -> Vec<&'t str> {
        let regex = Regex::new(format!("^(?:{})$", target));
        if regex.try_compile().is_err() {
            return Vec::new();
        }
        tagger.synthesize(lemma, |pos| regex.is_match(pos))
    }

    fn apply(
        &self,
        sentence: &MatchSentence,
        graph: &MatchGraph,
        id: GraphId,
        static_lemma: Option<&str>,
        group_text: &str,
        articles: Option<&ArticleData>,
    ) -> Option<Vec<String>> {
        let tagger = sentence.tagger();

        // `+DT`/`+INDT` without a regexp tag: LanguageTool's `EnglishSynthesizer`
        // returns the article chosen for the matched token itself (plus "the ..."
        // for `+DT`).
        if let Some(determiner) = self.determiner {
            if self.regex.is_none() {
                let article = articles.map_or(Article::Unknown, |data| data.article(group_text));
                let mut forms = vec![indefinite_article(article, group_text)];

                if determiner == Determiner::Definite {
                    forms.push(format!(
                        "the {}",
                        lowercase_first_char_if_capitalized(group_text)
                    ));
                }

                return Some(forms);
            }
        }

        // The article a `+DT`/`+INDT` regexp form is prefixed with; it is chosen
        // per lemma (`+INDT`) or is "the" (`+DT`).
        let determiner_prefix = |lemma: &str| match self.determiner {
            Some(Determiner::Definite) => "the ".to_string(),
            Some(Determiner::Indefinite) => {
                match articles.map_or(Article::Unknown, |data| data.article(lemma)) {
                    Article::A => "a ".to_string(),
                    Article::An => "an ".to_string(),
                    Article::Unknown => String::new(),
                }
            }
            None => String::new(),
        };

        let readings: Vec<(&str, &str)> = graph
            .by_id(id)
            .tokens(sentence)
            .flat_map(|token| {
                token
                    .word()
                    .tags()
                    .iter()
                    .map(|data| (data.lemma().as_str(), data.pos().as_str()))
            })
            // LanguageTool skips readings without a part-of-speech tag (the tokenizer's raw
            // reading); nlprule represents that as an empty POS tag.
            .filter(|(_, pos)| !pos.is_empty())
            .collect();

        let mut forms: Vec<String> = Vec::new();

        if let Some(lemma) = static_lemma {
            // A static lemma (`<match>lemma</match>`) is always synthesized like a POS regex in
            // LanguageTool, with the target POS taken from the matched token's readings.
            let target = self.static_target_pos_tag(&readings);
            forms.extend(
                self.synthesize_target(tagger, lemma, &target)
                    .into_iter()
                    .map(|form| format!("{}{}", determiner_prefix(lemma), form)),
            );
        } else if self.regex.is_some() {
            let target = self.target_pos_tag(&readings);
            for (lemma, _) in &readings {
                forms.extend(
                    self.synthesize_target(tagger, lemma, &target)
                        .into_iter()
                        .map(|form| format!("{}{}", determiner_prefix(lemma), form)),
                );
            }
        } else {
            for (lemma, _) in &readings {
                forms.extend(
                    tagger
                        .synthesize(lemma, |pos| pos == self.postag)
                        .into_iter()
                        .map(str::to_string),
                );
            }
        }

        forms.sort();
        forms.dedup();

        if forms.is_empty() {
            if static_lemma.is_some() || self.regex.is_some() {
                // LanguageTool emits "(token)" when nothing could be synthesized.
                forms.push(format!("({})", static_lemma.unwrap_or(group_text)));
            } else {
                return None;
            }
        }

        Some(forms)
    }
}

/// How LanguageTool's `<match include_skipped="...">` extends the match with the
/// tokens skipped by the `skip` attribute of the referenced pattern token.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IncludeRange {
    /// `all`: the matched tokens followed by the skipped tokens.
    All,
    /// `following`: only the skipped tokens that follow the matched token.
    Following,
}

/// The text of `group` with every token whose text the tokenizer normalized
/// (a curly apostrophe in a recognized contraction) replaced by its normalized
/// form, as LanguageTool's `\n` references expand to the token text.
fn normalized_text(group: &Group, sentence: &MatchSentence) -> String {
    let raw = group.text(sentence);
    if !raw.contains('\u{2019}') {
        return raw.to_string();
    }

    let mut out = String::with_capacity(raw.len());
    let mut position = group.span.start();
    for token in group.tokens(sentence) {
        out.push_str(sentence.slice(Span::from_positions(position, token.span().start())));
        let text = sentence.slice(token.span().clone());
        let word = token.word().as_str();
        if word != text && word == text.replace('\u{2019}', "'") {
            out.push_str(word);
        } else {
            out.push_str(text);
        }
        position = token.span().end();
    }
    out.push_str(sentence.slice(Span::from_positions(position, group.span.end())));
    out
}

/// A resolved `include_skipped`: which range to take and the graph index of the
/// skip group that follows the referenced element.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub(crate) struct IncludeSkipped {
    pub(crate) range: IncludeRange,
    pub(crate) group: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Match {
    pub(crate) id: GraphId,
    pub(crate) conversion: Conversion,
    pub(crate) pos_replacer: Option<PosReplacer>,
    pub(crate) regex_replacer: Option<(Regex, String)>,
    /// The static lemma of `<match>lemma</match>`, if present.
    pub(crate) static_lemma: Option<String>,
    /// The `include_skipped` attribute, resolved to the following skip group.
    pub(crate) include_skipped: Option<IncludeSkipped>,
}

impl Match {
    /// The alternative strings this match expands to, mirroring LanguageTool's `toFinalString`.
    ///
    /// A match that synthesizes several forms yields several alternatives; each is converted with
    /// the configured case conversion, matching the point at which LanguageTool converts case.
    pub(crate) fn forms(
        &self,
        sentence: &MatchSentence,
        graph: &MatchGraph,
        articles: Option<&ArticleData>,
    ) -> Option<Vec<String>> {
        let group_text = graph.by_id(self.id).text(sentence);

        let forms = if let Some(replacer) = &self.pos_replacer {
            // LanguageTool's `toFinalString` overwrites the token with the synthesized forms, so
            // `regexp_match`/`regexp_replace` does not apply here.
            let mut forms = replacer.apply(
                sentence,
                graph,
                self.id,
                self.static_lemma.as_deref(),
                group_text,
                articles,
            )?;

            // `include_skipped` appends the skipped tokens to every synthesized
            // form (or replaces them with just the skipped tokens for
            // "following"), matching `MatchState.toFinalString`.
            if let Some(include) = self.include_skipped {
                let skipped = self.skipped_text(sentence, graph);
                forms = match include.range {
                    IncludeRange::All => forms
                        .into_iter()
                        .map(|form| format!("{}{}", form, skipped))
                        .collect(),
                    IncludeRange::Following => vec![skipped.trim_start().to_string()],
                };
            }

            forms
        } else {
            let raw = self
                .static_lemma
                .clone()
                .unwrap_or_else(|| normalized_text(graph.by_id(self.id), sentence));

            // Without a synthesis, the match text is extended over the skipped
            // tokens directly.
            let raw = match self.include_skipped {
                None => raw,
                Some(include) => match include.range {
                    IncludeRange::All => {
                        format!("{}{}", raw, self.skipped_text(sentence, graph))
                    }
                    IncludeRange::Following => {
                        self.skipped_text(sentence, graph).trim_start().to_string()
                    }
                },
            };

            let raw = if let Some((regex, replacement)) = &self.regex_replacer {
                regex.replace_all(&raw, replacement)
            } else {
                raw
            };

            vec![raw]
        };

        Some(
            forms
                .into_iter()
                .map(|form| self.conversion.convert(&form, group_text))
                .collect(),
        )
    }

    /// The tokens skipped by the referenced element, including the whitespace
    /// before the first one (LanguageTool's `skippedTokens`).
    fn skipped_text<'a>(&self, sentence: &'a MatchSentence, graph: &MatchGraph) -> &'a str {
        match self.include_skipped {
            None => "",
            Some(include) => {
                let group = graph.by_id(self.id);
                let skipped = graph.by_index(include.group);

                if skipped.span.end() <= group.span.end() {
                    return "";
                }

                sentence.slice(Span::from_positions(group.span.end(), skipped.span.end()))
            }
        }
    }

    fn has_conversion(&self) -> bool {
        !matches!(self.conversion, Conversion::Nop)
    }

    /// Whether this match uses LanguageTool's `+DT`/`+INDT` determiner synthesis.
    pub(crate) fn has_determiner(&self) -> bool {
        self.pos_replacer
            .as_ref()
            .map_or(false, |replacer| replacer.determiner.is_some())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum SynthesizerPart {
    Text(String),
    // Regex with the `fancy_regex` backend is large on the stack
    Match(Box<Match>),
}

/// The first literal character after `parts`, and whether `parts` is the end of
/// the message, mirroring LanguageTool's `PatternRuleMatcher.concatWithoutExtraSpace`
/// right-hand side.
fn next_text_char(parts: &[SynthesizerPart]) -> (Option<char>, bool) {
    for part in parts {
        match part {
            SynthesizerPart::Text(t) => {
                if let Some(c) = t.chars().next() {
                    return (Some(c), false);
                }
            }
            // A following match renders as a non-whitespace reference placeholder.
            SynthesizerPart::Match(_) => return (None, false),
        }
    }
    (None, true)
}

/// LanguageTool's `WHITESPACE_OR_PUNCT` (`[\s,:;.!?]`).
fn is_whitespace_or_punct(c: char) -> bool {
    c.is_whitespace() || matches!(c, ',' | ':' | ';' | '.' | '!' | '?')
}

/// Whether LanguageTool's `SUGGESTION_PATTERN_SUPPRESS` removes `suggestion`: it holds
/// exactly one parenthesized part (a token that could not be synthesized) and no
/// other parentheses or angle brackets.
fn is_unsynthesized(suggestion: &str) -> bool {
    if suggestion.contains(['<', '>']) {
        return false;
    }
    let mut parens = suggestion.chars().filter(|c| matches!(c, '(' | ')'));
    matches!(
        (parens.next(), parens.next(), parens.next()),
        (Some('('), Some(')'), None)
    )
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Synthesizer {
    pub(crate) use_titlecase_adjust: bool,
    pub(crate) parts: Vec<SynthesizerPart>,
    /// LanguageTool's `suppress_misspelled="yes"` on the suggestion or the rule's message:
    /// an alternative containing a token that could not be synthesized (rendered as
    /// `(token)`) is dropped.
    pub(crate) suppress_misspelled: bool,
}

impl Synthesizer {
    /// The alternative strings this synthesizer can produce.
    ///
    /// A part that expands to several forms (inflection synthesis) yields several alternatives, in
    /// the same way LanguageTool's `toFinalString` + `combineLists` build a Cartesian product over
    /// the parts of a `<suggestion>`.
    pub(crate) fn alternatives(
        &self,
        sentence: &MatchSentence,
        graph: &MatchGraph,
        start: GraphId,
        _end: GraphId,
        articles: Option<&ArticleData>,
    ) -> Option<Vec<String>> {
        let mut output = vec![String::new()];
        // Whether the next text part should lose one leading space because the
        // preceding match expanded to the empty string (see below).
        let mut skip_leading_space = vec![false];

        let starts_with_conversion = match &self.parts[..] {
            [SynthesizerPart::Match(m), ..] => m.has_conversion(),
            _ => false,
        };

        for (idx, part) in self.parts.iter().enumerate() {
            match part {
                SynthesizerPart::Text(t) => {
                    for (i, prefix) in output.iter_mut().enumerate() {
                        if skip_leading_space[i] {
                            skip_leading_space[i] = false;
                            prefix.push_str(t.strip_prefix(' ').unwrap_or(t));
                        } else {
                            prefix.push_str(t);
                        }
                    }
                }
                SynthesizerPart::Match(m) => {
                    let nexts = m.forms(sentence, graph, articles)?;

                    // LanguageTool's `PatternRuleMatcher.formatMatches`: a
                    // reference to a token with `min="0"` that did not take part
                    // in the match expands to "" and the space next to it is
                    // removed (`concatWithoutExtraSpace`). Only a match that
                    // expands to exactly the empty string triggers this.
                    if nexts.len() == 1 && nexts[0].is_empty() {
                        let (right_char, at_end) = next_text_char(&self.parts[idx + 1..]);
                        for (i, prefix) in output.iter_mut().enumerate() {
                            if prefix.ends_with(' ')
                                && (at_end || right_char.map_or(false, is_whitespace_or_punct))
                            {
                                prefix.pop();
                            } else if prefix.is_empty() && right_char == Some(' ') {
                                skip_leading_space[i] = true;
                            }
                        }
                        continue;
                    }

                    let mut combined = Vec::with_capacity(output.len() * nexts.len());
                    let mut combined_skip = Vec::with_capacity(output.len() * nexts.len());
                    for (i, prefix) in output.iter().enumerate() {
                        for next in &nexts {
                            combined.push(format!("{}{}", prefix, next));
                            combined_skip.push(skip_leading_space[i]);
                        }
                    }
                    output = combined;
                    skip_leading_space = combined_skip;
                }
            }
        }

        // if the suggestion does not start with a case conversion match, make it title case if:
        // * at sentence start
        // * the replaced text is title case
        let make_uppercase = !starts_with_conversion
            && graph.groups()[graph.get_index(start)..]
                .iter()
                .find_map(|x| x.tokens(sentence).next())
                .map(|first_token| {
                    (self.use_titlecase_adjust
                        && first_token
                            .word()
                            .as_str()
                            .chars()
                            .next() // a word is expected to always have at least one char, but be defensive here
                            .map_or(false, char::is_uppercase))
                        || first_token.span().start() == sentence.span().start()
                })
                .unwrap_or(false);

        Some(
            output
                .into_iter()
                .map(|suggestion| {
                    let suggestion = utils::normalize_whitespace(&suggestion);
                    if make_uppercase {
                        utils::apply_to_first(&suggestion, |x| x.to_uppercase().collect())
                    } else {
                        suggestion
                    }
                })
                .filter(|suggestion| !(self.suppress_misspelled && is_unsynthesized(suggestion)))
                .collect(),
        )
    }

    pub fn apply(
        &self,
        sentence: &MatchSentence,
        graph: &MatchGraph,
        start: GraphId,
        end: GraphId,
        articles: Option<&ArticleData>,
    ) -> Option<String> {
        Some(
            self.alternatives(sentence, graph, start, end, articles)?
                .join("|"),
        )
    }

    /// Whether any part of this synthesizer uses LanguageTool's `+DT`/`+INDT`
    /// determiner synthesis (and therefore needs the article word lists).
    pub(crate) fn uses_determiner(&self) -> bool {
        self.parts.iter().any(|part| match part {
            SynthesizerPart::Match(m) => m.has_determiner(),
            SynthesizerPart::Text(_) => false,
        })
    }
}
