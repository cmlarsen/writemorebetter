//! WebAssembly bindings for writemorebetter.
//!
//! The compiled rule and dictionary binaries are LGPL-2.1 resources and are deliberately *not*
//! embedded in this module. The caller (JS) reads `en_tokenizer.bin` and `en_rules.bin` and passes
//! their bytes to [`Checker::new`], which keeps them as separately loaded data.
//!
//! The engine is built with the pure-Rust `regex-fancy` backend so it runs on `wasm32-unknown-unknown`.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use nlprule::{Rules, Tokenizer};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use writemorebetter_style::reports::{self, StyleResources};

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
/// Returns the offsets and the text's total UTF-16 length.
fn utf16_offsets(text: &str, mut wanted: Vec<usize>) -> (HashMap<usize, usize>, usize) {
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
    (offsets, units)
}

/// A grammar and spelling checker over a pre-loaded tokenizer and rule set.
#[wasm_bindgen]
pub struct Checker {
    tokenizer: Arc<Tokenizer>,
    rules: Rules,
    style: Arc<StyleResources>,
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

        Ok(Checker {
            tokenizer: Arc::new(tokenizer),
            rules,
            style: Arc::new(StyleResources::new()),
        })
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
        suggestions_json(text, &self.rules.suggest(text, &self.tokenizer))
    }

    /// Checks one paragraph of a larger document on its own, for an editor that rechecks only
    /// the paragraphs that changed. Returns a [`ParagraphCheck`]: the paragraph's suggestions
    /// (JSON as from `check`, offsets into `text`) without those of the rules that need the whole
    /// document (`EN_CONSISTENT_APOS`, `EN_WORD_COHERENCY`, `EN_REPEATEDWORDS`,
    /// `TOO_OFTEN_USED_*`), and its digest, the bytes those rules need from it. Keep the digest
    /// with the paragraph, replace both when it changes, and pass all digests to
    /// `resolveDocument`.
    ///
    /// `endsParagraph` (default `true`) says the paragraph ends at the end of `text`, for an
    /// editor that stores paragraphs without the blank line after them; pass `false` for slices
    /// that keep their line breaks, where a paragraph ends only at a blank line.
    #[wasm_bindgen(js_name = checkParagraph)]
    pub fn check_paragraph(
        &self,
        text: &str,
        ends_paragraph: Option<bool>,
    ) -> Result<ParagraphCheck, JsValue> {
        let (suggestions, digest) =
            self.rules
                .check_paragraph(text, ends_paragraph.unwrap_or(true), &self.tokenizer);
        Ok(ParagraphCheck {
            suggestions: suggestions_json(text, &suggestions)?,
            digest: digest.to_bytes(),
        })
    }

    /// Runs the whole-document rules over `digests`, an array with the `Uint8Array` digest of
    /// every paragraph in document order. Resolve again after every edit: one change can add or
    /// remove matches in paragraphs that did not change. A digest from another engine version,
    /// or an element that is not a digest, is listed in `stale`.
    #[wasm_bindgen(js_name = resolveDocument)]
    pub fn resolve_document(&self, digests: js_sys::Array) -> DocumentResolution {
        let digests: Vec<Vec<u8>> = digests
            .iter()
            .map(|digest| {
                digest
                    .dyn_into::<js_sys::Uint8Array>()
                    .map(|bytes| bytes.to_vec())
                    .unwrap_or_default()
            })
            .collect();
        DocumentResolution {
            matches: self.rules.resolve_document_bytes(&digests),
        }
    }
}

/// One paragraph checked by `checkParagraph`.
#[wasm_bindgen]
pub struct ParagraphCheck {
    suggestions: String,
    digest: Vec<u8>,
}

#[wasm_bindgen]
impl ParagraphCheck {
    /// The paragraph's suggestions without the whole-document rules', as JSON in the shape of
    /// `check` (offsets into the paragraph).
    #[wasm_bindgen(getter)]
    pub fn suggestions(&self) -> String {
        self.suggestions.clone()
    }

    /// The paragraph's digest: no text, only offsets, flags and lemmas. Store it with the
    /// paragraph.
    #[wasm_bindgen(getter)]
    pub fn digest(&self) -> Vec<u8> {
        self.digest.clone()
    }
}

