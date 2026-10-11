use nlprule::{Rules, Tokenizer};
use std::time::Instant;
fn rss() -> f64 {
    let o = std::process::Command::new("ps").args(["-o","rss=","-p",&std::process::id().to_string()]).output().unwrap();
    String::from_utf8_lossy(&o.stdout).trim().parse::<f64>().unwrap()/1024.0
}
fn main() {
    let d = std::env::args().nth(1).unwrap();
    println!("baseline {:.0} MB", rss());
    let t = Instant::now();
    let tok = Tokenizer::new(format!("{}/en_tokenizer.bin", d)).unwrap();
    println!("tokenizer loaded {:.0} MB in {:.2?}", rss(), t.elapsed());
    let t = Instant::now();
    let mut rules = Rules::new(format!("{}/en_rules.bin", d)).unwrap();
    println!("rules loaded {:.0} MB in {:.2?}", rss(), t.elapsed());
    let profile_name = std::env::var("PROFILE").unwrap_or_else(|_| "default".into());
    let profile = nlprule::profile::Profile::by_name(&profile_name)
        .unwrap_or_else(|| panic!("unknown profile: {}", profile_name));
    let enabled = |rules: &Rules| {
        rules.rules().iter().filter(|r| r.enabled()).count()
            + rules.builtins().iter().filter(|r| r.enabled()).count()
    };
    let before = enabled(&rules);
    let unmatched = rules.apply_profile(&profile);
    let after = enabled(&rules);
    println!("profile {}: {} rules disabled", profile.name, before - after);
    if !unmatched.is_empty() {
        eprintln!("profile {}: unmatched selectors: {:?}", profile.name, unmatched);
    }
    let variant = std::env::var("SPELL").unwrap_or_else(|_| "en_US".into());
    if !variant.is_empty() {
        let t = Instant::now();
        rules.spell_mut().options_mut().variant = Some(rules.spell().variant(&variant).unwrap());
        println!("speller {} enabled {:.0} MB in {:.2?}", variant, rss(), t.elapsed());
    }
    let text = std::fs::read_to_string(std::env::args().nth(2).unwrap()).unwrap();
    let t = Instant::now();
    let sugg = rules.suggest(&text, &tok);
    println!("checked {} chars, {} suggestions, {:.0} MB in {:.2?}", text.len(), sugg.len(), rss(), t.elapsed());
    let chars: Vec<char> = text.chars().collect();
    if std::env::var("SHOW").is_ok() { for s in &sugg {
        let line = chars[..s.span().char().start].iter().filter(|c| **c == '\n').count() + 1;
        let flagged: String = chars[s.span().char().clone()].iter().collect();
        println!("{} | {} | {:?} -> {:?} | {}", line, s.source(), flagged, s.replacements(), s.message());
    }}
}
