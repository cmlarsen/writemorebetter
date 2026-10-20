use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::reports::{
    consistency::{self, ConsistencyCheck, ConsistencyOptions, ConsistencyReport, ConsistencyWord},
    to_utf16_json, StyleResources,
};
use writemorebetter_style::{analyze_document, DocumentOptions, StyleDocument, StyleError};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

fn run_with(text: &str, options: &DocumentOptions) -> ConsistencyReport {
    consistency::report(
        &analyze_document(tokenizer(), text, options),
        &ConsistencyOptions::new(),
    )
    .unwrap()
}

fn run(text: &str) -> ConsistencyReport {
    run_with(text, &DocumentOptions::new())
}

fn counts(check: &ConsistencyCheck) -> Vec<(&str, usize)> {
    check
        .variants
        .iter()
        .map(|v| (v.form.as_str(), v.counts.all))
        .collect()
}

fn texts<'a>(text: &'a str, check: &ConsistencyCheck, form: &str) -> Vec<&'a str> {
    check
        .variants
        .iter()
        .find(|v| v.form == form)
        .unwrap()
        .spans
        .iter()
        .map(|span| &text[span.byte.clone()])
        .collect()
}

fn word<'a>(words: &'a [ConsistencyWord], key: &str) -> Option<&'a ConsistencyWord> {
    words.iter().find(|w| w.word == key)
}

fn forms(word: &ConsistencyWord) -> Vec<(&str, usize)> {
    word.variants
        .iter()
        .map(|v| (v.form.as_str(), v.counts.all))
        .collect()
}

#[test]
fn double_quotes_mix_curly_and_straight() {
    let text = "“Go,” she said.\n\"Stay,\" he said.\n";
    let report = run(text);
    assert!(report.double_quotes.mixed);
    assert_eq!(
        counts(&report.double_quotes),
        [("curly", 2), ("straight", 2)]
    );
    assert_eq!(texts(text, &report.double_quotes, "straight"), ["\"", "\""]);
    let curly = &report.double_quotes.variants[0];
    assert_eq!(curly.counts.dialogue, 2);
    let consistent = run("“Go,” she said.\n“Stay,” he said.\n");
    assert!(!consistent.double_quotes.mixed);
}

#[test]
fn inch_and_foot_marks_are_not_quotes() {
    let report = run("He was 5'10\" tall and wore “boots.”\n");
    assert_eq!(
        counts(&report.double_quotes),
        [("curly", 2), ("straight", 0)]
    );
    assert_eq!(
        counts(&report.single_quotes),
        [("curly", 0), ("straight", 0)]
    );
    assert!(!report.apostrophes.mixed);
}

#[test]
fn apostrophes_are_counted_apart_from_single_quotes() {
    let text = "‘Don’t,’ she said. Rock ’n’ roll got ’em dancing in the ’90s.\nThe dogs’ bowls were empty. It's late.\n";
    let report = run(text);
    assert_eq!(
        counts(&report.single_quotes),
        [("curly", 2), ("straight", 0)]
    );
    assert!(!report.single_quotes.mixed);
    assert_eq!(
        texts(text, &report.apostrophes, "curly").len(),
        6,
        "Don’t, ’n’ (two), ’em, ’90s and dogs’"
    );
    assert_eq!(texts(text, &report.apostrophes, "straight"), ["'"]);
    assert!(report.apostrophes.mixed);
}

#[test]
fn straight_single_quotes_and_elisions() {
    let text = "'Come here,' she said. 'Tis the season for rock 'n' roll.\n";
    let report = run(text);
    assert_eq!(
        counts(&report.single_quotes),
        [("curly", 0), ("straight", 2)]
    );
    assert_eq!(counts(&report.apostrophes), [("curly", 0), ("straight", 3)]);
    assert!(!report.single_quotes.mixed && !report.apostrophes.mixed);
}

#[test]
fn ellipsis_forms() {
    let text = "Wait… Then... And . . . no. Mr. Smith left. Well.... Fine.\n";
    let report = run(text);
    assert!(report.ellipses.mixed);
    assert_eq!(
        counts(&report.ellipses),
        [("character", 1), ("periods", 2), ("spacedPeriods", 1)]
    );
    assert_eq!(texts(text, &report.ellipses, "spacedPeriods"), [". . ."]);
    assert_eq!(texts(text, &report.ellipses, "periods"), ["...", "...."]);
    assert!(!run("Wait… then… no.\n").ellipses.mixed);
}

