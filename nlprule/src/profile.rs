//! Named rule profiles: sets of rules to disable, and of off-by-default rules to enable, for a
//! particular kind of text.
//!
//! A [`Profile`] is a name plus a list of *rule selector strings*. A selector is
//! either
//!
//! - a [`Selector`] as understood by [`Rules::select_mut`](crate::Rules::select_mut),
//!   written with `/` separators, e.g. `"TYPOGRAPHY/EN_QUOTES"` or `"REDUNDANCY"`, or
//! - a builtin rule id prefixed with `"BUILTIN/"`, e.g.
//!   `"BUILTIN/UPPERCASE_SENTENCE_START"`, or
//! - a builtin rule id and one of its cases, which disables only that case (see
//!   [`BuiltinRule::disable_case`](crate::builtin::BuiltinRule::disable_case)), e.g.
//!   `"BUILTIN/WHITESPACE_RULE/INDENTATION"`.
//!
//! The selectors are *identifiers*, not rule data: the list lives in the engine
//! source under the same MIT/Apache-2.0 license as the rest of the engine. It
//! names rules but never carries their definitions, patterns, or examples.
//!
//! The same selectors, without the case form, also *enable* rules that are off by
//! default (LanguageTool's `default="off"` and `picky` rules): a profile's
//! [`enable`](Profile::enable) list, applied after its disable list, or
//! [`Rules::enable_rules`], applied after the profile.
//!
//! # Examples
//!
//! ```no_run
//! use nlprule::{profile::Profile, Rules};
//!
//! let mut rules = Rules::new("path/to/en_rules.bin")?;
//! let unmatched = rules.apply_profile(&Profile::fiction());
//! assert!(unmatched.is_empty());
//! # Ok::<(), nlprule::Error>(())
//! ```

use crate::{rule::id::Selector, Error, Rules};
use std::convert::TryFrom;

/// The prefix that marks a selector as a builtin rule id rather than an XML rule selector.
const BUILTIN_PREFIX: &str = "BUILTIN/";

/// A named set of rules to disable, and of off-by-default rules to enable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    /// The profile name.
    pub name: String,
    /// The rules to disable, as selector strings (see the [module docs](self)).
    pub disable: Vec<String>,
    /// The rules to enable after `disable`, as selector strings without the builtin case
    /// form; for rules that are off by default, as in LanguageTool.
    pub enable: Vec<String>,
}

impl Profile {
    /// Creates a profile from a name and a disable list; it enables nothing.
    pub fn new(name: impl Into<String>, disable: Vec<String>) -> Self {
        Profile {
            name: name.into(),
            disable,
            enable: Vec::new(),
        }
    }

    /// The default profile: disables and enables nothing.
    pub fn default_profile() -> Profile {
        Profile::new("default", Vec::new())
    }

