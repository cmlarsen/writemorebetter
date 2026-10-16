//! An abstraction on top of Regular Expressions to add support for Serialization and
//! to modularize the Regex backend.
//! Adapts the approach from https://github.com/trishume/syntect/pull/270 with feature flags for the
//! different backends.

use once_cell::sync::{Lazy, OnceCell};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::TryInto;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

pub use regex_impl::{CaptureMatches, Captures, Match, Matches};

/// Deduplicates pattern strings across every lazily-cached [Regex] in the process.
///
/// Most rule patterns repeat (76% of the English grammar-rule pattern bytes are duplicates of an
/// earlier pattern), and every `Regex` would otherwise own its own copy. Interning stores each
/// distinct pattern once as an `Arc<str>` and hands out cheap clones. The pool is bounded by the
/// number of distinct patterns and is intentionally process-wide.
type BackendCell = OnceCell<Box<regex_impl::Regex>>;

struct PatternEntry {
    id: u64,
    backend: Weak<BackendCell>,
}

static PATTERN_POOL: Lazy<Mutex<HashMap<Arc<str>, PatternEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static NEXT_PATTERN_ID: AtomicU64 = AtomicU64::new(1);

fn intern_pattern(pattern: String) -> (Arc<str>, u64, Arc<BackendCell>) {
    let mut pool = PATTERN_POOL
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some((existing, entry)) = pool.get_key_value(pattern.as_str()) {
        let existing = existing.clone();
        let id = entry.id;
        let backend = entry.backend.upgrade().unwrap_or_else(|| {
            let backend = Arc::new(BackendCell::new());
            pool.get_mut(existing.as_ref()).unwrap().backend = Arc::downgrade(&backend);
            backend
        });
        return (existing, id, backend);
    }
    let id = NEXT_PATTERN_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("regex pattern identity exhausted");
    let interned: Arc<str> = Arc::from(pattern);
    let backend = Arc::new(BackendCell::new());
    pool.insert(
        interned.clone(),
        PatternEntry {
            id,
            backend: Arc::downgrade(&backend),
        },
    );
    (interned, id, backend)
}

const SHORT_CACHE_SLOTS: usize = 2048;

#[derive(Clone, Copy, Default)]
struct CacheEntry {
    key: (u64, u64, u64),
    value: Option<bool>,
}

struct ShortCache(Box<[CacheEntry]>);

impl Default for ShortCache {
    fn default() -> Self {
        Self(vec![CacheEntry::default(); SHORT_CACHE_SLOTS].into_boxed_slice())
    }
}

impl ShortCache {
    fn slot(key: (u64, u64, u64)) -> usize {
        ((key.0 ^ key.1 ^ key.2.rotate_left(23)).wrapping_mul(0x9e37_79b1_85eb_ca87) >> 32) as usize
            & (SHORT_CACHE_SLOTS - 1)
    }

    fn get(&self, key: (u64, u64, u64)) -> Option<bool> {
        let entry = &self.0[Self::slot(key)];
        if entry.key == key {
            entry.value
        } else {
            None
        }
    }

    fn insert(&mut self, key: (u64, u64, u64), value: bool) {
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
    pattern_id: u64,
    // The pool holds only a weak reference: compiled backends are freed with their last Regex.
    regex: Arc<BackendCell>,
}

impl Clone for Regex {
    fn clone(&self) -> Self {
        Regex {
            regex_str: self.regex_str.clone(),
            pattern_id: self.pattern_id,
            regex: self.regex.clone(),
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
        let (regex_str, pattern_id, regex) = intern_pattern(regex_str);
        Self {
            regex_str,
            pattern_id,
            regex,
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
        if text.len() > 15 {
            return self.regex().is_match(text);
        }
        let mut bytes = [0; 16];
        bytes[..text.len()].copy_from_slice(text.as_bytes());
        bytes[15] = text.len() as u8;
        let (low, high) = bytes.split_at(8);
        let key = (
            self.pattern_id,
            u64::from_le_bytes(low.try_into().unwrap()),
            u64::from_le_bytes(high.try_into().unwrap()),
        );
        if let Some(result) = SHORT_CACHE.with(|cache| cache.borrow().get(key)) {
            return result;
        }
        let result = self.regex().is_match(text);
        SHORT_CACHE.with(|cache| cache.borrow_mut().insert(key, result));
        result
    }

    pub(crate) fn is_match_uncached(regex_str: &str, text: &str) -> bool {
        regex_impl::Regex::new(regex_str).is_ok_and(|regex| regex.is_match(text))
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
    pub use fancy_regex::Match;
    use std::error::Error;
    pub type Captures<'t> = fancy_regex::Captures<'t, str>;
    pub struct Matches<'r, 't>(fancy_regex::Matches<'r, 't, str>);

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

    pub struct CaptureMatches<'r, 't>(fancy_regex::CaptureMatches<'r, 't, str>);

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
    fn shared_backends_are_released_and_reloaded_lazily() {
        let pattern = "^shared-backend-lifecycle-test$";
        let first = Regex::new(pattern.into());
        let second = Regex::new(pattern.into());
        let cloned = first.clone();
        assert!(Arc::ptr_eq(&first.regex, &second.regex));
        assert!(Arc::ptr_eq(&first.regex, &cloned.regex));
        assert!(first.is_match("shared-backend-lifecycle-test"));
        assert!(second.regex.get().is_some());
        let id = first.pattern_id;
        let weak = Arc::downgrade(&first.regex);
        drop(first);
        drop(second);
        drop(cloned);
        assert!(weak.upgrade().is_none());
        let reloaded = Regex::new(pattern.into());
        assert_eq!(reloaded.pattern_id, id);
        assert!(reloaded.regex.get().is_none());
        assert!(reloaded.is_match("shared-backend-lifecycle-test"));
    }

    #[test]
    fn collisions_and_empty_slots_are_misses() {
        let mut cache = ShortCache::default();
        assert_eq!(cache.get((0, 0, 0)), None);
        let first = (1, 42, 0);
        let second = (2..10000)
            .map(|i| (i, 42, 0))
            .find(|&key| ShortCache::slot(key) == ShortCache::slot(first))
            .unwrap();
        cache.insert(first, true);
        assert_eq!(cache.get(first), Some(true));
        cache.insert(second, false);
        assert_eq!(cache.get(first), None);
        assert_eq!(cache.get(second), Some(false));
    }

    #[test]
    fn dropping_and_reloading_patterns_cannot_reuse_a_cached_result() {
        let first = Regex::new("^(?:cache-lifetime-first|yes)$".into());
        let id = first.pattern_id;
        assert!(first.is_match("yes"));
        drop(first);
        let second = Regex::new("^(?:cache-lifetime-second|no)$".into());
        assert_ne!(second.pattern_id, id);
        assert!(!second.is_match("yes"));
        drop(second);
        let reloaded = Regex::new("^(?:cache-lifetime-first|yes)$".into());
        assert_eq!(reloaded.pattern_id, id);
        assert!(reloaded.is_match("yes"));
    }

    #[test]
    fn cached_results_match_backend_at_utf8_boundaries() {
        let regex = Regex::new("^[“”’—…\u{2028}a-z]+$".into());
        for text in [
            "",
            "a",
            "abcdefg",
            "abcdefgh",
            "”",
            "’",
            "—",
            "…",
            "\u{2028}",
            "curly”",
            "a\0",
            "line\n",
            "NO",
            "abcdefghijklmno",
            "abcdefghijklmnop",
            "abcdefghiNO",
            "abcdefg\0",
            "abcdefg\0\0",
            "curly“”—…",
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