#[test]
fn dash_forms_and_characters() {
    let text = "He paused—then left. She stopped — then ran. It was -- odd. Wait - what?\nPages 10–12 were well-known. “I—” he began. London – the city – slept.\n";
    let report = run(text);
    assert!(report.dashes.mixed);
    assert_eq!(
        counts(&report.dashes),
        [
            ("emClosed", 2),
            ("emSpaced", 1),
            ("enSpaced", 2),
            ("doubleHyphen", 1),
            ("spacedHyphen", 1),
        ]
    );
    assert_eq!(texts(text, &report.dashes, "doubleHyphen"), ["--"]);
    let characters: Vec<_> = report
        .dash_characters
        .iter()
        .map(|c| (c.form.as_str(), c.counts.all))
        .collect();
    assert_eq!(characters, [("hyphen", 4), ("enDash", 3), ("emDash", 3)]);
    let attributed = run("“Call me Ishmael.” —_Moby-Dick_. He paused—then left.\n");
    assert!(!attributed.dashes.mixed);
    let closed = run("He paused—then left. “I—” he began. She ran—fast.\n");
    assert!(!closed.dashes.mixed);
    assert_eq!(closed.dashes.variants[0].counts.all, 3);
    assert_eq!(closed.dashes.variants[0].counts.dialogue, 1);
}

#[test]
fn dash_beside_line_separator_is_not_spaced() {
    let text = "He paused—\u{2028}then left. She ran—fast.\n";
    let report = run(text);
    assert!(!report.dashes.mixed);
    assert_eq!(
        counts(&report.dashes)[..2],
        [("emClosed", 2), ("emSpaced", 0)]
    );
}

#[test]
fn capitalization_mixes_mid_sentence() {
    let text = "They talked of mining all night. The camp lived on Mining and little else.\n“The Captain is late,” she said. We waited for the captain.\n";
    let report = run(text);
    let mining = word(&report.capitalization, "mining").expect("mining is mixed");
    assert_eq!(forms(mining), [("Mining", 1), ("mining", 1)]);
    let captain = word(&report.capitalization, "captain").expect("captain is mixed");
    assert_eq!(forms(captain), [("Captain", 1), ("captain", 1)]);
    assert_eq!(captain.variants[0].counts.dialogue, 1);
    assert_eq!(captain.variants[1].counts.narration, 1);
    let span = &captain.variants[0].spans[0];
    assert_eq!(&text[span.byte.clone()], "Captain");
}

#[test]
fn capitalization_skips_sentence_starts_quotes_and_colons() {
    let report = run(
        "Mining was hard. They hated mining.\nHe said, “Captain, wait.” The captain did not.\nOne rule: Silence. Keep silence.\nStop! Silence fell again.\n",
    );
    assert!(
        report.capitalization.is_empty(),
        "{:?}",
        report.capitalization
    );
}

#[test]
fn capitalization_skips_names_titles_and_capitals() {
    let named = "They met Captain Ahab at dawn. Later the captain left.\nHe saw the CAPTAIN wave. Then the captain sat.\n";
    assert!(run(named).capitalization.is_empty());
    let skipped = "He gave Rose a rose. The rose wilted.\n";
    let options = DocumentOptions::new().with_skip_names(vec!["Rose".into()]);
    assert!(run_with(skipped, &options).capitalization.is_empty());
    let headed = "# The Captain Returns\n\nWe waited for the captain.\n";
    assert!(run(headed).capitalization.is_empty());
    let titled = "The Captain Returns\n\nWe waited for the captain all day.\n";
    assert!(run(titled).capitalization.is_empty());
    let emphasized = "He read _The Whale and his Captors_ twice. The whale swam.\n";
    assert!(run(emphasized).capitalization.is_empty());
    let contents = "Chapter one is called The Whiteness of the Whale. The whale was white.\n";
    assert!(run(contents).capitalization.is_empty());
    let verse = "He sang of storms, Like as the wounded whale. It was like a dream.\n";
    assert!(run(verse).capitalization.is_empty());
    let vocative = "“Yes, Captain,” she said. The captain nodded.\n";
    assert!(run(vocative).capitalization.is_empty());
    let pronoun = "Then I said I’d go. Then i left.\n";
    assert!(run(pronoun).capitalization.is_empty());
}

#[test]
fn capitalization_skips_name_homographs() {
    let report = run("We sail in May. You may go now. We met Will there. You will see.\n");
    assert!(
        word(&report.capitalization, "may").is_none(),
        "{:?}",
        report.capitalization
    );
}

#[test]
fn hyphenation_mixes_forms() {
    let text = "She sent an e-mail. Then another email came.\nWe stopped half-way. It was halfway done. We were half way there.\n";
    let report = run(text);
    let email = word(&report.hyphenation, "email").expect("email is mixed");
    assert_eq!(forms(email), [("e-mail", 1), ("email", 1)]);
    let halfway = word(&report.hyphenation, "halfway").expect("halfway is mixed");
    let mut found = forms(halfway);
    found.sort();
    assert_eq!(found, [("half way", 1), ("half-way", 1), ("halfway", 1)]);
    let open = halfway
        .variants
        .iter()
        .find(|v| v.form == "half way")
        .unwrap();
    assert_eq!(&text[open.spans[0].byte.clone()], "half way");
}

