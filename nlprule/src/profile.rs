//! Named rule profiles: sets of rules to disable for a particular kind of text.
//!
//! A [`Profile`] is a name plus a list of *rule selector strings*. A selector is
//! either
//!
//! - a [`Selector`] as understood by [`Rules::select_mut`](crate::Rules::select_mut),
//!   written with `/` separators, e.g. `"TYPOGRAPHY/EN_QUOTES"` or `"REDUNDANCY"`, or
//! - a builtin rule id prefixed with `"BUILTIN/"`, e.g.
//!   `"BUILTIN/UPPERCASE_SENTENCE_START"`.
//!
//! The selectors are *identifiers*, not rule data: the list lives in the engine
//! source under the same MIT/Apache-2.0 license as the rest of the engine. It
//! names rules but never carries their definitions, patterns, or examples.
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

use crate::{rule::id::Selector, Rules};
use std::convert::TryFrom;

/// The prefix that marks a selector as a builtin rule id rather than an XML rule selector.
const BUILTIN_PREFIX: &str = "BUILTIN/";

/// A named set of rules to disable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    /// The profile name.
    pub name: String,
    /// The rules to disable, as selector strings (see the [module docs](self)).
    pub disable: Vec<String>,
}

impl Profile {
    /// Creates a profile from a name and a disable list.
    pub fn new(name: impl Into<String>, disable: Vec<String>) -> Self {
        Profile {
            name: name.into(),
            disable,
        }
    }

    /// The default profile: disables nothing.
    pub fn default_profile() -> Profile {
        Profile {
            name: "default".to_owned(),
            disable: Vec::new(),
        }
    }

    /// The fiction profile: disables rules that are known to misfire on fiction drafts.
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
    /// Disables every rule named by `profile`.
    ///
    /// Grammar rules are selected with [`Rules::select_mut`](crate::Rules::select_mut)
    /// using each selector string; builtins are matched by the part of the selector
    /// after `"BUILTIN/"`.
    ///
    /// Returns the selectors that matched no rule, so a typo in a profile is caught by
    /// a test rather than silently ignored. Each unmatched selector is also logged at
    /// `warn` level.
    pub fn apply_profile(&mut self, profile: &Profile) -> Vec<String> {
        let mut unmatched = Vec::new();

        for selector in &profile.disable {
            let matched = if let Some(id) = selector.strip_prefix(BUILTIN_PREFIX) {
                let mut matched = false;
                for rule in self.builtins.iter_mut() {
                    if rule.id() == id {
                        rule.disable();
                        matched = true;
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

        unmatched
    }
}
