//! Sentence length and opening-pattern reports (#244). English classification is heuristic.
use crate::Document;
use nlprule::analysis::{AnalysisSentence, AnalysisSpan, AnalysisToken};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// Reporting context, determined by the first lexical word of a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Context {
    Narration,
    Dialogue,
}

/// Opening pattern. Unclassified preserves coverage and the denominator for ambiguous inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Start {
    Subject,
    Adverb,
    Gerund,
    Infinitive,
    SubordinatingConjunction,
    CoordinatingConjunction,
    PastParticiple,
    Inversion,
    Introductory,
    Unclassified,
}
const STARTS: [Start; 10] = [
    Start::Subject,
    Start::Adverb,
    Start::Gerund,
    Start::Infinitive,
    Start::SubordinatingConjunction,
    Start::CoordinatingConjunction,
    Start::PastParticiple,
    Start::Inversion,
    Start::Introductory,
    Start::Unclassified,
];

/// Caller-supplied thresholds and optional expected shares. Baselines may be partial.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub long_sentence_words: usize,
    pub minimum_sentences: usize,
    pub length_baseline: ByContext<BTreeMap<usize, f64>>,
    pub start_baseline: ByContext<BTreeMap<Start, f64>>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            long_sentence_words: 40,
            minimum_sentences: 200,
            length_baseline: ByContext::default(),
            start_baseline: ByContext::default(),
        }
    }
}

/// Invalid report options, reported before computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsError(pub String);
impl std::fmt::Display for OptionsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for OptionsError {}

/// A count and optional share/difference, suppressed below the sample-size threshold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Distribution {
    pub count: usize,
    pub share: Option<f64>,
    pub difference: Option<f64>,
}
/// Counts for narration and dialogue, computed independently.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ByContext<T> {
    pub narration: T,
    pub dialogue: T,
}
/// Statistics retained for each sentence in document order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentenceStats {
    pub span: AnalysisSpan,
    pub words: usize,
    pub dialogue_words: usize,
    pub narration_words: usize,
    pub context: Context,
    pub mixed: bool,
    pub start: Start,
    pub opening_word: String,
}
/// Exact word-count histogram, long sentences, and maximal overlapping monotony runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LengthReport {
    pub sentences: usize,
    pub histogram: BTreeMap<usize, Distribution>,
    pub long_sentences: Vec<AnalysisSpan>,
    pub monotonous_runs: Vec<AnalysisSpan>,
}
/// One opening category's distribution and sentence spans.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartCategory {
    pub distribution: Distribution,
    pub spans: Vec<AnalysisSpan>,
}
/// Opening categories and consecutive runs of three or more identical opening words.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartReport {
    pub sentences: usize,
    pub categories: BTreeMap<Start, StartCategory>,
    pub repeated_opening_runs: Vec<AnalysisSpan>,
}
/// Both reports from the same snapshot, without a second tokenization pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentenceReports {
    pub sentences: Vec<SentenceStats>,
    pub lengths: ByContext<LengthReport>,
    pub starts: ByContext<StartReport>,
}

fn lexical(token: &&AnalysisToken) -> bool {
    token.text.chars().any(char::is_alphanumeric)
}
fn has(token: &AnalysisToken, pos: &str) -> bool {
    token.readings.iter().any(|r| {
        r.pos == pos
            || r.pos
                .strip_prefix(pos)
                .is_some_and(|suffix| suffix.starts_with([':', '_']))
    })
}
fn noun(token: &AnalysisToken) -> bool {
    token.readings.iter().any(|r| r.pos.starts_with("NN"))
}
fn verb(token: &AnalysisToken) -> bool {
    token
        .readings
        .iter()
        .any(|r| r.pos.starts_with("VB") || r.pos == "MD")
}
fn lower(text: &str) -> String {
    text.to_lowercase().replace('’', "'")
}

