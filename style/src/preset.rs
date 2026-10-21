//! Style presets: one named, versioned JSON settings file that drives the grammar checker's rule
//! opt-in, the consistency report's preferred forms, house-style rules and the built-in preset
//! checks. See `docs/STYLE-PRESETS.md`.

use std::collections::BTreeMap;
use std::sync::Arc;

use nlprule::{profile::Profile, Rules};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::reports::consistency::{self, ConsistencyOptions, ConsistencyReport};
use crate::reports::house_style::{self, HouseStyleOptions, HouseStyleReport, HouseStyleRules};
use crate::reports::preset_checks::{self, PresetChecks, PresetChecksReport};
use crate::{Document, StyleError};

/// The preset file format this engine reads.
pub const PRESET_FORMAT: u32 = 1;

/// The preset an app should use when the author has not picked one.
pub const DEFAULT_PRESET: &str = "chicago-fiction";

const BUILTIN: &[(&str, &str)] = &[
    (
        "chicago-fiction",
        include_str!("../presets/chicago-fiction.json"),
    ),
    ("ap", include_str!("../presets/ap.json")),
    ("uk", include_str!("../presets/uk.json")),
];

/// How deep `extends` may chain through built-in presets.
const MAX_DEPTH: usize = 4;

/// Grammar-checker settings: a rule profile, then rules to turn off or on by selector.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct PresetGrammar {
    /// A rule profile by name (`"default"` or `"fiction"`), applied first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Selector → on (`true`) or off (`false`), in the selector syntax of `docs/PROFILE.md`. Every
    /// `false` applies before every `true`, so a narrower enable wins over a wider disable.
    pub rules: BTreeMap<String, bool>,
    /// A speller dictionary such as `"en_GB"`; unset keeps the checker's.
    #[serde(alias = "spell_variant", skip_serializing_if = "Option::is_none")]
    pub spell_variant: Option<String>,
}

impl PresetGrammar {
    /// The selectors the preset turns off, in order.
    pub fn disabled(&self) -> Vec<&str> {
        self.selectors(false)
    }

    /// The selectors the preset turns on, in order.
    pub fn enabled(&self) -> Vec<&str> {
        self.selectors(true)
    }

    fn selectors(&self, on: bool) -> Vec<&str> {
        self.rules
            .iter()
            .filter(|(_, value)| **value == on)
            .map(|(selector, _)| selector.as_str())
            .collect()
    }
}

/// House-style settings: Vale-format rules as a map from name to rule, and an embedded bundle.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct PresetHouseStyle {
    pub options: HouseStyleOptions,
    /// Rule name → Vale rule (the keys of a Vale rule file, as JSON).
    pub rules: BTreeMap<String, Value>,
    /// A Vale bundle as YAML or JSON text (see `docs/STYLE-HOUSE-STYLE.md`), run after `rules`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vale: Option<String>,
}

/// The file as written, before `extends` is resolved.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PresetFile {
    format: u32,
    name: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    extends: Option<String>,
    #[serde(default)]
    grammar: PresetGrammar,
    #[serde(default)]
    consistency: ConsistencyOptions,
    #[serde(default)]
    house_style: PresetHouseStyle,
    #[serde(default)]
    checks: PresetChecks,
}

/// A resolved, validated preset. `to_json` writes it back as a self-contained file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct StylePreset {
    pub format: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The preset this one was built on, as the file named it. `to_json` leaves it out: the
    /// written file already holds everything it inherited.
    #[serde(skip)]
    pub extends: Option<String>,
    pub grammar: PresetGrammar,
    pub consistency: ConsistencyOptions,
    pub house_style: PresetHouseStyle,
    pub checks: PresetChecks,
    #[serde(skip)]
    compiled: Arc<HouseStyleRules>,
}

/// Every report a preset drives, from one call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PresetReport {
    pub preset: String,
    pub version: Option<String>,
    pub consistency: ConsistencyReport,
    pub house_style: HouseStyleReport,
    pub checks: PresetChecksReport,
}

