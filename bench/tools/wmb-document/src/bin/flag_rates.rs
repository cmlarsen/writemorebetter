use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::time::Instant;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use writemorebetter_style::reports::cliches::{
    report as cliches, ClicheList as PhraseList, ClicheOptions,
};
use writemorebetter_style::reports::dialogue_tags::{
    non_speech_verbs, report as dialogue_tags, speech_verbs, DialogueTagOptions,
    DialogueTagVerbClass as VerbClass,
};
use writemorebetter_style::reports::echoes::{report as echoes, EchoOptions};
use writemorebetter_style::reports::repeated_phrases::{
    report as repeated_phrases, RepeatedPhraseOptions as PhraseOptions,
};
use writemorebetter_style::reports::{
    diction, overused_words, pronoun_starts, sensory_balance, sentence_length, sentence_starts,
    transitions, Split,
};
use writemorebetter_style::{analyze_document, DocumentOptions, Mode};

const ECHO_BANDS: [(usize, &str); 4] = [(5, "1_5"), (10, "6_10"), (25, "11_25"), (50, "26_50")];
const PRONOUNS: [&str; 7] = ["i", "you", "he", "she", "it", "we", "they"];
const OVERUSED_RATIOS: [f64; 5] = [3.0, 4.0, 5.0, 6.0, 8.0];
const REPEAT_WINDOWS: [usize; 4] = [2, 3, 4, 5];

fn split<T>(groups: &Split<T>, value: impl Fn(&T) -> Value) -> Value {
    json!({
        "all": value(&groups.all),
        "narration": value(&groups.narration),
        "dialogue": value(&groups.dialogue),
    })
}

fn by_mode(modes: impl Iterator<Item = Mode>) -> Value {
    let (mut narration, mut dialogue) = (0, 0);
    for mode in modes {
        match mode {
            Mode::Dialogue => dialogue += 1,
            _ => narration += 1,
        }
    }
    json!({"all": narration + dialogue, "narration": narration, "dialogue": dialogue})
}

fn transitions_group(group: &transitions::TransitionsGroup) -> Value {
    let types: BTreeMap<String, Value> = group
        .types
        .iter()
        .map(|share| {
            (
                name(share.kind),
                json!({"containing": share.containing.count, "opening": share.opening.count}),
            )
        })
        .collect();
    json!({
        "sentences": group.sentences,
        "containing": group.containing.count,
        "opening": group.opening.count,
        "types": types,
    })
}

fn pronoun_group(group: &pronoun_starts::PronounStartsGroup) -> Value {
    let mut pronouns: BTreeMap<&str, usize> = PRONOUNS.iter().map(|p| (*p, 0)).collect();
    let mut other = 0;
    for share in &group.pronouns {
        match pronouns.get_mut(share.pronoun.as_str()) {
            Some(count) => *count += share.share.count,
            None => other += share.share.count,
        }
    }
    json!({
        "sentences": group.sentences - group.excluded_sentences,
        "pronoun_starts": group.pronoun_starts.count,
        "pronouns": pronouns,
        "other": other,
        "runs": group.runs.len(),
    })
}

fn sensory_group(group: &sensory_balance::SensoryGroup) -> Value {
    let modalities: BTreeMap<String, usize> = group
        .modalities
        .iter()
        .map(|share| (name(share.modality), share.count))
        .collect();
    json!({"words": group.words, "sense_words": group.sense_words, "modalities": modalities})
}

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

fn length_group(group: &sentence_length::SentenceLengthGroup) -> Value {
    let histogram: BTreeMap<usize, usize> = group
        .histogram
        .iter()
        .map(|bucket| (bucket.words, bucket.sentences))
        .collect();
    json!({
        "histogram": histogram,
        "long_sentences": group.long_sentences.len(),
        "monotony_runs": group.monotony_runs.len(),
    })
}

fn length_unit(group: &sentence_length::SentenceLengthGroup) -> Value {
    let percentiles: BTreeMap<u8, Option<f64>> = group
        .percentiles
        .iter()
        .map(|p| (p.percentile, p.value))
        .collect();
    let differences: BTreeMap<u8, Option<f64>> = group
        .percentiles
        .iter()
        .map(|p| (p.percentile, p.difference))
        .collect();
    json!({
        "mean": group.mean,
        "percentiles": percentiles,
        "mean_difference": group.mean_difference,
        "percentile_differences": differences,
    })
}

