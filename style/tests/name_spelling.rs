use std::sync::{Arc, OnceLock};

use nlprule::Tokenizer;
use writemorebetter_style::reports::{
    name_spelling::{self, NameSpellingOptions, NameSpellingReason},
    StyleResources,
};
use writemorebetter_style::{analyze_document, Document, DocumentOptions, StyleDocument};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

fn run(text: &str, names: &[&str], compact: bool) -> name_spelling::NameSpellingReport {
    let options = DocumentOptions::new()
        .with_skip_names(names.iter().map(|s| (*s).to_owned()).collect())
        .with_token_details(!compact);
    let document = analyze_document(tokenizer(), text, &options);
    name_spelling::report(&document, &NameSpellingOptions::default()).unwrap()
}

#[test]
fn folded_edit_and_counts() {
    let report = run(
        "Mara saw Marra. Zoë saw Zoe. VAN DER BERG saw Vanderberg.",
        &["Mara", "Zoë", "Van der Berg", "Unused"],
        false,
    );
    assert_eq!(report.names.len(), 4);
    assert_eq!(report.names[0].counts.all, 1);
    assert_eq!(report.names[3].counts.all, 0);
    assert!(report
        .flags
        .iter()
        .any(|f| f.form == "Marra" && f.reason == NameSpellingReason::Edit));
    assert!(report
        .flags
        .iter()
        .any(|f| f.form == "Zoe" && f.reason == NameSpellingReason::Diacritics));
    assert!(report
        .flags
        .iter()
        .any(|f| f.form == "Vanderberg" && f.reason == NameSpellingReason::Spacing));
}

#[test]
fn listed_variants_win_and_nearby_real_names_flag() {
    let both = run("Mara met Mary.", &["Mara", "Mary"], false);
    assert!(both.flags.is_empty());
    assert_eq!(
        both.names.iter().map(|n| n.counts.all).collect::<Vec<_>>(),
        [1, 1]
    );
    let one = run("Mara met Mary.", &["Mara"], true);
    assert_eq!(one.flags.len(), 1);
    assert_eq!(one.flags[0].form, "Mary");
}

#[test]
fn ties_and_phrase_edits() {
    let tied = run("Mare entered.", &["Mara", "Mare", "Mari"], true);
    assert!(tied.flags.is_empty());
    let tied = run("Maro entered.", &["Mara", "Mari"], true);
    assert_eq!(tied.flags[0].name, "Mara");
    assert_eq!(tied.flags[0].alternatives, ["Mari"]);
    let phrase = run("Van der Breg entered.", &["Van der Berg"], true);
    assert_eq!(phrase.flags[0].form, "Van der Breg");
    assert_eq!(phrase.flags[0].reason, NameSpellingReason::Edit);
    assert_eq!(
        phrase.flags[0].span.byte.end - phrase.flags[0].span.byte.start,
        "Van der Breg".len()
    );
}