impl StylePreset {
    /// The built-in presets' names; the first is the default.
    pub fn names() -> Vec<&'static str> {
        BUILTIN.iter().map(|(name, _)| *name).collect()
    }

    /// A built-in preset by name.
    pub fn builtin(name: &str) -> Result<Self, StyleError> {
        Self::from_value(builtin_value(name, 0)?)
    }

    /// The default preset, `chicago-fiction`.
    pub fn default_preset() -> Self {
        Self::builtin(DEFAULT_PRESET).expect("the default preset is valid")
    }

    /// Parses a preset file (JSON). With `extends`, the file is applied over that built-in preset
    /// as a JSON merge patch (RFC 7396): objects merge key by key, any other value replaces the
    /// inherited one, and `null` removes it.
    pub fn parse(text: &str) -> Result<Self, StyleError> {
        let value: Value = serde_json::from_str(text).map_err(StyleError::preset)?;
        Self::from_value(resolve(value, 0)?)
    }

    /// A built-in preset when `text` is one's name, and otherwise a preset file.
    pub fn load(text: &str) -> Result<Self, StyleError> {
        if BUILTIN.iter().any(|(name, _)| *name == text) {
            Self::builtin(text)
        } else {
            Self::parse(text)
        }
    }

    fn from_value(value: Value) -> Result<Self, StyleError> {
        let file: PresetFile = serde_json::from_value(value).map_err(StyleError::preset)?;
        if file.format != PRESET_FORMAT {
            return Err(StyleError::preset(format!(
                "format {} is not supported; this engine reads format {PRESET_FORMAT}",
                file.format
            )));
        }
        if file.name.trim().is_empty() {
            return Err(StyleError::preset("`name` must not be empty"));
        }
        if let Some(profile) = file.grammar.profile.as_deref() {
            if Profile::by_name(profile).is_none() {
                return Err(StyleError::preset(format!("unknown profile: {profile}")));
            }
        }
        if let Some(check) = file.grammar.rules.keys().find(|key| key.trim().is_empty()) {
            return Err(StyleError::preset(format!(
                "grammar.rules has an empty selector {check:?}"
            )));
        }
        if file.consistency.minimum_minority == 0 {
            return Err(StyleError::preset(
                "consistency: minimumMinority must be at least 1 occurrence",
            ));
        }
        file.consistency
            .preferred
            .validate()
            .map_err(nested("consistency"))?;
        file.checks.validate().map_err(nested("checks"))?;
        let compiled = Arc::new(compile(&file.house_style).map_err(nested("houseStyle"))?);
        Ok(Self {
            format: file.format,
            name: file.name,
            version: file.version,
            description: file.description,
            extends: file.extends,
            grammar: file.grammar,
            consistency: file.consistency,
            house_style: file.house_style,
            checks: file.checks,
            compiled,
        })
    }

    /// The preset as a self-contained file, which `parse` reads back as an equal preset.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("presets serialize to JSON")
    }

    /// The compiled house-style rules: `houseStyle.rules`, then the embedded `vale` bundle.
    pub fn house_style_rules(&self) -> &HouseStyleRules {
        &self.compiled
    }

    /// Applies the grammar settings to a rule set: the profile, every disabled selector, every
    /// enabled selector, then the speller variant. Every selector and the variant are checked
    /// first, so an unknown one changes nothing. Returns how many rules went from off to on.
    ///
    /// Profiles cannot be undone on a loaded rule set, so apply a preset to freshly loaded rules;
    /// to switch presets, load the rules again.
    pub fn apply_to_rules(&self, rules: &mut Rules) -> Result<usize, StyleError> {
        let disabled = self.grammar.disabled();
        let enabled = self.grammar.enabled();
        let mut unknown = rules.unmatched_selectors(&disabled, true);
        unknown.extend(rules.unmatched_selectors(&enabled, false));
        if !unknown.is_empty() {
            return Err(StyleError::preset(format!(
                "unknown rule selectors: {unknown:?}"
            )));
        }
        let variant = self
            .grammar
            .spell_variant
            .as_deref()
            .map(|variant| rules.spell().variant(variant))
            .transpose()
            .map_err(|error| StyleError::preset(format!("unknown spell variant: {error}")))?;
        if let Some(profile) = self.grammar.profile.as_deref().and_then(Profile::by_name) {
            rules.apply_profile(&profile);
        }
        rules.apply_profile(&Profile::new(
            self.name.clone(),
            disabled
                .iter()
                .map(|selector| (*selector).to_owned())
                .collect(),
        ));
        let count = rules
            .enable_rules(&enabled)
            .map_err(|error| StyleError::preset(error.to_string()))?;
        if let Some(variant) = variant {
            rules.spell_mut().options_mut().variant = Some(variant);
        }
        Ok(count)
    }

    /// The consistency, house-style and preset-check reports with this preset's settings.
    pub fn report(&self, document: &Document) -> Result<PresetReport, StyleError> {
        Ok(PresetReport {
            preset: self.name.clone(),
            version: self.version.clone(),
            consistency: consistency::report(document, &self.consistency)?,
            house_style: house_style::report(document, &self.compiled, &self.house_style.options)?,
            checks: preset_checks::report(document, &self.checks)?,
        })
    }
}

