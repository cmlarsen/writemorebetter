//! An abstraction on top of Regular Expressions to add support for Serialization and
//! to modularize the Regex backend.
//! Adapts the approach from https://github.com/trishume/syntect/pull/270 with feature flags for the
//! different backends.

use once_cell::sync::{Lazy, OnceCell};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cell::RefCell;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

pub use regex_impl::{CaptureMatches, Captures, Match, Matches};

/// Deduplicates pattern strings across every lazily-cached [Regex] in the process.
///
/// Most rule patterns repeat (76% of the English grammar-rule pattern bytes are duplicates of an
/// earlier pattern), and every `Regex` would otherwise own its own copy. Interning stores each
/// distinct pattern once as an `Arc<str>` and hands out cheap clones. The pool is bounded by the
/// number of distinct patterns and is intentionally process-wide.
static PATTERN_POOL: Lazy<Mutex<HashSet<Arc<str>>>> = Lazy::new(|| Mutex::new(HashSet::new()));

fn intern_pattern(pattern: String) -> Arc<str> {
    let mut pool = PATTERN_POOL
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(existing) = pool.get(pattern.as_str()) {
        return existing.clone();
    }
    let interned: Arc<str> = Arc::from(pattern);
    pool.insert(interned.clone());
    interned
}

const SHORT_CACHE_SLOTS: usize = 512;

#[derive(Clone, Copy, Default)]
struct CacheEntry {
    key: (usize, u64),
    value: Option<bool>,
}

struct ShortCache(Box<[CacheEntry]>);

impl Default for ShortCache {
    fn default() -> Self {
        Self(vec![CacheEntry::default(); SHORT_CACHE_SLOTS].into_boxed_slice())
    }
}

impl ShortCache {
    fn slot(key: (usize, u64)) -> usize {
        ((key.0 as u64 ^ key.1).wrapping_mul(0x9e37_79b1_85eb_ca87) >> 32) as usize
            & (SHORT_CACHE_SLOTS - 1)
    }

    fn get(&self, key: (usize, u64)) -> Option<bool> {
        let entry = &self.0[Self::slot(key)];
        if entry.key == key {
            entry.value
        } else {
            None
        }
    }

    fn insert(&mut self, key: (usize, u64), value: bool) {
        self.0[Self::slot(key)] = CacheEntry {
            key,
            value: Some(value),
        };
    }
}

thread_local! {
    static SHORT_CACHE: RefCell<ShortCache> = RefCell::new(ShortCache::default());
}

#[derive(Debug)]
pub struct Regex {
    regex_str: Arc<str>,
    // Boxed so the cache slot is one pointer word. `OnceCell` stores its value inline, and a
    // compiled `fancy_regex::Regex` is large; with ~70k pattern `Regex`es in the English rule set
    // the inline slot alone cost several MB of the loaded rules. The box costs one allocation per
    // regex that is actually compiled. Serialization is unchanged: only `regex_str` is written.
    regex: OnceCell<Box<regex_impl::Regex>>,
}

impl Clone for Regex {
    fn clone(&self) -> Self {
        Regex {
            regex_str: self.regex_str.clone(),
            regex: OnceCell::new(),
        }
    }
}

impl Serialize for Regex {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.regex_str)
    }
}

impl<'de> Deserialize<'de> for Regex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let regex_str = String::deserialize(deserializer)?;
        Ok(Regex::new(regex_str))
    }
}

impl Hash for Regex {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.regex_str.hash(state);
    }
}

impl Regex {
    /// Create a new regex from the pattern string.
    ///
    /// Note that the regex compilation happens on first use, which is why this method does not
    /// return a result.
    pub fn new(regex_str: String) -> Self {
        Self {
            regex_str: intern_pattern(regex_str),
            regex: OnceCell::new(),
        }
    }

    /// Check whether the pattern compiles as a valid regex.
    #[allow(dead_code)] // used only in compile module
    pub fn try_compile(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        regex_impl::Regex::new(&self.regex_str).map(|_| ())
    }

    /// The pattern source. Used by the memory probe to count duplicated patterns.
    #[cfg(feature = "memory-probe")]
    #[allow(dead_code)]
    pub(crate) fn as_str(&self) -> &str {
        &self.regex_str
    }

    fn regex(&self) -> &regex_impl::Regex {
        self.regex.get_or_init(|| {
            Box::new(regex_impl::Regex::new(&self.regex_str).unwrap_or_else(|_| {
                panic!("regex string should be pre-tested: {}", self.regex_str)
            }))
        })
    }

