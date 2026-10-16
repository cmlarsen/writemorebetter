use std::sync::OnceLock;

use nlprule::Tokenizer;
use writemorebetter_style::reports::overused_words::{
    canonical_spelling, report, OverusedWordsBaseline, OverusedWordsOptions, FILLER_WORDS,
    SPELLING_VARIANTS,
};
use writemorebetter_style::{analyze_document, Document, DocumentOptions, StyleError};

const TEXT: &str = "Mara glanced at the door. She glanced at the window—then glanced back. \
“Why?” he asked… She glanced down. She’s glanced up.\u{2028}The lamp lit the lamp room. \
Rose glanced at Mara’s lamp. She knew that it was just very late, and that lamp was dim.";

fn tokenizer() -> &'static Tokenizer {
    static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();
    TOKENIZER.get_or_init(|| Tokenizer::new("../storage/en_tokenizer.bin").unwrap())
}

fn document(text: &str, options: &DocumentOptions) -> Document {
    analyze_document(tokenizer(), text, options)
}

fn synthetic() -> OverusedWordsBaseline {
    OverusedWordsBaseline::new()
        .with_lemma("glance", 100.0)
        .with_lemma("lamp", 50.0)
        .with_lemma("know", 2000.0)
        .with_lemma("rise", 300.0)
        .with_lemma("rose", 30.0)
        .with_lemma("mara", 1.0)
        .with_filler("just", 2000.0)
        .with_filler("that", 5000.0)
}

fn slices<'a>(text: &'a str, spans: &[writemorebetter_style::Span]) -> Vec<&'a str> {
    spans.iter().map(|span| &text[span.byte.clone()]).collect()
}

#[test]
fn flags_follow_from_rates_against_the_reference_table() {
    let skip = DocumentOptions::new().with_skip_names(vec!["Mara".into()]);
    let d = document(TEXT, &skip);
    let options = OverusedWordsOptions::new()
        .with_minimum_ratio(4.0)
        .with_minimum_count(5)
        .with_minimum_reference_per_million(10.0);
    let r = report(&d, &options, Some(&synthetic())).unwrap();
    assert_eq!(r.words, d.word_count);
    assert_eq!(r.overused.len(), 1, "{:?}", r.overused);
    let glance = &r.overused[0];
    assert_eq!(glance.lemma, "glance");
    assert_eq!(glance.count, 6);
    assert_eq!(slices(TEXT, &glance.spans), ["glanced"; 6]);
    let rate = 6.0 * 10_000.0 / r.words as f64;
    assert!((glance.per_ten_thousand_words - rate).abs() < 1e-9);
    assert!((glance.reference_per_ten_thousand_words - 1.0).abs() < 1e-12);
    assert!((glance.ratio - rate).abs() < 1e-9);
}

#[test]
fn names_capitalized_words_and_nouns_are_excluded_unless_asked() {
    let skip = DocumentOptions::new().with_skip_names(vec!["Mara".into()]);
    let d = document(TEXT, &skip);
    let loose = OverusedWordsOptions::new()
        .with_minimum_ratio(1.0)
        .with_minimum_count(1)
        .with_minimum_reference_per_million(0.0);
    let lemmas = |options: &OverusedWordsOptions| -> Vec<(String, usize)> {
        report(&d, options, Some(&synthetic()))
            .unwrap()
            .overused
            .into_iter()
            .map(|w| (w.lemma, w.count))
            .collect()
    };
    assert_eq!(
        lemmas(&loose),
        [("glance".to_owned(), 6), ("know".to_owned(), 1)]
    );
    let nouns = lemmas(&loose.clone().with_include_nouns(true));
    assert!(nouns.contains(&("lamp".to_owned(), 4)), "{nouns:?}");
    assert!(nouns
        .iter()
        .all(|(lemma, _)| !["rose", "rise", "mara"].contains(&lemma.as_str())));
    assert_eq!(
        lemmas(&loose.clone().with_minimum_reference_per_million(101.0)),
        [("know".to_owned(), 1)]
    );

    let unskipped = document(TEXT, &DocumentOptions::new());
    let r = report(
        &unskipped,
        &loose.with_include_nouns(true),
        Some(&synthetic()),
    )
    .unwrap();
    assert!(r.overused.iter().all(|w| w.lemma != "mara"));
    assert!(r.words > d.word_count);
}

