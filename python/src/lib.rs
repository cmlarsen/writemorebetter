//! Python bindings for writemorebetter (the `writemorebetter._native` extension module).
//!
//! The compiled rule and dictionary binaries are LGPL-2.1 resources and are deliberately *not*
//! embedded in this module or the wheel: the caller passes the paths of `en_tokenizer.bin` and
//! `en_rules.bin` to [`Checker::new`], which keeps them as separately loaded data.
//!
//! Every offset this module returns counts Unicode scalar values (code points), which is what a
//! Python `str` index counts, so `text[start:end]` is the flagged span. Rich results (analysis,
//! documents, style reports) cross the boundary as JSON strings that the pure-Python wrapper in
//! `python/writemorebetter/` decodes; their spans are code-point ranges too.
//!
//! [`Checker`] is `Send + Sync`: the rule set sits behind a read/write lock, and the GIL is
//! released while loading, checking and analyzing, so one checker can serve many threads.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use nlprule::{Rules, Tokenizer};
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;
use serde::Serialize;
use writemorebetter_style::reports::{self, StyleResources};

create_exception!(
    writemorebetter,
    WriteMoreBetterError,
    PyException,
    "A data file was read but could not be loaded (corrupt, or built for another version)."
);
create_exception!(
    writemorebetter,
    StyleError,
    PyValueError,
    "Invalid input to a style report. `kind` names the error; `report` and `message` are set \
     when the error carries them."
);

fn style_error(error: writemorebetter_style::StyleError) -> PyErr {
    let message = error.to_string();
    let fields = match serde_json::to_value(&error) {
        Ok(serde_json::Value::Object(fields)) => fields,
        _ => Default::default(),
    };
    Python::attach(|py| {
        let err = StyleError::new_err(message);
        let value = err.value(py);
        for (key, field) in fields {
            if let Some(field) = field.as_str() {
                let _ = value.setattr(key.as_str(), field);
            }
        }
        err
    })
}

fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("binding records serialize to JSON")
}

