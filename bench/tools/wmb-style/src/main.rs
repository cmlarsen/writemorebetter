use serde::Deserialize;
use serde_json::{json, Value};
use std::{error::Error, io, time::Instant};

#[derive(Deserialize)]
struct Input {
    cases: Vec<Case>,
    runs: usize,
    details: bool,
    #[serde(default)]
    reports: bool,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    paragraphs: Vec<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let input: Input = serde_json::from_reader(io::stdin())?;
    if input.runs == 0 {
        return Err("runs must be positive".into());
    }
    let start = Instant::now();
    let tokenizer =
        nlprule::Tokenizer::new(std::env::args().nth(1).ok_or("missing tokenizer path")?)?;
    let load = start.elapsed().as_secs_f64();
    let mut cases = Vec::new();
    for case in input.cases {
        let paragraphs: Vec<_> = case.paragraphs.iter().map(String::as_str).collect();
        let mut elapsed = Vec::new();
        let mut result = Value::Null;
        for run in 0..input.runs {
            let start = Instant::now();
            let document = writemorebetter_style::analyze_document(&tokenizer, &paragraphs);
            elapsed.push(start.elapsed().as_secs_f64());
            if run != 0 {
                continue;
            }
            let sentences: Vec<_> = document
                .paragraphs
                .iter()
                .flat_map(|p| &p.sentences)
                .collect();
            let tokens: Vec<_> = sentences.iter().flat_map(|s| &s.tokens).collect();
            result = json!({"id": case.id, "sentence_count": sentences.len(), "token_count": tokens.len(), "quote_count": document.dialogue_spans.len()});
            if input.reports {
                use writemorebetter_style::reports::sentences::{
                    report, LengthReport, Options, StartReport,
                };
                let reports = report(&document, &Options::default())?;
                let counts = |lengths: LengthReport, starts: StartReport| {
                    json!({
                        "sentences": lengths.sentences,
                        "lengths": lengths.histogram.into_iter().map(|(key,value)| (key,value.count)).collect::<std::collections::BTreeMap<_,_>>(),
                        "starts": starts.categories.into_iter().map(|(key,value)| (key,value.distribution.count)).collect::<std::collections::BTreeMap<_,_>>(),
                    })
                };
                result["report_counts"] = json!({"narration": counts(reports.lengths.narration,reports.starts.narration),
                    "dialogue": counts(reports.lengths.dialogue,reports.starts.dialogue)});
            }
            if input.details {
                result["sentences"] = json!(sentences
                    .iter()
                    .map(|s| {
                        let leading = s.text.len() - s.text.trim_start().len();
                        json!([
                            s.span.byte.start + leading,
                            s.span.byte.start + s.text.trim_end().len()
                        ])
                    })
                    .collect::<Vec<_>>());
                result["tokens"] = json!(tokens.iter().filter(|t| !t.is_whitespace).map(|t|
                    json!({"span": [t.span.byte.start, t.span.byte.end], "readings": t.readings})
                ).collect::<Vec<_>>());
                result["quotes"] = json!(document
                    .dialogue_spans
                    .iter()
                    .map(|s| [s.byte.start, s.byte.end])
                    .collect::<Vec<_>>());
            }
        }
        result["analysis_seconds"] = json!(elapsed);
        cases.push(result);
    }
    serde_json::to_writer(
        io::stdout(),
        &json!({"tool": "writemorebetter-document", "version": "0.8.0", "load_seconds": load, "cases": cases}),
    )?;
    Ok(())
}
