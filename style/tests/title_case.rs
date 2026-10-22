use writemorebetter_style::preset::StylePreset;
use writemorebetter_style::reports::title_case::{
    self, TitleCaseOptions, TitleCaseRule, TitleCaseStyle,
};
use writemorebetter_style::StyleError;

fn flags(style: TitleCaseStyle, heading: &str) -> Vec<(String, String, TitleCaseRule)> {
    title_case::report(&[heading], &TitleCaseOptions::new(style))
        .unwrap()
        .matches
        .into_iter()
        .map(|m| (m.word, m.replacement, m.rule))
        .collect()
}

fn words(style: TitleCaseStyle, heading: &str) -> Vec<String> {
    flags(style, heading).into_iter().map(|f| f.0).collect()
}

use TitleCaseRule::*;
use TitleCaseStyle::*;

#[test]
fn correct_headings_pass() {
    for heading in [
        "The Long Way Home",
        "Gone with the Wind",
        "A Tale of Two Cities",
        "Of Mice and Men",
        "What It Comes Down To",
        "Death in Venice",
        "The Girl Who Lived",
        "Why We Run: A Story of the Sea",
        "Journey through the Night",
        "Looking Up",
        "Before You Go",
        "Is It Love?",
        "Under the Dome",
        "Chapter 12: The Return",
        "So It Goes",
        "Rock ’n’ Roll Heaven",
        "Up-to-Date Maps",
        "My Mother-in-Law",
        "The Anti-intellectual",
        "Twenty-One Days",
        "The FBI Files",
        "McCall’s Last Stand",
        "Plan B",
        "3rd Time Lucky",
        "CHAPTER ONE",
    ] {
        assert!(
            words(Chicago, heading).is_empty(),
            "{heading}: {:?}",
            flags(Chicago, heading)
        );
    }
}

#[test]
fn chicago_lowercases_minor_words() {
    assert_eq!(
        flags(Chicago, "Gone With The Wind"),
        vec![
            ("With".into(), "with".into(), MinorWord),
            ("The".into(), "the".into(), MinorWord),
        ]
    );
    assert_eq!(
        words(Chicago, "A Tale Of Two Cities And A Dog"),
        ["Of", "And", "A"]
    );
    // Prepositions of any length; `to` and `as` always.
    assert_eq!(
        words(Chicago, "Walking Between The Worlds"),
        ["Between", "The"]
    );
    assert_eq!(
        words(Chicago, "Born To Run As Fast As Light"),
        ["To", "As", "As"]
    );
    // An ambiguous word is lowercase only before its object.
    assert_eq!(words(Chicago, "Journey Through the Night"), ["Through"]);
    assert!(words(Chicago, "Looking Up at Stars").is_empty());
    assert!(words(Chicago, "Looking up at Stars").is_empty());
    // Documented limits: a particle before its object is flagged, a preposition before a bare
    // noun is not, and `on` is always a preposition.
    assert_eq!(words(Chicago, "Turn Off the Light"), ["Off"]);
    assert!(words(Chicago, "Journey Through Time").is_empty());
    assert_eq!(words(Chicago, "Hold On Tight"), ["On"]);
}

#[test]
fn chicago_capitalizes_major_words() {
    assert_eq!(
        flags(Chicago, "the long way home"),
        vec![
            ("the".into(), "The".into(), FirstWord),
            ("long".into(), "Long".into(), MajorWord),
            ("way".into(), "Way".into(), MajorWord),
            ("home".into(), "Home".into(), LastWord),
        ]
    );
    // Short verbs, pronouns and subordinating conjunctions are major.
    assert_eq!(
        words(Chicago, "What it is if we go"),
        ["it", "is", "if", "we", "go"]
    );
    // `yet` and `so` are capitalized in Chicago style.
    assert_eq!(words(Chicago, "Not yet Home"), ["yet"]);
}

#[test]
fn first_last_and_after_colon() {
    assert_eq!(
        flags(Chicago, "of Mice and Men"),
        vec![("of".into(), "Of".into(), FirstWord)]
    );
    assert_eq!(
        flags(Chicago, "Where It Comes from"),
        vec![("from".into(), "From".into(), LastWord)]
    );
    // The last word before a colon and the first after it are capitalized.
    assert_eq!(
        flags(Chicago, "What We Fight for: a Memoir"),
        vec![
            ("for".into(), "For".into(), LastWord),
            ("a".into(), "A".into(), AfterColon),
        ]
    );
    assert_eq!(words(Chicago, "Who Goes There? the Return"), ["the"]);
}