fn starts_group(group: &sentence_starts::SentenceStartsGroup) -> Value {
    let categories: BTreeMap<String, f64> = group
        .categories
        .iter()
        .map(|share| (name(share.category), share.share.count as f64))
        .collect();
    let universal: BTreeMap<&str, f64> = group
        .universal
        .iter()
        .map(|share| (share.tag.as_str(), share.weight))
        .collect();
    json!({
        "sentences": group.sentences - group.excluded_sentences,
        "unknown_starts": group.unknown_starts,
        "ambiguous_starts": group.ambiguous_starts,
        "categories": categories,
        "universal": universal,
        "same_word_runs": group.same_word_runs.len(),
    })
}

fn starts_unit(group: &sentence_starts::SentenceStartsGroup) -> Value {
    let universal: BTreeMap<&str, Value> = group
        .universal
        .iter()
        .map(|share| {
            (
                share.tag.as_str(),
                json!({"percent": share.percent, "difference": share.difference}),
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
    let length = sentence_length::SentenceLengthOptions::new().with_long_sentences(true);
    let phrase_options = PhraseOptions::new();
    let tag_options = DialogueTagOptions::new();
    json!({
        "defaults": {
            "sentence_length_minimum_sentences": length.minimum_sentences,
            "long_sentence_words": length.long_sentence_words,
            "sentence_starts_minimum_sentences": sentence_starts::SentenceStartsOptions::new().minimum_sentences,
            "echo_window": EchoOptions::new().window,
            "phrase_min_length": phrase_options.minimum_length,
            "phrase_max_length": phrase_options.maximum_length,
            "phrase_min_content_words": phrase_options.minimum_content_words,
            "max_untagged_exchanges": tag_options.max_untagged_exchanges,
            "dialogue_tag_min_tags": tag_options.minimum_tags,
            "transitions_minimum_sentences": transitions::TransitionsOptions::new().minimum_sentences,
            "transitions_repeat_window": transitions::TransitionsOptions::new().repeat_window,
            "pronoun_starts_minimum_sentences": pronoun_starts::PronounStartsOptions::new().minimum_sentences,
            "sensory_minimum_sense_words": sensory_balance::SensoryBalanceOptions::new().minimum_sense_words,
            "overused_minimum_ratio": overused_words::OverusedWordsOptions::new().minimum_ratio,
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
        [tokenizer, length, starts, _table] => (
            nlprule::Tokenizer::new(tokenizer)?,
            baseline::<sentence_length::SentenceLengthBaseline>(length)?,
            baseline::<sentence_starts::SentenceStartsBaseline>(starts)?,
        ),
        _ => {
            return Err(
                "usage: flag_rates TOKENIZER LENGTH_BASELINE STARTS_BASELINE DICTION_TABLE < texts.jsonl | flag_rates --vocabulary TOKENIZER"
                    .into(),
            )
        }
    };
    let diction_table = match args[3].as_str() {
        "-" => None,
        path => Some(diction::ConcretenessTable::parse(
            &std::fs::read_to_string(path)?,
        )?),
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
            &sentence_length::SentenceLengthOptions::new().with_long_sentences(true),
            Some(&length_baseline),
        )?;
        lap("sentence_length", &mut timing);
        let starts = sentence_starts::report(
            &document,
            &sentence_starts::SentenceStartsOptions::new(),
            Some(&starts_baseline),
        )?;
        lap("sentence_starts", &mut timing);
        let echo_report = echoes(&document, &EchoOptions::new())?;
        lap("echoes", &mut timing);
        let phrase_report = repeated_phrases(&document, &PhraseOptions::new())?;
        lap("repeated_phrases", &mut timing);
        let cliche_report = cliches(&document, &phrase_list, &ClicheOptions::new())?;
        lap("cliches", &mut timing);
        let tag_report = dialogue_tags(&document, &DialogueTagOptions::new(), None)?;
        lap("dialogue_tags", &mut timing);
        let overused = overused_words::report(
            &document,
            &overused_words::OverusedWordsOptions::new(),
            overused_words::OverusedWordsBaseline::bundled(),
        )?;
        lap("overused_words", &mut timing);
        let sensory = sensory_balance::report(
            &document,
            sensory_balance::PerceptualNorms::bundled(),
            &sensory_balance::SensoryBalanceOptions::new(),
        )?;
        lap("sensory_balance", &mut timing);
        let diction = diction_table
            .as_ref()
            .map(|table| diction::report(&document, table, &diction::DictionOptions::new()))
            .transpose()?;
        lap("diction", &mut timing);
        let transition_report =
            transitions::report(&document, &transitions::TransitionsOptions::new())?;
        let repeat_sweep: BTreeMap<String, Value> = REPEAT_WINDOWS
            .iter()
            .map(|&window| {
                let options = transitions::TransitionsOptions::new().with_repeat_window(window);
                transitions::report(&document, &options).map(|report| {
                    (
                        window.to_string(),
                        by_mode(report.repeats.iter().map(|repeat| repeat.mode)),
                    )
                })
            })
            .collect::<Result<_, _>>()?;
        lap("transitions", &mut timing);
        let pronoun_report =
            pronoun_starts::report(&document, &pronoun_starts::PronounStartsOptions::new())?;
        lap("pronoun_starts", &mut timing);
        let ratio_sweep: BTreeMap<String, Value> = OVERUSED_RATIOS
            .iter()
            .map(|&ratio| {
                overused_words::report(
                    &document,
                    &overused_words::OverusedWordsOptions::new().with_minimum_ratio(ratio),
                    overused_words::OverusedWordsBaseline::bundled(),
                )
                .map(|report| (format!("{ratio}"), json!(report.overused.len())))
            })
            .collect::<Result<_, _>>()?;
        lap("overused_sweep", &mut timing);
        let modalities: BTreeMap<String, usize> = sensory
            .groups
            .all
            .modalities
            .iter()
            .map(|share| (name(share.modality), share.count))
            .collect();

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
            let context = match found.mode {
                Mode::Dialogue => "dialogue",
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

        let mut row = json!({
            "words": document.word_count,
            "timing_ms": timing,
            "overused_words": {
                "overused": overused.overused.len(),
                "occurrences": overused.overused.iter().map(|word| word.count).sum::<usize>(),
                "fillers": overused.fillers.iter().map(|word| word.count).sum::<usize>(),
            },
            "overused_ratio_sweep": ratio_sweep,
            "sensory_balance": {
                "sense_words": sensory.sense_words.len(),
                "modalities": modalities,
                "groups": split(&sensory.groups, sensory_group),
            },
            "diction": diction.map(|diction| json!({
                "abstract_sentences": diction.groups.all.abstract_sentences,
                "abstract_runs": diction.abstract_runs.len(),
                "rated_words": diction.groups.all.rated_words,
                "rating_sum": diction.groups.all.mean.unwrap_or(0.0) * diction.groups.all.rated_words as f64,
            })),
            "transitions": {
                "groups": split(&transition_report.groups, transitions_group),
                "repeats": by_mode(transition_report.repeats.iter().map(|repeat| repeat.mode)),
                "repeat_window": repeat_sweep,
            },
            "pronoun_starts": {
                "groups": split(&pronoun_report.groups, pronoun_group),
                "runs": by_mode(pronoun_report.groups.all.runs.iter().map(|run| run.mode)),
            },
            "sentence_length": {
                "all": length_group(&length.groups.all),
                "narration": length_group(&length.groups.narration),
                "dialogue": length_group(&length.groups.dialogue),
            },
            "sentence_starts": {
                "all": starts_group(&starts.groups.all),
                "narration": starts_group(&starts.groups.narration),
                "dialogue": starts_group(&starts.groups.dialogue),
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
                "narration_words": cliche_report.groups.narration.words,
                "dialogue_words": cliche_report.groups.dialogue.words,
                "kind": kinds,
                "source": sources,
                "entry": entries,
            },
            "dialogue_tags": {
                "tags": tag_report.tags.len(),
                "said_asked": tag_report.said_asked.count,
                "other_speech_verb": other_speech,
                "non_speech_verb": tag_report.non_speech.count,
                "manner_adverb": tag_report.with_adverb.count,
                "unlisted_verb": unlisted,
                "untagged_run": tag_report.untagged_runs.len(),
                "beats": tag_report.beats.len(),
                "speech_spans": tag_report.speech_spans,
                "exchanges": tag_report.exchanges,
                "verb": verbs,
            },
            "unit_report": {
                "sentence_length": {
                    "all": length_unit(&length.groups.all),
                    "narration": length_unit(&length.groups.narration),
                    "dialogue": length_unit(&length.groups.dialogue),
                },
                "sentence_starts": {
                    "all": starts_unit(&starts.groups.all),
                    "narration": starts_unit(&starts.groups.narration),
                    "dialogue": starts_unit(&starts.groups.dialogue),
                },
            },
        });
        if row["diction"].is_null() {
            row.as_object_mut().map(|row| row.remove("diction"));
        }
        serde_json::to_writer(&mut output, &row)?;
        writeln!(output)?;
        output.flush()?;
    }
    Ok(())
}
