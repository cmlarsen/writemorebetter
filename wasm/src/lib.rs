//! WebAssembly bindings for writemorebetter.
//!
//! The compiled rule and dictionary binaries are LGPL-2.1 resources and are deliberately *not*
//! embedded in this module. The caller (JS) reads `en_tokenizer.bin` and `en_rules.bin` and passes
//! their bytes to [`Checker::new`], which keeps them as separately loaded data.
//!
//! The engine is built with the pure-Rust `regex-fancy` backend so it runs on `wasm32-unknown-unknown`.

use std::collections::HashMap;
use std::io::Cursor;

use nlprule::{Rules, Tokenizer};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// A single suggestion, serialized to JSON for the JS side.
#[derive(Serialize)]
struct Item {
    /// Start of the flagged span, in Unicode scalar values (characters), not bytes.
    start: usize,
    /// End of the flagged span, in characters.
    end: usize,
    /// Start of the flagged span, in UTF-16 code units, for `String.prototype.slice`.
    start16: usize,
    /// End of the flagged span (exclusive), in UTF-16 code units.
    end16: usize,
    source: String,
    message: String,
    replacements: Vec<String>,
}

#[derive(Serialize)]
struct AnalysisSentence {
    text: String,
    start: usize,
    end: usize,
    tokens: Vec<AnalysisToken>,
}

#[derive(Serialize)]
struct AnalysisToken {
    text: String,
    start: usize,
    end: usize,
    readings: Vec<nlprule::analysis::AnalysisReading>,
    chunks: Vec<String>,
    is_whitespace: bool,
    is_punctuation: bool,
}