#[test]
fn short_names_only_fold_and_unknown_options() {
    let report = run(
        "Zo saw ZO. Zoa saw Zo.“, she said—then Zo…\u{2028}",
        &["Zo"],
        true,
    );
    assert!(!report.flags.iter().any(|f| f.form == "ZO"));
    assert_eq!(report.names[0].counts.all, 4);
    assert!(!report.flags.iter().any(|f| f.form == "Zoa"));
    let caps = run("MARA met MARRA.", &["Mara"], true);
    assert!(caps.flags.is_empty());
    assert_eq!(caps.names[0].counts.all, 1);
    let retained = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::default()),
        "Mara.",
        &DocumentOptions::new(),
    );
    assert!(retained
        .name_spelling(Some(r#"{"notAnOption":true}"#))
        .is_err());
}

#[test]
fn phonetic_matches_respellings_after_edit() {
    let text = "🙂 “Kathryn?” Stephen asked—then Steven replied…\u{2028}Katherine listened.";
    let report = run(text, &["Katherine", "Steven"], true);
    assert_eq!(
        report
            .flags
            .iter()
            .map(|f| (f.form.as_str(), f.reason))
            .collect::<Vec<_>>(),
        [
            ("Kathryn", NameSpellingReason::Phonetic),
            ("Stephen", NameSpellingReason::Phonetic)
        ]
    );
    assert!(report.flags.iter().all(|f| f.distance.is_none()));
    let document = analyze_document(
        tokenizer(),
        text,
        &DocumentOptions::new().with_skip_names(vec!["Katherine".into(), "Steven".into()]),
    );
    let no_phonetic =
        name_spelling::report(&document, &NameSpellingOptions::new().with_phonetic(false)).unwrap();
    assert!(no_phonetic.flags.is_empty());
}

#[test]
fn possessives_hyphens_and_unicode_spans() {
    let text = "🙂 “Marra’s—Marra's; Marra'… Mara-Lynn.”\u{2028}Zoe met Zoë and Zoe\u{308}.";
    let report = run(text, &["Mara", "Zoë", "Mara-Lynn"], true);
    for flag in &report.flags {
        assert_eq!(text.get(flag.span.byte.clone()), Some(flag.form.as_str()));
        assert!(!flag.form.ends_with(['\'', '’']));
    }
    assert_eq!(report.flags.iter().filter(|f| f.form == "Marra").count(), 3);
    assert!(report.flags.iter().any(|f| f.form == "Zoe"));
    assert!(!report.flags.iter().any(|f| f.form == "Zoe\u{308}"));
    assert_eq!(report.names[1].counts.all, 2);
    assert!(!report.flags.iter().any(|f| f.form == "Mara-Lynn"));
    assert_eq!(report.names[2].counts.all, 1);
}

#[test]
fn correct_equivalents_count_without_flags() {
    let report = run(
        "🙂 “ZOË”—Zoe\u{308}… ZOË’s!\u{2028}VAN DER BERG spoke. De Luca answered—van Gogh…",
        &["Zoë", "Van der Berg", "de Luca", "van Gogh"],
        true,
    );
    assert!(report.flags.is_empty(), "{:?}", report.flags);
    assert_eq!(
        report
            .names
            .iter()
            .map(|n| n.counts.all)
            .collect::<Vec<_>>(),
        [3, 1, 1, 1]
    );
}

#[test]
fn mixed_case_still_reports_a_visible_difference() {
    let report = run("MaRa met Mara.", &["Mara"], true);
    assert_eq!(report.flags.len(), 1);
    assert_eq!(report.flags[0].reason, NameSpellingReason::Case);
}

#[test]
fn skip_names_does_not_change_other_reports() {
    let plain = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::default()),
        "Mara ran. The colour faded.",
        &DocumentOptions::new(),
    );
    let roster = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::default()),
        "Mara ran. The colour faded.",
        &DocumentOptions::new().with_skip_names(vec!["Mara".into()]),
    );
    assert_eq!(
        plain.regional_spelling(None).unwrap().flags,
        roster.regional_spelling(None).unwrap().flags
    );
    let current = analyze_document(
        tokenizer(),
        "Mara ran. The colour faded.",
        &DocumentOptions::new().with_skip_names(vec!["Mara".into()]),
    );
    let mut old_json = serde_json::to_value(&current).unwrap();
    old_json.as_object_mut().unwrap().remove("skip_names");
    let old: Document = serde_json::from_value(old_json).unwrap();
    assert_eq!(current.word_count, old.word_count);
    assert_eq!(
        name_spelling::report(&old, &NameSpellingOptions::default())
            .unwrap()
            .names
            .len(),
        0
    );
    assert_eq!(
        writemorebetter_style::reports::regional_spelling::report(&current, &Default::default())
            .unwrap(),
        writemorebetter_style::reports::regional_spelling::report(&old, &Default::default())
            .unwrap()
    );
}

#[test]
fn dictionary_option_and_lowercase_guard() {
    let parsed: NameSpellingOptions =
        serde_json::from_str(r#"{"include_dictionary_words":true}"#).unwrap();
    assert!(parsed.include_dictionary_words);
    let document = analyze_document(
        tokenizer(),
        "Color faded.",
        &DocumentOptions::new().with_skip_names(vec!["Colon".into()]),
    );
    let strict = name_spelling::report(&document, &NameSpellingOptions::default()).unwrap();
    assert!(strict.flags.is_empty());
    let mut options = NameSpellingOptions::default();
    options.include_dictionary_words = true;
    let permissive = name_spelling::report(&document, &options).unwrap();
    assert_eq!(permissive.flags[0].form, "Color");
    let document = analyze_document(
        tokenizer(),
        "Color faded. The color returned.",
        &DocumentOptions::new().with_skip_names(vec!["Colon".into()]),
    );
    let protected = name_spelling::report(&document, &options).unwrap();
    assert!(protected.flags.is_empty());
}

#[test]
fn derived_forms_of_listed_names_are_not_variants() {
    let report = run(
        "🙂 “The Stirlings’ house?”—Nantucketers… Mingottian aplomb.\u{2028}Stirlng waited.",
        &["Stirling", "Nantucket", "Mingott"],
        true,
    );
    assert_eq!(
        report
            .flags
            .iter()
            .map(|f| f.form.as_str())
            .collect::<Vec<_>>(),
        ["Stirlng"]
    );
}

#[test]
fn capitalized_common_and_calendar_words_are_not_candidates() {
    let text = "🙂 “In June?”—the great Lady’s Cinque Port… Part 1.\u{2028}Jnae came.";
    let names = ["Jane", "Lydia", "Perth"];
    let report = run(text, &names, true);
    assert_eq!(
        report
            .flags
            .iter()
            .map(|f| (f.form.as_str(), f.name.as_str()))
            .collect::<Vec<_>>(),
        [("Jnae", "Jane")]
    );
    let document = analyze_document(
        tokenizer(),
        text,
        &DocumentOptions::new().with_skip_names(names.iter().map(|s| (*s).to_owned()).collect()),
    );
    let permissive = name_spelling::report(
        &document,
        &NameSpellingOptions::new().with_include_dictionary_words(true),
    )
    .unwrap();
    assert!(permissive.flags.iter().any(|f| f.form == "Lady"));
}

#[test]
fn medial_w_is_silent_in_phonetic_codes() {
    let report = run(
        "🙂 “Stewart?” she asked—Stuart… nodded.\u{2028}",
        &["Stuart"],
        true,
    );
    assert_eq!(report.flags.len(), 1);
    assert_eq!(report.flags[0].form, "Stewart");
    assert_eq!(report.flags[0].reason, NameSpellingReason::Phonetic);
}