#[test]
fn hyphenated_compounds() {
    assert_eq!(words(Chicago, "Up-To-Date Maps"), ["To"]);
    assert_eq!(words(Chicago, "My Mother-In-Law"), ["In"]);
    assert_eq!(words(Chicago, "The Self-sustaining City"), ["sustaining"]);
    assert_eq!(words(Chicago, "twenty-one Days"), ["twenty", "one"]);
    // After a prefix that cannot stand alone, either case passes.
    assert!(words(Chicago, "The Anti-Intellectual").is_empty());
    assert!(words(Chicago, "A Pre-war Story").is_empty());
    // The last part of the last word is the heading's last word.
    assert_eq!(words(Chicago, "The Long Run-in"), ["in"]);
    assert!(words(Chicago, "The Long Run-In").is_empty());
    assert!(words(Chicago, "Pre-War").is_empty());
    // Non-breaking and Unicode hyphens join too.
    assert_eq!(words(Chicago, "Up\u{2011}To\u{2010}Date"), ["To"]);
}

#[test]
fn ap_capitalizes_long_prepositions() {
    assert_eq!(words(Ap, "Gone with the Wind"), ["with"]);
    assert!(words(Ap, "Gone With the Wind").is_empty());
    assert_eq!(words(Ap, "Journey Through the Night"), Vec::<String>::new());
    assert_eq!(words(Ap, "Journey through the Night"), ["through"]);
    assert_eq!(words(Ap, "A Walk In The Park"), ["In", "The"]);
    // Short conjunctions, including `so` and `yet`, are lowercase.
    assert_eq!(words(Ap, "Not Yet Home So Far"), ["Yet", "So"]);
    assert_eq!(words(Ap, "Going Up the Hill"), ["Up"]);
}

#[test]
fn sentence_case() {
    assert!(words(Sentence, "The long way home").is_empty());
    assert!(words(Sentence, "Chapter 3: the long way home").is_empty());
    assert!(words(Sentence, "Chapter 3: The long way home").is_empty());
    assert_eq!(
        flags(Sentence, "The Long Way home"),
        vec![
            ("Long".into(), "long".into(), SentenceCase),
            ("Way".into(), "way".into(), SentenceCase),
        ]
    );
    assert_eq!(
        flags(Sentence, "the night"),
        vec![("the".into(), "The".into(), FirstWord)]
    );
    // Contractions of `I` keep their capital too, with straight or curly apostrophes.
    for heading in [
        "Where I'm going",
        "Where I’ve been",
        "Where I'd go",
        "What I’ll say…",
        "Where I’m going—now",
    ] {
        assert!(words(Sentence, heading).is_empty(), "{heading}");
    }
    // `I`, acronyms and names given in `skipNames` keep their capitals.
    let options =
        TitleCaseOptions::new(Sentence).with_skip_names(vec!["Mara Vale".into(), "London".into()]);
    let report = title_case::report(
        &[
            "When I met Mara in London",
            "Mara’s FBI file",
            "Back To London",
        ],
        &options,
    )
    .unwrap();
    let found: Vec<_> = report
        .matches
        .iter()
        .map(|m| (m.heading, m.word.as_str()))
        .collect();
    assert_eq!(found, [(2, "To")]);
    assert_eq!(report.headings, 3);
    assert_eq!(report.flagged_headings, 1);
}

