use std::sync::{Arc, OnceLock};

use nlprule::Tokenizer;
use writemorebetter_style::reports::diction::{self, ConcretenessTable, DictionOptions};
use writemorebetter_style::reports::{to_utf16_json, StyleResources};
use writemorebetter_style::{
    analyze_document, Document, DocumentOptions, StyleDocument, StyleError,
};

const INVENTED: &str = r#"{"lamp": 4.8, "stone": 4.6, "river": 4.4, "idea": 1.6, "freedom": 1.2, "truth": 1.4, "hope": 1.8, "theory": 2.0, "glimmer": 4.0, "Velvet": 4.2}"#;

fn tokenizer() -> &'static Tokenizer {
    static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();
    TOKENIZER.get_or_init(|| Tokenizer::new("../storage/en_tokenizer.bin").unwrap())
}

fn document(text: &str) -> Document {
    analyze_document(tokenizer(), text, &DocumentOptions::new())
}

fn table() -> ConcretenessTable {
    ConcretenessTable::parse(INVENTED).unwrap()
}

fn close(left: Option<f64>, right: f64) -> bool {
    left.is_some_and(|left| (left - right).abs() < 1e-9)
}

#[test]
fn means_follow_the_invented_table_per_sentence_and_paragraph() {
    let text = "The lamp lit the stone by the river. Hope and truth.\nFreedom is an idea.";
    let options = DictionOptions::new().with_minimum_rated_words(2);
    let r = diction::report(&document(text), &table(), &options).unwrap();
    assert_eq!(r.table_words, 10);
    assert_eq!(r.sentences.len(), 3);
    assert_eq!(r.sentences[0].rated_words, 3);
    assert!(close(r.sentences[0].mean, (4.8 + 4.6 + 4.4) / 3.0));
    assert!(!r.sentences[0].is_abstract);
    assert!(close(r.sentences[1].mean, 1.6));
    assert!(r.sentences[1].is_abstract);
    assert!(close(r.sentences[2].mean, 1.4));
    assert_eq!(r.paragraphs.len(), 2);
    assert_eq!(r.paragraphs[0].sentences, 0..2);
    assert_eq!(r.paragraphs[0].rated_words, 5);
    assert!(close(
        r.paragraphs[0].mean,
        (4.8 + 4.6 + 4.4 + 1.8 + 1.4) / 5.0
    ));
    assert_eq!(
        &text[r.paragraphs[1].span.byte.clone()],
        "Freedom is an idea."
    );
    assert_eq!(r.groups.all.sentences, 3);
    assert_eq!(r.groups.all.rated_words, 7);
    assert_eq!(r.groups.all.abstract_sentences, 2);
    assert!(close(r.groups.all.mean, (13.8 + 3.2 + 2.8) / 7.0));
    let sparse = diction::report(
        &document(text),
        &table(),
        &DictionOptions::new().with_minimum_rated_words(3),
    )
    .unwrap();
    assert_eq!(sparse.sentences[1].mean, None);
    assert!(!sparse.sentences[1].is_abstract);
    assert!(close(sparse.groups.all.mean, (13.8 + 3.2 + 2.8) / 7.0));
}

#[test]
fn runs_need_consecutive_abstract_narration_and_break_at_concrete_dialogue_and_scenes() {
    let abstract_sentence = "Hope is an idea of freedom.";
    let text = [
        abstract_sentence,
        "Yes.",
        abstract_sentence,
        abstract_sentence,
        "The lamp lit the stone by the river.",
        abstract_sentence,
        abstract_sentence,
        "“The truth is an idea of freedom,” she said.",
        abstract_sentence,
        abstract_sentence,
    ]
    .join(" ")
        + "\n\n\n"
        + &[abstract_sentence; 2].join(" ");
    let r = diction::report(&document(&text), &table(), &DictionOptions::new()).unwrap();
    assert_eq!(r.abstract_runs.len(), 1);
    assert_eq!(r.abstract_runs[0].sentences, 0..4);
    assert_eq!(
        text[r.abstract_runs[0].span.byte.clone()].trim_end(),
        &text[..text.find(" The lamp").unwrap()]
    );
    let two = diction::report(
        &document(&text),
        &table(),
        &DictionOptions::new().with_minimum_run_sentences(2),
    )
    .unwrap();
    let runs: Vec<_> = two
        .abstract_runs
        .iter()
        .map(|r| r.sentences.clone())
        .collect();
    assert_eq!(runs, [0..4, 5..7, 8..10, 10..12]);
    assert_eq!(two.sentences[7].mode, writemorebetter_style::Mode::Dialogue);
    assert!(two.sentences[7].mean.is_some_and(|mean| mean < 2.5));
    assert!(!two.sentences[7].is_abstract);
    assert_eq!(two.groups.dialogue.abstract_sentences, 0);
    assert_eq!(two.groups.all.abstract_sentences, 9);
}

