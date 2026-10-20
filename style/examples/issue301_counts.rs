use nlprule::Tokenizer;
use writemorebetter_style::reports::{adverbs, pronoun_starts, transitions};
use writemorebetter_style::{analyze_document, DocumentOptions};

fn main() {
    let tokenizer = Tokenizer::new("storage/en_tokenizer.bin").unwrap();
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).unwrap();
        let document = analyze_document(
            &tokenizer,
            &text,
            &DocumentOptions::new().with_hard_wrapped(path.contains("moby")),
        );
        let adverbs = adverbs::report(&document, &Default::default()).unwrap();
        let pronouns = pronoun_starts::report(&document, &Default::default()).unwrap();
        let transitions = transitions::report(&document, &Default::default()).unwrap();
        let condition = transitions
            .matches
            .iter()
            .filter(|item| item.kind == transitions::TransitionType::Condition)
            .count();
        println!(
            "{}",
            serde_json::json!({
                "file": path, "words": document.word_count,
                "adverbs": adverbs.groups.all.count,
                "narrationAdverbs": adverbs.groups.narration.count,
                "dialogueAdverbs": adverbs.groups.dialogue.count,
                "tagAdverbs": adverbs.matches.iter().filter(|item| item.tag_adverb).count(),
                "pronounWords": pronouns.groups.all.pronoun_words.count,
                "pronounPercent": pronouns.groups.all.pronoun_words.percent,
                "conditionMatches": condition,
                "transitionMatches": transitions.matches.len(),
                "transitionRepeats": transitions.repeats.len()
            })
        );
    }
}