    pub fn is_match(&self, text: &str) -> bool {
        if text.len() > 7 {
            return self.regex().is_match(text);
        }
        let mut bytes = [0; 8];
        bytes[..text.len()].copy_from_slice(text.as_bytes());
        bytes[7] = text.len() as u8;
        // The pool retains every pattern allocation, so its address is a stable identity.
        let key = (self.regex_str.as_ptr() as usize, u64::from_le_bytes(bytes));
        if let Some(result) = SHORT_CACHE.with(|cache| cache.borrow().get(key)) {
            return result;
        }
        let result = self.regex().is_match(text);
        SHORT_CACHE.with(|cache| cache.borrow_mut().insert(key, result));
        result
    }

    pub fn captures_iter<'r, 't>(&'r self, text: &'t str) -> regex_impl::CaptureMatches<'r, 't> {
        self.regex().captures_iter(text)
    }

    pub fn find_iter<'r, 't>(&'r self, text: &'t str) -> regex_impl::Matches<'r, 't> {
        self.regex().find_iter(text)
    }

    #[allow(dead_code)] // used only in compile module
    pub fn captures_len(&self) -> usize {
        self.regex().captures_len()
    }

    pub fn captures<'t>(&self, text: &'t str) -> Option<regex_impl::Captures<'t>> {
        self.regex().captures(text)
    }

    pub fn replace_all(&self, text: &str, replacement: &str) -> String {
        self.regex().replace_all(text, replacement)
    }
}

#[cfg(feature = "regex-fancy")]
mod regex_impl_fancy {
    pub use fancy_regex::{Captures, Match};
    use std::error::Error;
    pub struct Matches<'r, 't>(fancy_regex::Matches<'r, 't>);

    impl<'r, 't> Iterator for Matches<'r, 't> {
        type Item = Match<'t>;

        fn next(&mut self) -> Option<Self::Item> {
            match self.0.next() {
                Some(Ok(mat)) => Some(mat),
                // stop if an error is encountered
                None | Some(Err(_)) => None,
            }
        }
    }

    pub struct CaptureMatches<'r, 't>(fancy_regex::CaptureMatches<'r, 't>);

    impl<'r, 't> Iterator for CaptureMatches<'r, 't> {
        type Item = Captures<'t>;

        fn next(&mut self) -> Option<Self::Item> {
            match self.0.next() {
                Some(Ok(caps)) => Some(caps),
                // stop if an error is encountered
                None | Some(Err(_)) => None,
            }
        }
    }

    #[derive(Debug)]
    pub struct Regex {
        regex: fancy_regex::Regex,
    }

    impl Regex {
        pub fn new(regex_str: &str) -> Result<Self, Box<dyn Error + Send + Sync + 'static>> {
            Ok(Regex {
                regex: fancy_regex::Regex::new(regex_str)?,
            })
        }

        pub fn is_match(&self, text: &str) -> bool {
            // errors are treated as non-matches
            self.regex.is_match(text).unwrap_or(false)
        }

        pub fn captures_iter<'r, 't>(&'r self, text: &'t str) -> CaptureMatches<'r, 't> {
            CaptureMatches(self.regex.captures_iter(text))
        }

        pub fn find_iter<'r, 't>(&'r self, text: &'t str) -> Matches<'r, 't> {
            Matches(self.regex.find_iter(text))
        }

        pub fn captures_len(&self) -> usize {
            self.regex.captures_len()
        }

        pub fn captures<'t>(&self, text: &'t str) -> Option<Captures<'t>> {
            match self.regex.captures(text) {
                Ok(Some(captures)) => Some(captures),
                // errors treated as not matching
                Ok(None) | Err(_) => None,
            }
        }

        pub fn replace_all<'t>(&self, text: &'t str, replacement: &str) -> String {
            let mut index = 0;
            let mut out: Vec<String> = Vec::new();

            for captures in self.captures_iter(text) {
                let mat = captures.get(0).expect("0th capture group exists");

                out.push(text[index..mat.start()].to_string());

                let mut replacement = replacement.to_string();
                for i in 1..captures.len() {
                    replacement = replacement.replace(
                        &format!("${}", i),
                        captures.get(i).map_or("", |x| x.as_str()),
                    );
                }

                out.push(replacement);
                index = mat.end();
            }

            if index != text.len() {
                out.push(text[index..].to_string());
            }

            out.join("")
        }
    }
}

#[cfg(feature = "regex-onig")]
#[allow(dead_code)]
mod regex_impl_onig {
    use std::error::Error;

