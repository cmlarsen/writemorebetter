//! Rule-based grammatical error correction through parsing LanguageTool rules.
//! # Overview
//!
//! nlprule has the following core abstractions:
//! - A [Tokenizer][tokenizer::Tokenizer] to split a text into tokens and analyze it by chunking, lemmatizing and part-of-speech tagging. Can also be used independently of the grammatical rules.
//! - A [Rules][rules::Rules] structure containing a set of grammatical error correction rules.
//! - A [Spell][spell::Spell] spellchecker, stored with the rules and disabled by default.
//!
//! # Examples
//!
//! Correct a text:
//!
//! ```no_run
//! use nlprule::{Tokenizer, Rules};
//!
//! let tokenizer = Tokenizer::new("path/to/en_tokenizer.bin")?;
//! let mut rules = Rules::new("path/to/en_rules.bin")?;
//! // enable spellchecking
//! rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_GB")?);
//!
//! assert_eq!(
//!     rules.correct("I belive she was not been here since Monday.", &tokenizer),
//!     String::from("I believe she was not here since Monday.")
//! );
//! # Ok::<(), nlprule::Error>(())
//! ```
//!
//! Get suggestions and correct a text:
//!
//! ```no_run
//! use nlprule::{Tokenizer, Rules, types::Suggestion, rules::apply_suggestions};
//!
//! let tokenizer = Tokenizer::new("path/to/en_tokenizer.bin")?;
//! let rules = Rules::new("path/to/en_rules.bin")?;
//!
//! let text = "She was not been here since Monday.";
//!
//! let suggestions = rules.suggest(text, &tokenizer);
//! assert_eq!(*suggestions[0].span().char(), 4usize..16);
//! assert_eq!(suggestions[0].replacements(), vec!["was not", "has not been"]);
//! assert_eq!(suggestions[0].source(), "GRAMMAR/WAS_BEEN/1");
//! assert_eq!(suggestions[0].message(), "Did you mean was not or has not been?");
//!
//! let corrected = apply_suggestions(text, &suggestions);
//!
//! assert_eq!(corrected, "She was not here since Monday.");
//! # Ok::<(), nlprule::Error>(())
//! ```
//!
//! Tokenize & analyze a text:
//!
//! ```no_run
//! use nlprule::Tokenizer;
//!
//! let tokenizer = Tokenizer::new("path/to/en_tokenizer.bin")?;
//!
//! let text = "A brief example is shown.";
//!
//! // returns an iterator over sentences
//! let sentence = tokenizer.pipe(text).next().expect("`text` contains one sentence.");
//!
//! println!("{:#?}", sentence);
//! assert_eq!(sentence.tokens()[1].word().text().as_str(), "brief");
//! assert_eq!(sentence.tokens()[1].word().tags()[0].pos().as_str(), "JJ");
//! assert_eq!(sentence.tokens()[1].chunks(), vec!["I-NP-singular"]);
//! // some other information like char / byte span, lemmas etc. is also set!
//! # Ok::<(), nlprule::Error>(())
//! ```
//! ---
//! Binaries are distributed with [Github releases](https://github.com/bminixhofer/nlprule/releases).

#![warn(missing_docs)]
use std::io::{self, Read, Write};

use thiserror::Error;

pub mod builtin;
#[cfg(feature = "compile")]
pub mod compile;
mod filter;
#[cfg(feature = "memory-probe")]
#[allow(missing_docs)]
#[doc(hidden)]
pub mod memory_probe;
pub mod profile;
pub mod rule;
pub mod rules;
pub mod spell;
pub mod tokenizer;
pub mod types;
pub(crate) mod utils;

pub use rules::Rules;
pub use tokenizer::Tokenizer;

