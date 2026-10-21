//! Clarity threshold sweep for `bench/style/clarity.py`: reads `{"name", "text", "hardWrapped"}`
//! lines on stdin and prints one JSON summary per line; `--review` adds the default flags' text.
use std::collections::BTreeSet;
use std::io::{self, BufRead};

use nlprule::Tokenizer;
use serde_json::json;
use writemorebetter_style::reports::clarity::{self, ClarityOptions};
use writemorebetter_style::{analyze_document, DocumentOptions};

const MINIMUM_WORDS: [usize; 3] = [10, 12, 15];
const GLUE_PERCENT: [f64; 5] = [55.0, 60.0, 65.0, 70.0, 75.0];
const CLAUSE_WORDS: [usize; 5] = [15, 18, 20, 25, 30];

fn main() {
    let tokenizer = Tokenizer::new("storage/en_tokenizer.bin").unwrap();
    let review = std::env::args().any(|a| a == "--review");
    let defaults = ClarityOptions::new();
    for line in io::stdin().lock().lines() {
        let row: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
        let text = row["text"].as_str().unwrap();
        let options =
            DocumentOptions::new().with_hard_wrapped(row["hardWrapped"].as_bool().unwrap_or(false));
        let document = analyze_document(&tokenizer, text, &options);
        // Every sentence and every clause, so each sweep row filters one report.
        let all = clarity::report(
            &document,
            &ClarityOptions::new()
                .with_minimum_sentence_words(1)
                .with_sticky_glue_percent(0.0)
                .with_long_clause_words(1),
        )
        .unwrap();
        let mut percents: Vec<_> = all
            .sticky_sentences
            .iter()
            .filter(|s| s.words >= defaults.minimum_sentence_words)
            .map(|s| s.glue_percent)
            .collect();
        percents.sort_by(f64::total_cmp);
        let at = |p: f64| percents[((percents.len() - 1) as f64 * p).round() as usize];
        let mut clause_words: Vec<_> = all.long_clauses.iter().map(|c| c.words).collect();
        clause_words.sort_unstable();
        let clause_at =
            |p: f64| clause_words[((clause_words.len() - 1) as f64 * p).round() as usize];
        let sticky: Vec<_> = MINIMUM_WORDS
            .iter()
            .flat_map(|&words| GLUE_PERCENT.iter().map(move |&percent| (words, percent)))
            .map(|(words, percent)| {
                let flags = all
                    .sticky_sentences
                    .iter()
                    .filter(|s| s.words >= words && s.glue_percent >= percent)
                    .count();
                json!({"minimumSentenceWords": words, "stickyGluePercent": percent, "flags": flags})
            })
            .collect();
        let long: Vec<_> = CLAUSE_WORDS
            .iter()
            .map(|&words| {
                let clauses: Vec<_> = all.long_clauses.iter().filter(|c| c.words >= words).collect();
                let sentences: BTreeSet<_> = clauses.iter().map(|c| c.sentence).collect();
                json!({"longClauseWords": words, "flags": clauses.len(), "sentences": sentences.len()})
            })
            .collect();
        // Overlap with sentence length's long-sentence flag at its default of 40 words.
        let report = clarity::report(&document, &defaults).unwrap();
        let defaults_row = json!({
            "sticky": report.sticky_sentences.len(),
            "stickyInLongSentences": report.sticky_sentences.iter().filter(|s| s.words >= 40).count(),
            "longClauses": report.long_clauses.len(),
            "longClausesInLongSentences": report.long_clauses.iter().filter(|c| c.sentence_words >= 40).count(),
        });
        let mut flagged = Vec::new();
        if review {
            for s in &report.sticky_sentences {
                let mut marked = String::new();
                let mut at = s.span.byte.start;
                for glue in &s.glue {
                    marked += &text[at..glue.byte.start];
                    marked += &format!("[{}]", &text[glue.byte.clone()]);
                    at = glue.byte.end;
                }
                marked += &text[at..s.span.byte.end];
                flagged.push(format!("sticky {:.0}%: {marked}", s.glue_percent));
            }
            for c in &report.long_clauses {
                flagged.push(format!(
                    "clause {}: {}",
                    c.words,
                    &text[c.span.byte.clone()]
                ));
            }
        }
        let group = &all.groups.all;
        println!(
            "{}",
            json!({
                "name": row["name"],
                "words": all.words.all,
                "sentences": group.sentences,
                "eligibleSentences": percents.len(),
                "gluePercent": group.glue.count as f64 * 100.0 / all.words.all as f64,
                "eligibleGluePercentiles": {"p50": at(0.5), "p90": at(0.9), "p95": at(0.95), "p99": at(0.99)},
                "subordinateClauses": group.subordinate_clauses,
                "clauseWordPercentiles": {"p50": clause_at(0.5), "p90": clause_at(0.9), "p99": clause_at(0.99)},
                "sticky": sticky,
                "longClauses": long,
                "defaults": defaults_row,
                "review": flagged,
            })
        );
    }
}