#[test]
fn filler_words_report_by_rate_with_and_without_a_table() {
    let d = document(TEXT, &DocumentOptions::new());
    let options = OverusedWordsOptions::new();
    let r = report(&d, &options, None).unwrap();
    assert!(r.overused.is_empty());
    let fillers: Vec<_> = r
        .fillers
        .iter()
        .map(|f| (f.word.as_str(), f.count, f.ratio))
        .collect();
    assert_eq!(
        fillers,
        [("just", 1, None), ("that", 1, None), ("very", 1, None)]
    );
    let that = r.fillers.iter().find(|f| f.word == "that").unwrap();
    assert_eq!(slices(TEXT, &that.spans), ["that"]);
    assert!(TEXT[that.spans[0].byte.end..].starts_with(" it was"));
    assert!((that.per_ten_thousand_words - 10_000.0 / r.words as f64).abs() < 1e-9);

    let r = report(&d, &options, Some(&synthetic())).unwrap();
    let just = r.fillers.iter().find(|f| f.word == "just").unwrap();
    assert_eq!(just.reference_per_ten_thousand_words, Some(20.0));
    assert!((just.ratio.unwrap() - just.per_ten_thousand_words / 20.0).abs() < 1e-9);
    let very = r.fillers.iter().find(|f| f.word == "very").unwrap();
    assert_eq!(
        (very.reference_per_ten_thousand_words, very.ratio),
        (None, None)
    );
}

#[test]
fn spans_stay_on_char_boundaries_in_typographic_text() {
    let text =
        "😀 “She’d glanced—glanced…” She glanced.\u{2028}‘Glanced,’ she said. Really, she glanced.";
    let d = document(text, &DocumentOptions::new());
    let options = OverusedWordsOptions::new()
        .with_minimum_ratio(1.0)
        .with_minimum_count(1);
    let r = report(&d, &options, Some(&synthetic())).unwrap();
    let glance = r.overused.iter().find(|w| w.lemma == "glance").unwrap();
    assert_eq!(
        slices(text, &glance.spans),
        ["glanced", "glanced", "glanced", "Glanced", "glanced"]
    );
    for span in &glance.spans {
        assert_eq!(
            text[..span.byte.start].encode_utf16().count(),
            span.utf16.start
        );
        assert_eq!(text[..span.byte.start].chars().count(), span.char.start);
    }
    let really = r.fillers.iter().find(|f| f.word == "really").unwrap();
    assert_eq!(slices(text, &really.spans), ["Really"]);
}

