use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::pronunciation::{
    initial_sound, onset_exception, onset_exception_count, spelled_onset, CONSONANTS,
};
use writemorebetter_style::reports::alliteration::{self, AlliterationOptions, AlliterationReport};
use writemorebetter_style::reports::StyleResources;
use writemorebetter_style::{analyze_document, DocumentOptions, Mode, StyleDocument, StyleError};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

fn run(text: &str, options: &AlliterationOptions) -> AlliterationReport {
    let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
    alliteration::report(&document, options).unwrap()
}

fn words(text: &str) -> Vec<Vec<String>> {
    run(text, &AlliterationOptions::new())
        .runs
        .into_iter()
        .map(|run| run.words)
        .collect()
}

#[test]
fn table_corrects_the_spelling() {
    assert!(onset_exception_count() > 1000);
    for (word, sound) in [
        ("hour", "-"),
        ("honest", "-"),
        ("who", "hh"),
        ("sure", "sh"),
        ("chorus", "k"),
        ("chef", "sh"),
        ("get", "g"),
        ("the", "dh"),
    ] {
        assert_eq!(onset_exception(word), Some(sound), "{word}");
        assert_eq!(initial_sound(word), sound, "{word}");
    }
    for (word, sound) in [
        ("cat", "k"),
        ("kite", "k"),
        ("city", "s"),
        ("phone", "f"),
        ("knight", "n"),
        ("write", "r"),
        ("gem", "jh"),
        ("giggle", "g"),
        ("jam", "jh"),
        ("unit", "y"),
        ("under", "-"),
        ("unable", "-"),
        ("yellow", "y"),
        ("xylophone", "z"),
        ("shadow", "sh"),
        ("thin", "th"),
    ] {
        assert_eq!(spelled_onset(word), sound, "{word}");
        assert_eq!(initial_sound(word), sound, "{word}");
    }
    for word in ["Ahab’s", "Ahab's", "“Hour”", "hour-long", "well—", "…sure"] {
        assert!(
            CONSONANTS.contains(&initial_sound(word)) || initial_sound(word) == "-",
            "{word}"
        );
    }
    assert_eq!(initial_sound("Hour’s"), "-");
    assert_eq!(initial_sound("hour-long"), "-");
    assert_eq!(initial_sound("Sure-footed"), "sh");
}

#[test]
fn finds_runs_by_sound_not_letter() {
    assert_eq!(
        words("The cold, clear, cutting wind came."),
        vec![vec!["cold", "clear", "cutting"]]
    );
    // c, k and ch in chorus share one sound; ph and f share another.
    assert_eq!(
        words("Cats keep chorus lines."),
        vec![vec!["Cats", "keep", "chorus"]]
    );
    assert_eq!(
        words("Phantom fears filled him."),
        vec![vec!["Phantom", "fears", "filled"]]
    );
    // Same letter, different sounds: no run.
    assert!(words("Cheap chemical charts.").is_empty());
    assert!(words("Gentle girls get gems.").is_empty());
    // Silent letters: k in knot is n.
    assert_eq!(
        words("Nine knotted nets."),
        vec![vec!["Nine", "knotted", "nets"]]
    );
}

#[test]
fn function_words_and_repeats_do_not_count() {
    // `the`, `to`, `that`: unstressed and skipped, so the run continues across them.
    assert_eq!(
        words("Tom took the tram to the town that night."),
        vec![vec!["Tom", "took", "tram", "town"]]
    );
    // Only two different words.
    assert!(words("Dark, dark, dark days.").is_empty());
    assert_eq!(
        words("Dark, dark, deep days."),
        vec![vec!["Dark", "dark", "deep", "days"]]
    );
}

#[test]
fn gap_and_minimum_are_tunable() {
    let text = "Big red bold rusty bats.";
    assert!(words(text).is_empty());
    let loose = AlliterationOptions::new().with_maximum_gap(1);
    let found: Vec<_> = run(text, &loose)
        .runs
        .into_iter()
        .map(|run| run.words)
        .collect();
    assert_eq!(found, vec![vec!["Big", "bold", "bats"]]);
    let pairs = AlliterationOptions::new()
        .with_maximum_gap(0)
        .with_minimum_words(2);
    let found: Vec<_> = run("Silent seas. Bold bats.", &pairs)
        .runs
        .into_iter()
        .map(|run| (run.sound, run.words))
        .collect();
    assert_eq!(
        found,
        vec![
            ("s".to_owned(), vec!["Silent".to_owned(), "seas".to_owned()]),
            ("b".to_owned(), vec!["Bold".to_owned(), "bats".to_owned()]),
        ]
    );
    assert!(matches!(
        alliteration::report(
            &analyze_document(tokenizer(), "x", &DocumentOptions::new()),
            &AlliterationOptions::new().with_minimum_words(1)
        ),
        Err(StyleError::InvalidOptions { .. })
    ));
}

#[test]
fn runs_stay_inside_a_sentence() {
    assert!(words("Slow sun. Silver sea.").is_empty());
    assert!(words("Slow sun.\u{2028}Silver sea.").is_empty());
}