/// Classifies an English sentence's opening using disambiguated Penn-style readings.
/// Precedence and lexical introductory/conjunction lists are documented in style/README.md.
pub fn classify(sentence: &AnalysisSentence) -> Start {
    let tokens: Vec<_> = sentence.tokens.iter().filter(lexical).collect();
    let Some(first) = tokens.first() else {
        return Start::Unclassified;
    };
    let word = lower(&first.text);
    let second = tokens.get(1);
    let opening = tokens
        .iter()
        .take(5)
        .map(|t| lower(&t.text))
        .collect::<Vec<_>>()
        .join(" ");
    if [
        "for example",
        "in fact",
        "as a result",
        "of course",
        "in contrast",
        "on the other hand",
    ]
    .iter()
    .any(|phrase| opening == *phrase || opening.starts_with(&format!("{phrase} ")))
        || [
            "however",
            "therefore",
            "meanwhile",
            "finally",
            "indeed",
            "instead",
            "moreover",
            "nevertheless",
            "furthermore",
            "well",
            "yes",
            "no",
        ]
        .contains(&word.as_str())
    {
        return Start::Introductory;
    }
    if has(first, "TO") && second.is_some_and(|t| has(t, "VB")) {
        return Start::Infinitive;
    }
    if [
        "although", "because", "if", "unless", "though", "whereas", "while", "when", "whenever",
        "whether", "once", "since",
    ]
    .contains(&word.as_str())
    {
        return Start::SubordinatingConjunction;
    }
    if has(first, "CC") {
        return Start::CoordinatingConjunction;
    }
    if has(first, "VBG") {
        return Start::Gerund;
    }
    let introductory_comma = second.is_some_and(|second| {
        sentence.text[first.span.byte.end - sentence.span.byte.start
            ..second.span.byte.start - sentence.span.byte.start]
            .contains(',')
    });
    if has(first, "VBN")
        && (!has(first, "VBD") || second.is_some_and(|t| has(t, "IN")) || introductory_comma)
    {
        return Start::PastParticiple;
    }
    let auxiliary = first.readings.iter().any(|r| {
        [
            "be", "do", "have", "can", "will", "may", "must", "shall", "should", "would", "could",
            "might",
        ]
        .contains(&r.lemma.as_str())
    });
    let subject_follows = second.is_some_and(|t| {
        ["i", "you", "he", "she", "it", "we", "they"].contains(&lower(&t.text).as_str())
            || noun(t)
            || has(t, "DT")
    });
    if verb(first)
        && subject_follows
        && (auxiliary
            || has(first, "VBD")
            || has(first, "VBZ")
            || (has(first, "VBP") && !has(first, "VB")))
    {
        return Start::Inversion;
    }
    if has(first, "RB") || has(first, "RBR") || has(first, "RBS") {
        return Start::Adverb;
    }
    // A noun reading alone is weak evidence when a bare verb also survives. Leave
    // likely imperatives unclassified; retain noun + finite-verb subjects ("Rain fell").
    if has(first, "VB") && !auxiliary && !second.is_some_and(|t| verb(t)) {
        return Start::Unclassified;
    }
    if noun(first)
        || has(first, "PRP")
        || has(first, "PRP$")
        || has(first, "DT")
        || has(first, "JJ")
    {
        return Start::Subject;
    }
    if has(first, "IN") || has(first, "TO") {
        return Start::Introductory;
    }
    Start::Unclassified
}

// Group adjacent lexical tokens across apostrophes/hyphens; punctuation and whitespace
// otherwise separate words. Contractions and dropped-g words count once; emoji don't count.
fn words(sentence: &AnalysisSentence) -> Vec<(AnalysisSpan, String)> {
    let tokens: Vec<_> = sentence.tokens.iter().filter(lexical).collect();
    let mut words: Vec<(AnalysisSpan, String)> = Vec::new();
    for token in tokens {
        if let Some((span, surface)) = words.last_mut() {
            let gap_start = span.byte.end - sentence.span.byte.start;
            let gap_end = token.span.byte.start - sentence.span.byte.start;
            let gap = &sentence.text[gap_start..gap_end];
            let clitic =
                token.text.starts_with(['\'', '’']) || token.text == "n't" || token.text == "n’t";
            if (gap.is_empty() && clitic)
                || (!gap.is_empty() && gap.chars().all(|c| matches!(c, '\'' | '’' | '-')))
            {
                surface.push_str(gap);
                surface.push_str(&token.text);
                span.byte.end = token.span.byte.end;
                span.char.end = token.span.char.end;
                span.utf16.end = token.span.utf16.end;
                continue;
            }
        }
        words.push((token.span.clone(), token.text.clone()));
    }
    words
}

