use std::{sync::Arc, time::Instant};

use writemorebetter_style::{reports::StyleResources, DocumentOptions, StyleDocument};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "usage: report_timing TOKENIZER TEXT");
    let tokenizer = Arc::new(nlprule::Tokenizer::new(&args[0]).unwrap());
    let resources = Arc::new(StyleResources::new());
    let text = std::fs::read_to_string(&args[1]).unwrap();
    let options = DocumentOptions::new().with_hard_wrapped(true);
    let names = [
        "analysis",
        "echoes",
        "repeatedPhrases",
        "cliches",
        "dialogueTags",
        "sentenceLength",
        "sentenceStarts",
    ];
    let mut times = vec![Vec::new(); names.len()];
    for iteration in 0..4 {
        let start = Instant::now();
        let document = StyleDocument::new(tokenizer.clone(), resources.clone(), &text, &options);
        if iteration > 0 {
            times[0].push(start.elapsed().as_secs_f64() * 1000.0);
        }
        let calls: [Box<dyn Fn() -> String>; 6] = [
            Box::new(|| document.echoes(None).unwrap()),
            Box::new(|| document.repeated_phrases(None).unwrap()),
            Box::new(|| document.cliches(None).unwrap()),
            Box::new(|| document.dialogue_tags(None, None).unwrap()),
            Box::new(|| document.sentence_length(None, None).unwrap()),
            Box::new(|| document.sentence_starts(None, None).unwrap()),
        ];
        for (index, call) in calls.iter().enumerate() {
            let start = Instant::now();
            std::hint::black_box(call());
            if iteration > 0 {
                times[index + 1].push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
    }
    let medians: std::collections::BTreeMap<_, _> = names
        .into_iter()
        .zip(times.iter_mut().map(|values| {
            values.sort_by(f64::total_cmp);
            values[1]
        }))
        .collect();
    println!(
        "{}",
        serde_json::json!({"inputBytes": text.len(), "warmups": 1, "iterations": 3, "hardWrapped": true, "medianMs": medians})
    );
}
