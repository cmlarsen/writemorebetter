use nlprule::{builtin::en::HarperConfusion, Rules, Tokenizer};
use std::sync::OnceLock;

static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();

fn flags(rule: HarperConfusion, text: &str) -> Vec<(String, String)> {
    let tokenizer =
        TOKENIZER.get_or_init(|| Tokenizer::new("../storage/en_tokenizer.bin").unwrap());
    let chars: Vec<char> = text.chars().collect();
    tokenizer
        .pipe(text)
        .flat_map(|sentence| rule.check(&sentence))
        .map(|s| {
            (
                chars[s.span().char().clone()].iter().collect(),
                s.replacements()[0].clone(),
            )
        })
        .collect()
}

fn none(rule: HarperConfusion, texts: &[&str]) {
    for text in texts {
        assert!(
            flags(rule, text).is_empty(),
            "{} flagged {:?}",
            rule.id(),
            text
        );
    }
}

#[test]
fn waist_waste_dialogue_and_unicode() {
    let id = HarperConfusion::WaistWaste;
    assert_eq!(
        flags(
            id,
            "“That’s a huge waist of time,” she said—then sighed…\u{2028}"
        ),
        vec![("waist".into(), "waste".into())]
    );
    assert!(flags(
        id,
        "“Her waist of thirty inches,” she said…\u{2028}Then she left."
    )
    .is_empty());
    assert_eq!(
        flags(
            id,
            "“It’s a waist of your time,” she said—then left…\u{2028}"
        ),
        vec![("waist".into(), "waste".into())]
    );
}

/// Harper's `<adj> waist of` and `<be> a waist of` orders.
#[test]
fn waist_waste_adjective_and_lead_orders() {
    let id = HarperConfusion::WaistWaste;
    for text in [
        "It was a complete waist of energy.",
        "“What a total waist of a day,” she said—then laughed…",
        "It’s just a waist of breath…\u{2028}",
    ] {
        assert_eq!(
            flags(id, text),
            vec![("waist".into(), "waste".into())],
            "{text}"
        );
    }
}

#[test]
fn waist_waste_skips_compound_nouns() {
    none(
        HarperConfusion::WaistWaste,
        &[
            "She cinched the waist of her money belt.",
            "He tightened the waist of his space suit—hard…\u{2028}",
            "“The waist of my time machine,” he said.",
            "That’s a huge waist\u{2028}of time.",
        ],
    );
}

#[test]
fn bellow_below_dialogue_and_unicode() {
    let id = HarperConfusion::BellowBelow;
    assert_eq!(
        flags(id, "“It’s bellow the stairs,” she said—quietly…\u{2028}"),
        vec![("bellow".into(), "below".into())]
    );
    assert!(flags(id, "“Bellow!” he shouted—then paused…\u{2028}").is_empty());
    assert_eq!(
        flags(id, "The ship sank bellow the waves—slowly…"),
        vec![("bellow".into(), "below".into())]
    );
    assert_eq!(
        flags(id, "“Right bellow the window,” she whispered."),
        vec![("bellow".into(), "below".into())]
    );
}

#[test]
fn bellow_below_skips_transitive_bellow() {
    none(
        HarperConfusion::BellowBelow,
        &[
            "He began to bellow the orders.",
            "“Bellow the order again!” the captain roared.",
            "“Bellow the stairs,” she said—then pointed…",
            "I’ll bellow your name from the rooftops.",
            "The bull would bellow a warning before it charged.",
            "They heard him bellow that the ship was sinking.",
            "“Don’t bellow those words at me…”",
            "The ship sank bellow\u{2028}the waves.",
        ],
    );
}

#[test]
fn feel_fell_dialogue_and_unicode() {
    let id = HarperConfusion::FeelFell;
    assert_eq!(
        flags(id, "“I didn’t fell well,” she said—then left…\u{2028}"),
        vec![("fell".into(), "feel".into())]
    );
    assert_eq!(
        flags(id, "“Please fell free to sit,” he said—then waited…"),
        vec![("fell".into(), "feel".into())]
    );
    assert_eq!(
        flags(id, "“Fell free to ask,” she said."),
        vec![("Fell".into(), "Feel".into())]
    );
    assert_eq!(
        flags(id, "You’ll fell better soon…"),
        vec![("fell".into(), "feel".into())]
    );
    assert!(flags(id, "“She fell,” he said—then looked away…\u{2028}").is_empty());
    assert!(flags(id, "“She fell like a stone,” he said—then blinked…\u{2028}").is_empty());
}

