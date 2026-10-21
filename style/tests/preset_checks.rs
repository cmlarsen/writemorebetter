use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::reports::preset_checks::{
    self, NumberCheck, Possessives, PresetCheck, PresetCheckMatch, PresetChecks, QuotePunctuation,
    TimeAbbreviations,
};
use writemorebetter_style::reports::{to_utf16_json, StyleResources};
use writemorebetter_style::{analyze_document, DocumentOptions, Mode, StyleDocument, StyleError};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

fn run_with(text: &str, checks: &PresetChecks, options: &DocumentOptions) -> Vec<PresetCheckMatch> {
    let document = analyze_document(tokenizer(), text, options);
    preset_checks::report(&document, checks).unwrap().matches
}

fn run(text: &str, checks: &PresetChecks) -> Vec<PresetCheckMatch> {
    run_with(text, checks, &DocumentOptions::new())
}

fn texts<'a>(text: &'a str, matches: &[PresetCheckMatch]) -> Vec<&'a str> {
    matches
        .iter()
        .map(|found| &text[found.span.byte.clone()])
        .collect()
}

fn replacements(matches: &[PresetCheckMatch]) -> Vec<Option<&str>> {
    matches
        .iter()
        .map(|found| found.replacement.as_deref())
        .collect()
}

fn numbers(through: u32) -> PresetChecks {
    PresetChecks::new().with_numbers(Some(NumberCheck::new(through)))
}

fn quotes(style: QuotePunctuation) -> PresetChecks {
    PresetChecks::new().with_quote_punctuation(Some(style))
}

fn possessives(style: Possessives) -> PresetChecks {
    PresetChecks::new().with_possessives(Some(style))
}

fn times(style: TimeAbbreviations) -> PresetChecks {
    PresetChecks::new().with_time_abbreviations(Some(style))
}

fn family() -> PresetChecks {
    PresetChecks::new().with_family_titles(true)
}

#[test]
fn nothing_runs_by_default() {
    let text = "He found 7 coins in James’ car at 5 AM, “like so”. My Mom laughed.";
    assert!(run(text, &PresetChecks::new()).is_empty());
}

#[test]
fn numbers_through_one_hundred_are_spelled_out() {
    let text = "She waited 7 minutes, then 45 more. 100 crows. 101 crows. The 3rd time, 0 left.";
    let found = run(text, &numbers(100));
    assert_eq!(texts(text, &found), ["7", "45", "100", "3rd", "0"]);
    assert!(found.iter().all(|m| m.check == PresetCheck::Numbers));
    assert!(found.iter().all(|m| m.replacement.is_none()));
    assert_eq!(
        found[0].message,
        "Spell out whole numbers from zero through 100 in prose."
    );
}

#[test]
fn news_numbers_spell_out_one_through_nine() {
    let text = "She waited 7 minutes, then 45 more and 10 after that.";
    let ap = PresetChecks::new().with_numbers(Some(NumberCheck::new(9).with_match_larger(false)));
    assert_eq!(texts(text, &run(text, &ap)), ["7"]);
    // With the larger numbers in the sentence, the default keeps 7 a numeral.
    assert!(run(text, &numbers(9)).is_empty());
}

#[test]
fn number_exceptions_dates_times_addresses_years_percentages_decimals() {
    let text = "On May 5 and 12 June, the 4th of July, at 5:30 and 7 p.m. and 9am, \
                she moved to 12 Baker Street and Apt. 4, page 7, Chapter 12, in 1998 and AD 70. \
                It was 45% or 45 percent, 2.5 or 0.75 miles, 3–2 in the game, 5 km, $20, \
                1,000 crows, a 4x4, 3D glasses, the 90s, 07 and B-52s.";
    assert!(
        run(text, &numbers(100)).is_empty(),
        "{:?}",
        texts(text, &run(text, &numbers(100)))
    );
}

#[test]
fn small_numbers_beside_a_large_one_stay_numerals() {
    let text = "Of 250 guests, 12 stayed. Then 12 left. In 1998, 3 left.";
    // A year is not a larger quantity.
    assert_eq!(texts(text, &run(text, &numbers(100))), ["12", "3"]);
    assert_eq!(run(text, &numbers(100))[0].span.byte.start, 31);
}