/// The whole-document rules' matches over a document's digests, from `resolveDocument`. It
/// replaces the document's previous resolution.
#[wasm_bindgen]
pub struct DocumentResolution {
    matches: nlprule::DocumentMatches,
}

/// A suggestion as `check` returns it; only the fields `DocumentResolution.suggestions` reads.
#[derive(Deserialize)]
struct CheckedItem {
    start: usize,
    end: usize,
    source: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    replacements: Vec<String>,
}

#[wasm_bindgen]
impl DocumentResolution {
    /// The indices of the digests that are from another engine version, or not digests: check
    /// those paragraphs again and resolve again. While any are listed, matches can be missing in
    /// every paragraph.
    #[wasm_bindgen(getter)]
    pub fn stale(&self) -> Vec<u32> {
        self.matches.stale.iter().map(|x| *x as u32).collect()
    }

    /// The indices of the paragraphs that have whole-document matches, ascending.
    #[wasm_bindgen(getter)]
    pub fn paragraphs(&self) -> Vec<u32> {
        let mut paragraphs: Vec<u32> = self
            .matches
            .matches
            .iter()
            .map(|x| x.paragraph() as u32)
            .collect();
        paragraphs.dedup();
        paragraphs
    }

    /// The suggestions of paragraph `paragraph`, whose text is `text`: `checked`, the
    /// `suggestions` JSON `checkParagraph` returned for it, merged with its whole-document
    /// matches, as JSON in the shape of `check`. A match whose span no longer fits `text` is
    /// left out. `checked` is read by its `start`/`end` char offsets; one that does not fit
    /// `text` throws (the Swift binding leaves it out instead).
    pub fn suggestions(
        &self,
        paragraph: u32,
        text: &str,
        checked: &str,
    ) -> Result<String, JsValue> {
        let checked: Vec<CheckedItem> = serde_json::from_str(checked)
            .map_err(|err| JsValue::from_str(&format!("invalid suggestions JSON: {err}")))?;
        let checked = checked
            .into_iter()
            .map(|item| {
                let span = nlprule::types::Span::from_chars(text, item.start..item.end)
                    .ok_or_else(|| {
                        JsValue::from_str(&format!(
                            "suggestion {}..{} is not in the paragraph's text",
                            item.start, item.end
                        ))
                    })?;
                Ok(nlprule::types::Suggestion::new(
                    item.source,
                    item.message,
                    span,
                    item.replacements,
                ))
            })
            .collect::<Result<Vec<_>, JsValue>>()?;
        suggestions_json(
            text,
            &self.matches.suggestions(paragraph as usize, text, checked),
        )
    }
}

/// `suggestions` of `text` as the JSON array `check` returns.
fn suggestions_json(
    text: &str,
    suggestions: &[nlprule::types::Suggestion],
) -> Result<String, JsValue> {
    let (utf16, total16) = utf16_offsets(
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
                start16: utf16.get(&span.start).copied().unwrap_or(total16),
                end16: utf16.get(&span.end).copied().unwrap_or(total16),
                source: suggestion.source().to_string(),
                message: suggestion.message().to_string(),
                replacements: suggestion.replacements().to_vec(),
            }
        })
        .collect();

    serde_json::to_string(&items)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize suggestions: {err}")))
}

#[wasm_bindgen]
impl Checker {
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