#[test]
fn feel_fell_skips_fall_and_fell_in_narration() {
    none(
        HarperConfusion::FeelFell,
        &[
            "As night fell we made camp.",
            "As darkness fell I crept out of the barn.",
            "When the dark fell you could hear the wolves.",
            "Her hair fell free of its pins.",
            "The rope fell free and the bucket dropped.",
            "Her hair fell free to her waist.",
            "Snow fell that night—soft, endless…",
            "The silence that fell that afternoon was total.",
            "The oak was rotten, so we had to fell it.",
            "Rain fell good and hard on the roof.",
            "“They didn’t fell the oak,” he said.",
            "The night fell\u{2028}We waited.",
            "The tide fell\u{2028}that was all.",
            "“Please don’t fell\u{2028}good trees.”",
        ],
    );
}

#[test]
fn quite_quiet_dialogue_and_unicode() {
    let id = HarperConfusion::QuiteQuiet;
    for (text, from, to) in [
        (
            "“It’s very quite here at night,” she said—then shivered…",
            "quite",
            "quiet",
        ),
        (
            "“I couldn’t quiet understand,” he said—then shrugged…\u{2028}",
            "quiet",
            "quite",
        ),
        ("It won’t quiet fit.", "quiet", "quite"),
        ("“That was quiet remarkable…”", "quiet", "quite"),
        ("I find it quiet dumb.", "quiet", "quite"),
        ("The house was eerily quite.", "quite", "quiet"),
    ] {
        assert_eq!(flags(id, text), vec![(from.into(), to.into())], "{text}");
    }
}

#[test]
fn quite_quiet_skips_fiction_quiet() {
    none(
        HarperConfusion::QuiteQuiet,
        &[
            "“Won’t you quiet down?” she asked.",
            "He couldn’t quiet the baby—not with that noise…",
            "She couldn’t quiet her mind.",
            "I couldn’t quiet them.",
            "They couldn’t quiet things down.",
            "The room fell quiet after the announcement.",
            "She kept quiet about what happened.",
            "The room was quiet enough.",
            "It’s quiet there.",
            "Be quiet now.",
            "It was quiet again.",
            "The night was quiet, cold and still.",
            "The house was quiet dark\u{2028}tonight.",
            "It was quiet\u{2028}remarkable.",
            "It’s actually quite smart.",
            "It’s really quite smart.",
            "I’m quite happy with the result.",
            "But it’s not quite clear.",
            "You’d better try and sit quiet till morning.",
            "The quiet lights in the houses.",
            "She was quiet but firm.",
            "It was quiet outside.",
            "They kept it quiet until morning.",
            "If I go quiet for a week… yeah I’m dead.",
            "He went quiet about the whole thing.",
            "Please stay quiet during the presentation.",
            "She remained quiet in meetings.",
            "He grew quiet on the matter.",
            "Be quiet for a moment.",
            "Stay quiet and listen.",
            "Quiet down, everyone.",
            "It’s been quiet lately.",
            "It’s probably quite doable.",
            "The project is quite large.",
            "The Sneetches got really quite smart on that day.",
        ],
    );
}

#[test]
fn selected_confusions_are_enabled_in_the_rules_binary() {
    let rules = Rules::new("../storage/en_rules.bin").unwrap();
    for rule in [
        HarperConfusion::WaistWaste,
        HarperConfusion::BellowBelow,
        HarperConfusion::FeelFell,
        HarperConfusion::QuiteQuiet,
    ] {
        assert!(
            rules
                .builtins()
                .iter()
                .any(|registered| registered.builtin().id() == rule.id() && registered.enabled()),
            "{} is missing or disabled",
            rule.id()
        );
    }
}

/// Run with `cargo test -p nlprule --test harper_confusion -- --ignored --nocapture`.
#[test]
#[ignore]
fn public_fiction_flag_counts() {
    let tokenizer =
        TOKENIZER.get_or_init(|| Tokenizer::new("../storage/en_tokenizer.bin").unwrap());
    for rule in [
        HarperConfusion::WaistWaste,
        HarperConfusion::BellowBelow,
        HarperConfusion::FeelFell,
        HarperConfusion::QuiteQuiet,
    ] {
        let mut total = 0;
        for name in ["story1", "story2", "story3", "story4", "story5", "moby150k"] {
            let text = std::fs::read_to_string(format!("../bench/public/{name}.txt")).unwrap();
            let chars: Vec<char> = text.chars().collect();
            let mut count = 0;
            for sentence in tokenizer.pipe(&text) {
                for suggestion in rule.check(&sentence) {
                    let span = suggestion.span().char();
                    let start = span.start.saturating_sub(40);
                    let end = (span.end + 40).min(chars.len());
                    let context: String = chars[start..end].iter().collect();
                    println!("{} {} {:?}", rule.id(), name, context.replace('\n', " "));
                    count += 1;
                }
            }
            println!("{} {}: {}", rule.id(), name, count);
            total += count;
        }
        println!("{} TOTAL: {}", rule.id(), total);
    }
}
