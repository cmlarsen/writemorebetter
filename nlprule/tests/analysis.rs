use lazy_static::lazy_static;
use nlprule::{analysis::AnalysisSentence, Tokenizer};
use std::{collections::BTreeSet, fs, path::Path};

lazy_static! {
    static ref TOKENIZER: Tokenizer = Tokenizer::new("../storage/en_tokenizer.bin").unwrap();
}

fn readings(sentences: &[AnalysisSentence], surface: &str) -> Vec<(String, String)> {
    let token = sentences
        .iter()
        .flat_map(|s| &s.tokens)
        .find(|t| t.text == surface)
        .unwrap();
    token
        .readings
        .iter()
        .map(|r| (r.lemma.clone(), r.pos.clone()))
        .collect()
}

#[derive(Debug)]
struct ReferenceToken {
    text: String,
    readings: BTreeSet<(String, String)>,
    chunks: BTreeSet<String>,
}

/// Parse the CLI's surface[lemma/POS,...,chunk|chunk] format, including adjacent tokens.
fn parse_taggeronly(transcript: &str) -> Vec<ReferenceToken> {
    let mut tokens = Vec::new();
    for line in transcript.lines() {
        let mut rest = line.strip_prefix("<S> ").expect("CLI sentence marker");
        while !rest.trim().is_empty() {
            let (surface, tagged) = rest.split_once('[').expect("CLI token opening bracket");
            let (tags, remaining) = tagged.split_once(']').expect("CLI token closing bracket");
            let mut token = ReferenceToken {
                text: surface.trim().to_owned(),
                readings: BTreeSet::new(),
                chunks: BTreeSet::new(),
            };
            assert!(!token.text.is_empty(), "unexpected empty CLI token");
            for tag in tags.split(',') {
                let tag = tag.strip_prefix("</S>").unwrap_or(tag);
                if let Some((lemma, pos)) = tag.rsplit_once('/') {
                    // Keep this independent of the engine's SpecialPos list.
                    if !matches!(pos, "null" | "" | "UNKNOWN" | "SENT_START" | "SENT_END") {
                        token.readings.insert((lemma.to_owned(), pos.to_owned()));
                    }
                } else {
                    token.chunks.extend(tag.split('|').map(str::to_owned));
                }
            }
            tokens.push(token);
            rest = remaining;
        }
    }
    tokens
}

struct KnownChunkDivergence {
    name: &'static str,
    fixture: &'static str,
    token_index: usize,
    surface: &'static str,
    reference: &'static [&'static str],
    actual: &'static [&'static str],
}

// Existing chunker gap (review #255 finding 5d), not a reading or tokenization exception.
const KNOWN_CHUNK_DIVERGENCES: &[KnownChunkDivergence] = &[KnownChunkDivergence {
    name: "emoji_noun_phrase_chunk",
    fixture: "fiction_unicode",
    token_index: 0,
    surface: "😀",
    reference: &[],
    actual: &["B-NP-singular"],
}];

#[test]
fn every_lt66_transcript_matches_all_token_reading_and_chunk_sets() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/analysis");
    let mut fixtures: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.to_str().unwrap().ends_with(".tags.txt"))
        .collect();
    fixtures.sort();
    assert!(!fixtures.is_empty(), "no CLI reference transcripts found");
    let mut used_divergences = BTreeSet::new();
    for path in fixtures {
        let fixture = path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .strip_suffix(".tags.txt")
            .unwrap();
        let text = fs::read_to_string(path.with_file_name(format!("{fixture}.txt"))).unwrap();
        let reference = parse_taggeronly(&fs::read_to_string(&path).unwrap());
        let analysis = TOKENIZER.analyze(&text);
        let actual: Vec<_> = analysis
            .iter()
            .flat_map(|s| &s.tokens)
            .filter(|t| !t.is_whitespace)
            .collect();
        assert_eq!(actual.len(), reference.len(), "{fixture}: token count");
        for (index, (actual, reference)) in actual.iter().zip(&reference).enumerate() {
            let context = format!("{fixture}: token {index} ({:?})", actual.text);
            // The CLI prints straight apostrophes; analyze preserves the input surface.
            assert_eq!(
                actual.text.replace('’', "'"),
                reference.text,
                "{context}: surface"
            );
            let readings: BTreeSet<_> = actual
                .readings
                .iter()
                .map(|r| (r.lemma.clone(), r.pos.clone()))
                .collect();
            assert_eq!(readings, reference.readings, "{context}: readings");
            let chunks: BTreeSet<_> = actual.chunks.iter().cloned().collect();
            if let Some(known) = KNOWN_CHUNK_DIVERGENCES
                .iter()
                .find(|known| known.fixture == fixture && known.token_index == index)
            {
                assert_eq!(actual.text, known.surface, "{context}: allowlist surface");
                assert_eq!(
                    reference.chunks,
                    known.reference.iter().map(|s| (*s).to_owned()).collect(),
                    "{context}: reference chunks changed"
                );
                assert_eq!(
                    chunks,
                    known.actual.iter().map(|s| (*s).to_owned()).collect(),
                    "{context}: known chunks changed"
                );
                assert_ne!(
                    chunks, reference.chunks,
                    "{context}: remove resolved allowlist entry"
                );
                used_divergences.insert(known.name);
            } else {
                assert_eq!(chunks, reference.chunks, "{context}: chunks");
            }
        }
    }
    assert_eq!(
        used_divergences,
        KNOWN_CHUNK_DIVERGENCES
            .iter()
            .map(|known| known.name)
            .collect(),
        "allowlist entries must be exercised"
    );
}

