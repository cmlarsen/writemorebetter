use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::reports::{
    regional_spelling::{
        self, RegionalSpellingOptions, RegionalSpellingReason, RegionalSpellingReport,
        SpellingSystem,
    },
    StyleResources,
};
use writemorebetter_style::{analyze_document, DocumentOptions, Mode, StyleDocument, StyleError};

use SpellingSystem::{American, British, Oxford};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}
fn run_with(text: &str, options: &RegionalSpellingOptions) -> RegionalSpellingReport {
    regional_spelling::report(
        &analyze_document(tokenizer(), text, &DocumentOptions::new()),
        options,
    )
    .unwrap()
}
fn run(text: &str) -> RegionalSpellingReport {
    run_with(text, &RegionalSpellingOptions::default())
}
fn flagged(report: &RegionalSpellingReport) -> Vec<&str> {
    report.flags.iter().map(|flag| flag.form.as_str()).collect()
}
fn counts(report: &RegionalSpellingReport) -> Vec<usize> {
    report.systems.iter().map(|count| count.count).collect()
}

#[test]
fn flags_the_minority_system() {
    let report = run("The color faded. Her favorite neighbor realized it. The colour held.");
    assert_eq!(report.dominant, vec![American]);
    assert_eq!(flagged(&report), vec!["colour"]);
    let flag = &report.flags[0];
    assert_eq!(flag.reason, RegionalSpellingReason::System);
    assert_eq!(flag.systems, vec![British, Oxford]);
    assert_eq!(flag.dominant_form.as_deref(), Some("color"));
    assert_eq!(flag.word, "color");
    assert_eq!(flag.sentence, 2);

    let report = run("The colour faded. Her favourite neighbour realised it. The color held.");
    assert_eq!(report.dominant, vec![British]);
    assert_eq!(flagged(&report), vec!["color"]);
    assert_eq!(report.flags[0].dominant_form.as_deref(), Some("colour"));
}

#[test]
fn oxford_ize_is_not_american() {
    // Oxford spelling: -our and -re with -ize.
    let report = run("The colour of the centre. She realized, then organized the theatre.");
    assert_eq!(report.dominant, vec![Oxford]);
    assert_eq!(counts(&report), vec![2, 3, 5]);
    assert!(report.flags.is_empty());

    // An -ise British document flags -ize, and names the -ise form.
    let report =
        run("The colour of the centre. She realised it and organised it. She realized it.");
    assert_eq!(report.dominant, vec![British]);
    assert_eq!(flagged(&report), vec!["realized"]);
    assert_eq!(report.flags[0].systems, vec![American, Oxford]);
    assert_eq!(report.flags[0].dominant_form.as_deref(), Some("realised"));

    // With no -ise or -ize evidence, -ise and Oxford tie; only American spellings are flagged.
    let report = run("The colour of the centre. The colour of the theatre. The color.");
    assert_eq!(report.dominant, vec![British, Oxford]);
    assert_eq!(flagged(&report), vec!["color"]);
    assert_eq!(report.flags[0].dominant_form.as_deref(), Some("colour"));

    // A tie between -ise and -ize spellings flags neither.
    let report = run("The colour of the centre. She realised it. She organized it.");
    assert_eq!(report.dominant, vec![British, Oxford]);
    assert!(report.flags.is_empty());
}

#[test]
fn words_valid_in_both_are_not_markers() {
    // `among`, `check`, `story`, `program`, `practice`, `meter` and `tire` are used in British
    // English too, in at least one sense.
    let report = run(
        "Among the color and the honor, she would check the story, practice the program, read the \
         meter and tire. The color.",
    );
    assert_eq!(report.markers, 3);
    assert!(report.flags.is_empty());
    let report = run(
        "The colour and the honour in the centre. She would check the story, practice the program, read the meter \
         and tire.",
    );
    assert_eq!(report.dominant, vec![British, Oxford]);
    assert!(report.flags.is_empty());
}

#[test]
fn sense_dependent_spellings_never_mix_a_word() {
    // `check` the verb is British too, so `check` beside `cheque` is not a minority form.
    let report = run("The colour. The honour. She wrote a cheque. Check it. Check it again.");
    assert!(report.flags.is_empty());
    assert!(report.words.iter().all(|word| !word.mixed));
}

