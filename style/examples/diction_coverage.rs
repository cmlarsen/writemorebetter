use std::collections::{BTreeMap, HashSet};

use nlprule::Tokenizer;
use writemorebetter_style::reports::diction::{self, ConcretenessTable, DictionOptions};
use writemorebetter_style::{analyze_document, Document, DocumentOptions, Mode};

fn fold(text: &str) -> String {
    text.replace('’', "'").to_lowercase()
}

fn narration_rated(document: &Document, table: &ConcretenessTable) -> usize {
    diction::report(
        document,
        table,
        &DictionOptions::new().with_minimum_rated_words(1),
    )
    .unwrap()
    .sentences
    .iter()
    .filter(|sentence| sentence.mode == Mode::Narration)
    .map(|sentence| sentence.rated_words)
    .sum()
}

fn main() {
    let tokenizer = Tokenizer::new("storage/en_tokenizer.bin").unwrap();
    let custom = std::env::var("WMB_CONCRETENESS")
        .ok()
        .map(|path| ConcretenessTable::parse(&std::fs::read_to_string(path).unwrap()).unwrap());
    let bundled = custom.as_ref().unwrap_or(ConcretenessTable::bundled());
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).unwrap();
        let document = analyze_document(
            &tokenizer,
            &text,
            &DocumentOptions::new().with_hard_wrapped(path.contains("moby")),
        );
        let mut keys = HashSet::new();
        let mut unrated = BTreeMap::new();
        for token in document.words() {
            let surface = fold(&token.text);
            let mut candidates: Vec<String> = surface.split('-').map(str::to_owned).collect();
            candidates.push(surface.clone());
            candidates.extend(token.readings.iter().map(|r| fold(&r.lemma)));
            candidates.retain(|key| !key.is_empty() && !key.contains(char::is_whitespace));
            if document.mode_at(token.span.byte.start) == Mode::Narration
                && token.readings.iter().any(|r| {
                    ["NN", "VB", "JJ", "RB"]
                        .iter()
                        .any(|tag| r.pos.starts_with(tag))
                        && !r.pos.starts_with("NNP")
                })
                && candidates.iter().all(|key| bundled.get(key).is_none())
            {
                *unrated.entry(surface).or_insert(0usize) += 1;
            }
            keys.extend(candidates);
        }
        let universal = ConcretenessTable::new(keys.into_iter().map(|key| (key, 3.0))).unwrap();
        let mut unrated: Vec<_> = unrated.into_iter().collect();
        unrated.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let sample: Vec<_> = unrated
            .iter()
            .take(std::env::var("WMB_UNRATED").map_or(15, |n| n.parse().unwrap()))
            .map(|(word, count)| format!("{word}:{count}"))
            .collect();
        println!(
            "{path}\t{}\t{}\t{}",
            narration_rated(&document, bundled),
            narration_rated(&document, &universal),
            sample.join(" ")
        );
    }
}