    /// The fiction profile: disables rules that are known to misfire on fiction drafts, and
    /// enables `EN_CONSISTENT_APOS`, which is off by default in LanguageTool.
    ///
    /// The counts in the comments are how often each rule fired on the private draft
    /// corpus before tuning. This list is expected to change as the corpus grows; the
    /// [`Rules::apply_profile`] return value and its test guard against typos.
    pub fn fiction() -> Profile {
        Profile {
            name: "fiction".to_owned(),
            disable: [
                // Straight-to-curly quote suggestions (592).
                "TYPOGRAPHY/EN_QUOTES",
                // Dash typography (18).
                "PUNCTUATION/DASH_RULE",
                // Comma before "and" in a compound sentence (23).
                "PUNCTUATION/COMMA_COMPOUND_SENTENCE",
                "PUNCTUATION/COMMA_COMPOUND_SENTENCE_2",
                // Missing comma after an introductory phrase (6).
                "PUNCTUATION/MISSING_COMMA_AFTER_INTRODUCTORY_PHRASE",
                // Dialogue informality: gonna/gotta/wanna (9/4/1).
                "STYLE/GONNA",
                "STYLE/GOTTA",
                "STYLE/WANNA",
                // Verbless dialogue questions, e.g. "You good?" (2).
                "GRAMMAR/YOU_GOOD",
                // Stylistic redundancy advice, e.g. some of the -> some (not errors).
                "REDUNDANCY",
                // Repeated "Thank you for" in nearby sentences; dialogue repeats it on purpose.
                "REPETITIONS_STYLE/REP_THANK_YOU_FOR",
                // Indentation with spaces at the start of a paragraph or line, which
                // LanguageTool flags after a sentence end or a CRLF (403 on moby150k).
                "BUILTIN/WHITESPACE_RULE/INDENTATION",
                // LanguageTool's grammar.xml rule for two spaces between words, which only
                // repeats what the WHITESPACE_RULE builtin says, in different words.
                "TYPOGRAPHY/CONSECUTIVE_SPACES",
                // Successive sentences that begin with the same word, which fiction
                // uses for rhythm ("He waited. He listened. He...").
                "BUILTIN/ENGLISH_WORD_REPEAT_BEGINNING_RULE",
                // The off-by-default word-frequency hints skip dialogue once enabled: characters
                // repeat words on purpose, and narration is what the author controls.
                "BUILTIN/TOO_OFTEN_USED_ADJECTIVE_EN/DIRECT_SPEECH",
                "BUILTIN/TOO_OFTEN_USED_NOUN_EN/DIRECT_SPEECH",
                "BUILTIN/TOO_OFTEN_USED_VERB_EN/DIRECT_SPEECH",
            ]
            .iter()
            .map(|selector| (*selector).to_owned())
            .collect(),
            enable: [
                // A manuscript that mixes straight and curly apostrophes, often from text
                // pasted in from another editor. Lone curly quotes are not apostrophes to it.
                "BUILTIN/EN_CONSISTENT_APOS",
            ]
            .iter()
            .map(|selector| (*selector).to_owned())
            .collect(),
        }
    }

    /// Looks up a profile by name. `"default"` disables nothing; `"fiction"` is the
    /// [fiction profile](Profile::fiction).
    pub fn by_name(name: &str) -> Option<Profile> {
        match name {
            "default" => Some(Profile::default_profile()),
            "fiction" => Some(Profile::fiction()),
            _ => None,
        }
    }
}

impl Rules {
    /// Disables every rule named by `profile.disable`, then enables every rule named by
    /// `profile.enable`.
    ///
    /// Grammar rules are selected with [`Rules::select_mut`](crate::Rules::select_mut)
    /// using each selector string; builtins are matched by the part of the selector
    /// after `"BUILTIN/"`, and a selector that also names a case disables only that case.
    /// Enable selectors follow [`Rules::enable_rules`], except that one that matches no
    /// rule is reported here and does not stop the others.
    ///
    /// Returns the selectors that matched no rule, so a typo in a profile is caught by
    /// a test rather than silently ignored. Each unmatched selector is also logged at
    /// `warn` level.
    pub fn apply_profile(&mut self, profile: &Profile) -> Vec<String> {
        let mut unmatched = Vec::new();

        for selector in &profile.disable {
            let matched = if let Some(id) = selector.strip_prefix(BUILTIN_PREFIX) {
                let (id, case) = match id.split_once('/') {
                    Some((id, case)) => (id, Some(case)),
                    None => (id, None),
                };
                let mut matched = false;
                for rule in self.builtins.iter_mut() {
                    if rule.id() != id {
                        continue;
                    }
                    match case {
                        Some(case) => matched |= rule.disable_case(case),
                        None => {
                            rule.disable();
                            matched = true;
                        }
                    }
                }
                matched
            } else {
                match Selector::try_from(selector.as_str()) {
                    Ok(parsed) => {
                        let mut matched = false;
                        for rule in self.select_mut(&parsed) {
                            rule.disable();
                            matched = true;
                        }
                        matched
                    }
                    Err(_) => false,
                }
            };

            if !matched {
                log::warn!(
                    "profile \"{}\": selector \"{}\" matched no rules",
                    profile.name,
                    selector
                );
                unmatched.push(selector.clone());
            }
        }

        for selector in &profile.enable {
            if self.selector_matches(selector) {
                self.enable_rules(&[selector])
                    .expect("a selector that matches a rule enables it");
            } else {
                log::warn!(
                    "profile \"{}\": enable selector \"{}\" matched no rules",
                    profile.name,
                    selector
                );
                unmatched.push(selector.clone());
            }
        }

        unmatched
    }

