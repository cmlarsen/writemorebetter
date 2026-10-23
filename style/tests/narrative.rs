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

// Tense votes (#355), shared with person drift (#354).

use writemorebetter_style::narrative::{
    display_colon_lead_in, scene_tense, sentence_tense, NarrativeTense, SentenceTense,
    TenseContext, TenseExemption, TenseThresholds,
};
use writemorebetter_style::reports::tense_shift::{self, TenseShiftOptions};

fn vote_of(text: &str) -> (SentenceTense, Vec<TenseExemption>, Vec<String>) {
    let document = analyze(text);
    let sentence = document.sentences().next().unwrap();
    let vote = sentence_tense(&document, sentence, &TenseContext::new(&document, 0));
    let heads = vote
        .voting
        .iter()
        .map(|&index| sentence.tokens[index].text.clone())
        .collect();
    (vote.class, vote.exemptions, heads)
}

#[test]
fn sentence_tense_classes_and_exemptions() {
    use SentenceTense::*;
    use TenseExemption::*;
    for (text, class, exemptions, heads) in [
        ("She walked home.", Past, vec![], vec!["walked"]),
        ("The room is dark.", Present, vec![], vec!["is"]),
        (
            "It’s cold—colder than outside…",
            Present,
            vec![],
            vec!["’s"],
        ),
        ("She’d gone before.", Past, vec![], vec!["’d"]),
        (
            "She would sit by the window.",
            Unclassified,
            vec![Modal],
            vec![],
        ),
        ("She can see it now.", Unclassified, vec![Modal], vec![]),
        (
            "Where is he?",
            Unclassified,
            vec![QuestionOrExclamation],
            vec![],
        ),
        ("Let’s go.", Unclassified, vec![NoSubject], vec![]),
        ("They set it down.", Unclassified, vec![Ambiguous], vec![]),
        ("She knew that he lies.", Past, vec![Reported], vec!["knew"]),
        (
            "It was the house where she lives now.",
            Past,
            vec![Subordinate],
            vec!["was"],
        ),
        ("People are strange.", Unclassified, vec![Generic], vec![]),
        ("A man needs a job.", Unclassified, vec![Generic], vec![]),
        ("It is hard to say.", Unclassified, vec![Generic], vec![]),
        (
            "She always walks home.",
            Unclassified,
            vec![Habitual],
            vec![],
        ),
        ("I suppose so.", Unclassified, vec![NarratorFrame], vec![]),
        (
            "I told him, and June is a liar.",
            Mixed,
            vec![MixedSentence],
            vec!["told", "is"],
        ),
    ] {
        let heads: Vec<String> = heads.into_iter().map(str::to_owned).collect();
        assert_eq!(vote_of(text), (class, exemptions, heads), "{text}");
    }
}

#[test]
fn sentence_tense_respects_voices_and_unbalanced_quotes() {
    // Only the tag votes; the quote is not narration.
    assert_eq!(
        vote_of("“The room is dark,” she said."),
        (SentenceTense::Past, vec![], vec!["said".to_owned()])
    );
    let document = analyze("“The room is dark. The air is cold.");
    let sentence = document.sentences().next().unwrap();
    let vote = sentence_tense(&document, sentence, &TenseContext::new(&document, 0));
    assert_eq!(vote.class, SentenceTense::Unclassified);
    assert_eq!(vote.exemptions, [TenseExemption::UnbalancedQuotes]);
}

#[test]
fn display_colons_lead_in_reported_text() {
    for (text, expected) in [
        ("The sign read: Rooms To Let.", true),
        ("The note on the door: back soon.", true),
        ("She had one rule: never look back.", false),
    ] {
        let document = analyze(text);
        let tokens = sentence_tokens(&document, 0);
        let colon = tokens.iter().position(|token| token.text == ":").unwrap();
        assert_eq!(display_colon_lead_in(&tokens, colon), expected, "{text}");
    }
}

#[test]
fn scene_tense_thresholds() {
    let thresholds = TenseThresholds::default();
    assert_eq!(thresholds.minimum_sentences, 6);
    assert_eq!(thresholds.infer(6, 0), Some(NarrativeTense::Past));
    assert_eq!(thresholds.infer(5, 0), None);
    assert_eq!(thresholds.infer(7, 3), Some(NarrativeTense::Past));
    assert_eq!(thresholds.infer(6, 4), None);
    assert_eq!(thresholds.infer(1, 9), Some(NarrativeTense::Present));
}

#[test]
fn scene_tense_agrees_with_the_tense_shift_lens() {
    let text = "Mara walked home. She opened the gate. The dog barked twice. She fed him. The \
kettle boiled. She drank her tea. The room is dark.\n\n* * *\n\nAna walks to the window. She opens \
the shutters. The street is quiet. The baker sweeps his step. She watches him. He waves.\n\n* * \
*\n\nShe woke. It is late.";
    let document = analyze(text);
    let report = tense_shift::report(&document, &TenseShiftOptions::new()).unwrap();
    let found = scenes(&document);
    assert_eq!(found.len(), 3);
    for scene in &found {
        let shared = scene_tense(&document, scene, &TenseThresholds::default());
        assert_eq!(
            shared, report.scenes[scene.index].tense,
            "scene {}",
            scene.index
        );
    }
    assert_eq!(report.scenes[0].tense, Some(NarrativeTense::Past));
    assert_eq!(report.scenes[1].tense, Some(NarrativeTense::Present));
    assert_eq!(report.scenes[2].tense, None);
}
