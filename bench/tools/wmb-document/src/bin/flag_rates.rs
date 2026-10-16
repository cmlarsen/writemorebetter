use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::time::Instant;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use writemorebetter_style::reports::cliches::{cliches, PhraseContext, PhraseList};
use writemorebetter_style::reports::dialogue_tags::{
    dialogue_tags, non_speech_verbs, speech_verbs, DialogueTagOptions, VerbClass,
};
use writemorebetter_style::reports::echoes::{echoes, EchoOptions};
use writemorebetter_style::reports::phrases::{repeated_phrases, PhraseOptions};
use writemorebetter_style::reports::{sentence_length, sentence_starts};
use writemorebetter_style::{analyze_document, DocumentOptions};

const ECHO_BANDS: [(usize, &str); 4] = [(5, "1_5"), (10, "6_10"), (25, "11_25"), (50, "26_50")];

fn name<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn baseline<T: DeserializeOwned>(path: &str) -> Result<T, Box<dyn std::error::Error>> {
    let mut file: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    Ok(serde_json::from_value(file["baseline"].take())?)
}

fn fold(text: &str) -> String {
    text.replace('’', "'").to_lowercase()
}

fn length_group(group: &sentence_length::Group) -> Value {
    json!({
        "histogram": group.histogram,
        "long_sentences": group.long_sentences.len(),
        "monotony_runs": group.monotony_runs.len(),
    })
}

fn length_unit(group: &sentence_length::Group) -> Value {
    json!({
        "mean": group.mean,
        "percentiles": group.percentiles,
        "mean_difference": group.mean_difference,
        "percentile_differences": group.percentile_differences,
    })
}

fn starts_group(group: &sentence_starts::Group) -> Value {
    let categories: BTreeMap<String, f64> = group
        .categories
        .iter()
        .map(|(category, count)| (name(category), count.count))
        .collect();
    let universal: BTreeMap<&str, f64> = group
        .universal
        .iter()
        .map(|(tag, count)| (tag.as_str(), count.count))
        .collect();
    json!({
        "sentences": group.sentence_count,
        "unknown_starts": group.unknown_starts,
        "ambiguous_starts": group.ambiguous_starts,
        "categories": categories,
        "universal": universal,
        "same_word_runs": group.same_word_runs.len(),
    })
}

fn starts_unit(group: &sentence_starts::Group) -> Value {
    let universal: BTreeMap<&str, Value> = group
        .universal
        .iter()
        .map(|(tag, count)| {
            (
                tag.as_str(),
                json!({"percent": count.percent, "difference": count.difference_percentage_points}),
            )
        })
        .collect();
    json!({ "universal": universal })
}

