use nlprule::{Rules, Tokenizer};
use std::sync::{Arc, OnceLock};
use writemorebetter_style::preset::{StylePreset, DEFAULT_PRESET, PRESET_FORMAT};
use writemorebetter_style::reports::preset_checks::{
    ClockTimeCheck, OClock, Possessives, PresetCheck, QuotePunctuation, TimeAbbreviations,
};
use writemorebetter_style::reports::{to_utf16_json, StyleResources};
use writemorebetter_style::{DocumentOptions, Mode, StyleDocument, StyleError};

const RULES_PATH: &str = "../storage/en_rules.bin";

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

fn document(text: &str) -> StyleDocument {
    StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        text,
        &DocumentOptions::new(),
    )
}

fn invalid(text: &str) -> String {
    match StylePreset::parse(text) {
        Err(error @ StyleError::InvalidPreset { .. }) => error.to_string(),
        other => panic!("expected an invalid preset for {text}, got {other:?}"),
    }
}

fn rule_names(preset: &StylePreset) -> Vec<&str> {
    preset
        .house_style_rules()
        .rules()
        .iter()
        .map(|rule| rule.name.as_str())
        .collect()
}

fn sources(rules: &Rules, text: &str) -> Vec<String> {
    rules
        .suggest(text, tokenizer())
        .iter()
        .map(|suggestion| suggestion.source().to_owned())
        .collect()
}

#[test]
fn builtin_presets_load_by_name() {
    assert_eq!(StylePreset::names(), ["chicago-fiction", "ap", "uk"]);
    assert_eq!(DEFAULT_PRESET, "chicago-fiction");
    for name in StylePreset::names() {
        let preset = StylePreset::builtin(name).unwrap();
        assert_eq!(preset.name, name);
        assert_eq!(preset.format, PRESET_FORMAT);
        assert_eq!(preset.version.as_deref(), Some("1.2.0"));
        assert!(preset.description.is_some());
        assert_eq!(StylePreset::load(name).unwrap().to_json(), preset.to_json());
    }
    let error = StylePreset::builtin("mla").unwrap_err().to_string();
    assert!(error.contains("chicago-fiction, ap, uk"), "{error}");
    assert_eq!(StylePreset::default_preset().name, "chicago-fiction");
}

#[test]
fn chicago_fiction_settings() {
    let preset = StylePreset::default_preset();
    assert_eq!(preset.extends, None);
    assert_eq!(preset.grammar.profile.as_deref(), Some("fiction"));
    assert_eq!(preset.grammar.enabled(), ["STYLE/SERIAL_COMMA_ON"]);
    assert!(preset.grammar.disabled().is_empty());
    assert_eq!(preset.grammar.spell_variant, None);
    assert_eq!(
        preset.consistency.preferred.dashes.as_deref(),
        Some("emClosed")
    );
    assert_eq!(
        rule_names(&preset),
        ["Preset.Meridiem", "Preset.OK", "Preset.Toward"]
    );
    let numbers = preset.checks.numbers.as_ref().unwrap();
    assert_eq!(numbers.spell_out_through, 100);
    assert!(numbers.match_larger);
    assert_eq!(
        preset.checks.quote_punctuation,
        Some(QuotePunctuation::Inside)
    );
    assert_eq!(preset.checks.possessives, Some(Possessives::ApostropheS));
    assert_eq!(
        preset.checks.time_abbreviations,
        Some(TimeAbbreviations::Dotted)
    );
    assert_eq!(
        preset.checks.clock_times,
        Some(ClockTimeCheck::new().with_o_clock(OClock::Words))
    );
    assert!(preset.checks.family_titles);
}