impl From<nlprule::analysis::AnalysisSentence> for AnalysisSentence {
    fn from(sentence: nlprule::analysis::AnalysisSentence) -> Self {
        Self {
            text: sentence.text,
            start: sentence.span.utf16.start,
            end: sentence.span.utf16.end,
            tokens: sentence
                .tokens
                .into_iter()
                .map(|token| AnalysisToken {
                    text: token.text,
                    start: token.span.utf16.start,
                    end: token.span.utf16.end,
                    readings: token.readings,
                    chunks: token.chunks,
                    is_whitespace: token.is_whitespace,
                    is_punctuation: token.is_punctuation,
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct DocumentSpan {
    start: usize,
    end: usize,
}

#[derive(Serialize)]
struct DocumentToken {
    text: String,
    start: usize,
    end: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    readings: Option<Vec<nlprule::analysis::AnalysisReading>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chunks: Option<Vec<String>>,
    is_whitespace: bool,
    is_punctuation: bool,
    is_word: bool,
    is_skipped: bool,
}

#[derive(Serialize)]
struct DocumentSentence {
    text: String,
    start: usize,
    end: usize,
    tokens: Vec<DocumentToken>,
}

#[derive(Serialize)]
struct DocumentParagraph {
    text: String,
    start: usize,
    end: usize,
    sentences: Vec<DocumentSentence>,
}

#[derive(Serialize)]
struct TokenPosition {
    paragraph: usize,
    sentence: usize,
    token: usize,
}

#[derive(Serialize)]
struct Document {
    paragraphs: Vec<DocumentParagraph>,
    dialogue: Vec<DocumentSpan>,
    lemma_index: std::collections::BTreeMap<String, Vec<TokenPosition>>,
    word_count: usize,
}

impl From<writemorebetter_style::Document> for Document {
    fn from(document: writemorebetter_style::Document) -> Self {
        Self {
            paragraphs: document
                .paragraphs
                .into_iter()
                .map(|paragraph| DocumentParagraph {
                    text: paragraph.text,
                    start: paragraph.span.utf16.start,
                    end: paragraph.span.utf16.end,
                    sentences: paragraph
                        .sentences
                        .into_iter()
                        .map(|sentence| DocumentSentence {
                            text: sentence.text,
                            start: sentence.span.utf16.start,
                            end: sentence.span.utf16.end,
                            tokens: sentence
                                .tokens
                                .into_iter()
                                .map(|token| DocumentToken {
                                    text: token.text,
                                    start: token.span.utf16.start,
                                    end: token.span.utf16.end,
                                    readings: Some(token.readings),
                                    chunks: Some(token.chunks),
                                    is_whitespace: token.is_whitespace,
                                    is_punctuation: token.is_punctuation,
                                    is_word: token.is_word,
                                    is_skipped: token.is_skipped,
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
            dialogue: document
                .dialogue
                .into_iter()
                .map(|span| DocumentSpan {
                    start: span.utf16.start,
                    end: span.utf16.end,
                })
                .collect(),
            lemma_index: document
                .lemma_index
                .into_iter()
                .map(|(lemma, positions)| {
                    (
                        lemma,
                        positions
                            .into_iter()
                            .map(|position| TokenPosition {
                                paragraph: position.paragraph,
                                sentence: position.sentence,
                                token: position.token,
                            })
                            .collect(),
                    )
                })
                .collect(),
            word_count: document.word_count,
        }
    }
}

/// Maps each Unicode scalar offset in `wanted` to its UTF-16 offset with one pass over `text`.
///
/// Resolving each scalar offset with `text.chars().take(n)` would rescan the text once per
/// suggestion; this walks it once and records only the boundaries the suggestions need.
fn utf16_offsets(text: &str, mut wanted: Vec<usize>) -> HashMap<usize, usize> {
    wanted.sort_unstable();
    wanted.dedup();
    let mut offsets = HashMap::with_capacity(wanted.len());
    let mut pending = wanted.into_iter().peekable();
    let mut chars = 0;
    let mut units = 0;
    for ch in text.chars() {
        while pending.peek() == Some(&chars) {
            offsets.insert(chars, units);
            pending.next();
        }
        chars += 1;
        units += ch.len_utf16();
    }
    while pending.peek() == Some(&chars) {
        offsets.insert(chars, units);
        pending.next();
    }
    offsets
}

/// A grammar and spelling checker over a pre-loaded tokenizer and rule set.
#[wasm_bindgen]
pub struct Checker {
    tokenizer: Tokenizer,
    rules: Rules,
}

#[wasm_bindgen]
impl Checker {
    /// Creates a checker from the bytes of the tokenizer and rules binaries.
    ///
    /// Both byte buffers are taken by value: wasm-bindgen's `&[u8]` arguments are copied into
    /// linear memory and never freed by the generated shim, so every construction leaked their
    /// full size (about 21 MB for the English binaries) for the lifetime of the process. Owning
    /// them here lets each buffer be dropped as soon as it has been deserialized.
    ///
    /// `spell_variant` selects a speller dictionary when non-empty (e.g. `"en_US"`); pass
    /// `undefined` (or an empty string) to leave spelling off.
    ///
    /// `profile` selects a rule profile by name (`"default"` or `"fiction"`) when
    /// non-empty; pass `undefined` (or an empty string) to disable nothing. An unknown
    /// profile name is an error.
    #[wasm_bindgen(constructor)]
    pub fn new(
        tokenizer_bytes: Vec<u8>,
        rules_bytes: Vec<u8>,
        spell_variant: Option<String>,
        profile: Option<String>,
    ) -> Result<Checker, JsValue> {
        let tokenizer = Tokenizer::from_reader(Cursor::new(tokenizer_bytes))
            .map_err(|err| JsValue::from_str(&format!("failed to load tokenizer: {err}")))?;

        let mut rules = Rules::from_reader(Cursor::new(rules_bytes))
            .map_err(|err| JsValue::from_str(&format!("failed to load rules: {err}")))?;

        if let Some(name) = profile.filter(|p| !p.is_empty()) {
            let profile = nlprule::profile::Profile::by_name(&name)
                .ok_or_else(|| JsValue::from_str(&format!("unknown profile: {name}")))?;
            rules.apply_profile(&profile);
        }

        if let Some(variant) = spell_variant.filter(|v| !v.is_empty()) {
            let variant = rules
                .spell()
                .variant(&variant)
                .map_err(|err| JsValue::from_str(&format!("unknown spell variant: {err}")))?;
            rules.spell_mut().options_mut().variant = Some(variant);
        }

        Ok(Checker { tokenizer, rules })
    }

    /// Checks `text` and returns a JSON array of suggestions.
    ///
    /// Each element is `{"start": <char offset>, "end": <char offset>, "start16": <UTF-16 offset>,
    /// "end16": <UTF-16 offset>, "source": "...", "message": "...", "replacements": [...]}`.
    ///
    /// `start`/`end` count Unicode scalar values (the older convention, to be deprecated); the
    /// additive `start16`/`end16` count UTF-16 code units and match `analyze()` and
    /// `String.prototype.slice`.
    pub fn check(&self, text: &str) -> Result<String, JsValue> {
        let suggestions = self.rules.suggest(text, &self.tokenizer);
        let utf16 = utf16_offsets(
            text,
            suggestions
                .iter()
                .flat_map(|suggestion| {
                    let span = suggestion.span().char();
                    [span.start, span.end]
                })
                .collect(),
        );
        let items: Vec<Item> = suggestions
            .iter()
            .map(|suggestion| {
                let span = suggestion.span().char();
                Item {
                    start: span.start,
                    end: span.end,
                    start16: utf16.get(&span.start).copied().unwrap_or(span.start),
                    end16: utf16.get(&span.end).copied().unwrap_or(span.end),
                    source: suggestion.source().to_string(),
                    message: suggestion.message().to_string(),
                    replacements: suggestion.replacements().to_vec(),
                }
            })
            .collect();

        serde_json::to_string(&items)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize suggestions: {err}")))
    }

    /// Returns JSON sentences and tagged tokens, including whitespace, from one analysis pass.
    /// Surface text is preserved. `start` and exclusive `end` count UTF-16 code units in
    /// the original input, suitable for `text.slice(start, end)`.
    pub fn analyze(&self, text: &str) -> Result<String, JsValue> {
        let sentences: Vec<_> = self
            .tokenizer
            .analyze(text)
            .into_iter()
            .map(AnalysisSentence::from)
            .collect();
        serde_json::to_string(&sentences)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize analysis: {err}")))
    }

    #[wasm_bindgen(js_name = analyzeDocument)]
    pub fn analyze_document(
        &self,
        text: &str,
        options_json: Option<String>,
    ) -> Result<String, JsValue> {
        let options = options_json
            .map(|json| serde_json::from_str::<writemorebetter_style::DocumentOptions>(&json))
            .transpose()
            .map_err(|err| JsValue::from_str(&format!("invalid document options: {err}")))?
            .unwrap_or_default();
        let mut document = Document::from(writemorebetter_style::analyze_document(
            &self.tokenizer,
            text,
            &options,
        ));
        if !options.include_token_details {
            for token in document
                .paragraphs
                .iter_mut()
                .flat_map(|p| &mut p.sentences)
                .flat_map(|s| &mut s.tokens)
            {
                token.readings = None;
                token.chunks = None;
            }
        }
        serde_json::to_string(&document)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize document: {err}")))
    }

    /// Adds words to the user dictionary and returns how many were new.
    ///
    /// Use it for character names, invented places and coinages so the speller stops flagging
    /// them. Entries may be single words, whitespace-separated phrases (`"Port Royal"`), or words
    /// with an internal apostrophe; curly and straight apostrophes match each other. Matching is
    /// case-sensitive except that a lowercase entry also accepts its sentence-initial capitalized
    /// form (an added capitalized name does not make the lowercase form valid), and every entry
    /// accepts a possessive. Changes apply to the next `check`.
    #[wasm_bindgen(js_name = addWords)]
    pub fn add_words(&mut self, words: Vec<String>) -> usize {
        self.rules.add_words(words)
    }

    /// Removes words from the user dictionary and returns how many were present.
    #[wasm_bindgen(js_name = removeWords)]
    pub fn remove_words(&mut self, words: Vec<String>) -> usize {
        self.rules.remove_words(words)
    }

    /// Replaces the whole user dictionary and returns the number of entries now set.
    #[wasm_bindgen(js_name = setWords)]
    pub fn set_words(&mut self, words: Vec<String>) -> usize {
        self.rules.set_words(words)
    }

    /// Lists the user dictionary as a JSON array of strings, canonicalized and sorted.
    #[wasm_bindgen(js_name = words)]
    pub fn words(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.rules.words())
            .map_err(|err| JsValue::from_str(&format!("failed to serialize words: {err}")))
    }

    /// Selects the writer's mother tongue (e.g. `"de"`) so the false-friend rules for speakers
    /// of that language run; pass `undefined` (or an empty string) to turn them off again (the
    /// default). Changes apply to the next `check`.
    #[wasm_bindgen(js_name = setMotherTongue)]
    pub fn set_mother_tongue(&mut self, mother_tongue: Option<String>) {
        self.rules
            .set_mother_tongue(mother_tongue.as_deref().filter(|code| !code.is_empty()));
    }

    /// Lists the mother tongues the rules binary has false-friend rules for, as a JSON array of
    /// language codes.
    #[wasm_bindgen(js_name = motherTongues)]
    pub fn mother_tongues(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.rules.mother_tongues())
            .map_err(|err| JsValue::from_str(&format!("failed to serialize mother tongues: {err}")))
    }
}