fn vocabulary(tokenizer: &nlprule::Tokenizer) -> Value {
    let phrases = PhraseList::bundled(tokenizer);
    let mut entries: Vec<&str> = phrases
        .entries()
        .iter()
        .map(|e| e.phrase.as_str())
        .collect();
    entries.sort_unstable();
    entries.dedup();
    let mut sources: Vec<&str> = phrases
        .entries()
        .iter()
        .map(|e| e.source.as_str())
        .collect();
    sources.sort_unstable();
    sources.dedup();
    let length = sentence_length::Options::new();
    let phrase_options = PhraseOptions::new();
    let tag_options = DialogueTagOptions::new();
    json!({
        "defaults": {
            "sentence_length_minimum_sentences": length.minimum_sentences,
            "long_sentence_words": length.long_sentence_words,
            "sentence_starts_minimum_sentences": sentence_starts::Options::new().minimum_sentences,
            "echo_window": EchoOptions::new().window,
            "phrase_min_length": phrase_options.min_length,
            "phrase_max_length": phrase_options.max_length,
            "phrase_min_content_words": phrase_options.min_content_words,
            "max_untagged_exchanges": tag_options.max_untagged_exchanges,
            "dialogue_tag_min_tags": tag_options.min_tags,
        },
        "cliche_entries": entries,
        "cliche_sources": sources,
        "speech_verbs": speech_verbs().collect::<Vec<_>>(),
        "non_speech_verbs": non_speech_verbs().collect::<Vec<_>>(),
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    let (tokenizer, length_baseline, starts_baseline) = match args.as_slice() {
        [flag, tokenizer] if flag == "--vocabulary" => {
            let tokenizer = nlprule::Tokenizer::new(tokenizer)?;
            serde_json::to_writer(&mut output, &vocabulary(&tokenizer))?;
            writeln!(output)?;
            return Ok(());
        }
        [tokenizer, length, starts] => (
            nlprule::Tokenizer::new(tokenizer)?,
            baseline::<sentence_length::Baseline>(length)?,
            baseline::<sentence_starts::Baseline>(starts)?,
        ),
        _ => {
            return Err(
                "usage: flag_rates TOKENIZER LENGTH_BASELINE STARTS_BASELINE < texts.jsonl | flag_rates --vocabulary TOKENIZER"
                    .into(),
            )
        }
    };
    let phrase_list = PhraseList::bundled(&tokenizer);
    let speech: Vec<&str> = speech_verbs().collect();
    let non_speech: Vec<&str> = non_speech_verbs().collect();
    for line in io::stdin().lock().lines() {
        let text: String = serde_json::from_str(&line?)?;
        let mut timing = BTreeMap::new();
        let mut started = Instant::now();
        let mut lap = |stage: &'static str, timing: &mut BTreeMap<&'static str, f64>| {
            timing.insert(stage, started.elapsed().as_secs_f64() * 1000.0);
            started = Instant::now();
        };
        let document = analyze_document(&tokenizer, &text, &DocumentOptions::new());
        lap("document", &mut timing);
        let length = sentence_length::report(
            &document,
            &sentence_length::Options::new(),
            Some(&length_baseline),
        );
        lap("sentence_length", &mut timing);
        let starts = sentence_starts::report(
            &document,
            &sentence_starts::Options::new(),
            Some(&starts_baseline),
        );
        lap("sentence_starts", &mut timing);
        let echo_report = echoes(&document, &EchoOptions::new());
        lap("echoes", &mut timing);
        let phrase_report = repeated_phrases(&document, &PhraseOptions::new());
        lap("repeated_phrases", &mut timing);
        let cliche_report = cliches(&document, &phrase_list);
        lap("cliches", &mut timing);
        let tag_report = dialogue_tags(&document, &DialogueTagOptions::new());
        lap("dialogue_tags", &mut timing);

        let mut bands: BTreeMap<&str, usize> = ECHO_BANDS.iter().map(|(_, n)| (*n, 0)).collect();
        let mut forms: BTreeMap<&str, usize> =
            BTreeMap::from([("same_form", 0), ("different_form", 0)]);
        for echo in &echo_report.echoes {
            let band = ECHO_BANDS
                .iter()
                .find(|(limit, _)| echo.distance <= *limit)
                .map_or("over_50", |(_, band)| band);
            *bands.entry(band).or_default() += 1;
            let earlier = text.get(echo.earlier.byte.clone()).map(fold);
            let later = text.get(echo.later.byte.clone()).map(fold);
            *forms
                .entry(if earlier == later {
                    "same_form"
                } else {
                    "different_form"
                })
                .or_default() += 1;
        }

        let mut lengths: BTreeMap<String, Value> = BTreeMap::new();
        for phrase in &phrase_report.phrases {
            let entry = lengths
                .entry(phrase.keys.len().to_string())
                .or_insert_with(|| json!({"phrases": 0, "occurrences": 0}));
            entry["phrases"] = json!(entry["phrases"].as_u64().unwrap_or(0) + 1);
            entry["occurrences"] =
                json!(entry["occurrences"].as_u64().unwrap_or(0) + phrase.count as u64);
        }

        let mut kinds: BTreeMap<String, BTreeMap<&str, usize>> = BTreeMap::new();
        let mut sources: BTreeMap<String, BTreeMap<&str, usize>> = BTreeMap::new();
        let mut entries: BTreeMap<String, BTreeMap<&str, usize>> = BTreeMap::new();
        for found in &cliche_report.matches {
            let context = match found.context {
                PhraseContext::Dialogue => "dialogue",
                _ => "narration",
            };
            for (map, key) in [
                (&mut kinds, name(found.kind)),
                (&mut sources, found.source.clone()),
                (&mut entries, found.phrase.clone()),
            ] {
                *map.entry(key).or_default().entry(context).or_default() += 1;
            }
        }

        let mut verbs: BTreeMap<&str, usize> = BTreeMap::new();
        let (mut other_speech, mut unlisted) = (0, 0);
        for tag in &tag_report.tags {
            match tag.class {
                VerbClass::Speech => {
                    if !matches!(tag.lemma.as_str(), "say" | "ask") {
                        other_speech += 1;
                    }
                    if let Some(verb) = speech.iter().find(|verb| **verb == tag.lemma) {
                        *verbs.entry(verb).or_default() += 1;
                    }
                }
                VerbClass::NonSpeech => {
                    if let Some(verb) = non_speech.iter().find(|verb| **verb == tag.lemma) {
                        *verbs.entry(verb).or_default() += 1;
                    }
                }
                _ => unlisted += 1,
            }
        }

        serde_json::to_writer(
            &mut output,
            &json!({
                "words": document.word_count,
                "timing_ms": timing,
                "sentence_length": {
                    "all": length_group(&length.all),
                    "narration": length_group(&length.narration),
                    "dialogue": length_group(&length.dialogue),
                },
                "sentence_starts": {
                    "all": starts_group(&starts.all),
                    "narration": starts_group(&starts.narration),
                    "dialogue": starts_group(&starts.dialogue),
                },
                "echoes": {
                    "echoes": echo_report.echoes.len(),
                    "distance": bands,
                    "form": forms,
                },
                "repeated_phrases": {
                    "phrases": phrase_report.phrases.len(),
                    "occurrences": phrase_report.phrases.iter().map(|p| p.count).sum::<usize>(),
                    "length": lengths,
                },
                "cliches": {
                    "matches": cliche_report.matches.len(),
                    "narration_words": cliche_report.narration.words,
                    "dialogue_words": cliche_report.dialogue.words,
                    "kind": kinds,
                    "source": sources,
                    "entry": entries,
                },
                "dialogue_tags": {
                    "tags": tag_report.tags.len(),
                    "said_asked": tag_report.said_asked,
                    "other_speech_verb": other_speech,
                    "non_speech_verb": tag_report.non_speech,
                    "manner_adverb": tag_report.with_adverb,
                    "unlisted_verb": unlisted,
                    "untagged_run": tag_report.untagged_runs.as_ref().map_or(0, Vec::len),
                    "beats": tag_report.beats.len(),
                    "speech_spans": tag_report.speech_spans,
                    "exchanges": tag_report.exchanges,
                    "verb": verbs,
                },
                "unit_report": {
                    "sentence_length": {
                        "all": length_unit(&length.all),
                        "narration": length_unit(&length.narration),
                        "dialogue": length_unit(&length.dialogue),
                    },
                    "sentence_starts": {
                        "all": starts_unit(&starts.all),
                        "narration": starts_unit(&starts.narration),
                        "dialogue": starts_unit(&starts.dialogue),
                    },
                },
            }),
        )?;
        writeln!(output)?;
        output.flush()?;
    }
    Ok(())
}
