//! The tense-shift probe (#355): one JSON document per line in, one line out with the report and
//! every sentence's vote, so the bench can sample flags and re-inflect voting heads.
//!
//! In:  {"text": "...", "hardWrapped": false, "documentOptions": {...}, "options": {...}}
//! Out: {"report": TenseShiftReport, "sentences": [{"scene", "span", "class", "tagOnly",
//!       "pastPerfectOnly", "specific", "gnomic", "backstory", "narration", "chains": [{"head",
//!       "end", "text", "readings", "subject", "subjectReadings", "class", "exemption"}]}]}, spans
//!       with byte, char and UTF-16 ranges.
use std::io::{self, BufRead, Write};

use serde::Deserialize;
use serde_json::json;
use writemorebetter_style::narrative::{scene_votes, scenes, ChainClass, SentenceTense};
use writemorebetter_style::reports::tense_shift::{self, TenseShiftOptions};
use writemorebetter_style::{analyze_document, DocumentOptions, Span};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    text: String,
    #[serde(default)]
    hard_wrapped: bool,
    #[serde(default)]
    document_options: Option<DocumentOptions>,
    #[serde(default)]
    options: TenseShiftOptions,
}

fn class_name(class: ChainClass) -> &'static str {
    match class {
        ChainClass::Past => "past",
        ChainClass::PastPerfect => "pastPerfect",
        ChainClass::Present => "present",
        ChainClass::Modal { .. } => "modal",
        ChainClass::Ambiguous => "ambiguous",
    }
}

fn sentence_class(class: SentenceTense) -> &'static str {
    match class {
        SentenceTense::Past => "past",
        SentenceTense::Present => "present",
        SentenceTense::Mixed => "mixed",
        SentenceTense::Unclassified => "unclassified",
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [path] = args.as_slice() else {
        return Err("usage: tense_shift TOKENIZER < documents.jsonl".into());
    };
    let tokenizer = nlprule::Tokenizer::new(path)?;
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let input: Input = serde_json::from_str(&line?)?;
        let options = input
            .document_options
            .unwrap_or_default()
            .with_hard_wrapped(input.hard_wrapped);
        let document = analyze_document(&tokenizer, &input.text, &options);
        let report = tense_shift::report(&document, &input.options)?;
        let all: Vec<_> = document.sentences().collect();
        let mut sentences = Vec::new();
        for scene in scenes(&document) {
            for (index, vote) in scene_votes(&document, &scene) {
                let sentence = all[index];
                let tokens: Vec<_> = sentence
                    .tokens
                    .iter()
                    .filter(|t| !t.is_whitespace)
                    .collect();
                let span = match (tokens.first(), tokens.last()) {
                    (Some(first), Some(last)) => Span::covering(&first.span, &last.span),
                    _ => sentence.span.clone(),
                };
                let chains: Vec<_> = vote
                    .chains
                    .iter()
                    .map(|chain| {
                        let head = &sentence.tokens[chain.head];
                        json!({
                            "head": head.span,
                            "end": sentence.tokens[chain.end - 1].span,
                            "text": head.text,
                            "readings": head.readings.iter()
                                .map(|r| [r.lemma.as_str(), r.pos.as_str()]).collect::<Vec<_>>(),
                            "subject": chain.subject.map(|s| &sentence.tokens[s].text),
                            "subjectReadings": chain.subject.map(|s| sentence.tokens[s].readings.iter()
                                .map(|r| r.pos.as_str()).collect::<Vec<_>>()),
                            "class": class_name(chain.class),
                            "exemption": chain.exemption,
                        })
                    })
                    .collect();
                sentences.push(json!({
                    "index": index,
                    "scene": scene.index,
                    "span": span,
                    "class": sentence_class(vote.class),
                    "tagOnly": vote.tag_only,
                    "pastPerfectOnly": vote.past_perfect_only,
                    "specific": vote.specific,
                    "gnomic": vote.gnomic,
                    "backstory": vote.backstory,
                    "narration": document.sentence_narration_words(sentence).count(),
                    "chains": chains,
                }));
            }
        }
        writeln!(
            stdout,
            "{}",
            json!({"report": report, "sentences": sentences})
        )?;
        stdout.flush()?;
    }
    Ok(())
}
