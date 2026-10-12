//! Loads the English data and checks one paragraph.
//!
//! Usage: `cargo run --release --example check -- [data-dir] [text]`
//!
//! The data directory holds `en_tokenizer.bin` and `en_rules.bin` and defaults to
//! `storage/66`. This is the Rust usage shown in the repository README, kept here so CI
//! compiles it.

use std::path::PathBuf;

use nlprule::profile::Profile;
use nlprule::{Rules, Tokenizer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().unwrap_or_else(|| "storage/66".into()));
    let paragraph = args
        .next()
        .unwrap_or_else(|| "She was not been here since Monday.".to_string());

    let tokenizer = Tokenizer::new(dir.join("en_tokenizer.bin"))?;
    let mut rules = Rules::new(dir.join("en_rules.bin"))?;
    rules.apply_profile(&Profile::by_name("fiction").unwrap());
    if let Ok(variant) = rules.spell().variant("en_US") {
        rules.spell_mut().options_mut().variant = Some(variant);
    }

    for suggestion in rules.suggest(&paragraph, &tokenizer) {
        let span = suggestion.span().char();
        println!(
            "{}..{} {}: {}",
            span.start,
            span.end,
            suggestion.source(),
            suggestion.message()
        );
    }
    Ok(())
}