#[derive(Error, Debug)]
#[allow(missing_docs)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    /// (De)serialization error. Can have occured during deserialization or during serialization.
    #[error(transparent)]
    Serialization(#[from] bincode::Error),
    #[error(transparent)]
    IdError(#[from] rule::id::Error),
    #[error("unknown language variant: \"{0}\". known variants are: {1:?}.")]
    UnknownVariant(String, Vec<String>),
    /// The binary is not a writemorebetter tokenizer/rules binary, or it was written by an
    /// incompatible engine version. Carries a human-readable explanation.
    #[error("{0}")]
    Format(String),
}

/// The magic bytes that prefix every serialized [Tokenizer] binary.
///
/// The trailing `01` is part of the fixed magic, not the format version (which is written
/// separately in the header, see [TOKENIZER_FORMAT_VERSION]).
pub(crate) const TOKENIZER_MAGIC: &[u8; 8] = b"WMBTOK01";

/// The magic bytes that prefix every serialized [Rules] binary.
///
/// The trailing `01` is part of the fixed magic, not the format version (which is written
/// separately in the header, see [RULES_FORMAT_VERSION]).
pub(crate) const RULES_MAGIC: &[u8; 8] = b"WMBRUL01";

/// The on-disk format version of [Tokenizer] binaries.
///
/// Bump this whenever any type reachable from [Tokenizer] changes its serialized shape — a
/// new field, a changed field type, a reordered `enum`, or a changed `serde` attribute.
/// `bincode` has no schema, so without a bump an older or newer engine would deserialize the
/// payload into shifted fields and silently produce wrong results instead of failing.
///
/// Version 2: the rule-engine matcher representation became compact (`MatcherKind`,
/// `Box<[WordIdInt]>` text-matcher sets, `Vec<u32>` composition indices, `u32` quantifiers).
/// Version 3: `TokenizerLangOptions::chunk_number_from_last_token` was added.
/// Version 4: compositions store `PartId` indices into a shared `PartPool` instead of owning
/// their `Part`s.
/// Version 5: `Composition.start_keys` was added (a literal-token prefilter).
/// Version 6: identical antipattern compositions share entries in `PartPool`.
/// Version 7: `Composition.required_key` extends the literal prefilter.
/// Version 8: hashed collections (the tagger's tag store, the chunker's tag dictionary and
/// text-engine group maps) are written in sorted key order, and the chunker model's parameter
/// arrays are laid out in sorted context order, so the binary no longer depends on the
/// architecture's hash-table layout.
/// Version 9: `TaggerLangOptions::title_case_all_uppercase` was added.
/// Version 10: `TokenizerLangOptions::word_chars` was added.
/// Version 11: `MatcherKind::Synthesis` was added (`<match postag="…"/>` inside a `<token>`).
/// Version 12: `WordDataAtom::first_reading` was added (`scope="next"` exceptions see only the
/// next token's first reading).
/// Version 13: multiword entries carry their lemma, and `TokenizerLangOptions` gained the
/// `multiword_*` casing and tag-replacement options.
/// Version 14: the part pool stores each antipattern's `<marker>` range.
/// Version 15: `TaggerLangOptions::tag_g_dropped_ing` was added.
/// Version 16: `MatcherKind::AffixedGraph` was added (literal text around a `<match>` inside a
/// `<token>`).
pub const TOKENIZER_FORMAT_VERSION: u32 = 16;

/// The on-disk format version of [Rules] binaries.
///
/// Bump this whenever any type reachable from [Rules] changes its serialized shape. See
/// [TOKENIZER_FORMAT_VERSION] for why.
///
/// Version 3: rule-group unification and grammar-rule parsing changes (M4I).
/// Version 4: the compact matcher representation above, plus `Rule::examples` stored as a lazily
/// parsed blob ([`rule::Examples`]).
/// Version 5: `Matcher::reference_conversion` was added.
/// Version 6: the `+DT`/`+INDT` article word lists moved from every rule to one copy on `Rules`
/// (`Rule::articles` became `Rule::uses_articles`), and compositions store `PartId` indices into a
/// shared `Rules.parts` pool instead of owning their `Part`s.
/// Version 7: `Composition.start_keys` was added (a literal-token prefilter).
/// Version 8: identical antipattern compositions share entries in `Rules.parts`.
/// Version 9: `Composition.required_key` extends the literal prefilter.
/// Version 10: lazily parsed rule examples are stored as LZ4-compressed blocks.
/// Version 11: hashed collections (the spellchecker's maps and whitelist, builtin word lists and
/// replacement tables, filter tables, text-engine group maps) are written in sorted key order, as
/// in tokenizer version 8.
/// Version 12: `SpellLangOptions::word_chars` was added.
/// Version 13: `Rule::suppress_misspelled` and `Synthesizer::suppress_misspelled` were added.
/// Version 14: `MatcherKind::Synthesis` was added (`<match postag="…"/>` inside a `<token>`).
/// Version 15: `SpellLangOptions::default_variant` was added.
/// Version 16: `WordDataAtom::first_reading` was added, as in tokenizer version 12.
/// Version 17: `Rule::mother_tongue` was added.
/// Version 18: `Rule::raw_pos` was added.
/// Version 19: `Spell::phrases` was added.
/// Version 20: the part pool stores each antipattern's `<marker>` range, as in tokenizer
/// version 14.
/// Version 21: `SpellOptions::freq_weight` was removed.
/// Version 22: `RuleFilter` gained the `FindSuggestions`, `SuppressMisspelledSuggestions` and
/// `NumberInWord` filters.
/// Version 23: the `DoublePunctuation`, `MultipleWhitespace`, `SentenceWhitespace` and `EmptyLine`
/// builtins were added.
/// Version 24: `PosReplacer::spell_number` was added (`<match postag="_spell_number_"/>`).
/// Version 25: `MatcherKind::AffixedGraph` was added, as in tokenizer version 16.
/// Version 26: `Rule::repetition` was added.
/// Version 27: the `LongSentence`, `LongParagraph` and `WordRepeatBeginning` builtins were added.
pub const RULES_FORMAT_VERSION: u32 = 27;

/// The longest engine version string accepted in a binary header.
const MAX_ENGINE_VERSION_LEN: usize = 256;

/// Which of the two self-describing binaries a header belongs to.
#[derive(Clone, Copy)]
pub(crate) enum BinaryKind {
    Tokenizer,
    Rules,
}

impl BinaryKind {
    /// The fixed 8-byte magic prefix for this kind of binary.
    fn magic(self) -> &'static [u8; 8] {
        match self {
            BinaryKind::Tokenizer => TOKENIZER_MAGIC,
            BinaryKind::Rules => RULES_MAGIC,
        }
    }

    /// The name used for this kind of binary in error messages.
    fn name(self) -> &'static str {
        match self {
            BinaryKind::Tokenizer => "tokenizer",
            BinaryKind::Rules => "rules",
        }
    }

    /// The kind a magic prefix belongs to, or `None` if it is not one of ours.
    fn from_magic(magic: &[u8; 8]) -> Option<Self> {
        if magic == TOKENIZER_MAGIC {
            Some(BinaryKind::Tokenizer)
        } else if magic == RULES_MAGIC {
            Some(BinaryKind::Rules)
        } else {
            None
        }
    }
}

