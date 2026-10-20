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

#[test]
fn bellow_below_dialogue_and_unicode() {
    let id = HarperConfusion::BellowBelow;
    assert_eq!(
        flags(id, "“It’s bellow the stairs,” she said—quietly…\u{2028}"),
        vec![("bellow".into(), "below".into())]
    );
    assert!(flags(id, "“Bellow!” he shouted—then paused…\u{2028}").is_empty());
    assert_eq!(
        flags(id, "“Bellow the stairs,” she said—then pointed…\u{2028}"),
        vec![("Bellow".into(), "Below".into())]
    );
}

#[test]
fn feel_fell_dialogue_and_unicode() {
    let id = HarperConfusion::FeelFell;
    assert_eq!(
        flags(id, "“I didn’t fell good,” she said—then left…\u{2028}"),
        vec![("fell".into(), "feel".into())]
    );
    assert!(flags(id, "“She fell,” he said—then looked away…\u{2028}").is_empty());
    assert!(flags(id, "“She fell like a stone,” he said—then blinked…\u{2028}").is_empty());
}

#[test]
fn selected_confusions_are_enabled_in_the_rules_binary() {
    let rules = Rules::new("../storage/en_rules.bin").unwrap();
    for rule in [
        HarperConfusion::WaistWaste,
        HarperConfusion::BellowBelow,
        HarperConfusion::FeelFell,
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
