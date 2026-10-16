use std::sync::OnceLock;

use nlprule::Tokenizer;
use writemorebetter_style::reports::overused_words::{
    report, OverusedWordsBaseline, OverusedWordsOptions, FILLER_WORDS,
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
