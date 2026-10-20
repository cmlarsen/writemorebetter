//! House-style rules: a subset of Vale's rule schema (existence, substitution, consistency,
//! occurrence) read from caller-supplied YAML or JSON and matched over the document text.
//! Vale (MIT) defines the schema; this is an independent implementation. See
//! docs/STYLE-HOUSE-STYLE.md.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::Range;

use regex::{Regex, RegexBuilder, RegexSet, RegexSetBuilder};
use regex_syntax::ast::ErrorKind;
use regex_syntax::hir::{
    Capture, Class, ClassBytes, ClassBytesRange, ClassUnicode, ClassUnicodeRange, Hir, HirKind,
    Repetition,
};
use saphyr_parser::{Event, Parser, ScalarStyle};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{word_split, Split};
use crate::error::StyleError;
use crate::{Document, Mode, Span};

pub(crate) const REPORT: &str = "houseStyle";

// Each compiled pattern is bounded so a hostile rule file cannot exhaust memory.
const REGEX_SIZE_LIMIT: usize = 1 << 20;
const CLITICS: &[&str] = &["s", "d", "ll", "re", "ve", "m"];
// After a rejected match the search retries one character later, which can rescan the rest of
// the text. Retries stop once they could have rescanned it this many times over (plus a fixed
// allowance for short paragraphs); later rejected matches are skipped whole.
const RETRY_RESCANS: usize = 4;
const RETRY_ALLOWANCE: usize = 64 * 1024;
// Every character `char::is_whitespace` accepts, and the curly apostrophes; both fold for matching.
const SPACES: &[(char, char)] = &[
    ('\t', '\r'),
    (' ', ' '),
    ('\u{85}', '\u{85}'),
    ('\u{a0}', '\u{a0}'),
    ('\u{1680}', '\u{1680}'),
    ('\u{2000}', '\u{200a}'),
    ('\u{2028}', '\u{2029}'),
    ('\u{202f}', '\u{202f}'),
    ('\u{205f}', '\u{205f}'),
    ('\u{3000}', '\u{3000}'),
];
const APOSTROPHES: &[(char, char)] = &[('\u{2bc}', '\u{2bc}'), ('\u{2018}', '\u{2019}')];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum HouseStyleExtends {
    Existence,
    Substitution,
    Consistency,
    Occurrence,
}

impl fmt::Display for HouseStyleExtends {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Existence => "existence",
            Self::Substitution => "substitution",
            Self::Consistency => "consistency",
            Self::Occurrence => "occurrence",
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum HouseStyleLevel {
    #[default]
    Suggestion,
    Warning,
    Error,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum HouseStyleScope {
    #[default]
    Text,
    Paragraph,
    Sentence,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// One rule, in Vale's lowercase key spelling so a Vale rule file reads unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct HouseStyleRule {
    pub name: String,
    pub extends: HouseStyleExtends,
    pub message: String,
    pub level: HouseStyleLevel,
    #[serde(skip_serializing_if = "is_default")]
    pub scope: HouseStyleScope,
    #[serde(skip_serializing_if = "is_default")]
    pub ignorecase: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub nonword: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tokens: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub swap: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub either: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exceptions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

/// A YAML or JSON scalar read as text, so `max: 3` and `tokens: [24, ok]` both parse.
struct Text(String);

impl<'de> Deserialize<'de> for Text {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::String(text) => Ok(Self(text)),
            Value::Number(number) => Ok(Self(number.to_string())),
            Value::Bool(flag) => Ok(Self(flag.to_string())),
            other => Err(serde::de::Error::custom(format!(
                "expected text, found {other}"
            ))),
        }
    }
}

fn texts(items: Vec<Text>) -> Vec<String> {
    items.into_iter().map(|text| text.0).collect()
}

// Vale keys this engine accepts but has no use for: it never rewrites text (`action`), has no
// vocabularies (`vocab`) and reads plain prose without markup (`code`).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleSpec {
    #[serde(default)]
    name: Option<Text>,
    extends: String,
    message: Option<Text>,
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    ignorecase: bool,
    #[serde(default)]
    nonword: bool,
    #[serde(default)]
    tokens: Option<Vec<Text>>,
    #[serde(default)]
    swap: Option<BTreeMap<String, Text>>,
    #[serde(default)]
    either: Option<BTreeMap<String, Text>>,
    #[serde(default)]
    token: Option<Text>,
    #[serde(default)]
    max: Option<usize>,
    #[serde(default)]
    min: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    exceptions: Option<Vec<Text>>,
    #[serde(default)]
    link: Option<Text>,
    #[serde(default, rename = "description")]
    _description: Option<serde::de::IgnoredAny>,
    #[serde(default, rename = "action")]
    _action: Option<serde::de::IgnoredAny>,
    #[serde(default, rename = "vocab")]
    _vocab: Option<serde::de::IgnoredAny>,
    #[serde(default, rename = "code")]
    _code: Option<serde::de::IgnoredAny>,
}

fn invalid(rule: Option<&str>, message: impl fmt::Display) -> StyleError {
    StyleError::rules(REPORT, rule, message)
}

