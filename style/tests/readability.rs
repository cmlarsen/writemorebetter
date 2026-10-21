use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::reports::readability::{
    self, sentence_words, syllables, DictionarySyllables, HeuristicSyllables, ReadabilityOptions,
    ReadabilityScores, SyllableCounter,
};
use writemorebetter_style::reports::{to_utf16_json, StyleResources};
use writemorebetter_style::{
    analyze_document, Document, DocumentOptions, StyleDocument, StyleError,
};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

fn document(text: &str) -> Document {
    analyze_document(tokenizer(), text, &DocumentOptions::new())
}

fn close(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("score");
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

/// Formulas written out independently of the report, for hand-computed fixtures.
fn expect_scores(
    scores: &ReadabilityScores,
    sentences: f64,
    words: f64,
    syllables: f64,
    characters: f64,
) {
    assert_eq!(scores.sentences as f64, sentences);
    assert_eq!(scores.words as f64, words);
    assert_eq!(scores.syllables as f64, syllables);
    assert_eq!(scores.characters as f64, characters);
    close(
        scores.flesch_reading_ease,
        206.835 - 1.015 * (words / sentences) - 84.6 * (syllables / words),
    );
    close(
        scores.flesch_kincaid_grade,
        0.39 * (words / sentences) + 11.8 * (syllables / words) - 15.59,
    );
    close(
        scores.coleman_liau,
        0.0588 * (characters / words * 100.0) - 0.296 * (sentences / words * 100.0) - 15.8,
    );
    close(
        scores.automated_readability,
        4.71 * (characters / words) + 0.5 * (words / sentences) - 21.43,
    );
}

fn accuracy(list: &str) -> (usize, usize, Vec<String>) {
    accuracy_with(list, &HeuristicSyllables)
}

fn accuracy_with(list: &str, counter: &dyn SyllableCounter) -> (usize, usize, Vec<String>) {
    let mut misses = Vec::new();
    let mut total = 0;
    for line in list.lines() {
        let (word, expected) = line.split_once('\t').unwrap();
        total += 1;
        let got = counter.syllables(word);
        if got != expected.parse::<usize>().unwrap() {
            misses.push(format!("{word}: {got} (expected {expected})"));
        }
    }
    (total - misses.len(), total, misses)
}

#[test]
fn syllable_counter_accuracy() {
    // Development list: common fiction words, contractions, hyphenated compounds and the
    // hiatus/silent-e cases the rules were written for. Counts follow CMUdict's first entry.
    // It includes the documented misses (`hour`, `our`, `naive`), so the floor is below 100%.
    let (right, total, misses) = accuracy(include_str!("fixtures/syllables.tsv"));
    println!("development list: {right} of {total}");
    for miss in &misses {
        println!("  {miss}");
    }
    assert!(
        right * 1000 >= total * 960,
        "{right} of {total}: {misses:?}"
    );
}

#[test]
fn syllable_counter_heldout() {
    // Held out: a seeded random sample of words from bench/public, labeled after the rules
    // were frozen. It measures accuracy on running fiction vocabulary.
    let (right, total, misses) = accuracy(include_str!("fixtures/syllables-heldout.tsv"));
    println!("held-out list: {right} of {total}");
    for miss in &misses {
        println!("  {miss}");
    }
    assert!(right * 100 >= total * 98, "{right} of {total}: {misses:?}");
}

#[test]
fn syllables_handle_typography() {
    for (word, expected) in [
        ("can’t", 1),
        ("can't", 1),
        ("didn’t", 2),
        ("haven’t", 2),
        ("it’ll", 2),
        ("she’s", 1),
        ("James’s", 2),
        ("dogs’", 1),
        ("’em", 1),
        ("o’clock", 2),
        ("well-known", 2),
        ("self‐aware", 3),
        ("naïve", 2),
        ("café", 2),
        ("Something", 2),
        ("EVERYONE", 3),
        ("1999", 1),
        ("hmm", 1),
        ("’", 1),
    ] {
        assert_eq!(syllables(word), expected, "{word}");
    }
}

#[test]
fn words_join_contractions_and_hyphens() {
    let doc =
        document("She’s sure they can't—won’t—see the well-known door… I’m “fine.”\u{2028}Fine.");
    let words: Vec<String> = doc.sentences().flat_map(sentence_words).collect();
    assert_eq!(
        words,
        [
            "She’s",
            "sure",
            "they",
            "can't",
            "won’t",
            "see",
            "the",
            "well-known",
            "door",
            "I’m",
            "fine",
            "Fine"
        ]
    );
}

#[test]
fn hand_computed_scores() {
    // 2 sentences, 9 words, 10 syllables (hap-py), 27 characters.
    // FRE 206.835 − 1.015 × 4.5 − 84.6 × 10/9 = 108.2675
    // FKGL 0.39 × 4.5 + 11.8 × 10/9 − 15.59 = −0.72389
    // CLI 0.0588 × 300 − 0.296 × 22.2 − 15.8 = −4.73778
    // ARI 4.71 × 3 + 0.5 × 4.5 − 21.43 = −5.05
    let report = readability::report(
        &document("The cat sat on the mat. It was happy."),
        &ReadabilityOptions::new(),
    )
    .unwrap();
    let all = &report.groups.all;
    expect_scores(all, 2.0, 9.0, 10.0, 27.0);
    close(all.flesch_reading_ease, 108.2675);
    close(
        all.flesch_kincaid_grade,
        0.39 * 4.5 + 11.8 * 10.0 / 9.0 - 15.59,
    );
    assert!((all.flesch_kincaid_grade.unwrap() + 0.72389).abs() < 1e-4);
    assert!((all.coleman_liau.unwrap() + 4.73778).abs() < 1e-4);
    close(all.automated_readability, -5.05);
    close(Some(all.reading_minutes), 9.0 / 260.0);
    assert!(all.low_confidence);
    assert_eq!(report.groups.narration, *all);
    assert_eq!(report.groups.dialogue.words, 0);
    assert_eq!(report.groups.dialogue.flesch_reading_ease, None);
    assert_eq!(report.paragraphs.len(), 1);
    assert_eq!(report.paragraphs[0].scores.words, 9);
    assert!(report.paragraphs[0].scores.low_confidence);
}

#[test]
fn polysyllables_and_harder_text() {
    // 1 sentence, 8 words; syllables 1+5+3+1+5+1+1+3 = 20; characters 3+12+8+3+12+4+2+9 = 53.
    let report = readability::report(
        &document("The organization recorded its unbelievable loss in September."),
        &ReadabilityOptions::new(),
    )
    .unwrap();
    expect_scores(&report.groups.all, 1.0, 8.0, 20.0, 53.0);
    assert_eq!(report.groups.all.polysyllables, 4);
}

#[test]
fn fiction_typography_and_modes() {
    // Dialogue: I can’t stay she said — 5 words, 5 syllables, 16 characters.
    // Narration: The door old heavy swung open Nobody moved Outside rain fell —
    // 11 words, 16 syllables (heav-y, o-pen, no-bod-y, out-side), 50 characters.
    let text = "“I can’t stay,” she said. The door—old, heavy—swung open… Nobody moved.\u{2028}Outside, rain fell.";
    let doc = document(text);
    let report = readability::report(&doc, &ReadabilityOptions::new()).unwrap();
    let groups = &report.groups;
    assert_eq!(groups.dialogue.sentences, 1);
    expect_scores(&groups.dialogue, 1.0, 5.0, 5.0, 16.0);
    let narration = groups.narration.sentences as f64;
    assert!(narration >= 2.0, "{narration}");
    expect_scores(&groups.narration, narration, 11.0, 16.0, 50.0);
    expect_scores(&groups.all, narration + 1.0, 16.0, 21.0, 66.0);
    let paragraph = &report.paragraphs[0];
    assert!(!paragraph.dialogue_only);
    let mut expected = groups.all.clone();
    expected.low_confidence = true;
    assert_eq!(paragraph.scores, expected);
}

#[test]
fn dialogue_only_paragraphs_can_be_excluded() {
    let text = "“Go home now,” she said.\n\nThe road was empty and the wind was cold.";
    let doc = document(text);
    let kept = readability::report(&doc, &ReadabilityOptions::new()).unwrap();
    assert!(kept.paragraphs[0].dialogue_only);
    assert!(!kept.paragraphs[0].excluded);
    assert!(!kept.paragraphs[1].dialogue_only);
    assert_eq!(kept.groups.all.words, 14);
    assert_eq!(kept.groups.dialogue.words, 5);

    let options = ReadabilityOptions::new().with_exclude_dialogue_paragraphs(true);
    let excluded = readability::report(&doc, &options).unwrap();
    assert!(excluded.paragraphs[0].excluded);
    assert_eq!(excluded.paragraphs[0].scores, kept.paragraphs[0].scores);
    assert_eq!(excluded.groups.all.words, 9);
    assert_eq!(excluded.groups.dialogue.words, 0);
    // Only the whole-text reading time still counts the excluded paragraph.
    let mut all = excluded.groups.all.clone();
    close(Some(all.reading_minutes), 14.0 / 260.0);
    all.reading_minutes = excluded.groups.narration.reading_minutes;
    assert_eq!(all, excluded.groups.narration);
}

#[test]
fn paragraph_rows_follow_document_paragraphs() {
    let text = "One short line.\n\n* * *\n\nAnother short line here.";
    let doc = document(text);
    let report = readability::report(&doc, &ReadabilityOptions::new()).unwrap();
    assert_eq!(report.paragraphs.len(), doc.paragraphs.len());
    for (row, paragraph) in report.paragraphs.iter().zip(&doc.paragraphs) {
        assert_eq!(row.span, paragraph.span);
    }
    for row in &report.paragraphs {
        assert_eq!(
            row.scores.flesch_kincaid_grade.is_none(),
            row.scores.words == 0
        );
    }
    let words: usize = report.paragraphs.iter().map(|p| p.scores.words).sum();
    assert_eq!(words, report.groups.all.words);
    let options = ReadabilityOptions::new().with_minimum_paragraph_words(3);
    let report = readability::report(&doc, &options).unwrap();
    assert!(!report.paragraphs.last().unwrap().scores.low_confidence);
}

#[test]
fn confidence_and_reading_time_options() {
    let doc = document("The cat sat on the mat. It was happy.");
    let options = ReadabilityOptions::new()
        .with_minimum_sentences(2)
        .with_words_per_minute(90.0);
    let report = readability::report(&doc, &options).unwrap();
    assert!(!report.groups.all.low_confidence);
    assert!(report.groups.dialogue.low_confidence);
    close(Some(report.groups.all.reading_minutes), 0.1);
    for bad in [0.0, -5.0, f64::NAN, f64::INFINITY] {
        let options = ReadabilityOptions::new().with_words_per_minute(bad);
        assert!(matches!(
            readability::report(&doc, &options),
            Err(StyleError::InvalidOptions { .. })
        ));
    }
}

struct OneEach;
impl SyllableCounter for OneEach {
    fn syllables(&self, _word: &str) -> usize {
        1
    }
}

#[test]
fn syllable_counter_is_replaceable() {
    let doc = document("The organization recorded its unbelievable loss.");
    let report = readability::report_with(&doc, &ReadabilityOptions::new(), &OneEach).unwrap();
    assert_eq!(report.groups.all.syllables, report.groups.all.words);
    assert_eq!(report.groups.all.polysyllables, 0);
}

#[test]
fn compact_documents_are_supported() {
    let text = "“I can’t stay,” she said. The door—old, heavy—swung open.";
    let full = readability::report(&document(text), &ReadabilityOptions::new()).unwrap();
    let compact = analyze_document(
        tokenizer(),
        text,
        &DocumentOptions::new().with_token_details(false),
    );
    let compact = readability::report(&compact, &ReadabilityOptions::new()).unwrap();
    assert_eq!(full, compact);
}

#[test]
fn style_document_options_and_utf16_json() {
    let text = "“Café?” she asked.\u{2028}The naïve man—smiling…—nodded.";
    let style = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        text,
        &DocumentOptions::new(),
    );
    let report = style
        .readability(Some(r#"{"minimumSentences":1,"words_per_minute":200}"#))
        .unwrap();
    assert!(!report.groups.all.low_confidence);
    assert!(matches!(
        style.readability(Some(r#"{"wordsPerMinute":200,"bogus":1}"#)),
        Err(StyleError::InvalidOptions { .. })
    ));
    assert!(style.readability(Some(r#"{"wordsPerMinute":0}"#)).is_err());
    let json: serde_json::Value = serde_json::from_str(&to_utf16_json(&report)).unwrap();
    let paragraph = &json["paragraphs"][0];
    assert_eq!(paragraph["span"]["start"], 0);
    assert_eq!(paragraph["span"]["end"], text.encode_utf16().count());
    for key in [
        "fleschReadingEase",
        "fleschKincaidGrade",
        "colemanLiau",
        "automatedReadability",
        "readingMinutes",
        "lowConfidence",
        "polysyllables",
    ] {
        assert!(json["groups"]["all"].get(key).is_some(), "{key}");
    }
    assert_eq!(paragraph["dialogueOnly"], false);
}

#[test]
fn double_hyphen_dashes_split_words() {
    // Typed manuscripts use `--` for an em dash; `was--he` is two words, not one.
    let doc = document(
        "It was--he thought--late…\u{2028}“I was going to--” A well-known man’s hat—gone.",
    );
    let words: Vec<String> = doc.sentences().flat_map(sentence_words).collect();
    assert_eq!(
        words,
        [
            "It",
            "was",
            "he",
            "thought",
            "late",
            "I",
            "was",
            "going",
            "to",
            "A",
            "well-known",
            "man’s",
            "hat",
            "gone"
        ]
    );
    // 1 sentence, 5 words, 5 syllables: FKGL 0.39 × 5 + 11.8 − 15.59 = −1.84.
    let report = readability::report(
        &document("It was--he thought--late."),
        &ReadabilityOptions::new(),
    )
    .unwrap();
    expect_scores(&report.groups.all, 1.0, 5.0, 5.0, 18.0);
    assert!((report.groups.all.flesch_kincaid_grade.unwrap() + 1.84).abs() < 1e-9);
}

#[test]
fn numbers_and_abbreviations_stay_whole() {
    let doc = document("Mrs. Hale left at 3.5 p.m. on 9/11 with 1,000 men from the U.S. Army.");
    let words: Vec<String> = doc.sentences().flat_map(sentence_words).collect();
    assert_eq!(
        words,
        [
            "Mrs", "Hale", "left", "at", "3.5", "p.m", "on", "9/11", "with", "1,000", "men",
            "from", "the", "U.S", "Army"
        ]
    );
    // Title abbreviations are read in full: mis-ter, mis-sus, doc-tor.
    for (word, expected) in [("Mr", 2), ("Mrs", 2), ("Dr", 2), ("p.m", 1), ("1,000", 1)] {
        assert_eq!(syllables(word), expected, "{word}");
    }
}

#[test]
fn excluding_dialogue_keeps_whole_text_reading_time() {
    // Excluding dialogue-only paragraphs changes the indices, not how long the text takes to read.
    let text = "“Go home now, please, right now,” she said.\n\nThe road was empty.";
    let doc = document(text);
    let kept = readability::report(&doc, &ReadabilityOptions::new()).unwrap();
    let options = ReadabilityOptions::new().with_exclude_dialogue_paragraphs(true);
    let excluded = readability::report(&doc, &options).unwrap();
    assert_eq!(excluded.groups.all.words, 4);
    close(Some(kept.groups.all.reading_minutes), 12.0 / 260.0);
    close(Some(excluded.groups.all.reading_minutes), 12.0 / 260.0);
    close(Some(excluded.groups.narration.reading_minutes), 4.0 / 260.0);
}

#[test]
fn dictionary_counter_accuracy() {
    // Phase 2: CMUdict counts for the words the heuristic misses. Both lists follow CMUdict's
    // first entry, so the remaining misses are words CMUdict lacks or spells differently.
    for (name, list, floor) in [
        ("development", include_str!("fixtures/syllables.tsv"), 99),
        (
            "held-out",
            include_str!("fixtures/syllables-heldout.tsv"),
            99,
        ),
    ] {
        let (before, total, _) = accuracy(list);
        let (right, _, misses) = accuracy_with(list, &DictionarySyllables);
        println!("{name} list: {before} → {right} of {total}");
        for miss in &misses {
            println!("  {miss}");
        }
        assert!(right >= before, "{name}: {right} < {before}");
        assert!(
            right * 100 >= total * floor,
            "{right} of {total}: {misses:?}"
        );
    }
}

#[test]
fn dictionary_counter_handles_typography() {
    let counter = DictionarySyllables;
    for (word, expected) in [
        // In the table: the heuristic gets these wrong.
        ("maybe", 2),
        ("Maybe", 2),
        ("‘maybe’", 2),
        ("“Wednesday", 2),
        ("Wednesday’s", 2),
        ("Wednesday's", 2),
        ("business-like", 3),
        ("evening\u{2028}", 2),
        ("our", 2),
        ("U.S", 2),
        ("u.s.", 2),
        // Not in the table: the heuristic's count is CMUdict's.
        ("didn’t", 2),
        ("can’t", 1),
        ("James’s", 2),
        ("o’clock", 2),
        ("well-known", 2),
        ("naïve", 2),
        ("Mrs", 2),
        ("1999", 1),
        ("hmm", 1),
        ("’", 1),
        ("…", 1),
        ("—", 1),
    ] {
        assert_eq!(counter.syllables(word), expected, "{word}");
    }
}

#[test]
fn dictionary_is_the_default_counter() {
    // 2 sentences; maybe (2), naked (2), evening (2) are heuristic misses.
    let text = "“Maybe,” she said—naked…\u{2028}It was evening.";
    let doc = document(text);
    let default = readability::report(&doc, &ReadabilityOptions::new()).unwrap();
    let dictionary =
        readability::report_with(&doc, &ReadabilityOptions::new(), &DictionarySyllables).unwrap();
    let heuristic =
        readability::report_with(&doc, &ReadabilityOptions::new(), &HeuristicSyllables).unwrap();
    assert_eq!(default, dictionary);
    // Maybe she said naked It was evening: 2+1+1+2+1+1+2 = 10; the heuristic counts 9.
    assert_eq!(default.groups.all.words, 7);
    assert_eq!(default.groups.all.syllables, 10);
    assert_eq!(heuristic.groups.all.syllables, 9);
}

#[test]
fn bundled_syllable_table_is_well_formed() {
    let table = include_str!("../data/pronunciation/cmudict-syllables.tsv");
    assert!(table.contains("74790861f652b15e4ac49015a90074ad62a27690"));
    assert!(table.contains("81917843c7f44ce2b094ac63873c2c7a4cf802040792c455ba3ca406891c3d22"));
    let rows: Vec<(&str, usize)> = table
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let (word, count) = line.split_once('\t').unwrap();
            (word, count.parse().unwrap())
        })
        .collect();
    assert_eq!(
        rows.len(),
        writemorebetter_style::pronunciation::syllable_exception_count()
    );
    assert!(rows.windows(2).all(|pair| pair[0].0 < pair[1].0));
    for (word, count) in rows {
        assert!((1..=12).contains(&count), "{word}");
        assert!(
            word.chars()
                .all(|c| c.is_ascii_lowercase() || "'-.".contains(c)),
            "{word}"
        );
        assert!(word.starts_with(|c: char| c.is_ascii_lowercase()), "{word}");
        assert!(word.ends_with(|c: char| c.is_ascii_lowercase()), "{word}");
        assert_eq!(DictionarySyllables.syllables(word), count, "{word}");
    }
}