fn nested(section: &'static str) -> impl Fn(StyleError) -> StyleError {
    move |error| StyleError::preset(format!("{section}: {error}"))
}

fn compile(settings: &PresetHouseStyle) -> Result<HouseStyleRules, StyleError> {
    let mut rules = Vec::new();
    if !settings.rules.is_empty() {
        let bundle = serde_json::json!({ "rules": settings.rules }).to_string();
        rules.extend(HouseStyleRules::parse(&bundle)?.rules().iter().cloned());
    }
    if let Some(vale) = &settings.vale {
        rules.extend(HouseStyleRules::parse(vale)?.rules().iter().cloned());
    }
    HouseStyleRules::new(rules)
}

fn builtin_value(name: &str, depth: usize) -> Result<Value, StyleError> {
    let (_, text) = BUILTIN
        .iter()
        .find(|(builtin, _)| *builtin == name)
        .ok_or_else(|| {
            StyleError::preset(format!(
                "unknown preset {name:?}; the built-in presets are {}",
                StylePreset::names().join(", ")
            ))
        })?;
    resolve(
        serde_json::from_str(text).expect("built-in presets are JSON"),
        depth,
    )
}

/// Merges a file over the built-in preset it `extends`, recursively, keeping `extends` as written.
fn resolve(value: Value, depth: usize) -> Result<Value, StyleError> {
    let Value::Object(file) = value else {
        return Err(StyleError::preset("a preset must be a JSON object"));
    };
    let base = match file.get("extends") {
        None | Some(Value::Null) => {
            // Merging onto nothing drops `null`s, so they mean "unset" with or without a base.
            let mut resolved = Value::Object(Map::new());
            merge(&mut resolved, Value::Object(file));
            return Ok(resolved);
        }
        Some(Value::String(base)) if depth < MAX_DEPTH => base.clone(),
        Some(Value::String(_)) => {
            return Err(StyleError::preset("`extends` chains too deeply"));
        }
        Some(_) => return Err(StyleError::preset("`extends` must name a built-in preset")),
    };
    let mut merged = builtin_value(&base, depth + 1)?;
    // A preset names, versions and describes itself; a file that changes a built-in is not it.
    if let Value::Object(merged) = &mut merged {
        for key in ["name", "version", "description"] {
            merged.remove(key);
        }
    }
    merge(&mut merged, Value::Object(file));
    Ok(merged)
}

/// JSON merge patch (RFC 7396).
fn merge(target: &mut Value, patch: Value) {
    let Value::Object(patch) = patch else {
        *target = patch;
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    let target = target.as_object_mut().expect("made an object above");
    for (key, value) in patch {
        if value.is_null() {
            target.remove(&key);
        } else {
            merge(target.entry(key).or_insert(Value::Null), value);
        }
    }
}
