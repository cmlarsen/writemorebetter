use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::reports::house_style::{
    self, HouseStyleExtends, HouseStyleLevel, HouseStyleMatch, HouseStyleOptions, HouseStyleRule,
    HouseStyleRules,
};
use writemorebetter_style::reports::{to_utf16_json, StyleResources};
use writemorebetter_style::{analyze_document, DocumentOptions, Mode, StyleDocument, StyleError};

const BUNDLE: &str = include_str!("fixtures/house-style/bundle.yml");
const HEDGING: &str = include_str!("fixtures/house-style/Hedging.yml");

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

fn run_with(text: &str, rules: &str, options: &HouseStyleOptions) -> Vec<HouseStyleMatch> {
    let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
    let rules = HouseStyleRules::parse(rules).unwrap();
    house_style::report(&document, &rules, options)
        .unwrap()
        .matches
}

fn run(text: &str, rules: &str) -> Vec<HouseStyleMatch> {
    run_with(text, rules, &HouseStyleOptions::new())
}

fn texts<'a>(text: &'a str, matches: &[HouseStyleMatch]) -> Vec<&'a str> {
    matches
        .iter()
        .map(|found| &text[found.span.byte.clone()])
        .collect()
}

fn existence(tokens: &str, ignorecase: bool) -> String {
    format!(
        r#"{{"name": "T.Exist", "extends": "existence", "message": "Avoid '%s'.", "ignorecase": {ignorecase}, "tokens": {tokens}}}"#
    )
}

fn error(rules: &str) -> String {
    let error = HouseStyleRules::parse(rules).unwrap_err();
    let value = serde_json::to_value(&error).unwrap();
    assert_eq!(value["kind"], "invalidRules", "{error}");
    assert_eq!(value["report"], "houseStyle");
    error.to_string()
}