#[test]
fn news_ages_stay_numerals() {
    let text = "Her son is 5 years old, a 6-year-old boy, aged 7. She had 3 cats.";
    let ap =
        PresetChecks::new().with_numbers(Some(NumberCheck::new(9).with_ages_as_numerals(true)));
    assert_eq!(texts(text, &run(text, &ap)), ["3"]);
    assert_eq!(texts(text, &run(text, &numbers(9))), ["5", "6", "7", "3"]);
}

#[test]
fn numbers_handle_fiction_typography() {
    let text =
        "“I counted 7…” she said—then 9 more.\u{2028}It took 3 hours’ walk, the ’90s, 5’10”.";
    let found = run(text, &numbers(100));
    assert_eq!(texts(text, &found), ["7", "9", "3"]);
    assert_eq!(found[0].mode, Mode::Dialogue);
    assert_eq!(found[1].mode, Mode::Narration);
    let json: serde_json::Value = serde_json::from_str(&to_utf16_json(&found[0])).unwrap();
    assert_eq!(json["span"], serde_json::json!({"start": 11, "end": 12}));
}

#[test]
fn numbered_list_markers_are_not_counted() {
    let text = "1. Eggs\n\n2) Milk and 2 loaves";
    assert_eq!(texts(text, &run(text, &numbers(100))), ["2"]);
}

#[test]
fn us_periods_and_commas_go_inside_closing_quotes() {
    let text = "He called it “art”. She said “no”, and left. ‘Hi,’ he said. “Fine.” “Wait”…";
    let found = run(text, &quotes(QuotePunctuation::Inside));
    assert_eq!(texts(text, &found), ["”.", "”,"]);
    assert_eq!(replacements(&found), [Some(".”"), Some(",”")]);
    assert_eq!(
        found[0].message,
        "Put the period inside the closing quotation mark."
    );
    let straight = "He called it \"art\". He wrote 'gone', then left. She was 5'10\", tall.";
    let found = run(straight, &quotes(QuotePunctuation::Inside));
    assert_eq!(texts(straight, &found), ["\".", "',"]);
}

#[test]
fn us_quote_rule_skips_apostrophes_and_ellipses() {
    let text =
        "The dogs’, cats’ and birds’. Rock ’n’ roll, ’em. “Well”... he said.\u{2028}‘Tis true.";
    assert!(run(text, &quotes(QuotePunctuation::Inside)).is_empty());
}

#[test]
fn uk_flips_quote_punctuation_for_quoted_words() {
    let text = "He called it ‘rubbish.’ She wrote ‘maybe,’ then stopped.";
    let found = run(text, &quotes(QuotePunctuation::Outside));
    assert_eq!(texts(text, &found), [".’", ",’"]);
    assert_eq!(replacements(&found), [Some("’."), Some("’,")]);
    // Speech keeps its punctuation inside in both styles.
    let speech = "‘I’m going,’ she said. ‘It’s late.’";
    assert!(run(speech, &quotes(QuotePunctuation::Outside)).is_empty());
    // The US style flags the UK placement and the other way round.
    let us = "He called it ‘rubbish’.";
    assert_eq!(
        texts(us, &run(us, &quotes(QuotePunctuation::Inside))),
        ["’."]
    );
    assert!(run(us, &quotes(QuotePunctuation::Outside)).is_empty());
}

#[test]
fn possessive_of_names_ending_in_s() {
    let text = "James’ car, Ross' hat and the Joneses’ house. The dogs’ bowls. Americans’ rights.";
    let found = run(text, &possessives(Possessives::ApostropheS));
    assert_eq!(texts(text, &found), ["’", "'"]);
    assert_eq!(replacements(&found), [Some("’s"), Some("'s")]);
    assert_eq!(found[0].span.byte.start, "James".len());
    let news = "James’s car and Ross's hat. It’s James’s.";
    let found = run(news, &possessives(Possessives::ApostropheOnly));
    assert_eq!(texts(news, &found), ["’s", "'s", "’s"]);
    assert_eq!(replacements(&found), [Some("’"), Some("'"), Some("’")]);
    assert!(run(news, &possessives(Possessives::ApostropheS)).is_empty());
}