#[test]
fn minority_forms_of_one_word() {
    // British accepts both `cancelled` and `focused`; using two forms is still flagged.
    let report = run(
        "The colour. The honour. It was cancelled. It was cancelled. It was cancelled. It was \
         canceled.",
    );
    assert_eq!(report.dominant, vec![British, Oxford]);
    // `canceled` is American only, so the system flag wins over the form flag.
    assert_eq!(flagged(&report), vec!["canceled"]);
    assert_eq!(report.flags[0].reason, RegionalSpellingReason::System);

    let report =
        run("The color. The honor. She focused. She focused. She focussed. The canceled show.");
    assert_eq!(report.dominant, vec![American]);
    assert_eq!(flagged(&report), vec!["focussed"]);
    let word = report.words.iter().find(|w| w.mixed).unwrap();
    assert_eq!(word.word, "focused");
    assert_eq!(word.forms.len(), 2);
    assert_eq!(word.forms[0].form, "focused");
    assert_eq!(word.forms[0].counts.all, 2);
    assert_eq!(report.flags[0].dominant_form.as_deref(), Some("focused"));
    assert_eq!(
        report.flags[0].reason,
        RegionalSpellingReason::System,
        "`focussed` is British only"
    );

    // Both forms suit British spelling, so the rarer one is flagged as a form of the same word.
    let report = run("The colour. The honour. She focussed. She focussed. She focused.");
    assert_eq!(flagged(&report), vec!["focused"]);
    assert_eq!(report.flags[0].reason, RegionalSpellingReason::Form);
    assert_eq!(report.flags[0].dominant_form.as_deref(), Some("focussed"));
}

#[test]
fn proper_nouns_are_skipped() {
    let report = run(
        "They sailed past Pearl Harbor to the Labour Party office. Labour Party members met at \
         the Lincoln Center. The colour. The honour. The centre.",
    );
    assert_eq!(report.markers, 3);
    assert_eq!(report.dominant, vec![British, Oxford]);
    assert!(report.flags.is_empty());
    // Sentence-initial and quote-initial capitals still count.
    let report = run("Color drained from him. “Color me surprised,” she said. The colour.");
    assert_eq!(report.markers, 3);
    assert_eq!(flagged(&report), vec!["colour"]);
    // All-caps words count.
    let report = run("The color. The honor. “COLOUR!” she shouted.");
    assert_eq!(flagged(&report), vec!["colour"]);
}

#[test]
fn capitals_after_a_dash_or_ellipsis_open_the_sentence() {
    let report = run("—Colour drained from his face. …Honour, then. The color. The center.");
    assert_eq!(report.markers, 4);
    assert_eq!(report.dominant, vec![American, British, Oxford]);
    let report = run("—Colour drained from his face. The color. The honor. The center.");
    assert_eq!(flagged(&report), vec!["colour"]);
}

#[test]
fn quoted_titles_in_narration_are_skipped() {
    // A capital after a quote in narration is a title, not speech; in dialogue it still counts.
    let report = run("They boarded the “Endeavour” at dawn. The color. The honor. The center.");
    assert_eq!(report.markers, 3);
    assert!(report.flags.is_empty());
    let report = run("The color. The honor. The center. She looked up and said, “Grey.”");
    assert_eq!(flagged(&report), vec!["grey"]);
}

#[test]
fn mode_is_per_token() {
    // A tagged line holds dialogue and narration in one sentence; each marker takes its own mode.
    let text = "“Fine,” she said, studying the colour. She stared at the wall and said, “Grey.” \
                The color. The honor. The center.";
    let report = run(text);
    assert_eq!(flagged(&report), vec!["colour", "grey"]);
    assert_eq!(report.flags[0].mode, Mode::Narration);
    assert_eq!(report.flags[1].mode, Mode::Dialogue);
    let form = |word: &str, form: &str| {
        let word = report.words.iter().find(|w| w.word == word).unwrap();
        let form = word.forms.iter().find(|f| f.form == form).unwrap();
        (form.counts.dialogue, form.counts.narration)
    };
    assert_eq!(form("color", "colour"), (0, 1));
    assert_eq!(form("gray", "grey"), (1, 0));
}

#[test]
fn skip_names_are_not_counted() {
    let text = "Grey stood. Grey sat. The color. The honor. The center.";
    let options = DocumentOptions::new().with_skip_names(vec!["Grey".into()]);
    let document = analyze_document(tokenizer(), text, &options);
    let report = regional_spelling::report(&document, &Default::default()).unwrap();
    assert_eq!(report.markers, 3);
    assert!(report.flags.is_empty());
}