#[test]
fn existence_respects_ignorecase() {
    let text = "Very well. It was very cold and VERY late.";
    let sensitive = run(text, &existence(r#"["very"]"#, false));
    assert_eq!(texts(text, &sensitive), ["very"]);
    let insensitive = run(text, &existence(r#"["very"]"#, true));
    assert_eq!(texts(text, &insensitive), ["Very", "very", "VERY"]);
    assert_eq!(insensitive[0].message, "Avoid 'Very'.");
    assert_eq!(insensitive[0].extends, HouseStyleExtends::Existence);
    assert_eq!(insensitive[0].level, HouseStyleLevel::Suggestion);
}

#[test]
fn existence_needs_word_boundaries_unless_nonword() {
    let text = "The everyday and every day of every-thing.";
    assert_eq!(
        texts(text, &run(text, &existence(r#"["every"]"#, false))),
        ["every", "every"]
    );
    let nonword = r#"{"name": "T.Raw", "extends": "existence", "message": "m", "nonword": true, "tokens": ["every"]}"#;
    assert_eq!(texts(text, &run(text, nonword)).len(), 3);
}

#[test]
fn multiword_phrases_cross_soft_breaks_and_wrapped_lines() {
    let text = "She cared a lot.\u{2028}It mattered a\u{2028}lot to him. A  lot.";
    let found = run(text, &existence(r#"["a lot"]"#, true));
    assert_eq!(texts(text, &found), ["a lot", "a\u{2028}lot"]);
    let wrapped = "He said it mattered a\nlot, then left.";
    let document = analyze_document(
        tokenizer(),
        wrapped,
        &DocumentOptions::new().with_hard_wrapped(true),
    );
    let rules = HouseStyleRules::parse(&existence(r#"["a lot"]"#, true)).unwrap();
    let report = house_style::report(&document, &rules, &HouseStyleOptions::new()).unwrap();
    assert_eq!(texts(wrapped, &report.matches), ["a\nlot"]);
}

#[test]
fn curly_apostrophes_match_straight_rules_and_stay_inside_words() {
    let text = "I don’t know. Don't go. The advisor’s desk. ‘Tis late, o’clock.";
    assert_eq!(
        texts(text, &run(text, &existence(r#"["don't"]"#, true))),
        ["don’t", "Don't"]
    );
    // `don` and `t` are pieces of *don't*, not words.
    assert!(run(text, &existence(r#"["don", "t", "clock"]"#, true)).is_empty());
    // A clitic still ends the word before it.
    assert_eq!(
        texts(text, &run(text, &existence(r#"["advisor"]"#, false))),
        ["advisor"]
    );
    // An opening curly quote is not part of the word after it.
    assert_eq!(
        texts(text, &run(text, &existence(r#"["tis"]"#, true))),
        ["Tis"]
    );
    // Rules written with curly apostrophes match straight ones too.
    assert_eq!(
        texts(text, &run(text, &existence(r#"["don’t"]"#, true))),
        ["don’t", "Don't"]
    );
}

#[test]
fn em_dashes_ellipses_and_curly_quotes_are_boundaries() {
    let text = "It was very—very—late… “Very,” she said. Very…";
    let found = run(text, &existence(r#"["very"]"#, true));
    assert_eq!(texts(text, &found), ["very", "very", "Very", "Very"]);
    let modes: Vec<_> = found.iter().map(|found| found.mode).collect();
    assert_eq!(
        modes,
        [
            Mode::Narration,
            Mode::Narration,
            Mode::Dialogue,
            Mode::Narration
        ]
    );
}

#[test]
fn spans_are_utf16_in_bindings() {
    let text = "“Well—” 😀 a lot";
    let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
    let rules = HouseStyleRules::parse(&existence(r#"["a lot"]"#, false)).unwrap();
    let report = house_style::report(&document, &rules, &HouseStyleOptions::new()).unwrap();
    let span = &report.matches[0].span;
    assert_eq!(&text[span.byte.clone()], "a lot");
    assert_eq!(span.char, 10..15);
    assert_eq!(span.utf16, 11..16);
    let json: serde_json::Value = serde_json::from_str(&to_utf16_json(&report)).unwrap();
    assert_eq!(
        json["matches"][0]["span"],
        serde_json::json!({"start": 11, "end": 16})
    );
    assert_eq!(json["matches"][0]["extends"], "existence");
    assert_eq!(json["rules"], 1);
}

#[test]
fn skip_dialogue_leaves_quoted_speech_out() {
    let text = "“Towards the door,” she said, walking towards it.";
    let rules = r#"{"name": "T.Toward", "extends": "substitution", "message": "Use '%s' instead of '%s'.", "ignorecase": true, "swap": {"towards": "toward"}}"#;
    let all = run(text, rules);
    assert_eq!(texts(text, &all), ["Towards", "towards"]);
    assert_eq!(all[0].mode, Mode::Dialogue);
    let narration = run_with(
        text,
        rules,
        &HouseStyleOptions::new().with_skip_dialogue(true),
    );
    assert_eq!(texts(text, &narration), ["towards"]);
    assert_eq!(narration[0].mode, Mode::Narration);
}

#[test]
fn substitution_names_the_listed_replacement() {
    let text = "Send an e-mail afterwards. Email is fine. Okay, O.K., OK.";
    let found = run(text, BUNDLE);
    let toward: Vec<_> = found
        .iter()
        .filter(|found| found.rule == "House.Toward")
        .collect();
    let matched: Vec<_> = toward
        .iter()
        .map(|found| &text[found.span.byte.clone()])
        .collect();
    assert_eq!(matched, ["e-mail", "afterwards", "Okay", "O.K."]);
    assert_eq!(toward[0].replacement.as_deref(), Some("email"));
    assert_eq!(
        toward[1].message,
        "Use 'afterward' instead of 'afterwards'."
    );
    assert_eq!(toward[1].level, HouseStyleLevel::Error);
    // The replacement is the rule's own text, never a case-adjusted rewrite.
    assert_eq!(toward[2].replacement.as_deref(), Some("OK"));
}

#[test]
fn substitution_skips_text_already_in_a_listed_form() {
    let rules = r#"{"name": "T.Mail", "extends": "substitution", "message": "Use '%s'.", "ignorecase": true, "swap": {"e-?mail": "email|e-mail"}}"#;
    let text = "Email or e-mail or Email.";
    assert!(run(text, rules).is_empty());
}

#[test]
fn consistency_flags_the_variant_used_second() {
    let text =
        "The adviser came. Grey skies.\n\nThe advisor left. Gray skies, the Adviser’s gray hat.";
    let found: Vec<_> = run(text, BUNDLE)
        .into_iter()
        .filter(|found| found.rule == "House.Adviser")
        .collect();
    assert_eq!(texts(text, &found), ["advisor", "Gray", "gray"]);
    assert_eq!(found[0].replacement.as_deref(), Some("adviser"));
    assert_eq!(found[0].message, "Inconsistent spelling of 'advisor'.");
    assert_eq!(found[1].replacement.as_deref(), Some("grey"));
    // One variant alone is never flagged.
    assert!(run("Gray skies and a gray hat.", BUNDLE).is_empty());
}

#[test]
fn consistency_scope_limits_the_comparison() {
    let rule = |scope: &str| {
        format!(
            r#"{{"name": "T.Grey", "extends": "consistency", "message": "'%s'", "scope": "{scope}", "either": {{"grey": "gray"}}}}"#
        )
    };
    let text = "A grey dog.\n\nA gray cat.";
    assert_eq!(texts(text, &run(text, &rule("text"))), ["gray"]);
    assert!(run(text, &rule("paragraph")).is_empty());
}

#[test]
fn occurrence_counts_per_scope() {
    let text = "One, two, three, four, five. One, two.\n\nAnd, so, on.";
    let found: Vec<_> = run(text, BUNDLE)
        .into_iter()
        .filter(|found| found.rule == "House.Commas")
        .collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].count, Some(4));
    assert_eq!(&text[found[0].span.byte.clone()], ", two, three, four,");
    let paragraphs = r#"{"name": "T.Long", "extends": "occurrence", "message": "Too many.", "scope": "paragraph", "max": 4, "token": ","}"#;
    let found = run(text, paragraphs);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].count, Some(5));
    let minimum = r#"{"name": "T.Min", "extends": "occurrence", "message": "Needs a semicolon.", "scope": "paragraph", "min": 1, "token": ";"}"#;
    let found = run(text, minimum);
    assert_eq!(
        texts(text, &found),
        ["One, two, three, four, five. One, two.", "And, so, on."]
    );
    assert_eq!(found[0].count, Some(0));
}

#[test]
fn overlapping_matches_keep_the_longest_within_a_rule() {
    let text = "It was a lot of lot.";
    let found = run(text, &existence(r#"["lot", "a lot", "a lot of"]"#, false));
    assert_eq!(texts(text, &found), ["a lot of", "lot"]);
    // Separate rules each report their own match.
    let rules = format!(
        "[{}, {}]",
        existence(r#"["a lot"]"#, false),
        existence(r#"["lot of"]"#, false).replace("T.Exist", "T.Other")
    );
    let found = run(text, &rules);
    assert_eq!(texts(text, &found), ["a lot", "lot of"]);
    assert_eq!(found[0].rule, "T.Exist");
    assert_eq!(found[1].rule, "T.Other");
}

#[test]
fn exceptions_limit_and_skipped_names() {
    let text = "Mom said so. My mom agreed. Mom laughed.";
    let rules = r#"{"name": "T.Mom", "extends": "existence", "message": "'%s'", "tokens": ["[Mm]om"], "exceptions": ["Mom"]}"#;
    assert_eq!(texts(text, &run(text, rules)), ["mom"]);
    let limited = r#"{"name": "T.Mom", "extends": "existence", "message": "'%s'", "tokens": ["Mom"], "limit": 1}"#;
    assert_eq!(run(text, limited).len(), 1);
    let document = analyze_document(
        tokenizer(),
        text,
        &DocumentOptions::new().with_skip_names(vec!["Mom".into()]),
    );
    let rules = HouseStyleRules::parse(&existence(r#"["mom"]"#, true)).unwrap();
    let report = house_style::report(&document, &rules, &HouseStyleOptions::new()).unwrap();
    assert_eq!(texts(text, &report.matches), ["mom"]);
}

#[test]
fn groups_count_levels_by_mode() {
    let text = "“Towards it,” he said. She walked towards it, sort of.";
    let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
    let rules = HouseStyleRules::parse(BUNDLE).unwrap();
    let report = house_style::report(&document, &rules, &HouseStyleOptions::new()).unwrap();
    assert_eq!(report.rules, 4);
    assert_eq!(report.groups.all.errors, 2);
    assert_eq!(report.groups.dialogue.errors, 1);
    assert_eq!(report.groups.narration.warnings, 1);
    assert_eq!(report.groups.all.words, 10);
}

#[test]
fn vale_fixture_round_trips() {
    let rules = HouseStyleRules::parse(BUNDLE).unwrap();
    assert_eq!(rules.len(), 4);
    let json = rules.to_json();
    let again = HouseStyleRules::parse(&json).unwrap();
    assert_eq!(again, rules);
    assert_eq!(again.to_json(), json);
    // The same rule read as a lone Vale file named after it.
    let hedging = HouseStyleRule::parse("House.Hedging", HEDGING).unwrap();
    assert_eq!(&hedging, &rules.rules()[1]);
    assert_eq!(hedging.tokens, ["a bit", "sort of", "kind of", "somewhat"]);
    assert_eq!(
        hedging.link.as_deref(),
        Some("https://example.com/house-style#hedging")
    );
    // Bundle shapes: a list, and a map from name to rule.
    let list = format!("[{}]", serde_json::to_string(&hedging).unwrap());
    assert_eq!(
        HouseStyleRules::parse(&list).unwrap().rules(),
        std::slice::from_ref(&hedging)
    );
    let map = format!("rules:\n  House.Hedging:\n{}", indent(HEDGING));
    assert_eq!(HouseStyleRules::parse(&map).unwrap().rules(), [hedging]);
    // Running the YAML and its JSON round trip gives the same report.
    let text = "He walked towards a bit of grey sky, then gray.";
    assert_eq!(run(text, BUNDLE), run(text, &json));
}

fn indent(text: &str) -> String {
    text.lines().map(|line| format!("    {line}\n")).collect()
}

#[test]
fn invalid_rules_are_errors() {
    let base = r#""extends": "existence", "message": "m", "tokens": ["a"]"#;
    let cases = [
        (
            format!(r#"{{"name": "X", {base}, "tokenz": 1}}"#),
            "unknown field `tokenz`",
        ),
        (
            r#"{"name": "X", "extends": "capitalization", "message": "m"}"#.into(),
            "extends `capitalization` is not supported",
        ),
        (
            r#"{"name": "X", "extends": "existence", "message": "m", "tokens": ["(unclosed"]}"#
                .into(),
            "rule X: pattern `(unclosed`",
        ),
        (
            r#"{"name": "X", "extends": "existence", "message": "m"}"#.into(),
            "existence rules need `tokens`",
        ),
        (
            r#"{"name": "X", "extends": "existence", "tokens": ["a"]}"#.into(),
            "a rule needs a message",
        ),
        (format!("[{{{base}}}]"), "every rule needs a name"),
        (
            format!(r#"[{{"name": "X", {base}}}, {{"name": "X", {base}}}]"#),
            "two rules share this name",
        ),
        (
            format!(r#"{{"name": "X", {base}, "level": "fatal"}}"#),
            "level `fatal`",
        ),
        (
            format!(r#"{{"name": "X", {base}, "scope": "heading"}}"#),
            "scope `heading`",
        ),
        (
            format!(r#"{{"name": "X", {base}, "swap": {{"a": "b"}}}}"#),
            "`swap` is not used by existence rules",
        ),
        (
            r#"{"name": "X", "extends": "occurrence", "message": "m", "token": ","}"#.into(),
            "occurrence rules need `max` or `min`",
        ),
        (
            r#"{"name": "X", "extends": "existence", "message": "m", "raw": ["a"]}"#.into(),
            "unknown field `raw`",
        ),
        ("name: X\nextends: [existence\n".into(), "YAML line"),
        (
            "base: &b {a: 1}\nother: *b\n".into(),
            "aliases are not supported",
        ),
        ("name: X\nname: Y\n".into(), "duplicate key `name`"),
        ("42".into(), "expected a rule"),
    ];
    for (rules, expected) in cases {
        let message = error(&rules);
        assert!(message.contains(expected), "{rules}: {message}");
    }
    let error = HouseStyleRule::parse("House.Two", BUNDLE).unwrap_err();
    assert!(
        error.to_string().contains("expected exactly one rule"),
        "{error}"
    );
}

#[test]
fn empty_bundles_and_yaml_comments_are_valid() {
    assert!(HouseStyleRules::parse("[]").unwrap().is_empty());
    assert!(HouseStyleRules::parse("# nothing yet\n")
        .unwrap()
        .is_empty());
    assert!(HouseStyleRules::parse("rules: []").unwrap().is_empty());
}

#[test]
fn style_document_method_parses_rules_and_options() {
    let document = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        "“Towards it,” he said, walking towards it.",
        &DocumentOptions::new(),
    );
    let report = document
        .house_style(Some(r#"{"skipDialogue":true}"#), Some(BUNDLE))
        .unwrap();
    assert_eq!(report.matches.len(), 1);
    let report = document
        .house_style(Some(r#"{"skip_dialogue":false}"#), Some(BUNDLE))
        .unwrap();
    assert_eq!(report.matches.len(), 2);
    assert!(matches!(
        document.house_style(Some(r#"{"skipDialog":true}"#), Some(BUNDLE)),
        Err(StyleError::InvalidOptions { .. })
    ));
    let missing = document.house_style(None, None).unwrap_err();
    assert_eq!(
        serde_json::to_value(&missing).unwrap()["kind"],
        "invalidRules"
    );
    assert!(document.house_style(None, Some("{")).is_err());
}

#[test]
fn compact_documents_are_enough() {
    let text = "She walked towards it, sort of.";
    let document = analyze_document(
        tokenizer(),
        text,
        &DocumentOptions::new().with_token_details(false),
    );
    let rules = HouseStyleRules::parse(BUNDLE).unwrap();
    let report = house_style::report(&document, &rules, &HouseStyleOptions::new()).unwrap();
    assert_eq!(texts(text, &report.matches), ["towards", "sort of"]);
}
