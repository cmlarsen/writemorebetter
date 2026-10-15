//! Local document benchmark. Prints aggregate counts only; use public-domain input for sharing.
use std::{error::Error, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if !(3..=4).contains(&args.len()) {
        return Err("usage: document TOKENIZER.bin TEXT.txt [REPEATS]".into());
    }
    let input = std::fs::read_to_string(&args[2])?;
    let repeats = args
        .get(3)
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(1);
    if repeats == 0 {
        return Err("REPEATS must be at least one".into());
    }
    // Gutenberg text is hard-wrapped: only blank lines define paragraphs here.
    let separator = if input.contains("\r\n") {
        "\r\n\r\n"
    } else {
        "\n\n"
    };
    let text = std::iter::repeat_n(input.as_str(), repeats)
        .collect::<Vec<_>>()
        .join(separator);
    let paragraphs: Vec<_> = text.split(separator).collect();
    let start = Instant::now();
    let tokenizer = nlprule::Tokenizer::new(&args[1])?;
    let load = start.elapsed();
    let start = Instant::now();
    let document = writemorebetter_style::analyze_document(&tokenizer, &paragraphs);
    let elapsed = start.elapsed();
    let sentences: usize = document.paragraphs.iter().map(|p| p.sentences.len()).sum();
    let tokens: usize = document
        .paragraphs
        .iter()
        .flat_map(|p| &p.sentences)
        .map(|s| s.tokens.len())
        .sum();
    println!("{} bytes, {} paragraphs, {} sentences, {} tokens, {} lemmas, {} dialogue spans; load {:.3}s, document {:.3}s", document.text.len(), document.paragraphs.len(), sentences, tokens, document.lemma_index.len(), document.dialogue_spans.len(), load.as_secs_f64(), elapsed.as_secs_f64());
    Ok(())
}