/// Write the self-describing header that prefixes a serialized binary payload:
/// an 8-byte magic, the little-endian `u32` format version, and the engine version
/// (`env!("CARGO_PKG_VERSION")`) as a little-endian `u32` length followed by UTF-8 bytes.
pub(crate) fn write_binary_header<W: Write>(
    writer: &mut W,
    kind: BinaryKind,
    format_version: u32,
) -> Result<(), Error> {
    writer.write_all(kind.magic())?;
    writer.write_all(&format_version.to_le_bytes())?;
    let engine_version = env!("CARGO_PKG_VERSION");
    writer.write_all(&(engine_version.len() as u32).to_le_bytes())?;
    writer.write_all(engine_version.as_bytes())?;
    Ok(())
}

/// Read and validate a header written by [write_binary_header] for a binary of `expected`
/// kind and `expected_format_version`.
///
/// Any binary that is not exactly ours — an older or newer format, the other kind of binary,
/// or something else entirely — fails with [Error::Format] rather than silently deserializing
/// into garbage.
pub(crate) fn read_binary_header<R: Read>(
    reader: &mut R,
    expected: BinaryKind,
    expected_format_version: u32,
) -> Result<(), Error> {
    let truncated = || {
        Error::Format(format!(
            "not a writemorebetter {} binary: truncated or empty file",
            expected.name()
        ))
    };

    let mut magic = [0u8; 8];
    if let Err(error) = reader.read_exact(&mut magic) {
        return Err(match error.kind() {
            io::ErrorKind::UnexpectedEof => truncated(),
            _ => Error::Io(error),
        });
    }

    if &magic != expected.magic() {
        return Err(Error::Format(match BinaryKind::from_magic(&magic) {
            Some(found) => format!(
                "not a writemorebetter {} binary: found a {} binary instead",
                expected.name(),
                found.name()
            ),
            None => format!("not a writemorebetter {} binary", expected.name()),
        }));
    }

    let mut version = [0u8; 4];
    if let Err(error) = reader.read_exact(&mut version) {
        return Err(match error.kind() {
            io::ErrorKind::UnexpectedEof => truncated(),
            _ => Error::Io(error),
        });
    }
    let format_version = u32::from_le_bytes(version);

    // Read the engine version even when the format version matches: a mismatch report needs it.
    let mut length = [0u8; 4];
    if let Err(error) = reader.read_exact(&mut length) {
        return Err(match error.kind() {
            io::ErrorKind::UnexpectedEof => truncated(),
            _ => Error::Io(error),
        });
    }
    let length = u32::from_le_bytes(length) as usize;
    if length > MAX_ENGINE_VERSION_LEN {
        return Err(Error::Format(format!(
            "invalid writemorebetter {} binary header: engine version length {length} is too large",
            expected.name()
        )));
    }
    let mut engine_version = vec![0u8; length];
    reader.read_exact(&mut engine_version)?;
    let engine_version = String::from_utf8(engine_version).map_err(|_| {
        Error::Format(format!(
            "invalid writemorebetter {} binary header: engine version is not valid UTF-8",
            expected.name()
        ))
    })?;

    if format_version != expected_format_version {
        return Err(Error::Format(format!(
            "writemorebetter {} binary format version {format_version} is not supported by this \
             engine (expected format version {expected_format_version}); the binary was written by \
             engine version {engine_version}",
            expected.name()
        )));
    }

    Ok(())
}

/// Gets the canonical filename for the tokenizer binary for a language code in ISO 639-1 (two-letter) format.
pub fn tokenizer_filename(lang_code: &str) -> String {
    format!("{}_tokenizer.bin", lang_code)
}

/// Gets the canonical filename for the rules binary for a language code in ISO 639-1 (two-letter) format.
pub fn rules_filename(lang_code: &str) -> String {
    format!("{}_rules.bin", lang_code)
}

/// Gets the canonical filename for the tokenizer binary for a language code in ISO 639-1 (two-letter) format.
#[macro_export]
macro_rules! tokenizer_filename {
    ($lang_code:literal) => {
        concat!($lang_code, "_tokenizer.bin")
    };
}

/// Gets the canonical filename for the rules binary for a language code in ISO 639-1 (two-letter) format.
#[macro_export]
macro_rules! rules_filename {
    ($lang_code:literal) => {
        concat!($lang_code, "_rules.bin")
    };
}