#[test]
fn ap_and_uk_extend_chicago_fiction() {
    let ap = StylePreset::builtin("ap").unwrap();
    assert_eq!(ap.extends.as_deref(), Some("chicago-fiction"));
    assert_eq!(ap.grammar.profile.as_deref(), Some("fiction"));
    assert_eq!(ap.grammar.enabled(), ["STYLE/SERIAL_COMMA_OFF"]);
    let numbers = ap.checks.numbers.as_ref().unwrap();
    assert_eq!(numbers.spell_out_through, 9);
    assert!(numbers.ages_as_numerals && !numbers.match_larger);
    assert_eq!(ap.checks.possessives, Some(Possessives::ApostropheOnly));
    assert_eq!(ap.consistency.preferred.dashes.as_deref(), Some("emSpaced"));
    assert_eq!(
        ap.checks.clock_times,
        Some(ClockTimeCheck::new().with_o_clock(OClock::Numerals))
    );
    // Inherited unchanged.
    assert_eq!(ap.checks.quote_punctuation, Some(QuotePunctuation::Inside));
    assert_eq!(
        ap.consistency.preferred.double_quotes.as_deref(),
        Some("curly")
    );

    let uk = StylePreset::builtin("uk").unwrap();
    assert!(uk.grammar.enabled().is_empty());
    assert_eq!(uk.grammar.spell_variant.as_deref(), Some("en_GB"));
    assert_eq!(uk.checks.quote_punctuation, Some(QuotePunctuation::Outside));
    assert_eq!(uk.consistency.preferred.dashes.as_deref(), Some("enSpaced"));
    assert_eq!(rule_names(&uk), ["Preset.Meridiem", "Preset.OK"]);
    assert_eq!(uk.checks.numbers.as_ref().unwrap().spell_out_through, 100);
    assert_eq!(
        uk.checks.clock_times,
        Some(ClockTimeCheck::new().with_o_clock(OClock::Words))
    );
}

#[test]
fn user_preset_extends_and_overrides_a_builtin() {
    let preset = StylePreset::parse(
        r#"{
            "format": 1,
            "name": "my-house",
            "version": "2024.1",
            "extends": "uk",
            "grammar": {"rules": {"PLAIN_ENGLISH": true, "STYLE/SERIAL_COMMA_ON": true, "STYLE/GONNA": false}},
            "consistency": {"minimumMinority": 2, "preferred": {"dashes": null, "ellipses": "character"}},
            "houseStyle": {
                "options": {"skipDialogue": true},
                "rules": {"Preset.OK": {"level": "error", "swap": {"o\\.k\\.": null}}},
                "vale": "name: House.Hedging\nextends: existence\nmessage: \"Consider cutting '%s'.\"\ntokens: [sort of]\n"
            },
            "checks": {"numbers": null, "familyTitles": false}
        }"#,
    )
    .unwrap();
    assert_eq!(preset.name, "my-house");
    assert_eq!(preset.version.as_deref(), Some("2024.1"));
    assert_eq!(preset.extends.as_deref(), Some("uk"));
    // Grammar: the profile and spell variant are inherited, rules merge selector by selector.
    assert_eq!(preset.grammar.profile.as_deref(), Some("fiction"));
    assert_eq!(preset.grammar.spell_variant.as_deref(), Some("en_GB"));
    assert_eq!(
        preset.grammar.enabled(),
        ["PLAIN_ENGLISH", "STYLE/SERIAL_COMMA_ON"]
    );
    assert_eq!(preset.grammar.disabled(), ["STYLE/GONNA"]);
    // Consistency: `null` removes an inherited preference.
    assert_eq!(preset.consistency.minimum_minority, 2);
    assert_eq!(preset.consistency.preferred.dashes, None);
    assert_eq!(
        preset.consistency.preferred.ellipses.as_deref(),
        Some("character")
    );
    assert_eq!(
        preset.consistency.preferred.double_quotes.as_deref(),
        Some("curly")
    );
    // House style: a rule merges key by key; the embedded Vale bundle adds its rules.
    assert!(preset.house_style.options.skip_dialogue);
    assert_eq!(
        rule_names(&preset),
        ["Preset.Meridiem", "Preset.OK", "House.Hedging"]
    );
    let ok = &preset.house_style_rules().rules()[1];
    assert_eq!(serde_json::to_value(ok.level).unwrap(), "error");
    assert_eq!(ok.swap.keys().collect::<Vec<_>>(), ["ok", "okay"]);
    // Checks: `null` and `false` turn inherited checks off.
    assert_eq!(preset.checks.numbers, None);
    assert!(!preset.checks.family_titles);
    assert_eq!(
        preset.checks.quote_punctuation,
        Some(QuotePunctuation::Outside)
    );
}

#[test]
fn a_preset_without_extends_stands_alone() {
    let preset = StylePreset::parse(
        r#"{"format": 1, "name": "bare", "checks": {"quotePunctuation": "inside", "numbers": null}}"#,
    )
    .unwrap();
    assert_eq!(preset.grammar.profile, None);
    assert!(preset.grammar.rules.is_empty());
    assert!(preset.house_style_rules().is_empty());
    assert_eq!(preset.consistency.preferred.dashes, None);
    assert_eq!(preset.checks.numbers, None);
    assert_eq!(
        preset.checks.quote_punctuation,
        Some(QuotePunctuation::Inside)
    );
}