fn distribution(count: usize, total: usize, minimum: usize, baseline: Option<f64>) -> Distribution {
    let share = (total > 0 && total >= minimum).then(|| count as f64 / total as f64);
    Distribution {
        count,
        share,
        difference: share
            .zip(baseline)
            .map(|(actual, expected)| actual - expected),
    }
}
fn joined(first: &AnalysisSpan, last: &AnalysisSpan) -> AnalysisSpan {
    AnalysisSpan {
        byte: first.byte.start..last.byte.end,
        char: first.char.start..last.char.end,
        utf16: first.utf16.start..last.utf16.end,
    }
}

fn monotony(stats: &[SentenceStats], context: Context) -> Vec<AnalysisSpan> {
    let mut result = Vec::new();
    let (mut low, mut high) = (VecDeque::<usize>::new(), VecDeque::<usize>::new());
    let mut left = 0;
    for right in 0..=stats.len() {
        let boundary = right == stats.len() || stats[right].context != context;
        if boundary {
            if right - left >= 4 {
                result.push(joined(&stats[left].span, &stats[right - 1].span));
            }
            low.clear();
            high.clear();
            left = right + 1;
            continue;
        }
        while low
            .back()
            .is_some_and(|&i| stats[i].words >= stats[right].words)
        {
            low.pop_back();
        }
        while high
            .back()
            .is_some_and(|&i| stats[i].words <= stats[right].words)
        {
            high.pop_back();
        }
        low.push_back(right);
        high.push_back(right);
        if stats[*high.front().unwrap()].words - stats[*low.front().unwrap()].words > 2 {
            if right - left >= 4 {
                result.push(joined(&stats[left].span, &stats[right - 1].span));
            }
            while stats[*high.front().unwrap()].words - stats[*low.front().unwrap()].words > 2 {
                if low.front() == Some(&left) {
                    low.pop_front();
                }
                if high.front() == Some(&left) {
                    high.pop_front();
                }
                left += 1;
            }
        }
    }
    result
}

