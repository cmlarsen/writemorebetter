//! Shared narrative helpers (#353): the verb-chain classifier, pronoun person, imperatives and
//! scenes that the voice layer, person drift (#354) and tense shift (#355) build on. Fixture text
//! is written for these tests (CC0).

use nlprule::Tokenizer;
use std::sync::OnceLock;
use writemorebetter_style::narrative::{
    chains, imperative_start, paragraph_quotes_balanced, pronoun_person, scenes, Case, ChainClass,
    NarrativePerson,
};
use writemorebetter_style::{analyze_document, Document, DocumentOptions, Token};

fn tokenizer() -> &'static Tokenizer {
    static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();
    TOKENIZER.get_or_init(|| Tokenizer::new("../storage/en_tokenizer.bin").unwrap())
}

fn analyze(text: &str) -> Document {
    analyze_document(tokenizer(), text, &DocumentOptions::new())
}

fn sentence_tokens(document: &Document, sentence: usize) -> Vec<&Token> {
    document
        .sentences()
        .nth(sentence)
        .unwrap()
        .tokens
        .iter()
        .filter(|token| !token.is_whitespace)
        .collect()
}

/// The class of each chain head in a one-sentence text, by head word.
fn classes(text: &str) -> Vec<(String, ChainClass)> {
    let document = analyze(text);
    let tokens = sentence_tokens(&document, 0);
    chains(&tokens)
        .into_iter()
        .map(|chain| (tokens[chain.head].text.clone(), chain.class))
        .collect()
}

#[test]
fn chain_heads_and_classes() {
    use ChainClass::*;
    for (text, head, class) in [
        ("She’d gone.", "’d", PastPerfect),
        ("She'd gone.", "'d", PastPerfect),
        ("She had gone.", "had", PastPerfect),
        ("She’d go.", "’d", Modal { present: false }),
        ("She can go.", "can", Modal { present: true }),
        ("She will go.", "will", Modal { present: true }),
        ("She would go.", "would", Modal { present: false }),
        ("She can’t go.", "ca", Modal { present: true }),
        ("She sets it down.", "sets", Present),
        ("She set it down.", "set", Past),
        ("They set it down.", "set", Ambiguous),
        ("She walked home.", "walked", Past),
        ("I walk home.", "walk", Present),
        ("He’s lying.", "’s", Present),
        ("‘Tis late.", "Tis", Present),
    ] {
        assert_eq!(classes(text), [(head.to_owned(), class)], "{text}");
    }
}

#[test]
fn plural_nouns_are_not_chains() {
    let found = classes("The leaves fell.");
    assert!(found.iter().all(|(head, _)| head != "leaves"), "{found:?}");
    assert_eq!(found, [("fell".to_owned(), ChainClass::Past)]);
}

#[test]
fn chains_record_subjects_and_clauses() {
    let document = analyze("She stopped, and then he’s gone.");
    let tokens = sentence_tokens(&document, 0);
    let found = chains(&tokens);
    assert_eq!(found.len(), 2);
    assert_eq!(tokens[found[0].subject.unwrap()].text, "She");
    assert_eq!(tokens[found[1].subject.unwrap()].text, "he");
    assert_eq!(found[0].clause, 0);
    assert_eq!(found[1].clause, 1);
}

#[test]
fn pronoun_person_and_case() {
    use Case::*;
    use NarrativePerson::*;
    let document = analyze(
        "I told him my plan, and she saw me with her own eyes, so we laughed at ourselves.",
    );
    let tokens = sentence_tokens(&document, 0);
    let found: Vec<(String, NarrativePerson, Case)> = (0..tokens.len())
        .filter_map(|index| {
            pronoun_person(&tokens, index)
                .map(|(person, case)| (tokens[index].text.clone(), person, case))
        })
        .collect();
    let expected: Vec<(String, NarrativePerson, Case)> = [
        ("I", First, Subject),
        ("him", Third, Object),
        ("my", First, Possessive),
        ("she", Third, Subject),
        ("me", First, Object),
        ("her", Third, Possessive),
        ("we", First, Subject),
        ("ourselves", First, Reflexive),
    ]
    .into_iter()
    .map(|(word, person, case)| (word.to_owned(), person, case))
    .collect();
    assert_eq!(found, expected);
}