impl RuleSpec {
    fn into_rule(self, name: Option<String>) -> Result<HouseStyleRule, StyleError> {
        let name = name
            .or(self.name.map(|text| text.0))
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| invalid(None, "every rule needs a name"))?;
        let fail = |message: String| invalid(Some(&name), message);
        let extends = match self.extends.as_str() {
            "existence" => HouseStyleExtends::Existence,
            "substitution" => HouseStyleExtends::Substitution,
            "consistency" => HouseStyleExtends::Consistency,
            "occurrence" => HouseStyleExtends::Occurrence,
            other => {
                return Err(fail(format!(
                    "extends `{other}` is not supported; use existence, substitution, consistency or occurrence"
                )))
            }
        };
        let level = match self.level.as_deref() {
            None | Some("suggestion") => HouseStyleLevel::Suggestion,
            Some("warning") => HouseStyleLevel::Warning,
            Some("error") => HouseStyleLevel::Error,
            Some(other) => {
                return Err(fail(format!(
                    "level `{other}` is not one of suggestion, warning, error"
                )))
            }
        };
        let scope = match self.scope.as_deref() {
            None | Some("text") => HouseStyleScope::Text,
            Some("paragraph") => HouseStyleScope::Paragraph,
            Some("sentence") => HouseStyleScope::Sentence,
            Some(other) => {
                return Err(fail(format!(
                    "scope `{other}` is not supported; plain prose has text, paragraph and sentence"
                )))
            }
        };
        let message = self
            .message
            .map(|text| text.0)
            .ok_or_else(|| fail("a rule needs a message".into()))?;
        let rule = HouseStyleRule {
            name: name.clone(),
            extends,
            message,
            level,
            scope,
            ignorecase: self.ignorecase,
            nonword: self.nonword,
            tokens: self.tokens.map(texts).unwrap_or_default(),
            swap: pairs(self.swap),
            either: pairs(self.either),
            token: self.token.map(|text| text.0),
            max: self.max,
            min: self.min,
            limit: self.limit,
            exceptions: self.exceptions.map(texts).unwrap_or_default(),
            link: self.link.map(|text| text.0),
        };
        rule.validate()?;
        Ok(rule)
    }
}

fn pairs(map: Option<BTreeMap<String, Text>>) -> BTreeMap<String, String> {
    map.into_iter()
        .flatten()
        .map(|(key, value)| (key, value.0))
        .collect()
}

impl HouseStyleRule {
    /// Parses one Vale rule file (YAML or JSON). Vale names a rule after its file, so the
    /// caller passes the name, for example `"House.Toward"`.
    pub fn parse(name: &str, text: &str) -> Result<Self, StyleError> {
        let mut documents = documents(text)?
            .into_iter()
            .filter(|document| !document.is_null());
        let (Some(document), None) = (documents.next(), documents.next()) else {
            return Err(invalid(Some(name), "expected exactly one rule"));
        };
        rule_from_value(document, Some(name.to_owned()))
    }

    fn validate(&self) -> Result<(), StyleError> {
        use HouseStyleExtends::*;
        let fail = |message: &str| Err(invalid(Some(&self.name), message));
        let used = [
            ("tokens", !self.tokens.is_empty(), self.extends == Existence),
            ("swap", !self.swap.is_empty(), self.extends == Substitution),
            (
                "either",
                !self.either.is_empty(),
                self.extends == Consistency,
            ),
            ("token", self.token.is_some(), self.extends == Occurrence),
            ("max", self.max.is_some(), self.extends == Occurrence),
            ("min", self.min.is_some(), self.extends == Occurrence),
            (
                "exceptions",
                !self.exceptions.is_empty(),
                matches!(self.extends, Existence | Substitution),
            ),
        ];
        for (key, present, allowed) in used {
            if present && !allowed {
                return Err(invalid(
                    Some(&self.name),
                    format!("`{key}` is not used by {} rules", self.extends),
                ));
            }
        }
        match self.extends {
            Existence if self.tokens.is_empty() => fail("existence rules need `tokens`"),
            Substitution if self.swap.is_empty() => fail("substitution rules need `swap`"),
            Consistency if self.either.is_empty() => fail("consistency rules need `either`"),
            Occurrence if self.token.is_none() => fail("occurrence rules need `token`"),
            Occurrence if self.max.is_none() && self.min.is_none() => {
                fail("occurrence rules need `max` or `min`")
            }
            _ => Ok(()),
        }
    }
}

fn rule_from_value(value: Value, name: Option<String>) -> Result<HouseStyleRule, StyleError> {
    let label = name
        .clone()
        .or_else(|| value.get("name").and_then(Value::as_str).map(str::to_owned));
    serde_json::from_value::<RuleSpec>(value)
        .map_err(|error| invalid(label.as_deref(), error))?
        .into_rule(name)
}

/// A rule bundle, compiled once and reused across documents.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HouseStyleRules {
    rules: Vec<HouseStyleRule>,
    // `None` for a rule in `unsupported`.
    #[serde(skip)]
    compiled: Vec<Option<Compiled>>,
    #[serde(skip)]
    unsupported: Vec<HouseStyleRuleError>,
}

/// A rule that parsed but cannot run here, such as one whose pattern uses lookaround or a
/// backreference (Vale's regex engine has both; this one has neither). The rest of the bundle runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HouseStyleRuleError {
    pub rule: String,
    pub message: String,
}