#[test]
fn hyphenation_guards() {
    // Predicative open compounds follow the attributive-hyphen convention, not a second form.
    let predicative = run("She was a well-known author. Her work was well known.\n");
    assert!(
        predicative.hyphenation.is_empty(),
        "{:?}",
        predicative.hyphenation
    );
    let attributive = run("She was a well-known author. He was a well known painter.\n");
    assert!(word(&attributive.hyphenation, "wellknown").is_some());
    // Ordinary word pairs are different words from their closed form.
    let function = run("Every one of them came. Everyone left. Maybe it may be so.\n");
    assert!(
        function.hyphenation.is_empty(),
        "{:?}",
        function.hyphenation
    );
    let unseeded = run("The news paper lay there. She read the newspaper.\n");
    assert!(word(&unseeded.hyphenation, "newspaper").is_some());
    let elisions = run("Rock ’n’ roll got ’em going. Rock ’n’ roll again.\n");
    assert!(elisions.hyphenation.is_empty() && elisions.capitalization.is_empty());
}

#[test]
fn utf16_spans_with_line_separators_and_astral_text() {
    let text = "🙂 “Wait…” she said—\u{2028}then \"stop\"... The Mining town.\nShe hated mining.\n";
    let report = run(text);
    assert!(report.double_quotes.mixed && report.ellipses.mixed);
    let json: serde_json::Value = serde_json::from_str(&to_utf16_json(&report)).unwrap();
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let straight = &json["doubleQuotes"]["variants"][1]["spans"][0];
    let (start, end) = (
        straight["start"].as_u64().unwrap() as usize,
        straight["end"].as_u64().unwrap() as usize,
    );
    assert_eq!(String::from_utf16(&utf16[start..end]).unwrap(), "\"");
    let mining = word(&report.capitalization, "mining").unwrap();
    let span = &mining.variants[0].spans[0];
    assert_eq!(&text[span.byte.clone()], "Mining");
    assert_eq!(
        text.chars()
            .skip(span.char.start)
            .take(span.char.len())
            .collect::<String>(),
        "Mining"
    );
}

#[test]
fn options_and_compact_documents() {
    let text = "“Go,” she said. \"Stay,\" he said. \"Now,\" he said.\n";
    let document = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        text,
        &DocumentOptions::new(),
    );
    assert!(document.consistency(None).unwrap().double_quotes.mixed);
    let strict = document
        .consistency(Some(r#"{"minimumMinority":3}"#))
        .unwrap();
    assert!(!strict.double_quotes.mixed);
    assert!(
        !document
            .consistency(Some(r#"{"minimum_minority":3}"#))
            .unwrap()
            .double_quotes
            .mixed
    );
    assert!(matches!(
        document.consistency(Some(r#"{"bogus":1}"#)),
        Err(StyleError::InvalidOptions { .. })
    ));
    assert!(matches!(
        document.consistency(Some(r#"{"minimumMinority":0}"#)),
        Err(StyleError::InvalidOptions { .. })
    ));
    let compact = run_with(
        "They talked of mining. The camp lived on Mining.\n",
        &DocumentOptions::new().with_token_details(false),
    );
    assert!(word(&compact.capitalization, "mining").is_some());
    let json = to_utf16_json(&document.consistency(None).unwrap());
    for key in [
        "doubleQuotes",
        "singleQuotes",
        "dashCharacters",
        "capitalization",
    ] {
        assert!(json.contains(&format!("\"{key}\"")), "{key}");
    }
    assert_eq!(
        run(text).double_quotes.variants[1].counts.dialogue,
        4,
        "straight quotes open and close two speeches"
    );
}

/// Public measurement recorded in docs/STYLE-CONSISTENCY.md:
/// `cargo test -p writemorebetter-style --test consistency -- --ignored --nocapture`.
#[test]
#[ignore]
fn public_counts() {
    for (name, hard_wrapped) in [
        ("story1", false),
        ("story2", false),
        ("story3", false),
        ("story4", false),
        ("story5", false),
        ("moby150k", true),
    ] {
        let text = std::fs::read_to_string(format!("../bench/public/{name}.txt")).unwrap();
        let report = run_with(
            &text,
            &DocumentOptions::new().with_hard_wrapped(hard_wrapped),
        );
        let checks = [
            ("doubleQuotes", &report.double_quotes),
            ("singleQuotes", &report.single_quotes),
            ("apostrophes", &report.apostrophes),
            ("ellipses", &report.ellipses),
            ("dashes", &report.dashes),
        ];
        let mixed: Vec<_> = checks
            .iter()
            .filter(|(_, check)| check.mixed)
            .map(|(name, _)| *name)
            .collect();
        println!(
            "{name}: mixed checks {mixed:?}; capitalization {}; hyphenation {}",
            report.capitalization.len(),
            report.hyphenation.len()
        );
        for (check, value) in checks {
            println!("  {check}: {:?}", counts(value));
        }
        for (kind, words) in [
            ("capitalization", &report.capitalization),
            ("hyphenation", &report.hyphenation),
        ] {
            for word in words {
                println!("  {kind} {}: {:?}", word.word, forms(word));
                for span in &word.variants.last().unwrap().spans {
                    let start = text.floor_char_boundary(span.byte.start.saturating_sub(40));
                    let end = text.ceil_char_boundary((span.byte.end + 20).min(text.len()));
                    println!("      {:?}", &text[start..end]);
                }
            }
        }
    }
}
