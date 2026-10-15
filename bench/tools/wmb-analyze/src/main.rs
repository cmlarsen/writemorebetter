use std::io::{self, BufRead, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("expected tokenizer path")?;
    let tokenizer = nlprule::Tokenizer::new(path)?;
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    for line in io::stdin().lock().lines() {
        let text: String = serde_json::from_str(&line?)?;
        serde_json::to_writer(&mut output, &tokenizer.analyze(&text))?;
        writeln!(output)?;
        output.flush()?;
    }
    Ok(())
}