#[test]
fn roman_numeral_i_is_not_a_pronoun() {
    for (text, word) in [
        ("Henry I ruled.", "I"),
        ("Chapter I begins.", "I"),
        ("The letter I is thin.", "I"),
        // An initial in an abbreviation (#354).
        ("They had dances at I.O.O.F. Hall.", "I"),
    ] {
        let document = analyze(text);
        let tokens = sentence_tokens(&document, 0);
        let index = tokens.iter().position(|token| token.text == word).unwrap();
        assert_eq!(pronoun_person(&tokens, index), None, "{text}");
    }
    let document = analyze("Then I ruled.");
    let tokens = sentence_tokens(&document, 0);
    assert_eq!(
        pronoun_person(&tokens, 1).map(|(person, _)| person),
        Some(NarrativePerson::First)
    );
    // they, it and one carry no person for these lenses.
    let document = analyze("They said it was one.");
    let tokens = sentence_tokens(&document, 0);
    assert!((0..tokens.len()).all(|index| pronoun_person(&tokens, index).is_none()));
}

#[test]
fn imperatives() {
    for (text, expected) in [
        ("Get out now.", true),
        ("Keep moving.", true),
        ("Run-down shack.", false),
        ("She ran.", false),
        ("Do you know?", false),
    ] {
        let document = analyze(text);
        assert_eq!(
            imperative_start(&sentence_tokens(&document, 0)),
            expected,
            "{text}"
        );
    }
}

#[test]
fn scenes_start_at_breaks_and_chapters() {
    let text = "Chapter 1\n\nShe woke. It rained.\nHe slept.\n\n* * *\n\nMorning came.\n\n\n\nNight fell. The end.\n\nChapter 2\n\nA new day.";
    let document = analyze(text);
    let found = scenes(&document);
    let summary: Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> = found
        .iter()
        .map(|scene| (scene.paragraphs.clone(), scene.sentences.clone()))
        .collect();
    assert_eq!(
        summary,
        [(0..2, 0..3), (2..3, 3..4), (3..4, 4..6), (4..5, 6..7)]
    );
    assert_eq!(
        found.iter().map(|scene| scene.index).collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
    assert_eq!(&text[found[1].span.byte.clone()], "Morning came.");
    assert_eq!(
        &text[found[0].span.byte.clone()],
        "She woke. It rained.\nHe slept."
    );
}

#[test]
fn quote_balance() {
    let document =
        analyze("“Hello,” she said.\nShe said “hello and left.\n‘Tis late, she thought.");
    assert!(paragraph_quotes_balanced(&document, 0));
    assert!(!paragraph_quotes_balanced(&document, 1));
    assert!(paragraph_quotes_balanced(&document, 2));
}

/// *mine* the noun carries no person (#354); the possessive pronoun does.
#[test]
fn mine_the_noun() {
    for text in [
        "They opened the first mine.",
        "It was the habit of the mine.",
        "The railway linked mine with mine.",
        "The electricity was carried into every mine.",
    ] {
        let document = analyze(text);
        let tokens = sentence_tokens(&document, 0);
        assert!(
            (0..tokens.len()).all(|index| pronoun_person(&tokens, index).is_none()),
            "{text}"
        );
    }
    for text in [
        "The book was mine.",
        "A friend of mine came.",
        "Mine was red.",
    ] {
        let document = analyze(text);
        let tokens = sentence_tokens(&document, 0);
        let index = tokens
            .iter()
            .position(|token| token.text.eq_ignore_ascii_case("mine"))
            .unwrap();
        assert_eq!(
            pronoun_person(&tokens, index),
            Some((NarrativePerson::First, Case::Possessive)),
            "{text}"
        );
    }
}