    /// Enables every rule named by `selectors`, e.g. LanguageTool's off-by-default and
    /// `picky` rules such as `"PLAIN_ENGLISH"`, `"STYLE/PASSIVE_VOICE"` or
    /// `"BUILTIN/EN_REDUNDANCY_REPLACE"`. Call it after [`Rules::apply_profile`] so a
    /// selector can turn back on a rule the profile disabled.
    ///
    /// Selectors use the profile syntax (see the [module docs](self)) except the builtin
    /// case form (`"BUILTIN/ID/CASE"`), which only disables. Every selector is checked
    /// before any rule changes: if one matches no rule, nothing is enabled and
    /// [`Error::UnknownSelectors`] lists the unmatched ones.
    ///
    /// Returns how many grammar and builtin rules changed from disabled to enabled.
    pub fn enable_rules<S: AsRef<str>>(&mut self, selectors: &[S]) -> Result<usize, Error> {
        let unmatched: Vec<String> = selectors
            .iter()
            .map(AsRef::as_ref)
            .filter(|selector| !self.selector_matches(selector))
            .map(str::to_owned)
            .collect();
        if !unmatched.is_empty() {
            return Err(Error::UnknownSelectors(unmatched));
        }

        let mut enabled = 0;
        for selector in selectors.iter().map(AsRef::as_ref) {
            if let Some(id) = selector.strip_prefix(BUILTIN_PREFIX) {
                for rule in self.builtins.iter_mut().filter(|rule| rule.id() == id) {
                    enabled += usize::from(!rule.enabled());
                    rule.enable();
                }
            } else if let Ok(parsed) = Selector::try_from(selector) {
                for rule in self.select_mut(&parsed) {
                    enabled += usize::from(!rule.enabled());
                    rule.enable();
                }
            }
        }
        Ok(enabled)
    }

    /// The selectors in `selectors` that name no grammar or builtin rule, without changing any
    /// rule. With `cases`, the builtin case form (`"BUILTIN/ID/CASE"`) that
    /// [`Rules::apply_profile`] accepts is checked too; without it, as for
    /// [`Rules::enable_rules`], that form is unmatched.
    pub fn unmatched_selectors<S: AsRef<str>>(&self, selectors: &[S], cases: bool) -> Vec<String> {
        selectors
            .iter()
            .map(AsRef::as_ref)
            .filter(|selector| {
                let case = selector
                    .strip_prefix(BUILTIN_PREFIX)
                    .and_then(|id| id.split_once('/'))
                    .filter(|_| cases);
                match case {
                    Some((id, case)) => !self
                        .builtins
                        .iter()
                        .any(|rule| rule.id() == id && rule.has_case(case)),
                    None => !self.selector_matches(selector),
                }
            })
            .map(str::to_owned)
            .collect()
    }

    /// Whether an enable selector names at least one grammar or builtin rule.
    fn selector_matches(&self, selector: &str) -> bool {
        match selector.strip_prefix(BUILTIN_PREFIX) {
            Some(id) => self.builtins.iter().any(|rule| rule.id() == id),
            None => match Selector::try_from(selector) {
                Ok(parsed) => self.select(&parsed).next().is_some(),
                Err(_) => false,
            },
        }
    }
}