#[test]
fn possessive_check_skips_closing_quotes_and_typography() {
    let text = "‘Ask Charles’ she said.\u{2028}“James’—no.” Thomas’…";
    assert!(run(text, &possessives(Possessives::ApostropheS)).is_empty());
}

#[test]
fn time_abbreviations() {
    let text = "At 5 AM, 6am, 7 pm, 8 P.M., 9 a.m. and 10:30\u{a0}PM. I am 5 am I.";
    let found = run(text, &times(TimeAbbreviations::Dotted));
    // A final period after an undotted form is absorbed: it also ends the sentence.
    assert_eq!(texts(text, &found), ["AM", "am", "pm", "PM.", "am"]);
    assert_eq!(
        replacements(&found),
        [
            Some("a.m."),
            Some("a.m."),
            Some("p.m."),
            Some("p.m."),
            Some("a.m.")
        ]
    );
    let undotted = "At 9 a.m. on Friday and at 10 P.M.";
    let found = run(undotted, &times(TimeAbbreviations::Undotted));
    assert_eq!(texts(undotted, &found), ["a.m", "P.M"]);
    assert_eq!(replacements(&found), [Some("am"), Some("pm")]);
}

#[test]
fn family_titles_as_names_and_after_possessives() {
    let text = "My Mom called. I asked Mom, and Dad laughed. Thanks, mom. Her dad left. \
                I told dad. Jake’s Mom waved. Mom and dad came. My mom and dad came. \
                A single dad. Dad jokes.";
    let found = run(text, &family());
    assert_eq!(
        texts(text, &found),
        ["Mom", "mom", "dad", "Mom", "dad"],
        "{found:#?}"
    );
    assert_eq!(
        replacements(&found),
        [
            Some("mom"),
            Some("Mom"),
            Some("Dad"),
            Some("mom"),
            Some("Dad")
        ]
    );
    assert_eq!(found[1].mode, Mode::Narration);
}

#[test]
fn family_titles_in_dialogue_with_curly_quotes() {
    let text = "“Mom, wait—” she said. “Thanks, mum…”\u{2028}“Where’s dad?”";
    let found = run(text, &family());
    assert_eq!(texts(text, &found), ["mum", "dad"]);
    assert!(found.iter().all(|found| found.mode == Mode::Dialogue));
}

#[test]
fn family_titles_without_token_readings() {
    let text = "I asked mom. My Mom came. A working mom.";
    let compact = DocumentOptions::new().with_token_details(false);
    let found = run_with(text, &family(), &compact);
    assert_eq!(texts(text, &found), ["mom", "Mom"]);
}

#[test]
fn skipped_names_are_never_flagged() {
    let text = "I asked mom about James’ car.";
    let options = DocumentOptions::new().with_skip_names(vec!["James".into()]);
    let checks = family().with_possessives(Some(Possessives::ApostropheS));
    let found = run_with(text, &checks, &options);
    assert_eq!(texts(text, &found), ["mom", "’"]);
}

#[test]
fn counts_split_by_mode() {
    let text = "“I saw 7,” she said. He saw 8.";
    let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
    let report = preset_checks::report(&document, &numbers(100)).unwrap();
    assert_eq!(report.counts.all, 2);
    assert_eq!(report.counts.dialogue, 1);
    assert_eq!(report.counts.narration, 1);
}

#[test]
fn options_are_strict_camel_case() {
    let style = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        "He saw 7 crows at 5 AM.",
        &DocumentOptions::new(),
    );
    let report = style
        .preset_checks(Some(
            r#"{"numbers": {"spellOutThrough": 9}, "time_abbreviations": "dotted"}"#,
        ))
        .unwrap();
    assert_eq!(report.matches.len(), 2);
    for bad in [
        r#"{"numbers": {"spellOutThru": 9}}"#,
        r#"{"quotes": "inside"}"#,
        r#"{"quotePunctuation": "sideways"}"#,
        r#"{"numbers": {"spellOutThrough": 5000}}"#,
    ] {
        let error = style.preset_checks(Some(bad)).unwrap_err();
        assert!(
            matches!(error, StyleError::InvalidOptions { .. }),
            "{bad}: {error}"
        );
    }
    let json = to_utf16_json(&report);
    assert!(json.contains(r#""check":"numbers""#), "{json}");
    assert!(json.contains(r#""check":"timeAbbreviations""#), "{json}");
}