#[test]
fn narration_and_dialogue_are_separate() {
    let text = "“Please put the pail down,” Peter pleaded patiently.";
    let report = run(text, &AlliterationOptions::new());
    let found: Vec<_> = report
        .runs
        .iter()
        .map(|run| (run.mode, run.words.clone()))
        .collect();
    assert_eq!(
        found,
        vec![
            (
                Mode::Dialogue,
                vec!["Please".into(), "put".into(), "pail".into()]
            ),
            (
                Mode::Narration,
                vec!["Peter".into(), "pleaded".into(), "patiently".into()]
            ),
        ]
    );
    assert_eq!(report.groups.dialogue.runs, 1);
    assert_eq!(report.groups.narration.runs, 1);
    assert_eq!(report.groups.all.runs, 2);
    assert!(report.groups.all.per_thousand_words > 0.0);
    let narration = run(text, &AlliterationOptions::new().with_skip_dialogue(true));
    assert_eq!(narration.runs.len(), 1);
    assert_eq!(narration.groups.dialogue.words, 0);
    assert_eq!(narration.groups.all.words, narration.groups.narration.words);
}

#[test]
fn utf8_text_and_spans() {
    // Curly quotes and apostrophes, an em dash, an ellipsis and U+2028 around and inside runs.
    for text in [
        "She’s seen seven silver swans—so still… “Simply stunning.”",
        "She's seen seven silver swans\u{2028}so still...",
        "‘Seven silver swans,’ she said.",
    ] {
        let report = run(text, &AlliterationOptions::new());
        assert!(!report.runs.is_empty(), "{text}");
        for run in &report.runs {
            assert_eq!(run.sound, "s");
            assert_eq!(
                &text[run.span.byte.clone()].chars().next(),
                &run.words[0].chars().next()
            );
            for (span, word) in run.spans.iter().zip(&run.words) {
                assert_eq!(&text[span.byte.clone()], word);
                let utf16: Vec<u16> = text.encode_utf16().collect();
                assert_eq!(
                    String::from_utf16(&utf16[span.utf16.clone()]).unwrap(),
                    *word
                );
            }
        }
    }
    // A contraction of a function word is skipped; a possessive splits off its stem.
    assert_eq!(
        words("She’d seen Sam’s sister sing."),
        vec![vec!["seen", "Sam", "sister", "sing"]]
    );
}

#[test]
fn skipped_names_are_left_out() {
    let options = DocumentOptions::new().with_skip_names(vec!["Peter Piper".into()]);
    let document = analyze_document(tokenizer(), "Peter Piper picked pickled peppers.", &options);
    let report = alliteration::report(&document, &AlliterationOptions::new()).unwrap();
    assert_eq!(report.runs[0].words, vec!["picked", "pickled", "peppers"]);
}

#[test]
fn style_document_method_and_options() {
    let document = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        "Wild winds whipped the water.",
        &DocumentOptions::new().with_token_details(false),
    );
    let report = document.alliteration(None).unwrap();
    assert_eq!(report.runs.len(), 1);
    assert_eq!(report.runs[0].sound, "w");
    let none = document
        .alliteration(Some(
            r#"{"minimumWords":5,"maximumGap":0,"skipDialogue":true}"#,
        ))
        .unwrap();
    assert!(none.runs.is_empty());
    assert!(document
        .alliteration(Some(r#"{"minimum_words":3}"#))
        .is_ok());
    assert!(matches!(
        document.alliteration(Some(r#"{"window":3}"#)),
        Err(StyleError::InvalidOptions { .. })
    ));
    let json = writemorebetter_style::reports::to_utf16_json(&report);
    assert!(
        json.contains(r#""span":{"start":0,"end":28}"#)
            && json.contains(r#""spans":[{"start":0,"end":4},{"start":5,"end":10}"#),
        "{json}"
    );
}

/// Counts on `bench/public`, printed for docs/STYLE-ALLITERATION.md:
/// `cargo test -p writemorebetter-style --test alliteration -- --ignored --nocapture public_counts`
#[test]
#[ignore]
fn public_counts() {
    let dir = std::path::Path::new("../bench/public");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    files.sort();
    for options in [
        AlliterationOptions::new(),
        AlliterationOptions::new().with_maximum_gap(1),
        AlliterationOptions::new().with_minimum_words(4),
    ] {
        println!("{options:?}");
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let name = path.file_stem().unwrap().to_string_lossy();
            let hard_wrapped = name.starts_with("moby");
            let document = analyze_document(
                tokenizer(),
                &text,
                &DocumentOptions::new().with_hard_wrapped(hard_wrapped),
            );
            let mut times: Vec<_> = (0..21)
                .map(|_| {
                    let start = std::time::Instant::now();
                    alliteration::report(&document, &options).unwrap();
                    start.elapsed()
                })
                .collect();
            times.sort();
            let report = alliteration::report(&document, &options).unwrap();
            let g = &report.groups;
            println!("{name}\tmedian {:.2} ms", times[10].as_secs_f64() * 1000.0);
            println!(
                "{name}\twords {}/{}\truns {} ({:.2}/1k) narration {} ({:.2}/1k) dialogue {} ({:.2}/1k)",
                g.narration.words,
                g.dialogue.words,
                g.all.runs,
                g.all.per_thousand_words,
                g.narration.runs,
                g.narration.per_thousand_words,
                g.dialogue.runs,
                g.dialogue.per_thousand_words,
            );
            if std::env::var("SHOW").is_ok() {
                for run in &report.runs {
                    println!("  {:?} {} {:?}", run.mode, run.sound, run.words);
                }
            }
        }
    }
}
