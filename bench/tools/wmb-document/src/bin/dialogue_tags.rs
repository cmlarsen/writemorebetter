use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::ops::Range;

use serde::Deserialize;
use serde_json::{json, Value};
use writemorebetter_style::reports::dialogue_tags::{
    dialogue_tags, DialogueTagOptions, TagPosition, VerbClass,
};
use writemorebetter_style::{analyze_document, DocumentOptions, Span};

#[derive(Deserialize)]
struct Input {
    file: String,
    text: String,
}

fn record(
    file: &str,
    report: &str,
    pattern: &str,
    byte: Range<usize>,
    utf16: Range<usize>,
    text: &str,
) -> Value {
    json!({
        "file": file,
        "report": report,
        "pattern": pattern,
        "byte": {"start": byte.start, "end": byte.end},
        "utf16": {"start": utf16.start, "end": utf16.end},
        "text": text.get(byte).unwrap_or_default(),
    })
}

fn span_record(file: &str, report: &str, pattern: &str, span: &Span, text: &str) -> Value {
    record(
        file,
        report,
        pattern,
        span.byte.clone(),
        span.utf16.clone(),
        text,
    )
}

fn first_word(file: &str, report: &str, pattern: &str, span: &Span, text: &str) -> Value {
    let surface = text.get(span.byte.clone()).unwrap_or_default();
    let length = surface
        .char_indices()
        .find(|(_, ch)| !ch.is_alphanumeric() && !matches!(ch, '\'' | '’'))
        .map_or(surface.len(), |(index, _)| index);
    let units = surface
        .get(..length)
        .unwrap_or_default()
        .encode_utf16()
        .count();
    record(
        file,
        report,
        pattern,
        span.byte.start..span.byte.start + length,
        span.utf16.start..span.utf16.start + units,
        text,
    )
}

fn name<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (tokenizer, counts) = match args.as_slice() {
        [tokenizer] => (tokenizer, false),
        [tokenizer, flag] if flag == "--counts" => (tokenizer, true),
        _ => return Err("usage: dialogue_tags TOKENIZER [--counts] < documents.jsonl".into()),
    };
    let tokenizer = nlprule::Tokenizer::new(tokenizer)?;
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    for line in io::stdin().lock().lines() {
        let input: Input = serde_json::from_str(&line?)?;
        let document = analyze_document(&tokenizer, &input.text, &DocumentOptions::new());
        let report = dialogue_tags(&document, &DialogueTagOptions::new());
        if counts {
            let mut verbs: BTreeMap<String, usize> = BTreeMap::new();
            let mut positions: BTreeMap<String, usize> = BTreeMap::new();
            let mut unlisted = 0;
            for tag in &report.tags {
                *positions.entry(name(tag.position)).or_default() += 1;
                if tag.class == VerbClass::Unlisted {
                    unlisted += 1;
                } else {
                    *verbs.entry(tag.lemma.clone()).or_default() += 1;
                }
            }
            serde_json::to_writer(
                &mut output,
                &json!({
                    "file": input.file,
                    "words": document.word_count,
                    "dialogue_spans": report.dialogue_spans,
                    "speech_spans": report.speech_spans,
                    "tagged_spans": report.tagged_spans,
                    "exchanges": report.exchanges,
                    "tags": report.tags.len(),
                    "said_asked": report.said_asked,
                    "with_adverb": report.with_adverb,
                    "non_speech": report.non_speech,
                    "unlisted": unlisted,
                    "beats": report.beats.len(),
                    "untagged_runs": report.untagged_runs.as_ref().map_or(0, Vec::len),
                    "untagged_run_exchanges": report.untagged_runs.iter().flatten().map(|run| run.exchanges).sum::<usize>(),
                    "positions": positions,
                    "verbs": verbs,
                }),
            )?;
            writeln!(output)?;
            output.flush()?;
            continue;
        }
        let file = input.file.as_str();
        let text = input.text.as_str();
        let mut rows = Vec::new();
        for span in &document.dialogue {
            rows.push(span_record(file, "dialogue_span", "dialogue", span, text));
        }
        for tag in &report.tags {
            let position = match tag.position {
                TagPosition::Before => "before",
                TagPosition::After => "after",
                _ => "split",
            };
            rows.push(span_record(
                file,
                "dialogue_tag_position",
                position,
                &tag.verb,
                text,
            ));
            rows.push(span_record(
                file,
                "dialogue_tag_verb",
                &name(tag.class),
                &tag.verb,
                text,
            ));
            if let Some(adverb) = &tag.adverb {
                rows.push(span_record(
                    file,
                    "dialogue_tag_adverb",
                    "adverb",
                    adverb,
                    text,
                ));
            }
            rows.push(first_word(file, "dialogue_beat", "tag", &tag.clause, text));
        }
        for beat in &report.beats {
            rows.push(first_word(file, "dialogue_beat", "beat", &beat.span, text));
        }
        for run in report.untagged_runs.iter().flatten() {
            rows.push(span_record(
                file,
                "dialogue_untagged_run",
                "untagged_run",
                &run.span,
                text,
            ));
        }
        for row in rows {
            serde_json::to_writer(&mut output, &row)?;
            writeln!(output)?;
        }
    }
    output.flush()?;
    Ok(())
}