#[test]
fn hyphenated_compounds() {
    let text = "The grey-green sea. The color. The honor. The center.";
    let report = run(text);
    assert_eq!(flagged(&report), vec!["grey"]);
    let span = &report.flags[0].span;
    assert_eq!(&text[span.byte.clone()], "grey");
    assert_eq!(span.char, 4..8);
}

#[test]
fn utf8_spans_and_typography() {
    // Curly quotes, curly apostrophes, em dashes, ellipses and U+2028 around the markers.
    let text = "“The colour’s gone—gone…” She’d realised.\u{2028}The colour… and the honour—yes. \
                “It’s the color,” he said.";
    let report = run(text);
    assert_eq!(report.dominant, vec![British]);
    assert_eq!(flagged(&report), vec!["color"]);
    let span = &report.flags[0].span;
    assert_eq!(&text[span.byte.clone()], "color");
    let prefix = &text[..span.byte.start];
    assert_eq!(span.char.start, prefix.chars().count());
    assert_eq!(span.utf16.start, prefix.encode_utf16().count());
    assert_eq!(span.utf16.end - span.utf16.start, 5);
    let colour = report.words.iter().find(|w| w.word == "color").unwrap();
    let spans: Vec<_> = colour
        .forms
        .iter()
        .flat_map(|form| form.spans.iter().map(|span| &text[span.byte.clone()]))
        .collect();
    assert_eq!(spans, vec!["color", "colour", "colour"]);
    // Dialogue and narration counts are kept apart.
    let form = colour.forms.iter().find(|f| f.form == "colour").unwrap();
    assert_eq!((form.counts.dialogue, form.counts.narration), (1, 1));
}

#[test]
fn minimum_markers_and_explicit_system() {
    let report = run("The colour. The color.");
    assert!(report.dominant.is_empty());
    assert!(report.flags.is_empty());
    assert_eq!(report.markers, 2);

    let options = RegionalSpellingOptions::new().with_minimum_markers(2);
    assert!(
        run_with("The colour. The color.", &options)
            .flags
            .is_empty(),
        "a tie flags nothing"
    );

    let options = RegionalSpellingOptions::new().with_system(Some(American));
    let report = run_with("The colour. The realised plan.", &options);
    assert_eq!(report.dominant, vec![American]);
    assert_eq!(flagged(&report), vec!["colour", "realised"]);
    assert_eq!(report.flags[1].dominant_form.as_deref(), Some("realized"));

    let options = RegionalSpellingOptions::new().with_system(Some(Oxford));
    let report = run_with("The color was realized and realised.", &options);
    assert_eq!(flagged(&report), vec!["color", "realised"]);
}

#[test]
fn retained_document_options() {
    let document = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        "The color. The honor. The colour.",
        &DocumentOptions::new(),
    );
    assert_eq!(document.regional_spelling(None).unwrap().flags.len(), 1);
    let report = document
        .regional_spelling(Some(r#"{"minimumMarkers":4}"#))
        .unwrap();
    assert!(report.flags.is_empty());
    // Below the minimum there is no dominant system, so minority forms are not flagged either.
    let report = run_with(
        "The colour. The colour. The color.",
        &RegionalSpellingOptions::new().with_minimum_markers(4),
    );
    assert!(report.dominant.is_empty());
    assert!(report.words[0].mixed);
    assert!(report.flags.is_empty());
    let report = document
        .regional_spelling(Some(r#"{"system":"british"}"#))
        .unwrap();
    assert_eq!(report.flags.len(), 2);
    assert!(document
        .regional_spelling(Some(r#"{"minimum_markers":1}"#))
        .is_ok());
    assert!(matches!(
        document.regional_spelling(Some(r#"{"bogus":1}"#)),
        Err(StyleError::InvalidOptions { .. })
    ));
    assert!(matches!(
        document.regional_spelling(Some(r#"{"minimumMarkers":0}"#)),
        Err(StyleError::InvalidOptions { .. })
    ));
    assert!(matches!(
        document.regional_spelling(Some(r#"{"system":"canadian"}"#)),
        Err(StyleError::InvalidOptions { .. })
    ));
}

#[test]
fn compact_documents() {
    let text = "The color. The honor. The colour.";
    let compact = DocumentOptions::new().with_token_details(false);
    let document = analyze_document(tokenizer(), text, &compact);
    let report = regional_spelling::report(&document, &Default::default()).unwrap();
    assert_eq!(report, run(text));
    assert_eq!(flagged(&report), vec!["colour"]);
}