impl PartialEq for HouseStyleRules {
    fn eq(&self, other: &Self) -> bool {
        self.rules == other.rules
    }
}

impl HouseStyleRules {
    /// Parses a bundle from YAML or JSON text. Accepted shapes, in one document or several
    /// `---`-separated YAML documents: a single rule mapping with `name`, a list of rule mappings
    /// each with `name`, or `{rules: …}` holding such a list or a map from name to rule.
    pub fn parse(text: &str) -> Result<Self, StyleError> {
        let mut rules = Vec::new();
        for document in documents(text)? {
            collect_rules(document, &mut rules)?;
        }
        Self::new(rules)
    }

    pub fn new(rules: Vec<HouseStyleRule>) -> Result<Self, StyleError> {
        let mut names = BTreeSet::new();
        let mut compiled = Vec::with_capacity(rules.len());
        let mut unsupported = Vec::new();
        for rule in &rules {
            if !names.insert(rule.name.as_str()) {
                return Err(invalid(Some(&rule.name), "two rules share this name"));
            }
            rule.validate()?;
            match Compiled::new(rule) {
                Ok(rule) => compiled.push(Some(rule)),
                Err(Failure::Unsupported(message)) => {
                    compiled.push(None);
                    unsupported.push(HouseStyleRuleError {
                        rule: rule.name.clone(),
                        message,
                    });
                }
                Err(Failure::Invalid(error)) => return Err(error),
            }
        }
        Ok(Self {
            rules,
            compiled,
            unsupported,
        })
    }

    pub fn rules(&self) -> &[HouseStyleRule] {
        &self.rules
    }

    /// The rules that are kept in the bundle but skipped when it runs.
    pub fn unsupported(&self) -> &[HouseStyleRuleError] {
        &self.unsupported
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The bundle as `{"rules": [...]}` JSON, which `parse` reads back unchanged.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("house-style rules serialize to JSON")
    }
}

fn collect_rules(value: Value, rules: &mut Vec<HouseStyleRule>) -> Result<(), StyleError> {
    match value {
        Value::Null => Ok(()),
        Value::Array(items) => {
            for item in items {
                rules.push(rule_from_value(item, None)?);
            }
            Ok(())
        }
        Value::Object(map) if map.contains_key("extends") => {
            rules.push(rule_from_value(Value::Object(map), None)?);
            Ok(())
        }
        Value::Object(mut map) if map.len() == 1 && map.contains_key("rules") => {
            match map.remove("rules").unwrap_or_default() {
                Value::Object(named) => {
                    for (name, rule) in named {
                        rules.push(rule_from_value(rule, Some(name))?);
                    }
                    Ok(())
                }
                list @ Value::Array(_) => collect_rules(list, rules),
                _ => Err(invalid(None, "`rules` must be a list or a map of rules")),
            }
        }
        _ => Err(invalid(
            None,
            "expected a rule, a list of rules, or {rules: ...}",
        )),
    }
}

/// Reads YAML (or JSON, which is valid YAML) into JSON values, one per YAML document. JSON goes
/// through the YAML parser too, so a duplicate key is an error in both.
fn documents(text: &str) -> Result<Vec<Value>, StyleError> {
    enum Frame {
        List(Vec<Value>),
        Map(Map<String, Value>, Option<String>),
    }
    let mut documents = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    for event in Parser::new_from_str(text) {
        let (event, span) = event.map_err(|error| {
            invalid(
                None,
                format!(
                    "YAML line {} column {}: {}",
                    error.marker().line(),
                    error.marker().col() + 1,
                    error.info()
                ),
            )
        })?;
        let line = span.start.line();
        let value = match event {
            Event::SequenceStart(..) => {
                stack.push(Frame::List(Vec::new()));
                continue;
            }
            Event::MappingStart(..) => {
                stack.push(Frame::Map(Map::new(), None));
                continue;
            }
            Event::SequenceEnd | Event::MappingEnd => match stack.pop() {
                Some(Frame::List(items)) => Value::Array(items),
                Some(Frame::Map(map, _)) => Value::Object(map),
                None => continue,
            },
            Event::Scalar(text, style, _, tag) => {
                if let Some(Frame::Map(_, key @ None)) = stack.last_mut() {
                    *key = Some(text.into_owned());
                    continue;
                }
                let tagged_text = tag.is_some_and(|tag| tag.suffix == "str");
                scalar(&text, style == ScalarStyle::Plain && !tagged_text)
            }
            Event::Alias(_) => {
                return Err(invalid(
                    None,
                    format!("YAML line {line}: aliases are not supported"),
                ))
            }
            _ => continue,
        };
        match stack.last_mut() {
            None => documents.push(value),
            Some(Frame::List(items)) => items.push(value),
            Some(Frame::Map(map, key)) => {
                let Some(key) = key.take() else {
                    return Err(invalid(
                        None,
                        format!("YAML line {line}: mapping keys must be plain text"),
                    ));
                };
                if map.contains_key(&key) {
                    return Err(invalid(
                        None,
                        format!("YAML line {line}: duplicate key `{key}`"),
                    ));
                }
                map.insert(key, value);
            }
        }
    }
    Ok(documents)
}