/// Computes both reports. Shares/differences use each context's own sentence count.
pub fn report(document: &Document, options: &Options) -> Result<SentenceReports, OptionsError> {
    if options.long_sentence_words == 0 {
        return Err(OptionsError("long_sentence_words must be positive".into()));
    }
    for values in [
        options
            .length_baseline
            .narration
            .values()
            .collect::<Vec<_>>(),
        options.length_baseline.dialogue.values().collect(),
        options.start_baseline.narration.values().collect(),
        options.start_baseline.dialogue.values().collect(),
    ] {
        if values
            .iter()
            .any(|&&v| !v.is_finite() || !(0.0..=1.0).contains(&v))
            || values.iter().copied().sum::<f64>() > 1.0 + 1e-9
        {
            return Err(OptionsError(
                "baseline shares must be finite, between zero and one, and sum to at most one"
                    .into(),
            ));
        }
    }
    let sentences: Vec<_> = document
        .paragraphs
        .iter()
        .flat_map(|p| &p.sentences)
        .filter_map(|sentence| {
            let words = words(sentence);
            let (first, opening) = words.first()?;
            let dialogue_words = words
                .iter()
                .filter(|(span, _)| document.is_dialogue(span))
                .count();
            let context = if document.is_dialogue(first) {
                Context::Dialogue
            } else {
                Context::Narration
            };
            Some(SentenceStats {
                span: sentence.span.clone(),
                words: words.len(),
                dialogue_words,
                narration_words: words.len() - dialogue_words,
                context,
                mixed: dialogue_words > 0 && dialogue_words < words.len(),
                start: classify(sentence),
                opening_word: lower(opening),
            })
        })
        .collect();
    let build = |context| {
        let (length_baseline, start_baseline) = match context {
            Context::Narration => (
                &options.length_baseline.narration,
                &options.start_baseline.narration,
            ),
            Context::Dialogue => (
                &options.length_baseline.dialogue,
                &options.start_baseline.dialogue,
            ),
        };
        let selected: Vec<_> = sentences.iter().filter(|s| s.context == context).collect();
        let total = selected.len();
        let mut counts = BTreeMap::<usize, usize>::new();
        for sentence in &selected {
            *counts.entry(sentence.words).or_default() += 1;
        }
        for length in length_baseline.keys() {
            counts.entry(*length).or_default();
        }
        let histogram = counts
            .into_iter()
            .map(|(length, count)| {
                (
                    length,
                    distribution(
                        count,
                        total,
                        options.minimum_sentences,
                        length_baseline.get(&length).copied(),
                    ),
                )
            })
            .collect();
        let categories = STARTS
            .into_iter()
            .map(|category| {
                let spans: Vec<_> = selected
                    .iter()
                    .filter(|s| s.start == category)
                    .map(|s| s.span.clone())
                    .collect();
                (
                    category,
                    StartCategory {
                        distribution: distribution(
                            spans.len(),
                            total,
                            options.minimum_sentences,
                            start_baseline.get(&category).copied(),
                        ),
                        spans,
                    },
                )
            })
            .collect();
        let mut runs = Vec::new();
        let mut left = 0;
        while left < sentences.len() {
            let mut right = left + 1;
            while right < sentences.len()
                && sentences[right].context == sentences[left].context
                && sentences[right].opening_word == sentences[left].opening_word
            {
                right += 1;
            }
            if sentences[left].context == context && right - left >= 3 {
                runs.push(joined(&sentences[left].span, &sentences[right - 1].span));
            }
            left = right;
        }
        (
            LengthReport {
                sentences: total,
                histogram,
                long_sentences: selected
                    .iter()
                    .filter(|s| s.words > options.long_sentence_words)
                    .map(|s| s.span.clone())
                    .collect(),
                monotonous_runs: monotony(&sentences, context),
            },
            StartReport {
                sentences: total,
                categories,
                repeated_opening_runs: runs,
            },
        )
    };
    let (narration_lengths, narration_starts) = build(Context::Narration);
    let (dialogue_lengths, dialogue_starts) = build(Context::Dialogue);
    Ok(SentenceReports {
        sentences,
        lengths: ByContext {
            narration: narration_lengths,
            dialogue: dialogue_lengths,
        },
        starts: ByContext {
            narration: narration_starts,
            dialogue: dialogue_starts,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotony_matches_maximal_pairwise_windows() {
        // Exhaustive small sequences catch drifting adjacent-length comparisons, missed
        // overlapping windows, and containment errors in the linear sliding-window pass.
        for code in 0..4usize.pow(7) {
            let mut digits = code;
            let stats: Vec<_> = (0..7)
                .map(|i| {
                    let words = 2 + digits % 4;
                    digits /= 4;
                    SentenceStats {
                        span: AnalysisSpan {
                            byte: i..i + 1,
                            char: i..i + 1,
                            utf16: i..i + 1,
                        },
                        words,
                        dialogue_words: 0,
                        narration_words: words,
                        context: Context::Narration,
                        mixed: false,
                        start: Start::Unclassified,
                        opening_word: String::new(),
                    }
                })
                .collect();
            let valid = |lo: usize, hi: usize| {
                let counts = stats[lo..hi].iter().map(|s| s.words);
                counts.clone().max().unwrap() - counts.min().unwrap() <= 2
            };
            let mut expected = Vec::new();
            for lo in 0..stats.len() {
                for hi in lo + 4..=stats.len() {
                    if valid(lo, hi)
                        && (lo == 0 || !valid(lo - 1, hi))
                        && (hi == stats.len() || !valid(lo, hi + 1))
                    {
                        expected.push(lo..hi);
                    }
                }
            }
            let actual: Vec<_> = monotony(&stats, Context::Narration)
                .into_iter()
                .map(|span| span.byte)
                .collect();
            assert_eq!(
                actual,
                expected,
                "sequence {:?}",
                stats.iter().map(|s| s.words).collect::<Vec<_>>()
            );
        }
    }
}