#[test]
fn inputs_are_validated_and_compact_documents_rejected() {
    let d = document(TEXT, &DocumentOptions::new());
    let bad = OverusedWordsOptions::new().with_minimum_ratio(0.0);
    assert!(matches!(
        report(&d, &bad, None),
        Err(StyleError::InvalidOptions { .. })
    ));
    for baseline in [
        OverusedWordsBaseline::new().with_lemma("glance", -1.0),
        OverusedWordsBaseline::new().with_lemma("glance", f64::NAN),
        OverusedWordsBaseline::new().with_filler("glance", 1.0),
        OverusedWordsBaseline::new().with_lemma("Glance", 10.0),
        OverusedWordsBaseline::new().with_lemma("o’clock", 10.0),
        OverusedWordsBaseline::new().with_lemma("grey", 10.0),
    ] {
        match report(&d, &OverusedWordsOptions::new(), Some(&baseline)) {
            Err(StyleError::InvalidBaseline { report, .. }) => assert_eq!(report, "overusedWords"),
            other => panic!("{other:?}"),
        }
    }
    assert!(serde_json::from_str::<OverusedWordsBaseline>(r#"{"lemma":{}}"#).is_err());
    assert!(serde_json::from_str::<OverusedWordsOptions>(r#"{"minimumCounts":1}"#).is_err());
    let compact = document(TEXT, &DocumentOptions::new().with_token_details(false));
    assert!(matches!(
        report(&compact, &OverusedWordsOptions::new(), Some(&synthetic())),
        Err(StyleError::NeedsTokenDetails { .. })
    ));
}

#[test]
fn bundled_table_covers_fillers_and_common_content_lemmas() {
    let bundled = OverusedWordsBaseline::bundled().unwrap();
    bundled.validate().unwrap();
    assert!(bundled.lemmas.len() > 10_000);
    for word in FILLER_WORDS {
        assert!(bundled.fillers[word] > 0.0, "{word}");
    }
    for lemma in ["glance", "nod", "smile", "shrug", "murmur", "look"] {
        assert!(bundled.lemmas[lemma] >= 10.0, "{lemma}");
    }
    for lemma in ["be", "have", "just", "very", "out", "like"] {
        assert!(!bundled.lemmas.contains_key(lemma), "{lemma}");
    }
    assert!(bundled
        .lemmas
        .iter()
        .all(|(lemma, rate)| *rate >= 1.0 && lemma.to_lowercase() == *lemma));
    let file: serde_json::Value =
        serde_json::from_str(include_str!("../data/baselines/overused-words.json")).unwrap();
    assert!(file["source"]["license"]
        .as_str()
        .unwrap()
        .starts_with("CC BY 3.0"));
}

fn loose() -> OverusedWordsOptions {
    OverusedWordsOptions::new()
        .with_minimum_ratio(0.001)
        .with_minimum_count(1)
        .with_minimum_reference_per_million(0.0)
}

fn flags(text: &str, options: &OverusedWordsOptions) -> Vec<(String, usize, f64)> {
    let d = document(text, &DocumentOptions::new());
    report(&d, options, OverusedWordsBaseline::bundled())
        .unwrap()
        .overused
        .into_iter()
        .map(|w| (w.lemma, w.count, w.reference_per_ten_thousand_words))
        .collect()
}

const CHAPTER: &str = "The rain kept on through the evening. Somebody had left a cup on the sill. \
The radio hummed a song nobody in the house could name. Outside, a dog barked twice and stopped. \
Her brother wrote letters he never sent, and their mother pretended not to know. ";

#[test]
fn prepositions_compare_against_their_preposition_uses() {
    let bundled = OverusedWordsBaseline::bundled().unwrap();
    for (lemma, adverb_and_adjective_only) in [
        ("above", 20.2),
        ("near", 72.2),
        ("inside", 188.0),
        ("outside", 152.0),
        ("past", 199.0),
        ("opposite", 54.6),
        ("aboard", 1.37),
    ] {
        let rate = bundled.rate(lemma).unwrap();
        assert!(rate > adverb_and_adjective_only * 1.2, "{lemma}: {rate}");
    }
    let mut text = CHAPTER.repeat(110);
    for place in ["door", "fire", "window", "stairs", "gate"] {
        text.push_str(&format!("She stood near the {place}. "));
    }
    let d = document(&text, &DocumentOptions::new());
    assert!(d.word_count > 4_500, "{}", d.word_count);
    let near = |options: &OverusedWordsOptions| {
        report(&d, options, Some(bundled))
            .unwrap()
            .overused
            .into_iter()
            .find(|w| w.lemma == "near")
    };
    assert_eq!(near(&OverusedWordsOptions::new()), None);
    let near = near(&loose()).unwrap();
    assert_eq!(near.count, 5);
    assert!(near.ratio < 4.0, "{}", near.ratio);
    assert!(near.per_ten_thousand_words / 0.722 > 4.0);
}

#[test]
fn british_and_american_spellings_share_one_reference_rate() {
    let windows = SPELLING_VARIANTS.windows(2);
    assert!(windows.clone().all(|pair| pair[0].0 < pair[1].0));
    let bundled = OverusedWordsBaseline::bundled().unwrap();
    for (british, american) in SPELLING_VARIANTS {
        assert_eq!(canonical_spelling(british), american);
        assert_eq!(canonical_spelling(american), american);
        assert!(!bundled.lemmas.contains_key(british), "{british}");
        assert_eq!(bundled.rate(british), bundled.rate(american), "{british}");
    }
    assert!(bundled.rate("gray").unwrap() > 120.0);
    let pair = |british: &str, american: &str| {
        let text = |word: &str| format!("The {word} cat sat. A {word} dog ran. ");
        let (b, a) = (
            flags(&text(british), &loose()),
            flags(&text(american), &loose()),
        );
        let find = |found: &[(String, usize, f64)], lemma: &str| {
            found.iter().find(|(l, _, _)| l == lemma).cloned().unwrap()
        };
        let (b, a) = (find(&b, british), find(&a, american));
        assert_eq!((b.1, b.2), (a.1, a.2), "{british}/{american}");
    };
    pair("grey", "gray");
    pair("colourful", "colorful");
    let realised = flags(
        "She realised it. He realized it. They realised it.",
        &loose(),
    );
    let realize = realised.iter().find(|(l, _, _)| l == "realise").unwrap();
    assert_eq!(realize.1, 3);
    assert_eq!(Some(realize.2 * 100.0), bundled.rate("realize"));
}

#[test]
fn hyphenated_words_count_their_head() {
    let text = "She half-smiled. He half-smiled back. A well-known face, a blue-grey coat, \
twenty-five men, a grey dog.";
    let found = flags(text, &loose());
    let lemma = |name: &str| found.iter().find(|(l, _, _)| l == name).map(|f| f.1);
    assert_eq!(lemma("smile"), Some(2));
    assert_eq!(lemma("know"), Some(1));
    assert_eq!(lemma("grey"), Some(2));
    assert!(
        found
            .iter()
            .all(|(l, _, _)| !l.contains('-') && l != "five"),
        "{found:?}"
    );
    let d = document(text, &DocumentOptions::new());
    let r = report(
        &d,
        &loose(),
        Some(&synthetic().with_lemma("well-known", 10.0)),
    )
    .unwrap();
    assert!(r.overused.iter().any(|w| w.lemma == "well-known"));
}

#[test]
fn clause_final_pronoun_that_is_not_a_filler() {
    let d = document(
        "He said that. She knew that he lied. “I want that,” she said. It was so cold that we left.",
        &DocumentOptions::new(),
    );
    let r = report(&d, &OverusedWordsOptions::new(), None).unwrap();
    let that = r.fillers.iter().find(|f| f.word == "that").unwrap();
    assert_eq!(that.count, 2);
}

#[test]
fn minimum_count_scales_with_length_beyond_a_chapter() {
    let options = OverusedWordsOptions::new();
    assert_eq!(options.minimum_count_for(0), 5);
    assert_eq!(options.minimum_count_for(10_000), 5);
    assert_eq!(options.minimum_count_for(25_000), 8);
    assert_eq!(options.minimum_count_for(90_000), 15);
    assert_eq!(
        options
            .clone()
            .with_count_scale_words(0)
            .minimum_count_for(90_000),
        5
    );
    let mut text = CHAPTER.repeat(450);
    text.push_str(&"She glanced up. ".repeat(7));
    let d = document(&text, &DocumentOptions::new());
    assert!(d.word_count > 20_000);
    let glance = |options: &OverusedWordsOptions| {
        let table = OverusedWordsBaseline::new().with_lemma("glance", 10.0);
        report(&d, options, Some(&table))
            .unwrap()
            .overused
            .iter()
            .any(|w| w.lemma == "glance")
    };
    assert!(!glance(&options));
    assert!(glance(&options.with_count_scale_words(0)));
}