    #[wasm_bindgen(js_name = styleDocument)]
    /// Retains one analysis. Compact documents support only clichés and sentence length.
    pub fn style_document(
        &self,
        text: &str,
        options_json: Option<String>,
    ) -> Result<StyleDocument, JsValue> {
        let options =
            reports::parse_document_options(options_json.as_deref()).map_err(style_error)?;
        Ok(StyleDocument {
            inner: writemorebetter_style::StyleDocument::new(
                self.tokenizer.clone(),
                self.style.clone(),
                text,
                &options,
            ),
        })
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

    /// Enables rules that are off by default, such as LanguageTool's `picky` style rules, by
    /// the selector syntax profiles use (`"PLAIN_ENGLISH"`, `"STYLE/PASSIVE_VOICE"`,
    /// `"BUILTIN/EN_REDUNDANCY_REPLACE"`; see `docs/PROFILE.md`). It applies on top of the
    /// constructor's profile, so it can turn back on a rule the profile disabled. Returns how
    /// many rules changed from off to on. A selector that matches no rule is an error and
    /// nothing is enabled. Changes apply to the next `check`.
    #[wasm_bindgen(js_name = enableRules)]
    pub fn enable_rules(&mut self, selectors: Vec<String>) -> Result<usize, JsValue> {
        self.rules
            .enable_rules(&selectors)
            .map_err(|err| JsValue::from_str(&err.to_string()))
    }

    /// Sets the minimum percentage of the `TOO_OFTEN_USED_ADJECTIVE_EN`, `TOO_OFTEN_USED_NOUN_EN`
    /// and `TOO_OFTEN_USED_VERB_EN` rules: a word is flagged once it makes up at least `percent`
    /// of all the adjectives, nouns or verbs of the checked text. The default is 5. A value
    /// outside 1–100 is an error and nothing changes. It does not enable the rules, which are
    /// off by default (see `enableRules`). Changes apply to the next `check`.
    #[wasm_bindgen(js_name = setTooOftenUsedPercent)]
    pub fn set_too_often_used_percent(&mut self, percent: u32) -> Result<(), JsValue> {
        self.rules
            .set_too_often_used_percent(percent as usize)
            .map_err(|err| JsValue::from_str(&err.to_string()))
    }

    /// Sets the level of the `READABILITY_RULE_SIMPLE` and `READABILITY_RULE_DIFFICULT` rules,
    /// from 0 (very difficult) to 6 (very easy): `READABILITY_RULE_SIMPLE` flags a paragraph whose
    /// Flesch reading level is above it, `READABILITY_RULE_DIFFICULT` one whose level is below
    /// it. The default is 3. A value above 6 is an error and nothing changes. It does not enable
    /// the rules, which are off by default (see `enableRules`). Changes apply to the next `check`
    /// and `resolveDocument`.
    #[wasm_bindgen(js_name = setReadabilityLevel)]
    pub fn set_readability_level(&mut self, level: u32) -> Result<(), JsValue> {
        self.rules
            .set_readability_level(level as usize)
            .map_err(|err| JsValue::from_str(&err.to_string()))
    }
}

fn style_error(error: writemorebetter_style::StyleError) -> JsValue {
    let message = error.to_string();
    let js = js_sys::Error::new(&message);
    js.set_name("StyleError");
    if let Ok(serde_json::Value::Object(fields)) = serde_json::to_value(&error) {
        for (key, value) in fields.into_iter().filter(|(key, _)| key != "message") {
            if let Some(value) = value.as_str() {
                let _ =
                    js_sys::Reflect::set(&js, &JsValue::from_str(&key), &JsValue::from_str(value));
            }
        }
    }
    js.into()
}

#[wasm_bindgen]
pub struct StyleDocument {
    inner: writemorebetter_style::StyleDocument,
}

#[wasm_bindgen]
impl StyleDocument {
    /// Counts for the retained analysis, returned as DocumentStats JSON.
    pub fn stats(&self) -> String {
        reports::to_utf16_json(&self.inner.stats())
    }
    #[wasm_bindgen(js_name = echoes)]
    /// Returns this report on the retained analysis; options default when omitted.
    pub fn echoes(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .echoes(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
    #[wasm_bindgen(js_name = repeatedPhrases)]
    /// Returns this report on the retained analysis; options default when omitted.
    pub fn repeated_phrases(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .repeated_phrases(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
    #[wasm_bindgen(js_name = cliches)]
    /// Returns this report on the retained analysis; options default when omitted.
    pub fn cliches(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .cliches(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
    #[wasm_bindgen(js_name = dialogueTags)]
    /// Returns this report on the retained analysis; options default when omitted.
    /// Baseline: omitted/null means bundled; "none" disables comparisons; otherwise pass a JSON object.
    pub fn dialogue_tags(
        &self,
        options_json: Option<String>,
        baseline_json: Option<String>,
    ) -> Result<String, JsValue> {
        self.inner
            .dialogue_tags(options_json.as_deref(), baseline_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
    #[wasm_bindgen(js_name = sentenceLength)]
    /// Returns this report on the retained analysis; options default when omitted.
    /// Baseline: omitted/null means bundled; "none" disables comparisons; otherwise pass a JSON object.
    pub fn sentence_length(
        &self,
        options_json: Option<String>,
        baseline_json: Option<String>,
    ) -> Result<String, JsValue> {
        self.inner
            .sentence_length(options_json.as_deref(), baseline_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
    #[wasm_bindgen(js_name = sentenceStarts)]
    /// Returns this report on the retained analysis; options default when omitted.
    /// Baseline: omitted/null means bundled; "none" disables comparisons; otherwise pass a JSON object.
    pub fn sentence_starts(
        &self,
        options_json: Option<String>,
        baseline_json: Option<String>,
    ) -> Result<String, JsValue> {
        self.inner
            .sentence_starts(options_json.as_deref(), baseline_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
    #[wasm_bindgen(js_name = transitions)]
    pub fn transitions(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .transitions(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
    #[wasm_bindgen(js_name = pronounStarts)]
    pub fn pronoun_starts(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .pronoun_starts(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
    /// Returns manner adverbs on the retained analysis; options default when omitted.
    #[wasm_bindgen(js_name = adverbs)]
    pub fn adverbs(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .adverbs(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
    #[wasm_bindgen(js_name = sensoryBalance)]
    /// Returns this report on the retained analysis; options default when omitted.
    pub fn sensory_balance(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .sensory_balance(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
    #[wasm_bindgen(js_name = pacing)]
    /// Returns the opt-in pacing lens on the retained analysis.
    pub fn pacing(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .pacing(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
    #[wasm_bindgen(js_name = diction)]
    /// Returns this report on the retained analysis; options default when omitted.
    /// Table: a word-to-concreteness JSON object or TSV text; omitting it uses the bundled table.
    pub fn diction(
        &self,
        options_json: Option<String>,
        table: Option<String>,
    ) -> Result<String, JsValue> {
        self.inner
            .diction(options_json.as_deref(), table.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
    #[wasm_bindgen(js_name = overusedWords)]
    /// Returns this report on the retained analysis; options default when omitted.
    /// Baseline: omitted/null means bundled; "none" disables comparisons; otherwise pass a JSON object.
    pub fn overused_words(
        &self,
        options_json: Option<String>,
        baseline_json: Option<String>,
    ) -> Result<String, JsValue> {
        self.inner
            .overused_words(options_json.as_deref(), baseline_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
    #[wasm_bindgen(js_name = consistency)]
    /// Returns the opt-in consistency lens on the retained analysis; options default when omitted.
    pub fn consistency(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .consistency(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }

    #[wasm_bindgen(js_name = readability)]
    /// Returns the opt-in readability lens on the retained analysis; options default when omitted.
    pub fn readability(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .readability(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }

    #[wasm_bindgen(js_name = passive)]
    /// Returns this report on the retained analysis; options default when omitted.
    /// Baseline: omitted/null, "bundled" and "none" disable comparisons (no published band yet);
    /// "publicStandIn" selects the provisional public stand-in band; otherwise pass a JSON object.
    pub fn passive(
        &self,
        options_json: Option<String>,
        baseline_json: Option<String>,
    ) -> Result<String, JsValue> {
        self.inner
            .passive(options_json.as_deref(), baseline_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }

    #[wasm_bindgen(js_name = clarity)]
    /// Returns the opt-in clarity lens (sticky sentences, long subordinate clauses).
    pub fn clarity(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .clarity(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }

    #[wasm_bindgen(js_name = houseStyle)]
    /// Runs caller-supplied house-style rules (Vale-format YAML or JSON) on the retained analysis.
    /// Omitting the rules is an invalidRules error; options default when omitted.
    pub fn house_style(
        &self,
        options_json: Option<String>,
        rules: Option<String>,
    ) -> Result<String, JsValue> {
        self.inner
            .house_style(options_json.as_deref(), rules.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }

    #[wasm_bindgen(js_name = regionalSpelling)]
    /// Returns this report on the retained analysis; options default when omitted.
    pub fn regional_spelling(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .regional_spelling(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }

    #[wasm_bindgen(js_name = emotionTells)]
    /// Returns the opt-in emotion-tells lens on the retained analysis; narration only by default.
    pub fn emotion_tells(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .emotion_tells(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
}

#[wasm_bindgen(typescript_custom_section)]
const STYLE_RESULT_TYPES: &str = include_str!("../style-results.d.ts");

/// The built-in style presets' names; the first, `"chicago-fiction"`, is the default.
#[wasm_bindgen(js_name = stylePresetNames)]
pub fn style_preset_names() -> Vec<String> {
    writemorebetter_style::preset::StylePreset::names()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// A built-in style preset by name, resolved to self-contained `StylePreset` JSON.
#[wasm_bindgen(js_name = loadStylePreset)]
pub fn load_style_preset(name: &str) -> Result<String, JsValue> {
    writemorebetter_style::preset::StylePreset::builtin(name)
        .map(|preset| preset.to_json())
        .map_err(style_error)
}

/// Validates a preset file (JSON), resolves its `extends`, and returns self-contained
/// `StylePreset` JSON. Unknown keys and invalid values throw a `StyleError` (`invalidPreset`).
#[wasm_bindgen(js_name = parseStylePreset)]
pub fn parse_style_preset(text: &str) -> Result<String, JsValue> {
    writemorebetter_style::preset::StylePreset::parse(text)
        .map(|preset| preset.to_json())
        .map_err(style_error)
}

#[wasm_bindgen]
impl Checker {
    /// Applies a style preset's grammar settings: its profile, the rules it turns off and on, and
    /// its speller variant. `preset` is a built-in preset's name or a preset file (JSON). Every
    /// selector is checked first, so an unknown one throws and changes nothing. Returns how many
    /// rules went from off to on. Apply a preset to a new `Checker`; to switch presets, construct
    /// another (profiles cannot be undone).
    #[wasm_bindgen(js_name = applyPreset)]
    pub fn apply_preset(&mut self, preset: &str) -> Result<usize, JsValue> {
        writemorebetter_style::preset::StylePreset::load(preset)
            .and_then(|preset| preset.apply_to_rules(&mut self.rules))
            .map_err(style_error)
    }
}

#[wasm_bindgen]
impl StyleDocument {
    #[wasm_bindgen(js_name = presetChecks)]
    /// Runs the built-in preset checks; options are `PresetChecks` JSON (every check off by default).
    pub fn preset_checks(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .preset_checks(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }

    #[wasm_bindgen(js_name = preset)]
    /// Runs every report a style preset drives (consistency, house style, preset checks) and returns
    /// `PresetReport` JSON. `preset` is a built-in preset's name or a preset file (JSON); omitted
    /// selects `"chicago-fiction"`.
    pub fn preset(&self, preset: Option<String>) -> Result<String, JsValue> {
        self.inner
            .preset(preset.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
}

#[wasm_bindgen]
impl StyleDocument {
    #[wasm_bindgen(js_name = alliteration)]
    /// Returns the opt-in alliteration lens on the retained analysis; options default when omitted.
    pub fn alliteration(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .alliteration(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
}

/// Checks headings such as chapter titles against a title-case style and returns
/// `TitleCaseReport` JSON. `headingsJSON` is a JSON array of strings; options are
/// `TitleCaseOptions` JSON (Chicago-style headline case when omitted). Spans are UTF-16 offsets
/// into each heading.
#[wasm_bindgen(js_name = titleCase)]
pub fn title_case(headings_json: &str, options_json: Option<String>) -> Result<String, JsValue> {
    use writemorebetter_style::reports::title_case;
    let headings = title_case::parse_headings(headings_json).map_err(style_error)?;
    let options = title_case::parse_options(options_json.as_deref()).map_err(style_error)?;
    title_case::report(&headings, &options)
        .map(|report| reports::to_utf16_json(&report))
        .map_err(style_error)
}

#[wasm_bindgen]
impl StyleDocument {
    #[wasm_bindgen(js_name = drafting)]
    pub fn drafting(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .drafting(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }

    #[wasm_bindgen(js_name = vagueWords)]
    /// Returns the opt-in vague-words lens (fillers, hedges, vague quantities, nouns and
    /// adjectives) on the retained analysis, narration and dialogue counted separately.
    pub fn vague_words(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .vague_words(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }

    #[wasm_bindgen(js_name = hiddenVerbs)]
    /// Returns the opt-in hidden-verbs lens (nominalizations after a light verb or before *of*)
    /// on the retained analysis; each match names the related verb but never rewrites.
    pub fn hidden_verbs(&self, options_json: Option<String>) -> Result<String, JsValue> {
        self.inner
            .hidden_verbs(options_json.as_deref())
            .map(|report| reports::to_utf16_json(&report))
            .map_err(style_error)
    }
}