    pub struct CaptureMatches<'r, 't>(onig::FindCaptures<'r, 't>, &'t str);

    #[derive(Debug)]
    pub struct Captures<'t>(onig::Captures<'t>, &'t str);

    pub struct Matches<'r, 't>(onig::FindMatches<'r, 't>, &'t str);

    #[derive(Debug)]
    pub struct Match<'t> {
        text: &'t str,
        start: usize,
        end: usize,
    }

    impl<'t> Match<'t> {
        pub fn start(&self) -> usize {
            self.start
        }

        pub fn end(&self) -> usize {
            self.end
        }

        pub fn as_str(&self) -> &'t str {
            self.text
        }
    }

    impl<'t> Captures<'t> {
        pub fn get(&self, index: usize) -> Option<Match<'t>> {
            let (start, end) = self.0.pos(index)?;
            let text = self.0.at(index)?;

            Some(Match { text, start, end })
        }

        pub fn iter(&'t self) -> impl Iterator<Item = Option<Match<'t>>> {
            self.0.iter_pos().map(move |mat| {
                mat.map(|(start, end)| Match {
                    text: &self.1[start..end],
                    start,
                    end,
                })
            })
        }

        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        pub fn len(&self) -> usize {
            self.0.len()
        }
    }

    impl<'r, 't> Iterator for CaptureMatches<'r, 't> {
        type Item = Captures<'t>;

        fn next(&mut self) -> Option<Self::Item> {
            self.0.next().map(|x| Captures(x, self.1))
        }
    }

    impl<'r, 't> Iterator for Matches<'r, 't> {
        type Item = Match<'t>;

        fn next(&mut self) -> Option<Self::Item> {
            self.0.next().map(|(start, end)| Match {
                text: &self.1[start..end],
                start,
                end,
            })
        }
    }

    #[derive(Debug)]
    pub struct Regex {
        regex: onig::Regex,
    }

    impl Regex {
        pub fn new(regex_str: &str) -> Result<Self, Box<dyn Error + Send + Sync + 'static>> {
            let mut case_sensitive = true;
            let regex_str = if let Some(stripped) = regex_str.strip_suffix("(?i)") {
                case_sensitive = false;
                stripped
            } else {
                regex_str
            };

            let regex = onig::Regex::with_options(
                regex_str,
                onig::RegexOptions::REGEX_OPTION_CAPTURE_GROUP
                    | if case_sensitive {
                        onig::RegexOptions::REGEX_OPTION_NONE
                    } else {
                        onig::RegexOptions::REGEX_OPTION_IGNORECASE
                    },
                onig::Syntax::default(),
            )?;

            Ok(Regex { regex })
        }

        pub fn is_match(&self, text: &str) -> bool {
            self.regex.is_match(text)
        }

        pub fn captures_iter<'r, 't>(&'r self, text: &'t str) -> CaptureMatches<'r, 't> {
            CaptureMatches(self.regex.captures_iter(text), text)
        }

        pub fn find_iter<'r, 't>(&'r self, text: &'t str) -> Matches<'r, 't> {
            Matches(self.regex.find_iter(text), text)
        }

        pub fn captures_len(&self) -> usize {
            self.regex.captures_len() + 1
        }

        pub fn captures<'t>(&self, text: &'t str) -> Option<Captures<'t>> {
            self.regex.captures(text).map(|x| Captures(x, text))
        }

        pub fn replace_all(&self, text: &str, replacement: &str) -> String {
            self.regex.replace_all(&text, |caps: &onig::Captures| {
                let mut replacement = replacement.to_owned();

                for i in 1..caps.len() {
                    replacement = replacement.replace(&format!("${}", i), caps.at(i).unwrap_or(""));
                }

                replacement
            })
        }
    }
}

#[cfg(feature = "regex-all-test")]
mod regex_impl_all {
    //! This backend is only used for testing. It uses all other backends and assert they do the same thing.

    use super::{regex_impl_fancy as impl_fancy, regex_impl_onig as impl_onig};
    pub use impl_fancy::{CaptureMatches, Captures, Match, Matches};
    use itertools::{EitherOrBoth, Itertools};
    use std::error::Error;

    macro_rules! option_eq {
        ($a:expr, $b:expr) => {
            match (&$a, &$b) {
                (&Some(ref lhs), &Some(ref rhs)) if lhs == rhs => true,
                (&None, &None) => true,
                _ => false,
            }
        };
    }

