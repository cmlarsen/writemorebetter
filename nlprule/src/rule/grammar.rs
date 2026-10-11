use super::engine::composition::{GraphId, MatchGraph, MatchSentence};
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
    fn convert(&self, input: &str, sample: &str) -> String {
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

/// An example associated with a [Rule][crate::rule::Rule].
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Example {
    pub(crate) text: String,
    pub(crate) suggestion: Option<Suggestion>,
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
    ) -> Option<Vec<String>> {
        let tagger = sentence.tagger();

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
                    .map(str::to_string),
            );
        } else if self.regex.is_some() {
            let target = self.target_pos_tag(&readings);
            for (lemma, _) in &readings {
                forms.extend(
                    self.synthesize_target(tagger, lemma, &target)
                        .into_iter()
                        .map(str::to_string),
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Match {
    pub(crate) id: GraphId,
    pub(crate) conversion: Conversion,
    pub(crate) pos_replacer: Option<PosReplacer>,
    pub(crate) regex_replacer: Option<(Regex, String)>,
    /// The static lemma of `<match>lemma</match>`, if present.
    pub(crate) static_lemma: Option<String>,
}

impl Match {
    /// The alternative strings this match expands to, mirroring LanguageTool's `toFinalString`.
    ///
    /// A match that synthesizes several forms yields several alternatives; each is converted with
    /// the configured case conversion, matching the point at which LanguageTool converts case.
    fn forms(&self, sentence: &MatchSentence, graph: &MatchGraph) -> Option<Vec<String>> {
        let group_text = graph.by_id(self.id).text(sentence);

        let forms = if let Some(replacer) = &self.pos_replacer {
            // LanguageTool's `toFinalString` overwrites the token with the synthesized forms, so
            // `regexp_match`/`regexp_replace` does not apply here.
            replacer.apply(
                sentence,
                graph,
                self.id,
                self.static_lemma.as_deref(),
                group_text,
            )?
        } else {
            let raw = self
                .static_lemma
                .clone()
                .unwrap_or_else(|| group_text.to_string());

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

    fn has_conversion(&self) -> bool {
        !matches!(self.conversion, Conversion::Nop)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum SynthesizerPart {
    Text(String),
    // Regex with the `fancy_regex` backend is large on the stack
    Match(Box<Match>),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Synthesizer {
    pub(crate) use_titlecase_adjust: bool,
    pub(crate) parts: Vec<SynthesizerPart>,
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
    ) -> Option<Vec<String>> {
        let mut output = vec![String::new()];

        let starts_with_conversion = match &self.parts[..] {
            [SynthesizerPart::Match(m), ..] => m.has_conversion(),
            _ => false,
        };

        for part in &self.parts {
            let nexts: Vec<String> = match part {
                SynthesizerPart::Text(t) => vec![t.clone()],
                SynthesizerPart::Match(m) => m.forms(sentence, graph)?,
            };

            let mut combined = Vec::with_capacity(output.len() * nexts.len());
            for prefix in &output {
                for next in &nexts {
                    combined.push(format!("{}{}", prefix, next));
                }
            }
            output = combined;
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
                .collect(),
        )
    }

    pub fn apply(
        &self,
        sentence: &MatchSentence,
        graph: &MatchGraph,
        start: GraphId,
        end: GraphId,
    ) -> Option<String> {
        Some(self.alternatives(sentence, graph, start, end)?.join("|"))
    }
}
