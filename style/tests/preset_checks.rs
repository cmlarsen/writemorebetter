use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::reports::preset_checks::{
    self, ClockTimeCheck, NumberCheck, OClock, Possessives, PresetCheck, PresetCheckMatch,
    PresetChecks, QuotePunctuation, TimeAbbreviations,
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

fn clock(o_clock: OClock) -> PresetChecks {
    PresetChecks::new().with_clock_times(Some(ClockTimeCheck::new().with_o_clock(o_clock)))
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
fn centuries_get_their_own_message() {
    let text =
        "The 19th-century ruins—“built in the 3rd century,” he said—outlived the 20th\u{2028}\
                centuries’ ends… The 4th century-old door. The 5th time.";
    let found = run(text, &numbers(100));
    assert_eq!(texts(text, &found), ["19th", "3rd", "20th", "4th", "5th"]);
    let century = "Spell out centuries in prose, as in “the nineteenth century”.";
    assert!(found[..4].iter().all(|m| m.message == century));
    assert_eq!(
        found[4].message,
        "Spell out whole numbers from zero through 100 in prose."
    );
    // News style spells out only one through nine, so the 19th century stays a numeral.
    assert!(run("The 19th century.", &numbers(9)).is_empty());
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
fn numbers_listed_after_labels_and_dates_are_labels_and_dates() {
    let text = "Read chapters 3 and 4 tonight. See pages 5 and 6. On May 5 and 6 we met. \
                On May 5, 6, and 7 we met. Rooms 12 and 14 were empty. Pages 5 to 9— \
                verses 10 through 12… AD 70 or 80.\u{2028}“Chapters 3 & 4,” she said.";
    let found = run(text, &numbers(100));
    assert!(found.is_empty(), "{:?}", texts(text, &found));
    // A comma alone carries the date only into a list; after it, a plain number still counts.
    let text = "On May 5, 3 people came. Page 5. And 6 more.";
    assert_eq!(texts(text, &run(text, &numbers(100))), ["3", "6"]);
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
fn apostrophe_only_skips_contracted_s() {
    let text = "Chris’s going home. James’s here now. Ross's been gone a year.\u{2028}\
                “Jess’s not coming—” Thomas’s gone… Chris’s car.";
    let found = run(text, &possessives(Possessives::ApostropheOnly));
    assert_eq!(texts(text, &found), ["’s"]);
    assert_eq!(found[0].span.byte.start, text.rfind("’s car").unwrap());
}

#[test]
fn possessives_without_token_readings_skip_sentence_start() {
    let text = "Parents’ evening was long. Dogs’ tails wag. “Students’ desks were bare.”\u{2028}\
                Then James’ car—gone…";
    let compact = DocumentOptions::new().with_token_details(false);
    let found = run_with(text, &possessives(Possessives::ApostropheS), &compact);
    assert_eq!(texts(text, &found), ["’"]);
    assert_eq!(found[0].span.byte.start, text.find("’ car").unwrap());
    // With readings, the plurals are told apart by their tags.
    let found = run(text, &possessives(Possessives::ApostropheS));
    assert_eq!(texts(text, &found), ["’"]);
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
    // Mid-sentence both periods go; a final one that ends the sentence stays.
    assert_eq!(texts(undotted, &found), ["a.m.", "P.M"]);
    assert_eq!(replacements(&found), [Some("am"), Some("pm")]);
}

#[test]
fn undotted_times_keep_only_a_sentence_ending_period() {
    let text = "“At 9 a.m.” She left at 5 p.m.\u{2028}Then 6 a.m.—no, 7 a.m.… at 8 am. on Friday.";
    let found = run(text, &times(TimeAbbreviations::Undotted));
    assert_eq!(texts(text, &found), ["a.m", "p.m", "a.m.", "a.m."]);
    assert!(found
        .iter()
        .all(|m| m.replacement.as_deref() == Some("am") || m.replacement.as_deref() == Some("pm")));
}

#[test]
fn spelled_out_hours_with_a_meridiem() {
    let text =
        "“Be here at 6:00 a.m.” She meant eight a.m.—not six thirty p.m.… At ten forty-five \
                PM, then eight-thirty a.m.\u{2028}It was Ten am. Meet at twelve pm or twelve a.m.";
    let found = run(text, &clock(OClock::Words));
    assert_eq!(
        texts(text, &found),
        [
            "eight a.m.",
            "six thirty p.m.",
            "ten forty-five PM",
            "eight-thirty a.m.",
            "Ten am",
            "twelve pm",
            "twelve a.m."
        ]
    );
    assert!(found.iter().all(|m| m.check == PresetCheck::ClockTimes));
    assert!(found.iter().all(|m| m.replacement.is_none()));
    assert_eq!(
        found[0].message,
        "Use numerals with “a.m.”, or spell the time out with “in the morning” instead."
    );
    assert!(found[1].message.contains("“in the evening”"));
    assert!(found[5].message.contains("“noon”"));
    assert!(found[6].message.contains("“midnight”"));
    assert_eq!(found[0].mode, Mode::Narration);
}

#[test]
fn spelled_out_hours_skip_verbs_and_other_words() {
    // `am` before a word is the verb or a radio band; a number word that is not an hour, or a
    // line break before the abbreviation, is not a time.
    let text =
        "Which one am I? One AM station played. Twenty a.m. Five hundred a.m. Eight\u{2028}a.m.";
    assert!(run(text, &clock(OClock::Words)).is_empty());
}

#[test]
fn o_clock_hours() {
    let text =
        "We left at 6 o'clock, or 7 o’clock—or eleven o’clock. At 7:00 o'clock. At 6 o’clock a.m.";
    let words = run(text, &clock(OClock::Words));
    assert_eq!(texts(text, &words), ["6", "7", "6", "o’clock a.m."]);
    assert_eq!(words[0].message, "Spell out the hour before “o’clock”.");
    assert_eq!(words[3].message, "Use “o’clock” or “a.m.”, not both.");
    let numerals = run(text, &clock(OClock::Numerals));
    assert_eq!(texts(text, &numerals), ["eleven", "o’clock a.m."]);
    let either = run(text, &clock(OClock::Either));
    assert_eq!(texts(text, &either), ["o’clock a.m."]);
    // Not hours: no clock-time match.
    let text = "At 0 o'clock, 13 o’clock or 99 o'clock.";
    assert!(run(text, &clock(OClock::Words)).is_empty());
}

#[test]
fn doubled_time_words_after_a_closed_up_numeral() {
    let text = "Call at 6pm tonight, or 7AM in the morning…";
    let found = run(text, &clock(OClock::Words));
    assert_eq!(texts(text, &found), ["pm tonight", "AM in the morning"]);
}

#[test]
fn numbers_leave_o_clock_to_the_clock_check() {
    let text = "We left at 6 o'clock with 3 bags.";
    assert_eq!(texts(text, &run(text, &numbers(100))), ["6", "3"]);
    let both =
        numbers(100).with_clock_times(Some(ClockTimeCheck::new().with_o_clock(OClock::Either)));
    assert_eq!(texts(text, &run(text, &both)), ["3"]);
}

#[test]
fn doubled_time_words() {
    let text = "At 5 p.m. in the evening, and 6 a.m. this morning. “At 10 PM tonight,” she said. \
                At 2 a.m. in the afternoon. At 9 a.m. This morning I left. At 5 pm, in the evening \
                light. At 11 p.m. at night.\u{2028}At eight a.m. in the morning…";
    let found = run(text, &clock(OClock::Words));
    assert_eq!(
        texts(text, &found),
        [
            "p.m. in the evening",
            "a.m. this morning",
            "PM tonight",
            "p.m. at night",
            "eight a.m.",
            "a.m. in the morning"
        ]
    );
    assert_eq!(
        found[0].message,
        "“p.m.” and “in the evening” say the same thing; keep one."
    );
    assert_eq!(found[2].mode, Mode::Dialogue);
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
fn family_titles_listed_after_a_determiner_share_it() {
    let text = "My mom, dad, and I went home. Her mom, dad and sister came. \
                My sister, dad and I ate. The kids' mom waved. The kids’ mom waved.\u{2028}\
                “Our mom, dad—” Their mom, dad…";
    let found = run(text, &family());
    assert!(found.is_empty(), "{:?}", texts(text, &found));
    // A list without a determiner, a vocative and a straight opening quote still read as names.
    let text = "Mom, dad and I went. Thanks, dad. He said, 'dad, wait.'";
    assert_eq!(texts(text, &run(text, &family())), ["dad", "dad", "dad"]);
    let compact = DocumentOptions::new().with_token_details(false);
    let text = "My mom, dad, and I went home. The kids' mom waved.";
    assert!(run_with(text, &family(), &compact).is_empty());
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
            r#"{"numbers": {"spellOutThrough": 9}, "time_abbreviations": "dotted", "clockTimes": {}}"#,
        ))
        .unwrap();
    assert_eq!(report.matches.len(), 2);
    for bad in [
        r#"{"numbers": {"spellOutThru": 9}}"#,
        r#"{"quotes": "inside"}"#,
        r#"{"quotePunctuation": "sideways"}"#,
        r#"{"numbers": {"spellOutThrough": 5000}}"#,
        r#"{"clockTimes": {"oClock": "roman"}}"#,
        r#"{"clockTimes": true}"#,
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