/// A flagged span as `(start, end, source, message, replacements)`, in code points.
type SuggestionTuple = (usize, usize, String, String, Vec<String>);

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
            start: sentence.span.char.start,
            end: sentence.span.char.end,
            tokens: sentence
                .tokens
                .into_iter()
                .map(|token| AnalysisToken {
                    text: token.text,
                    start: token.span.char.start,
                    end: token.span.char.end,
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
    lemma_index: BTreeMap<String, Vec<TokenPosition>>,
    word_count: usize,
}

impl Document {
    fn new(document: writemorebetter_style::Document, token_details: bool) -> Self {
        Self {
            paragraphs: document
                .paragraphs
                .into_iter()
                .map(|paragraph| DocumentParagraph {
                    text: paragraph.text,
                    start: paragraph.span.char.start,
                    end: paragraph.span.char.end,
                    sentences: paragraph
                        .sentences
                        .into_iter()
                        .map(|sentence| DocumentSentence {
                            text: sentence.text,
                            start: sentence.span.char.start,
                            end: sentence.span.char.end,
                            tokens: sentence
                                .tokens
                                .into_iter()
                                .map(|token| DocumentToken {
                                    text: token.text,
                                    start: token.span.char.start,
                                    end: token.span.char.end,
                                    readings: token_details.then_some(token.readings),
                                    chunks: token_details.then_some(token.chunks),
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
                    start: span.char.start,
                    end: span.char.end,
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

/// A grammar and spelling checker over a loaded tokenizer and rule set.
#[pyclass(frozen, module = "writemorebetter._native")]
struct Checker {
    tokenizer: Arc<Tokenizer>,
    rules: RwLock<Rules>,
    style: Arc<StyleResources>,
}

impl Checker {
    fn read_rules(&self) -> RwLockReadGuard<'_, Rules> {
        self.rules
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_rules(&self) -> RwLockWriteGuard<'_, Rules> {
        self.rules
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn load(
        tokenizer_path: PathBuf,
        rules_path: PathBuf,
        spell_variant: Option<String>,
        profile: String,
    ) -> PyResult<Self> {
        let tokenizer_bytes = std::fs::read(&tokenizer_path)?;
        let rules_bytes = std::fs::read(&rules_path)?;
        let tokenizer = Tokenizer::from_reader(Cursor::new(tokenizer_bytes)).map_err(|err| {
            WriteMoreBetterError::new_err(format!(
                "failed to load tokenizer {}: {err}",
                tokenizer_path.display()
            ))
        })?;
        let mut rules = Rules::from_reader(Cursor::new(rules_bytes)).map_err(|err| {
            WriteMoreBetterError::new_err(format!(
                "failed to load rules {}: {err}",
                rules_path.display()
            ))
        })?;

        if !profile.is_empty() {
            let profile = nlprule::profile::Profile::by_name(&profile)
                .ok_or_else(|| PyValueError::new_err(format!("unknown profile: {profile}")))?;
            rules.apply_profile(&profile);
        }

        if let Some(variant) = spell_variant.filter(|v| !v.is_empty()) {
            let variant = rules
                .spell()
                .variant(&variant)
                .map_err(|err| PyValueError::new_err(format!("unknown spell variant: {err}")))?;
            rules.spell_mut().options_mut().variant = Some(variant);
        }

        Ok(Checker {
            tokenizer: Arc::new(tokenizer),
            rules: RwLock::new(rules),
            style: Arc::new(StyleResources::new()),
        })
    }
}

#[pymethods]
impl Checker {
    #[new]
    #[pyo3(signature = (tokenizer_path, rules_path, spell_variant=None, profile=String::from("fiction")))]
    fn new(
        py: Python<'_>,
        tokenizer_path: PathBuf,
        rules_path: PathBuf,
        spell_variant: Option<String>,
        profile: String,
    ) -> PyResult<Self> {
        py.detach(|| Checker::load(tokenizer_path, rules_path, spell_variant, profile))
    }

    /// Returns `(start, end, source, message, replacements)` tuples; offsets are code points.
    fn check(&self, py: Python<'_>, text: String) -> Vec<SuggestionTuple> {
        py.detach(|| {
            self.read_rules()
                .suggest(&text, &self.tokenizer)
                .iter()
                .map(|suggestion| {
                    let span = suggestion.span().char();
                    (
                        span.start,
                        span.end,
                        suggestion.source().to_string(),
                        suggestion.message().to_string(),
                        suggestion.replacements().to_vec(),
                    )
                })
                .collect()
        })
    }

    /// Returns the sentences and tagged tokens as JSON, with code-point offsets.
    fn analyze_json(&self, py: Python<'_>, text: String) -> String {
        py.detach(|| {
            let sentences: Vec<_> = self
                .tokenizer
                .analyze(&text)
                .into_iter()
                .map(AnalysisSentence::from)
                .collect();
            to_json(&sentences)
        })
    }

    #[pyo3(signature = (text, options_json=None))]
    fn analyze_document_json(
        &self,
        py: Python<'_>,
        text: String,
        options_json: Option<String>,
    ) -> PyResult<String> {
        let options =
            reports::parse_document_options(options_json.as_deref()).map_err(style_error)?;
        Ok(py.detach(|| {
            let document =
                writemorebetter_style::analyze_document(&self.tokenizer, &text, &options);
            to_json(&Document::new(document, options.include_token_details))
        }))
    }

    #[pyo3(signature = (text, options_json=None))]
    fn style_document(
        &self,
        py: Python<'_>,
        text: String,
        options_json: Option<String>,
    ) -> PyResult<StyleDocument> {
        let options =
            reports::parse_document_options(options_json.as_deref()).map_err(style_error)?;
        Ok(py.detach(|| StyleDocument {
            inner: writemorebetter_style::StyleDocument::new(
                self.tokenizer.clone(),
                self.style.clone(),
                &text,
                &options,
            ),
        }))
    }

    fn add_words(&self, words: Vec<String>) -> usize {
        self.write_rules().add_words(words)
    }

    fn remove_words(&self, words: Vec<String>) -> usize {
        self.write_rules().remove_words(words)
    }

    fn set_words(&self, words: Vec<String>) -> usize {
        self.write_rules().set_words(words)
    }

    fn clear_words(&self) {
        self.write_rules().clear_words();
    }

    fn words(&self) -> Vec<String> {
        self.read_rules().words()
    }

    #[pyo3(signature = (mother_tongue=None))]
    fn set_mother_tongue(&self, mother_tongue: Option<String>) {
        self.write_rules()
            .set_mother_tongue(mother_tongue.as_deref().filter(|code| !code.is_empty()));
    }

    fn mother_tongues(&self) -> Vec<String> {
        self.read_rules()
            .mother_tongues()
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    fn enable_rules(&self, selectors: Vec<String>) -> PyResult<usize> {
        self.write_rules()
            .enable_rules(&selectors)
            .map_err(|err| PyValueError::new_err(err.to_string()))
    }

    fn apply_preset(&self, preset: String) -> PyResult<usize> {
        writemorebetter_style::preset::StylePreset::load(&preset)
            .and_then(|preset| preset.apply_to_rules(&mut self.write_rules()))
            .map_err(style_error)
    }
}

/// One retained document analysis that every style report reads.
#[pyclass(frozen, module = "writemorebetter._native")]
struct StyleDocument {
    inner: writemorebetter_style::StyleDocument,
}

#[pymethods]
impl StyleDocument {
    fn stats_json(&self) -> String {
        reports::to_char_json(&self.inner.stats())
    }

    /// Runs one report by its snake_case name and returns it as JSON with code-point spans.
    /// `extra` is the report's second input: a baseline selector or JSON for the baselined
    /// reports, the concreteness table for `diction`, the rules for `house_style` and the preset
    /// for `preset`; the other reports take none.
    #[pyo3(signature = (name, options_json=None, extra=None))]
    fn report_json(
        &self,
        py: Python<'_>,
        name: String,
        options_json: Option<String>,
        extra: Option<String>,
    ) -> PyResult<String> {
        let doc = &self.inner;
        let options = options_json.as_deref();
        let extra = extra.as_deref();
        let no_extra = |name: &str| {
            if extra.is_some() {
                Err(PyValueError::new_err(format!(
                    "the {name} report takes no second input"
                )))
            } else {
                Ok(())
            }
        };
        macro_rules! run {
            ($call:expr) => {
                py.detach(|| $call.map(|report| reports::to_char_json(&report)))
                    .map_err(style_error)
            };
        }
        let name = name.as_str();
        match name {
            "echoes" => no_extra(name).and_then(|_| run!(doc.echoes(options))),
            "repeated_phrases" => no_extra(name).and_then(|_| run!(doc.repeated_phrases(options))),
            "cliches" => no_extra(name).and_then(|_| run!(doc.cliches(options))),
            "dialogue_tags" => run!(doc.dialogue_tags(options, extra)),
            "sentence_length" => run!(doc.sentence_length(options, extra)),
            "sentence_starts" => run!(doc.sentence_starts(options, extra)),
            "transitions" => no_extra(name).and_then(|_| run!(doc.transitions(options))),
            "pronoun_starts" => no_extra(name).and_then(|_| run!(doc.pronoun_starts(options))),
            "adverbs" => no_extra(name).and_then(|_| run!(doc.adverbs(options))),
            "sensory_balance" => no_extra(name).and_then(|_| run!(doc.sensory_balance(options))),
            "pacing" => no_extra(name).and_then(|_| run!(doc.pacing(options))),
            "diction" => run!(doc.diction(options, extra)),
            "overused_words" => run!(doc.overused_words(options, extra)),
            "consistency" => no_extra(name).and_then(|_| run!(doc.consistency(options))),
            "readability" => no_extra(name).and_then(|_| run!(doc.readability(options))),
            "passive" => run!(doc.passive(options, extra)),
            "clarity" => no_extra(name).and_then(|_| run!(doc.clarity(options))),
            "house_style" => run!(doc.house_style(options, extra)),
            "regional_spelling" => {
                no_extra(name).and_then(|_| run!(doc.regional_spelling(options)))
            }
            "emotion_tells" => no_extra(name).and_then(|_| run!(doc.emotion_tells(options))),
            "preset_checks" => no_extra(name).and_then(|_| run!(doc.preset_checks(options))),
            "preset" => {
                if options.is_some() {
                    return Err(PyValueError::new_err(
                        "the preset report takes the preset only, not options",
                    ));
                }
                run!(doc.preset(extra))
            }
            "alliteration" => no_extra(name).and_then(|_| run!(doc.alliteration(options))),
            "vague_words" => no_extra(name).and_then(|_| run!(doc.vague_words(options))),
            "hidden_verbs" => no_extra(name).and_then(|_| run!(doc.hidden_verbs(options))),
            _ => Err(PyValueError::new_err(format!("unknown report: {name}"))),
        }
    }
}

/// The built-in style presets' names; the first, `"chicago-fiction"`, is the default.
#[pyfunction]
fn style_preset_names() -> Vec<String> {
    writemorebetter_style::preset::StylePreset::names()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// A built-in style preset by name, resolved to self-contained `StylePreset` JSON.
#[pyfunction]
fn load_style_preset_json(name: String) -> PyResult<String> {
    writemorebetter_style::preset::StylePreset::builtin(&name)
        .map(|preset| preset.to_json())
        .map_err(style_error)
}

/// Validates a preset file (JSON), resolves its `extends`, and returns `StylePreset` JSON.
#[pyfunction]
fn parse_style_preset_json(text: String) -> PyResult<String> {
    writemorebetter_style::preset::StylePreset::parse(&text)
        .map(|preset| preset.to_json())
        .map_err(style_error)
}

/// Checks headings against a title-case style; spans are code-point offsets into each heading.
#[pyfunction]
#[pyo3(signature = (headings, options_json=None))]
fn title_case_json(headings: Vec<String>, options_json: Option<String>) -> PyResult<String> {
    use writemorebetter_style::reports::title_case;
    let options = title_case::parse_options(options_json.as_deref()).map_err(style_error)?;
    title_case::report(&headings, &options)
        .map(|report| reports::to_char_json(&report))
        .map_err(style_error)
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Checker>()?;
    m.add_class::<StyleDocument>()?;
    m.add(
        "WriteMoreBetterError",
        m.py().get_type::<WriteMoreBetterError>(),
    )?;
    m.add("StyleError", m.py().get_type::<StyleError>())?;
    m.add_function(wrap_pyfunction!(style_preset_names, m)?)?;
    m.add_function(wrap_pyfunction!(load_style_preset_json, m)?)?;
    m.add_function(wrap_pyfunction!(parse_style_preset_json, m)?)?;
    m.add_function(wrap_pyfunction!(title_case_json, m)?)?;
    Ok(())
}