#[test]
fn tsv_tables_accept_two_columns_or_a_brysbaert_style_header() {
    let plain = ConcretenessTable::parse("lamp\t4.8\nidea\t1.6\n").unwrap();
    assert_eq!(plain.len(), 2);
    assert_eq!(plain.get("idea"), Some(1.6));
    let header = ConcretenessTable::parse(
        "Word\tBigram\tConc.M\tConc.SD\nLamp\t0\t4.7\t0.5\nidea\t0\t1.5\t0.9\n",
    )
    .unwrap();
    assert_eq!(header.get("lamp"), Some(4.7));
    assert_eq!(header.get("idea"), Some(1.5));
    let named = ConcretenessTable::parse("word\tconcreteness\nlamp\t3\n").unwrap();
    for bom in [
        "\u{feff}Word\tConc.M\nlamp\t4.7\n",
        "\u{feff}lamp\t4.7\n",
        "\u{feff}{\"lamp\": 4.7}",
    ] {
        assert_eq!(
            ConcretenessTable::parse(bom).unwrap().get("lamp"),
            Some(4.7),
            "{bom:?}"
        );
    }
    assert_eq!(
        ConcretenessTable::from_json("\u{feff}{\"lamp\": 4.7}")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(named.get("lamp"), Some(3.0));
    assert_eq!(table().get("velvet"), Some(4.2));
    for bad in [
        "",
        "{}",
        "lamp",
        "lamp\tbright",
        r#"{"lamp": "bright"}"#,
        r#"{"": 2}"#,
        "{",
    ] {
        assert!(
            matches!(
                ConcretenessTable::parse(bad),
                Err(StyleError::InvalidTable { .. })
            ),
            "{bad:?}"
        );
    }
}

#[test]
fn lemmas_names_and_unicode_spans() {
    let text = "The lamps’ glimmering—stones… \u{2028}“Ideas,” she said. Then Hope left.";
    let r = diction::report(
        &document(text),
        &table(),
        &DictionOptions::new().with_minimum_rated_words(1),
    )
    .unwrap();
    assert_eq!(r.sentences[0].rated_words, 3);
    assert!(close(r.sentences[0].mean, (4.8 + 4.0 + 4.6) / 3.0));
    assert_eq!(r.sentences.last().unwrap().rated_words, 0);
    let rated: usize = r.sentences.iter().map(|s| s.rated_words).sum();
    assert_eq!(rated, 4);
    let json: serde_json::Value = serde_json::from_str(&to_utf16_json(&r)).unwrap();
    let utf16: Vec<u16> = text.encode_utf16().collect();
    for sentence in json["sentences"].as_array().unwrap() {
        let span = &sentence["span"];
        let (start, end) = (
            span["start"].as_u64().unwrap() as usize,
            span["end"].as_u64().unwrap() as usize,
        );
        assert!(String::from_utf16(&utf16[start..end]).is_ok());
    }
    assert!(json["sentences"][0]["abstract"].is_boolean());
}

#[test]
fn past_tense_verbs_are_rated_by_the_verb_lemma() {
    let table =
        ConcretenessTable::parse(r#"{"feel": 2.0, "felt": 4.9, "see": 3.0, "saw": 4.8}"#).unwrap();
    let text = "I felt it. He saw it. The felt was there. The saw was there.";
    let r = diction::report(
        &document(text),
        &table,
        &DictionOptions::new().with_minimum_rated_words(1),
    )
    .unwrap();
    let means: Vec<_> = r.sentences.iter().map(|s| s.mean.unwrap()).collect();
    assert_eq!(means, [2.0, 3.0, 4.9, 4.8]);
}

#[test]
fn missing_and_invalid_tables_are_errors_not_empty_reports() {
    let style = StyleDocument::new(
        Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()),
        Arc::new(StyleResources::new()),
        "Hope is an idea.",
        &DocumentOptions::new(),
    );
    let missing = style.diction(None, None).unwrap_err();
    assert!(matches!(missing, StyleError::MissingTable { .. }));
    assert!(missing
        .to_string()
        .contains("diction needs a caller-supplied"));
    assert_eq!(
        serde_json::to_value(&missing).unwrap(),
        serde_json::json!({"kind": "missingTable", "report": "diction"})
    );
    assert!(matches!(
        style.diction(None, Some("lamp")),
        Err(StyleError::InvalidTable { .. })
    ));
    assert!(matches!(
        style.diction(Some(r#"{"bogus":1}"#), Some(INVENTED)),
        Err(StyleError::InvalidOptions { .. })
    ));
    assert!(matches!(
        style.diction(Some(r#"{"abstractBelow":"x"}"#), None),
        Err(StyleError::InvalidOptions { .. })
    ));
    let report = style
        .diction(Some(r#"{"minimum_rated_words":1}"#), Some(INVENTED))
        .unwrap();
    assert!(report.sentences[0].is_abstract);
    for (table, abstract_sentence) in [
        (r#"{"hope": 4.5, "idea": 4.5}"#, false),
        (INVENTED, true),
        (INVENTED, true),
    ] {
        let again = style
            .diction(Some(r#"{"minimumRatedWords":1}"#), Some(table))
            .unwrap();
        assert_eq!(again.sentences[0].is_abstract, abstract_sentence);
    }
    let compact = analyze_document(
        tokenizer(),
        "Hope.",
        &DocumentOptions::new().with_token_details(false),
    );
    assert!(matches!(
        diction::report(&compact, &table(), &DictionOptions::new()),
        Err(StyleError::NeedsTokenDetails { .. })
    ));
    assert!(matches!(
        diction::report(
            &document("Hope."),
            &table(),
            &DictionOptions::new().with_abstract_below(f64::INFINITY)
        ),
        Err(StyleError::InvalidOptions { .. })
    ));
}
