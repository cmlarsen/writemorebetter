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
    /// `spell_variant` selects a speller dictionary when non-empty (e.g. `"en_US"`); pass
    /// `undefined` (or an empty string) to leave spelling off.
    ///
    /// `profile` selects a rule profile by name (`"default"` or `"fiction"`) when
    /// non-empty; pass `undefined` (or an empty string) to disable nothing. An unknown
    /// profile name is an error.
    #[wasm_bindgen(constructor)]
    pub fn new(
        tokenizer_bytes: &[u8],
        rules_bytes: &[u8],
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
}
