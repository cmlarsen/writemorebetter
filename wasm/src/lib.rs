//! WebAssembly bindings for writemorebetter.
//!
//! The compiled rule and dictionary binaries are LGPL-2.1 resources and are deliberately *not*
//! embedded in this module. The caller (JS) reads `en_tokenizer.bin` and `en_rules.bin` and passes
//! their bytes to [`Checker::new`], which keeps them as separately loaded data.
//!
//! The engine is built with the pure-Rust `regex-fancy` backend so it runs on `wasm32-unknown-unknown`.

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
    /// Each element is `{"start": <char offset>, "end": <char offset>, "source": "...",
    /// "message": "...", "replacements": [...]}`.
    pub fn check(&self, text: &str) -> Result<String, JsValue> {
        let suggestions = self.rules.suggest(text, &self.tokenizer);
        let items: Vec<Item> = suggestions
            .iter()
            .map(|suggestion| {
                let span = suggestion.span().char();
                Item {
                    start: span.start,
                    end: span.end,
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