    impl<'t> PartialEq<Match<'t>> for impl_onig::Match<'t> {
        fn eq(&self, other: &impl_fancy::Match<'t>) -> bool {
            self.start() == other.start()
                && self.end() == other.end()
                && self.as_str() == other.as_str()
        }
    }

    impl<'t> PartialEq<Captures<'t>> for impl_onig::Captures<'t> {
        fn eq(&self, other: &impl_fancy::Captures<'t>) -> bool {
            self.len() == other.len()
                && self.iter().zip(other.iter()).all(|(a, b)| option_eq!(a, b))
        }
    }

    #[derive(Debug)]
    pub struct Regex {
        fancy_regex: impl_fancy::Regex,
        onig_regex: impl_onig::Regex,
    }

    impl Regex {
        pub fn new(regex_str: &str) -> Result<Self, Box<dyn Error + Send + Sync + 'static>> {
            let fancy_regex = impl_fancy::Regex::new(regex_str);
            let onig_regex = impl_onig::Regex::new(regex_str);

            Ok(Regex {
                fancy_regex: fancy_regex?,
                onig_regex: onig_regex?,
            })
        }

        pub fn is_match(&self, text: &str) -> bool {
            let match_fancy = self.fancy_regex.is_match(text);

            assert_eq!(
                match_fancy,
                self.onig_regex.is_match(text),
                "{} {:?}",
                text,
                self.fancy_regex
            );
            match_fancy
        }

        pub fn captures_iter<'r, 't>(&'r self, text: &'t str) -> CaptureMatches<'r, 't> {
            assert!(
                self.fancy_regex
                    .captures_iter(text)
                    .zip_longest(self.onig_regex.captures_iter(text))
                    .all(|elem| {
                        if let EitherOrBoth::Both(a, b) = elem {
                            b == a
                        } else {
                            false
                        }
                    }),
                "{:?}",
                self.fancy_regex
            );

            self.fancy_regex.captures_iter(text)
        }

        pub fn find_iter<'r, 't>(&'r self, text: &'t str) -> Matches<'r, 't> {
            assert!(
                self.fancy_regex
                    .find_iter(text)
                    .zip_longest(self.onig_regex.find_iter(text))
                    .all(|elem| {
                        if let EitherOrBoth::Both(a, b) = elem {
                            b == a
                        } else {
                            false
                        }
                    }),
                "{:?}",
                self.fancy_regex
            );

            self.fancy_regex.find_iter(text)
        }

        pub fn captures_len(&self) -> usize {
            let out = self.fancy_regex.captures_len();
            assert_eq!(
                out,
                self.onig_regex.captures_len(),
                "{:?}",
                self.fancy_regex
            );
            out
        }

        pub fn captures<'t>(&self, text: &'t str) -> Option<Captures<'t>> {
            let out = self.fancy_regex.captures(text);
            let onig_out = self.onig_regex.captures(text);
            assert!(
                option_eq!(onig_out, out),
                "{:?}: Fancy: {:#?}, Onig: {:#?}",
                self.fancy_regex,
                out,
                onig_out
            );
            out
        }

        pub fn replace_all(&self, text: &str, replacement: &str) -> String {
            let out = self.fancy_regex.replace_all(text, replacement);
            assert_eq!(
                out,
                self.onig_regex.replace_all(text, replacement),
                "{:?}",
                self.fancy_regex
            );
            out
        }
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "regex-all-test")] {
        use regex_impl_all as regex_impl;
    } else if #[cfg(feature = "regex-onig")] {
        use regex_impl_onig as regex_impl;
    } else {
        use regex_impl_fancy as regex_impl;
    }
}

#[cfg(test)]
mod short_cache_tests {
    use super::*;

    #[test]
    fn collisions_and_empty_slots_are_misses() {
        let mut cache = ShortCache::default();
        assert_eq!(cache.get((0, 0)), None);
        let first = (1, 42);
        let second = (2..10000)
            .map(|i| (i, 42))
            .find(|&key| ShortCache::slot(key) == ShortCache::slot(first))
            .unwrap();
        cache.insert(first, true);
        assert_eq!(cache.get(first), Some(true));
        cache.insert(second, false);
        assert_eq!(cache.get(first), None);
        assert_eq!(cache.get(second), Some(false));
    }

    #[test]
    fn cached_results_match_backend_at_utf8_boundaries() {
        let regex = Regex::new("^[“”’—…\u{2028}a-z]+$".into());
        for text in [
            "", "a", "abcdefg", "abcdefgh", "”", "’", "—", "…", "\u{2028}", "curly”", "a\0",
            "line\n", "NO",
        ] {
            let expected = regex.regex().is_match(text);
            assert_eq!(regex.is_match(text), expected, "{text:?}");
            assert_eq!(regex.is_match(text), expected, "{text:?}");
        }
        let other = Regex::new("^NO$".into());
        assert!(other.is_match("NO"));
        assert!(!regex.is_match("NO"));
    }
}
