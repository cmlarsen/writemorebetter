use std::collections::BTreeSet;
use std::io::{self, BufRead, Write};

use writemorebetter_style::reports::sentence_starts::penn_to_universal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: lemmatize_forms TOKENIZER < forms.tsv")?;
    let tokenizer = nlprule::Tokenizer::new(path)?;
    let tagger = tokenizer.tagger();
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    for line in io::stdin().lock().lines() {
        let line = line?;
        let mut fields = line.splitn(3, '\t');
        let (form, tag, count) = (
            fields.next().unwrap_or_default(),
            fields.next().unwrap_or_default(),
            fields.next().unwrap_or_default(),
        );
        if form.starts_with('#') {
            writeln!(output, "{form}\t{tag}\t{count}\t\t0")?;
            continue;
        }
        let readings: Vec<(String, String)> = tagger
            .get_tags(form)
            .map(|data| {
                (
                    data.lemma().as_str().to_owned(),
                    data.pos().as_str().to_owned(),
                )
            })
            .collect();
        let proper = form.chars().next().is_some_and(char::is_uppercase)
            && readings
                .iter()
                .any(|(_, pos)| pos == "NNP" || pos == "NNPS");
        let lemmas: BTreeSet<String> = readings
            .iter()
            .filter(|(_, pos)| pos != "NNP" && pos != "NNPS" && penn_to_universal(pos) == Some(tag))
            .map(|(lemma, _)| lemma.replace('’', "'").to_lowercase())
            .filter(|lemma| {
                !lemma.contains(char::is_whitespace) && lemma.chars().any(char::is_alphabetic)
            })
            .collect();
        let lemmas: Vec<_> = lemmas.into_iter().collect();
        writeln!(
            output,
            "{form}\t{tag}\t{count}\t{}\t{}",
            lemmas.join("|"),
            u8::from(proper)
        )?;
    }
    Ok(())
}