#[test]
fn utf8_headings_and_spans() {
    // Curly quotes and apostrophes, em dashes, ellipses and U+2028 separate words; spans are
    // byte, char and UTF-16 offsets into the heading.
    let heading = "“Don’t Look Back”—a Girl … From\u{2028}the 🌊 Sea";
    let report = title_case::report(&[heading], &TitleCaseOptions::new(Chicago)).unwrap();
    let found: Vec<_> = report
        .matches
        .iter()
        .map(|m| (m.word.as_str(), m.rule))
        .collect();
    assert_eq!(found, [("From", MinorWord)]);
    let from = &report.matches[0].span;
    assert_eq!(&heading[from.byte.clone()], "From");
    let chars: Vec<char> = heading.chars().collect();
    assert_eq!(chars[from.char.clone()].iter().collect::<String>(), "From");
    let utf16: Vec<u16> = heading.encode_utf16().collect();
    assert_eq!(
        String::from_utf16(&utf16[from.utf16.clone()]).unwrap(),
        "From"
    );
    // Spans after an emoji differ between chars and UTF-16 units.
    let sea = title_case::report(&["the 🌊 sea"], &TitleCaseOptions::new(Chicago)).unwrap();
    let last = sea.matches.last().unwrap();
    assert_eq!(last.word, "sea");
    assert_eq!(last.span.char, 6..9);
    assert_eq!(last.span.utf16, 7..10);
    // Curly and straight apostrophes inside a word are part of it.
    assert_eq!(words(Chicago, "Where’s the Light"), Vec::<String>::new());
    assert_eq!(words(Chicago, "where's the light"), ["where's", "light"]);
    assert_eq!(flags(Chicago, "‘tis the Season")[0].1, "Tis");
    // An ellipsis or dash at the end does not hide the last word.
    assert!(words(Chicago, "Waiting For…").is_empty());
    assert!(words(Chicago, "Waiting For—").is_empty());
}

#[test]
fn options_are_strict_and_json() {
    let options: TitleCaseOptions =
        serde_json::from_str(r#"{"style":"ap","skipNames":["Mara"]}"#).unwrap();
    assert_eq!(options.style, Ap);
    assert!(serde_json::from_str::<TitleCaseOptions>(r#"{"style":"apa"}"#).is_err());
    assert!(serde_json::from_str::<TitleCaseOptions>(r#"{"case":"ap"}"#).is_err());
    assert!(serde_json::from_str::<TitleCaseOptions>(r#"{"skip_names":[]}"#).is_ok());
    assert!(matches!(
        title_case::report(
            &["A"],
            &TitleCaseOptions::new(Chicago).with_skip_names(vec![" ".into()])
        ),
        Err(StyleError::InvalidOptions { .. })
    ));
    let report = title_case::report(&["gone With the wind"], &TitleCaseOptions::default()).unwrap();
    let json = writemorebetter_style::reports::to_utf16_json(&report);
    assert!(
        json.starts_with(r#"{"style":"chicago","headings":1,"flaggedHeadings":1,"matches":[{"heading":0,"span":{"start":0,"end":4},"word":"gone","replacement":"Gone","rule":"firstWord"}"#),
        "{json}"
    );
}

#[test]
fn presets_choose_a_style() {
    let headings = ["Gone With the Wind", "The long way home"];
    let chicago = StylePreset::builtin("chicago-fiction").unwrap();
    assert_eq!(chicago.title_case, Some(Chicago));
    let report = chicago.title_case(&headings, vec![]).unwrap();
    assert_eq!(report.style, Chicago);
    let words: Vec<_> = report.matches.iter().map(|m| m.word.as_str()).collect();
    assert_eq!(words, ["With", "long", "way", "home"]);
    let ap = StylePreset::builtin("ap").unwrap();
    let words: Vec<_> = ap
        .title_case(&headings, vec![])
        .unwrap()
        .matches
        .into_iter()
        .map(|m| m.word)
        .collect();
    assert_eq!(words, ["long", "way", "home"]);
    let uk = StylePreset::builtin("uk").unwrap();
    let report = uk.title_case(&headings, vec!["Wind".into()]).unwrap();
    assert_eq!(report.style, Sentence);
    let words: Vec<_> = report.matches.iter().map(|m| m.word.as_str()).collect();
    assert_eq!(words, ["With"]);
    // A user preset can switch style or drop it (Chicago is the fallback).
    let custom =
        StylePreset::parse(r#"{"format":1,"name":"mine","extends":"uk","titleCase":"ap"}"#)
            .unwrap();
    assert_eq!(custom.title_case, Some(Ap));
    let unset = StylePreset::parse(r#"{"format":1,"name":"mine","extends":"uk","titleCase":null}"#)
        .unwrap();
    assert_eq!(unset.title_case, None);
    assert_eq!(unset.title_case(&headings, vec![]).unwrap().style, Chicago);
    assert!(StylePreset::parse(r#"{"format":1,"name":"x","titleCase":"apa"}"#).is_err());
    // Round trip through the self-contained form.
    let again = StylePreset::parse(&ap.to_json()).unwrap();
    assert_eq!(again.title_case, Some(Ap));
}