// YAML 1.2 core schema for plain scalars; floats stay text so a token like `1.50` survives.
fn scalar(text: &str, plain: bool) -> Value {
    if !plain {
        return Value::String(text.to_owned());
    }
    match text {
        "" | "~" | "null" | "Null" | "NULL" => Value::Null,
        "true" | "True" | "TRUE" => Value::Bool(true),
        "false" | "False" | "FALSE" => Value::Bool(false),
        _ => text
            .parse::<i64>()
            .map_or_else(|_| Value::String(text.to_owned()), Value::from),
    }
}

#[derive(Debug, Clone)]
struct Compiled {
    // existence: tokens; substitution: swap keys; occurrence: the one token.
    patterns: Vec<Regex>,
    // All of `patterns` at once, to skip a unit in one pass when none of them occurs in it.
    // `None` when the combined set is over the size limit.
    set: Option<RegexSet>,
    replacements: Vec<Option<String>>,
    either: Vec<[(Regex, String); 2]>,
    // All exceptions as one alternation, searched for within a match.
    exceptions: Option<Regex>,
}

enum Failure {
    Unsupported(String),
    Invalid(StyleError),
}

fn fold_char(ch: char) -> char {
    match ch {
        '’' | '‘' | 'ʼ' => '\'',
        ch if ch.is_whitespace() => ' ',
        ch => ch,
    }
}

/// Folds text as matching sees it: curly apostrophes to `'`, every whitespace character to a space.
fn fold(text: &str) -> String {
    text.chars().map(fold_char).collect()
}

/// Rewrites a parsed pattern to match folded text. Literal characters fold as the text does, and
/// a class that holds a whitespace character or a curly apostrophe also matches its fold, so
/// `a\nlot`, `[\u{2028}]` and a literal no-break space all match a folded space.
fn fold_hir(hir: &Hir) -> Hir {
    match hir.kind() {
        HirKind::Literal(literal) => match std::str::from_utf8(&literal.0) {
            Ok(text) => Hir::literal(fold(text).into_bytes()),
            Err(_) => hir.clone(),
        },
        HirKind::Class(Class::Unicode(class)) => {
            let mut class = class.clone();
            for (held, to) in [(SPACES, ' '), (APOSTROPHES, '\'')] {
                let mut held = ClassUnicode::new(
                    held.iter()
                        .map(|&(start, end)| ClassUnicodeRange::new(start, end)),
                );
                held.intersect(&class);
                if !held.ranges().is_empty() {
                    class.push(ClassUnicodeRange::new(to, to));
                }
            }
            Hir::class(Class::Unicode(class))
        }
        HirKind::Class(Class::Bytes(class)) => {
            let mut class = class.clone();
            let mut held = ClassBytes::new([ClassBytesRange::new(b'\t', b'\r')]);
            held.intersect(&class);
            if !held.ranges().is_empty() {
                class.push(ClassBytesRange::new(b' ', b' '));
            }
            Hir::class(Class::Bytes(class))
        }
        HirKind::Repetition(repetition) => Hir::repetition(Repetition {
            min: repetition.min,
            max: repetition.max,
            greedy: repetition.greedy,
            sub: Box::new(fold_hir(&repetition.sub)),
        }),
        HirKind::Capture(capture) => Hir::capture(Capture {
            index: capture.index,
            name: capture.name.clone(),
            sub: Box::new(fold_hir(&capture.sub)),
        }),
        HirKind::Concat(items) => Hir::concat(items.iter().map(fold_hir).collect()),
        HirKind::Alternation(items) => Hir::alternation(items.iter().map(fold_hir).collect()),
        HirKind::Empty | HirKind::Look(_) => hir.clone(),
    }
}

/// Parses a pattern and returns it folded, as regex syntax. Lookaround and backreferences, which
/// Vale's regexp2 engine supports and `regex` does not, are `Unsupported`; other errors are invalid.
fn folded_pattern(rule: &str, pattern: &str, ignorecase: bool) -> Result<String, Failure> {
    regex_syntax::ParserBuilder::new()
        .case_insensitive(ignorecase)
        .multi_line(true)
        .build()
        .parse(pattern)
        .map(|hir| fold_hir(&hir).to_string())
        .map_err(|error| {
            let message = format!("pattern `{pattern}`: {error}");
            match &error {
                regex_syntax::Error::Parse(parse)
                    if matches!(
                        parse.kind(),
                        ErrorKind::UnsupportedLookAround | ErrorKind::UnsupportedBackreference
                    ) =>
                {
                    Failure::Unsupported(message)
                }
                _ => Failure::Invalid(invalid(Some(rule), message)),
            }
        })
}

fn build(rule: &str, pattern: &str, folded: &str) -> Result<Regex, Failure> {
    RegexBuilder::new(folded)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
        .map_err(|error| {
            Failure::Invalid(invalid(Some(rule), format!("pattern `{pattern}`: {error}")))
        })
}