#[test]
fn analysis_readings_follow_lt66_probes() {
    // Expected readings come from LanguageTool 6.6 --taggeronly, using English.
    // See fixtures/analysis/README.md and the checked-in CLI transcripts. These are independent expectations,
    // not a snapshot of our pipe() output. Sentence-end pseudo readings are not exported.
    let sentences = TOKENIZER.analyze(include_str!("fixtures/analysis/multiword.txt"));
    for surface in ["O", "BRIEN"] {
        assert_eq!(
            readings(&sentences, surface),
            vec![("O'Brien".into(), "NNP".into())]
        );
    }
    let sentences = TOKENIZER.analyze(include_str!("fixtures/analysis/dropped_g.txt"));
    assert_eq!(
        readings(&sentences, "goin'"),
        vec![("go".into(), "VBG".into())]
    );
    assert_eq!(
        readings(&sentences, "'m"),
        vec![("be".into(), "VBP".into())]
    );
    assert_eq!(
        sentences[0]
            .tokens
            .iter()
            .find(|t| t.text == "goin'")
            .unwrap()
            .chunks,
        ["I-VP"]
    );

    let sentences = TOKENIZER.analyze(include_str!("fixtures/analysis/fiction_unicode.txt"));
    assert_eq!(
        readings(&sentences, "’s"),
        vec![("be".into(), "VBZ".into())]
    );
    assert_eq!(
        readings(&sentences, "waiting"),
        vec![("wait".into(), "VBG".into())]
    );
    assert_eq!(
        readings(&sentences, "again"),
        vec![("again".into(), "RB".into())]
    );

    let sentences = TOKENIZER.analyze(include_str!("fixtures/analysis/paragraphs.txt"));
    assert_eq!(
        sentences
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>(),
        ["She walked home.\n\n", "He stayed."]
    );
    assert_eq!(
        readings(&sentences, "walked"),
        vec![("walk".into(), "VBD".into())]
    );
    assert_eq!(
        readings(&sentences, "stayed"),
        vec![("stay".into(), "VBD".into())]
    );
}

#[test]
fn original_surfaces_partition_sentences_in_all_offset_units() {
    for text in [
        "  😀 “She’s goin’—again…”\u{2028}\n\nHe left.\t",
        "I’m e\u{301}… ‘fine,’ she said.\r\n\r\nThey waited!",
        "She used L’ORÉAL—twice.",
    ] {
        let utf16: Vec<_> = text.encode_utf16().collect();
        let chars: Vec<_> = text.chars().collect();
        for sentence in TOKENIZER.analyze(text) {
            assert_eq!(text.get(sentence.span.byte.clone()).unwrap(), sentence.text);
            assert_eq!(
                String::from_utf16(&utf16[sentence.span.utf16.clone()]).unwrap(),
                sentence.text
            );
            let mut cursor = sentence.span.byte.start;
            for token in &sentence.tokens {
                assert_eq!(token.span.byte.start, cursor);
                assert!(token.span.byte.end > cursor);
                assert_eq!(text.get(token.span.byte.clone()).unwrap(), token.text);
                assert_eq!(
                    chars[token.span.char.clone()].iter().collect::<String>(),
                    token.text
                );
                assert_eq!(
                    String::from_utf16(&utf16[token.span.utf16.clone()]).unwrap(),
                    token.text
                );
                if token.is_whitespace {
                    assert!(token.readings.is_empty());
                }
                cursor = token.span.byte.end;
            }
            assert_eq!(cursor, sentence.span.byte.end);
            assert_eq!(
                sentence
                    .tokens
                    .iter()
                    .map(|t| t.text.as_str())
                    .collect::<String>(),
                sentence.text
            );
        }
    }
}

#[test]
fn punctuation_and_whitespace_are_explicit() {
    let sentences = TOKENIZER.analyze("😀 “I’m fine—really…”\u{2028}");
    let tokens: Vec<_> = sentences.iter().flat_map(|s| &s.tokens).collect();
    for surface in ["“", "—", "…", "”"] {
        assert!(
            tokens
                .iter()
                .find(|t| t.text == surface)
                .unwrap()
                .is_punctuation
        );
    }
    let emoji = tokens.iter().find(|t| t.text == "😀").unwrap();
    assert!(!emoji.is_punctuation && !emoji.is_whitespace);
    let clitic = tokens.iter().find(|t| t.text == "’m").unwrap();
    assert!(!clitic.is_punctuation);
    assert!(tokens
        .iter()
        .any(|t| t.is_whitespace && t.text.contains('\u{2028}')));
}

#[test]
fn empty_analysis_and_owned_results() {
    for text in ["", " \t\n\u{2028}"] {
        assert!(TOKENIZER.analyze(text).is_empty());
    }
    let sentences = {
        let tokenizer = Tokenizer::new("../storage/en_tokenizer.bin").unwrap();
        let text = String::from("She walked home.");
        tokenizer.analyze(&text)
    };
    assert_eq!(sentences[0].text, "She walked home.");
}
