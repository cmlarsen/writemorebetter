//! Analyze a text file without running grammar rules; print aggregate timings only.
use std::{error::Error, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: analyze TOKENIZER.bin TEXT.txt".into());
    }
    let text = std::fs::read_to_string(&args[2])?;
    let start = Instant::now();
    let tokenizer = nlprule::Tokenizer::new(&args[1])?;
    let loaded = start.elapsed();
    let start = Instant::now();
    let sentences = tokenizer.analyze(&text);
    let elapsed = start.elapsed();
    let tokens: usize = sentences.iter().map(|s| s.tokens.len()).sum();
    println!(
        "{} bytes, {} sentences, {} tokens; load {:.3}s, analyze {:.3}s",
        text.len(),
        sentences.len(),
        tokens,
        loaded.as_secs_f64(),
        elapsed.as_secs_f64()
    );
    Ok(())
}