#[test]
fn resolved_presets_round_trip() {
    for name in StylePreset::names() {
        let preset = StylePreset::builtin(name).unwrap();
        let json = preset.to_json();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value.get("extends").is_none(), "{json}");
        let again = StylePreset::parse(&json).unwrap();
        assert_eq!(again.to_json(), json);
        assert_eq!(again.house_style_rules(), preset.house_style_rules());
    }
}

#[test]
fn a_user_preset_does_not_inherit_the_builtin_name_version_or_description() {
    let error = invalid(r#"{"extends": "uk", "checks": {"numbers": null}}"#);
    assert!(error.contains("missing field `name`"), "{error}");
    let preset =
        StylePreset::parse(r#"{"name": "house", "extends": "uk", "checks": {"numbers": null}}"#)
            .unwrap();
    assert_eq!(preset.name, "house");
    assert_eq!(preset.version, None);
    assert_eq!(preset.description, None);
    assert_eq!(preset.extends.as_deref(), Some("uk"));
    assert!(invalid(r#"{"name": " ", "extends": "uk"}"#).contains("`name` must not be empty"));
}

#[test]
fn presets_are_validated_strictly() {
    for (text, expected) in [
        (
            r#"{"format": 1, "name": "x", "colour": true}"#,
            "unknown field `colour`",
        ),
        (
            r#"{"format": 1, "name": "x", "grammar": {"enable": ["X"]}}"#,
            "unknown field `enable`",
        ),
        (
            r#"{"format": 1, "name": "x", "grammar": {"rules": {"X": "on"}}}"#,
            "expected a boolean",
        ),
        (
            r#"{"format": 1, "name": "x", "grammar": {"profile": "poetry"}}"#,
            "unknown profile: poetry",
        ),
        (
            r#"{"format": 1, "name": "x", "consistency": {"preferred": {"quotes": "curly"}}}"#,
            "unknown field `quotes`",
        ),
        (
            r#"{"format": 1, "name": "x", "consistency": {"preferred": {"dashes": "emDash"}}}"#,
            "preferred.dashes must be one of",
        ),
        (
            r#"{"format": 1, "name": "x", "consistency": {"minimumMinority": 0}}"#,
            "minimumMinority",
        ),
        (
            r#"{"format": 1, "name": "x", "checks": {"numbers": {"through": 9}}}"#,
            "unknown field `through`",
        ),
        (
            r#"{"format": 1, "name": "x", "checks": {"possessives": "both"}}"#,
            "unknown variant `both`",
        ),
        (
            r#"{"format": 1, "name": "x", "houseStyle": {"rules": {"R": {"extends": "existence", "message": "m", "tokens": ["a"], "raw": ["b"]}}}}"#,
            "houseStyle: invalid houseStyle rules: rule R",
        ),
        (
            r#"{"format": 1, "name": "x", "houseStyle": {"vale": "extends: nope"}}"#,
            "houseStyle",
        ),
        (
            r#"{"format": 1, "name": "x", "houseStyle": {"skipDialogue": true}}"#,
            "unknown field `skipDialogue`",
        ),
        (r#"{"format": 2, "name": "x"}"#, "format 2 is not supported"),
        (r#"{"name": "x"}"#, "missing field `format`"),
        (r#"{"format": 1, "name": " "}"#, "`name` must not be empty"),
        (
            r#"{"format": 1, "name": "x", "extends": "mla"}"#,
            "unknown preset \"mla\"",
        ),
        (
            r#"{"format": 1, "name": "x", "extends": 3}"#,
            "`extends` must name a built-in preset",
        ),
        (r#"[1]"#, "a preset must be a JSON object"),
        ("chicago", "expected"),
    ] {
        let error = invalid(text);
        assert!(error.starts_with("invalid style preset: "), "{error}");
        assert!(error.contains(expected), "{text}: {error}");
    }
    // A duplicate rule name between `rules` and the Vale bundle is an error, not an override.
    let error = invalid(
        r#"{"format": 1, "name": "x", "extends": "chicago-fiction", "houseStyle": {"vale": "name: Preset.OK\nextends: existence\nmessage: m\ntokens: [a]\n"}}"#,
    );
    assert!(error.contains("two rules share this name"), "{error}");
}

#[test]
fn chicago_applies_to_the_grammar_checker() {
    let text = "We bought apples, pears and plums.";
    let mut rules = Rules::new(RULES_PATH).unwrap();
    assert!(!sources(&rules, text)
        .iter()
        .any(|source| source.starts_with("STYLE/SERIAL_COMMA_ON")));
    let enabled = StylePreset::default_preset()
        .apply_to_rules(&mut rules)
        .unwrap();
    assert!(enabled > 0);
    assert!(sources(&rules, text)
        .iter()
        .any(|source| source.starts_with("STYLE/SERIAL_COMMA_ON")));
    // The fiction profile came with it.
    let quotes = "She said \"hello\" to him.";
    assert!(!sources(&rules, quotes)
        .iter()
        .any(|source| source.starts_with("TYPOGRAPHY/EN_QUOTES")));
}

#[test]
fn ap_and_uk_apply_their_own_grammar_settings() {
    let mut ap = Rules::new(RULES_PATH).unwrap();
    StylePreset::builtin("ap")
        .unwrap()
        .apply_to_rules(&mut ap)
        .unwrap();
    let serial = "We bought apples, pears, and plums.";
    assert!(sources(&ap, serial)
        .iter()
        .any(|source| source.starts_with("STYLE/SERIAL_COMMA_OFF")));
    assert!(!sources(&ap, "We bought apples, pears and plums.")
        .iter()
        .any(|source| source.starts_with("STYLE/SERIAL_COMMA_ON")));

    let mut uk = Rules::new(RULES_PATH).unwrap();
    StylePreset::builtin("uk")
        .unwrap()
        .apply_to_rules(&mut uk)
        .unwrap();
    assert_eq!(
        uk.spell().options().variant.as_ref().unwrap().as_str(),
        "en_GB"
    );
    assert!(!sources(&uk, "We bought apples, pears and plums.")
        .iter()
        .any(|source| source.starts_with("STYLE/SERIAL_COMMA")));
}

#[test]
fn unknown_selectors_change_no_rules() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let text = "We bought apples, pears and plums.";
    let before = sources(&rules, text);
    for bad in [
        r#"{"format": 1, "name": "x", "grammar": {"rules": {"STYLE/SERIAL_COMMA_ON": true, "NO_SUCH_RULE": false}}}"#,
        r#"{"format": 1, "name": "x", "grammar": {"rules": {"STYLE/SERIAL_COMMA_ON": true, "BUILTIN/WHITESPACE_RULE/INDENTATION": true}}}"#,
        r#"{"format": 1, "name": "x", "grammar": {"rules": {"STYLE/SERIAL_COMMA_ON": true}, "spellVariant": "en_XX"}}"#,
    ] {
        let preset = StylePreset::parse(bad).unwrap();
        let error = preset.apply_to_rules(&mut rules).unwrap_err().to_string();
        assert!(
            error.starts_with("invalid style preset: unknown"),
            "{error}"
        );
        assert_eq!(sources(&rules, text), before);
    }
    // The case form disables one case of a builtin.
    let preset = StylePreset::parse(
        r#"{"format": 1, "name": "x", "grammar": {"rules": {"BUILTIN/WHITESPACE_RULE/INDENTATION": false}}}"#,
    )
    .unwrap();
    assert_eq!(preset.apply_to_rules(&mut rules).unwrap(), 0);
}

#[test]
fn one_call_runs_every_report_a_preset_drives() {
    let text =
        "“Mom, it’s 7 o’clock,” I said. She walked towards the door—okay—with James’ keys.\n\n\
                He called it ‘rubbish’. It was 5 AM.";
    let style = document(text);
    let chicago = style.preset(None).unwrap();
    assert_eq!(chicago.preset, "chicago-fiction");
    assert_eq!(chicago.version.as_deref(), Some("1.2.0"));
    let checks: Vec<(PresetCheck, &str)> = chicago
        .checks
        .matches
        .iter()
        .map(|found| (found.check, &text[found.span.byte.clone()]))
        .collect();
    assert_eq!(
        checks,
        [
            (PresetCheck::ClockTimes, "7"),
            (PresetCheck::Possessives, "’"),
            (PresetCheck::QuotePunctuation, "’."),
            (PresetCheck::TimeAbbreviations, "AM."),
        ]
    );
    assert_eq!(chicago.checks.matches[0].mode, Mode::Dialogue);
    let house: Vec<&str> = chicago
        .house_style
        .matches
        .iter()
        .map(|found| &text[found.span.byte.clone()])
        .collect();
    assert_eq!(house, ["towards", "okay"]);
    assert_eq!(
        chicago.consistency.dashes.preferred.as_deref(),
        Some("emClosed")
    );
    assert_eq!(chicago.consistency.dashes.non_preferred, 0);
    assert_eq!(chicago.consistency.single_quotes.non_preferred, 0);

    let uk = style.preset(Some("uk")).unwrap();
    let house: Vec<&str> = uk
        .house_style
        .matches
        .iter()
        .map(|found| &text[found.span.byte.clone()])
        .collect();
    assert_eq!(house, ["okay"]);
    assert_eq!(uk.consistency.dashes.non_preferred, 2);
    assert!(!uk
        .checks
        .matches
        .iter()
        .any(|found| found.check == PresetCheck::QuotePunctuation));

    let ap = style.preset(Some("ap")).unwrap();
    assert!(ap
        .checks
        .matches
        .iter()
        .all(|found| found.check != PresetCheck::Possessives));
    // AP sets the hour before *o'clock* as a numeral.
    assert!(ap
        .checks
        .matches
        .iter()
        .all(|found| &text[found.span.byte.clone()] != "7"));

    let json: serde_json::Value = serde_json::from_str(&to_utf16_json(&chicago)).unwrap();
    assert_eq!(
        json["checks"]["matches"][0]["span"],
        serde_json::json!({"start": 11, "end": 12})
    );
    assert_eq!(json["houseStyle"]["rules"], 3);
    assert_eq!(json["consistency"]["dashes"]["preferred"], "emClosed");
    assert_eq!(json["consistency"]["dashes"]["nonPreferred"], 0);

    let error = style.preset(Some("{\"format\": 1}")).unwrap_err();
    assert!(matches!(error, StyleError::InvalidPreset { .. }), "{error}");
}

#[test]
fn preset_reports_handle_fiction_typography() {
    let text = "“OK… 3 more,” she said—“towards the end.”\u{2028}Ross’ dad’s 4th.";
    let report = document(text).preset(Some("chicago-fiction")).unwrap();
    let checks: Vec<&str> = report
        .checks
        .matches
        .iter()
        .map(|found| &text[found.span.byte.clone()])
        .collect();
    assert_eq!(checks, ["3", "’", "4th"]);
    let house: Vec<&str> = report
        .house_style
        .matches
        .iter()
        .map(|found| &text[found.span.byte.clone()])
        .collect();
    assert_eq!(house, ["towards"]);
}

/// Prints each preset's style-report counts on the public texts:
/// `cargo test -p writemorebetter-style --test presets -- --ignored --nocapture public_counts`.
#[test]
#[ignore]
fn public_counts() {
    for preset in StylePreset::names() {
        let preset = StylePreset::builtin(preset).unwrap();
        for (name, hard_wrapped) in [
            ("story1", false),
            ("story2", false),
            ("story3", false),
            ("story4", false),
            ("story5", false),
            ("moby150k", true),
        ] {
            let text = std::fs::read_to_string(format!("../bench/public/{name}.txt")).unwrap();
            let style = StyleDocument::new(
                tokenizer().clone(),
                Arc::new(StyleResources::new()),
                &text,
                &DocumentOptions::new().with_hard_wrapped(hard_wrapped),
            );
            let report = preset.report(style.document()).unwrap();
            let mut checks = std::collections::BTreeMap::<String, usize>::new();
            for found in &report.checks.matches {
                *checks
                    .entry(
                        serde_json::to_value(found.check)
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .to_owned(),
                    )
                    .or_default() += 1;
            }
            let mut house = std::collections::BTreeMap::<&str, usize>::new();
            for found in &report.house_style.matches {
                *house.entry(found.rule.as_str()).or_default() += 1;
            }
            let consistency = &report.consistency;
            let off: usize = [
                &consistency.double_quotes,
                &consistency.single_quotes,
                &consistency.apostrophes,
                &consistency.ellipses,
                &consistency.dashes,
            ]
            .iter()
            .map(|check| check.non_preferred)
            .sum();
            println!(
                "{} {name}: checks {} {checks:?}; house {} {house:?}; non-preferred {off}",
                preset.name,
                report.checks.matches.len(),
                report.house_style.matches.len(),
            );
        }
    }
}