/// Vale's exception alternation: longest term first, a plain term's periods literal, and each
/// term case-sensitive unless it starts with `(?i)`, whatever the rule's `ignorecase`.
fn exception_pattern(rule: &str, exceptions: &[String]) -> Result<Option<String>, Failure> {
    if exceptions.is_empty() {
        return Ok(None);
    }
    let mut terms: Vec<&String> = exceptions.iter().collect();
    terms.sort_by_key(|term| std::cmp::Reverse(term.len()));
    let mut alternatives = Vec::with_capacity(terms.len());
    for term in terms {
        let term = if term.chars().any(|ch| r"\+*?()|[]{}^$".contains(ch)) {
            term.clone()
        } else {
            term.replace('.', r"\.")
        };
        let term = if term.starts_with("(?i)") {
            term
        } else {
            format!("(?-i){term}")
        };
        // Checked alone first, so an error names the term rather than the whole alternation.
        folded_pattern(rule, &term, false)?;
        alternatives.push(format!("(?:{term})"));
    }
    folded_pattern(rule, &alternatives.join("|"), false).map(Some)
}

impl Compiled {
    fn new(rule: &HouseStyleRule) -> Result<Self, Failure> {
        let name = rule.name.as_str();
        let compile = |pattern: &str| {
            let folded = folded_pattern(name, pattern, rule.ignorecase)?;
            Ok::<_, Failure>((build(name, pattern, &folded)?, folded))
        };
        let exceptions = match exception_pattern(name, &rule.exceptions)? {
            Some(folded) => Some(build(name, &rule.exceptions.join("|"), &folded)?),
            None => None,
        };
        let mut compiled = Self {
            patterns: Vec::new(),
            set: None,
            replacements: Vec::new(),
            either: Vec::new(),
            exceptions,
        };
        let mut sources = Vec::new();
        for token in rule.tokens.iter().chain(&rule.token) {
            let (pattern, folded) = compile(token)?;
            compiled.patterns.push(pattern);
            compiled.replacements.push(None);
            sources.push(folded);
        }
        for (pattern, replacement) in &rule.swap {
            let (pattern, folded) = compile(pattern)?;
            compiled.patterns.push(pattern);
            compiled.replacements.push(Some(replacement.clone()));
            sources.push(folded);
        }
        compiled.set = RegexSetBuilder::new(&sources)
            .size_limit(REGEX_SIZE_LIMIT * 8)
            .build()
            .ok();
        for (left, right) in &rule.either {
            compiled.either.push([
                (compile(left)?.0, left.clone()),
                (compile(right)?.0, right.clone()),
            ]);
        }
        Ok(compiled)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct HouseStyleOptions {
    /// Leave quoted speech out entirely: no matches in it, and it never counts toward
    /// consistency or occurrence.
    #[serde(alias = "skip_dialogue")]
    pub skip_dialogue: bool,
}

impl HouseStyleOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_skip_dialogue(mut self, skip_dialogue: bool) -> Self {
        self.skip_dialogue = skip_dialogue;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HouseStyleMatch {
    pub rule: String,
    pub extends: HouseStyleExtends,
    pub level: HouseStyleLevel,
    pub message: String,
    pub span: Span,
    pub mode: Mode,
    /// Substitution: the rule's listed replacement. Consistency: the variant used first.
    pub replacement: Option<String>,
    /// Occurrence: how many times the token appeared in the scope.
    pub count: Option<usize>,
    pub link: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HouseStyleCounts {
    pub suggestions: usize,
    pub warnings: usize,
    pub errors: usize,
    pub words: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HouseStyleReport {
    pub rules: usize,
    /// Rules in the bundle that were skipped because they cannot run here.
    pub unsupported: Vec<HouseStyleRuleError>,
    pub matches: Vec<HouseStyleMatch>,
    pub groups: Split<HouseStyleCounts>,
}

/// A paragraph or sentence of text, with curly apostrophes folded to `'` and every whitespace
/// character to a space, so rules written with straight quotes and spaces match fiction.
/// Folding is char-for-char, so `starts` maps folded byte offsets back to original ones.
struct Unit<'a> {
    text: &'a str,
    span: &'a Span,
    folded: String,
    starts: Vec<usize>,
    group: usize,
    dialogue_only: bool,
}

impl<'a> Unit<'a> {
    fn new(text: &'a str, span: &'a Span, group: usize, dialogue_only: bool) -> Self {
        let mut folded = String::with_capacity(text.len());
        let mut starts = Vec::with_capacity(text.len() + 1);
        for (index, ch) in text.char_indices() {
            let ch = fold_char(ch);
            starts.extend(std::iter::repeat_n(index, ch.len_utf8()));
            folded.push(ch);
        }
        starts.push(text.len());
        Self {
            text,
            span,
            folded,
            starts,
            group,
            dialogue_only,
        }
    }

    fn original(&self, folded: Range<usize>) -> Range<usize> {
        self.starts[folded.start]..self.starts[folded.end]
    }

    fn span(&self, range: Range<usize>) -> Span {
        let before = &self.text[..range.start];
        let inside = &self.text[range.clone()];
        let char_start = self.span.char.start + before.chars().count();
        let utf16_start = self.span.utf16.start + before.encode_utf16().count();
        Span {
            byte: self.span.byte.start + range.start..self.span.byte.start + range.end,
            char: char_start..char_start + inside.chars().count(),
            utf16: utf16_start..utf16_start + inside.encode_utf16().count(),
        }
    }
}

fn wordish(text: &str, index: usize, ch: char) -> bool {
    if ch.is_alphanumeric() || ch == '_' {
        return true;
    }
    ch == '\''
        && text[..index]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
        && text[index + 1..]
            .chars()
            .next()
            .is_some_and(char::is_alphanumeric)
}

/// A word boundary that treats an apostrophe inside a word as part of it, so `don` does not
/// match in *don't*, while a word followed by a clitic (*advisor's*, *she'd*) still ends there.
fn boundary(text: &str, at: usize) -> bool {
    let Some(before) = text[..at].chars().next_back() else {
        return true;
    };
    let Some(after) = text[at..].chars().next() else {
        return true;
    };
    let before_index = at - before.len_utf8();
    if !wordish(text, before_index, before) || !wordish(text, at, after) {
        return true;
    }
    if after != '\'' || !before.is_alphanumeric() {
        return false;
    }
    let rest = &text[at + 1..];
    let clitic: String = rest.chars().take_while(|ch| ch.is_alphanumeric()).collect();
    let following = rest[clitic.len()..].chars().next();
    CLITICS.contains(&clitic.to_lowercase().as_str())
        && !following.is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
}

fn find(pattern: &Regex, text: &str, nonword: bool) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut budget = RETRY_RESCANS * text.len() + RETRY_ALLOWANCE;
    let mut at = 0;
    while at <= text.len() {
        let Some(m) = pattern.find_at(text, at) else {
            break;
        };
        if !m.is_empty() && (nonword || (boundary(text, m.start()) && boundary(text, m.end()))) {
            found.push(m.range());
            at = m.end();
            continue;
        }
        // Retry one character later: a shorter match may start inside a rejected one. A retry
        // can rescan everything after `at`, so it is charged that much; once the budget is spent,
        // skip the rejected match instead, which keeps a greedy token such as `.*a` from making a
        // long paragraph quadratic.
        let next = m.start() + text[m.start()..].chars().next().map_or(1, char::len_utf8);
        let cost = text.len() - at;
        if cost <= budget {
            budget -= cost;
            at = next;
        } else {
            at = m.end().max(next);
        }
    }
    found
}

fn first_word(text: &str) -> Option<Range<usize>> {
    let (start, _) = text.char_indices().find(|(_, ch)| ch.is_alphanumeric())?;
    let end = text[start..]
        .char_indices()
        .skip(1)
        .map(|(index, _)| start + index)
        .find(|&index| boundary(text, index))
        .unwrap_or(text.len());
    Some(start..end)
}

struct Candidate {
    unit: usize,
    range: Range<usize>,
    alternative: usize,
}

struct Context<'a> {
    document: &'a Document,
    options: &'a HouseStyleOptions,
    units: Vec<Unit<'a>>,
    skipped: Vec<Range<usize>>,
}

impl Context<'_> {
    fn mode(&self, unit: &Unit, range: &Range<usize>) -> Mode {
        self.document.mode_at(unit.span.byte.start + range.start)
    }

    // A match is kept unless it is in skipped dialogue or touches a skipped name.
    fn keep(&self, unit: &Unit, range: &Range<usize>) -> bool {
        let start = unit.span.byte.start + range.start;
        let end = unit.span.byte.start + range.end;
        // Skipped tokens are in text order, so the first one ending after `start` is the only
        // candidate for an overlap.
        let next = self.skipped.partition_point(|skipped| skipped.end <= start);
        !(self.options.skip_dialogue && self.mode(unit, range) == Mode::Dialogue)
            && !self
                .skipped
                .get(next)
                .is_some_and(|skipped| skipped.start < end)
    }

    fn find(&self, rule: &HouseStyleRule, pattern: &Regex, unit: usize) -> Vec<Range<usize>> {
        let unit = &self.units[unit];
        find(pattern, &unit.folded, rule.nonword)
            .into_iter()
            .map(|range| unit.original(range))
            .filter(|range| self.keep(unit, range))
            .collect()
    }
}

// `count` lets `%d` take an argument too, as in Vale's occurrence messages.
fn format_message(message: &str, arguments: &[&str], count: bool) -> String {
    let mut out = String::with_capacity(message.len());
    let mut arguments = arguments.iter();
    let mut rest = message;
    while let Some(index) = rest.find('%') {
        out.push_str(&rest[..index]);
        let tail = &rest[index..];
        if let Some(after) = tail.strip_prefix("%%") {
            out.push('%');
            rest = after;
        } else if let Some((argument, after)) = tail
            .strip_prefix("%s")
            .or_else(|| tail.strip_prefix("%d").filter(|_| count))
            .and_then(|after| Some((arguments.next()?, after)))
        {
            out.push_str(argument);
            rest = after;
        } else {
            out.push('%');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

// Keeps the longest of overlapping candidates, the earlier-listed one on a tie.
fn without_overlaps(mut candidates: Vec<Candidate>) -> Vec<Candidate> {
    candidates.sort_by(|a, b| {
        (a.unit, a.range.start, b.range.end, a.alternative).cmp(&(
            b.unit,
            b.range.start,
            a.range.end,
            b.alternative,
        ))
    });
    let mut kept: Vec<Candidate> = Vec::new();
    for candidate in candidates {
        if kept.last().is_some_and(|last| {
            last.unit == candidate.unit && candidate.range.start < last.range.end
        }) {
            continue;
        }
        kept.push(candidate);
    }
    kept
}

struct Found {
    span: Span,
    message: String,
    replacement: Option<String>,
    count: Option<usize>,
}

fn run_rule(context: &Context, rule: &HouseStyleRule, compiled: &Compiled) -> Vec<Found> {
    let mut found = Vec::new();
    match rule.extends {
        HouseStyleExtends::Existence | HouseStyleExtends::Substitution => {
            let mut candidates = Vec::new();
            for unit in 0..context.units.len() {
                let hits = compiled
                    .set
                    .as_ref()
                    .map(|set| set.matches(&context.units[unit].folded));
                for (alternative, pattern) in compiled.patterns.iter().enumerate() {
                    if hits.as_ref().is_some_and(|hits| !hits.matched(alternative)) {
                        continue;
                    }
                    for range in context.find(rule, pattern, unit) {
                        let text = fold(&context.units[unit].text[range.clone()]);
                        // As in Vale, a listed form must match exactly, whatever `ignorecase`.
                        let replaced = compiled.replacements[alternative].as_deref().is_some_and(
                            |replacement| replacement.split('|').any(|option| fold(option) == text),
                        );
                        let excepted = compiled.exceptions.as_ref().is_some_and(|exceptions| {
                            !find(exceptions, &text, rule.nonword).is_empty()
                        });
                        if !replaced && !excepted {
                            candidates.push(Candidate {
                                unit,
                                range,
                                alternative,
                            });
                        }
                    }
                }
            }
            for candidate in without_overlaps(candidates) {
                let matched = &context.units[candidate.unit].text[candidate.range.clone()];
                let replacement = compiled.replacements[candidate.alternative].clone();
                let message = match &replacement {
                    Some(replacement) => {
                        format_message(&rule.message, &[replacement, matched], false)
                    }
                    None => format_message(&rule.message, &[matched], false),
                };
                found.push(Found {
                    span: context.units[candidate.unit].span(candidate.range),
                    message,
                    replacement,
                    count: None,
                });
            }
        }
        HouseStyleExtends::Consistency => {
            let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
            for (index, unit) in context.units.iter().enumerate() {
                groups.entry(unit.group).or_default().push(index);
            }
            let mut candidates = Vec::new();
            for units in groups.values() {
                for (alternative, pair) in compiled.either.iter().enumerate() {
                    let [left, right] = pair.each_ref().map(|(pattern, _)| {
                        units
                            .iter()
                            .flat_map(|&unit| {
                                context
                                    .find(rule, pattern, unit)
                                    .into_iter()
                                    .map(move |range| (unit, range))
                            })
                            .collect::<Vec<_>>()
                    });
                    let (Some(first_left), Some(first_right)) = (left.first(), right.first())
                    else {
                        continue;
                    };
                    let left_first =
                        (first_left.0, first_left.1.start) <= (first_right.0, first_right.1.start);
                    let (flagged, side) = if left_first { (right, 1) } else { (left, 0) };
                    let preferred = 1 - side;
                    for (unit, range) in flagged {
                        candidates.push(Candidate {
                            unit,
                            range,
                            alternative: alternative * 2 + preferred,
                        });
                    }
                }
            }
            for candidate in without_overlaps(candidates) {
                let matched = &context.units[candidate.unit].text[candidate.range.clone()];
                let preferred =
                    &compiled.either[candidate.alternative / 2][candidate.alternative % 2].1;
                found.push(Found {
                    span: context.units[candidate.unit].span(candidate.range.clone()),
                    message: format_message(&rule.message, &[matched], false),
                    replacement: Some(preferred.clone()),
                    count: None,
                });
            }
        }
        HouseStyleExtends::Occurrence => {
            // Per scope group: its units, and each token occurrence as (unit, range).
            type Occurrences = (Vec<usize>, Vec<(usize, Range<usize>)>);
            let mut groups: BTreeMap<usize, Occurrences> = BTreeMap::new();
            for (index, unit) in context.units.iter().enumerate() {
                if context.options.skip_dialogue && unit.dialogue_only {
                    continue;
                }
                let group = groups.entry(unit.group).or_default();
                group.0.push(index);
                group.1.extend(
                    context
                        .find(rule, &compiled.patterns[0], index)
                        .into_iter()
                        .map(|range| (index, range)),
                );
            }
            for (units, occurrences) in groups.values() {
                let count = occurrences.len();
                let over = rule.max.is_some_and(|max| count > max);
                let under = rule.min.is_some_and(|min| count < min);
                if !(over || under) {
                    continue;
                }
                // As in Vale: too many lands on the first match past `max`, too few on the
                // scope's first word (or the whole scope, if it has no word).
                let span = if over {
                    let (unit, range) = &occurrences[rule.max.unwrap_or_default()];
                    context.units[*unit].span(range.clone())
                } else {
                    units
                        .iter()
                        .find_map(|&unit| {
                            let unit = &context.units[unit];
                            first_word(&unit.folded).map(|range| unit.span(unit.original(range)))
                        })
                        .unwrap_or_else(|| {
                            Span::covering(
                                context.units[units[0]].span,
                                context.units[units[units.len() - 1]].span,
                            )
                        })
                };
                found.push(Found {
                    span,
                    message: format_message(&rule.message, &[&count.to_string()], true),
                    replacement: None,
                    count: Some(count),
                });
            }
        }
    }
    if let Some(limit) = rule.limit {
        found.truncate(limit);
    }
    found
}

pub fn report(
    document: &Document,
    rules: &HouseStyleRules,
    options: &HouseStyleOptions,
) -> Result<HouseStyleReport, StyleError> {
    let mut report = HouseStyleReport {
        rules: rules.len(),
        unsupported: rules.unsupported.clone(),
        matches: Vec::new(),
        groups: word_split(document).map(|words| HouseStyleCounts {
            words,
            ..HouseStyleCounts::default()
        }),
    };
    let skipped = document
        .sentences()
        .flat_map(|sentence| &sentence.tokens)
        .filter(|token| token.is_skipped)
        .map(|token| token.span.byte.clone())
        .collect::<Vec<_>>();
    let paragraph_units = || {
        document
            .paragraphs
            .iter()
            .enumerate()
            .map(|(index, paragraph)| {
                let dialogue_only = !paragraph.sentences.is_empty()
                    && paragraph
                        .sentences
                        .iter()
                        .all(|sentence| document.sentence_mode(sentence) == Mode::Dialogue);
                (
                    paragraph.text.as_str(),
                    &paragraph.span,
                    index,
                    dialogue_only,
                )
            })
    };
    let mut contexts: BTreeMap<HouseStyleScope, Context> = BTreeMap::new();
    let mut found = Vec::new();
    for (index, (rule, compiled)) in rules.rules.iter().zip(&rules.compiled).enumerate() {
        let Some(compiled) = compiled else {
            continue;
        };
        let context = contexts.entry(rule.scope).or_insert_with(|| {
            let units = match rule.scope {
                HouseStyleScope::Sentence => document
                    .sentences()
                    .enumerate()
                    .map(|(index, sentence)| {
                        let dialogue = document.sentence_mode(sentence) == Mode::Dialogue;
                        Unit::new(&sentence.text, &sentence.span, index, dialogue)
                    })
                    .collect(),
                HouseStyleScope::Paragraph => paragraph_units()
                    .map(|(text, span, index, dialogue)| Unit::new(text, span, index, dialogue))
                    .collect(),
                HouseStyleScope::Text => paragraph_units()
                    .map(|(text, span, _, dialogue)| Unit::new(text, span, 0, dialogue))
                    .collect(),
            };
            Context {
                document,
                options,
                units,
                skipped: skipped.clone(),
            }
        });
        for item in run_rule(context, rule, compiled) {
            found.push((index, item.message, item.replacement, item.count, item.span));
        }
    }
    found.sort_by(|a, b| {
        (a.4.byte.start, b.4.byte.end, a.0).cmp(&(b.4.byte.start, a.4.byte.end, b.0))
    });
    for (index, message, replacement, count, span) in found {
        let rule = &rules.rules[index];
        let mode = document.mode_at(span.byte.start);
        for counts in report.groups.all_and_mode_mut(mode) {
            match rule.level {
                HouseStyleLevel::Suggestion => counts.suggestions += 1,
                HouseStyleLevel::Warning => counts.warnings += 1,
                HouseStyleLevel::Error => counts.errors += 1,
            }
        }
        report.matches.push(HouseStyleMatch {
            rule: rule.name.clone(),
            extends: rule.extends,
            level: rule.level,
            message,
            span,
            mode,
            replacement,
            count,
            link: rule.link.clone(),
        });
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apostrophes_inside_words_are_not_boundaries() {
        let text = "don't advisor's o'clock 'tis";
        assert!(!boundary(text, 3));
        assert!(boundary(text, 13));
        assert!(!boundary(text, 17));
        assert!(boundary(text, 24));
    }

    #[test]
    fn messages_format_like_vale() {
        assert_eq!(
            format_message("Use '%s', not '%s'.", &["a", "b"], false),
            "Use 'a', not 'b'."
        );
        assert_eq!(format_message("100%% %s %d", &["x"], false), "100% x %d");
        assert_eq!(format_message("%s and %s", &["x"], false), "x and %s");
        assert_eq!(format_message("%d (%s)", &["4", "4"], true), "4 (4)");
    }

    #[test]
    fn folds_cover_every_whitespace_character() {
        let listed = |ch: char| {
            SPACES
                .iter()
                .any(|&(start, end)| (start..=end).contains(&ch))
        };
        assert!((char::MIN..=char::MAX).all(|ch| ch.is_whitespace() == listed(ch)));
        for &(start, end) in APOSTROPHES {
            assert!((start..=end).all(|ch| fold_char(ch) == '\''));
        }
    }

    #[test]
    fn yaml_scalars_follow_the_core_schema() {
        let values = documents("a: 3\nb: '3'\nc: true\nd: 1.50\ne: ~\nf: !!str 4\n").unwrap();
        assert_eq!(
            values,
            vec![
                serde_json::json!({"a": 3, "b": "3", "c": true, "d": "1.50", "e": null, "f": "4"})
            ]
        );
    }
}
