//! Constructs the builtin rules for a language from the build directory.
//!
//! Builtin data lists derived from LanguageTool are read from
//! `<build_dir>/builtin/<name>.txt` and serialized into the rules binary, which
//! keeps that LGPL data out of the engine source.

use std::path::Path;

use fs_err as fs;
use log::warn;

use crate::{
    builtin::{en::*, Builtin, BuiltinRule},
    tokenizer::Tokenizer,
};

use super::Error;

/// Builds the builtin rules for the language `lang_code`.
pub(crate) fn for_language(
    lang_code: &str,
    build_dir: &Path,
    tokenizer: &Tokenizer,
) -> Result<Vec<BuiltinRule>, Error> {
    match lang_code.trim() {
        "en" => Ok(vec![
            BuiltinRule::new(Builtin::UnpairedBrackets(UnpairedBrackets::new())),
            BuiltinRule::new(Builtin::UnpairedQuotes(UnpairedQuotes::new())),
            BuiltinRule::new(Builtin::CommaParenthesisWhitespace(
                comma_parenthesis_whitespace(build_dir),
            )),
            BuiltinRule::new(Builtin::AVsAn(a_vs_an(build_dir))),
            BuiltinRule::new(Builtin::WordRepeat(word_repeat(build_dir, tokenizer))),
        ]),
        _ => Ok(Vec::new()),
    }
}

fn comma_parenthesis_whitespace(build_dir: &Path) -> CommaParenthesisWhitespace {
    CommaParenthesisWhitespace::new(read_list(build_dir, "comma_parenthesis_whitespace_domains"))
}

/// Reads the two article word lists. Entries from a `*`-prefixed line are kept
/// case-sensitively; all other entries are lowercased.
fn a_vs_an(build_dir: &Path) -> AVsAn {
    let normalize = |lines: Vec<String>| {
        lines
            .into_iter()
            .map(|line| {
                if let Some(case_sensitive) = line.strip_prefix('*') {
                    case_sensitive.to_owned()
                } else {
                    line.to_lowercase()
                }
            })
            .collect()
    };

    let requires_a = normalize(read_list(build_dir, "a_vs_an_a"));
    let requires_an = normalize(read_list(build_dir, "a_vs_an_an"));

    AVsAn::new(requires_a, requires_an)
}

fn word_repeat(build_dir: &Path, tokenizer: &Tokenizer) -> WordRepeat {
    let exceptions = read_list(build_dir, "word_repeat_exceptions")
        .into_iter()
        .map(|line| {
            let tokens: Vec<String> = tokenizer
                .get_token_strs(&line)
                .into_iter()
                .map(|token| token.to_lowercase())
                .collect();

            // A single-word entry allows that word to be repeated.
            if tokens.len() == 1 {
                vec![tokens[0].clone(), tokens[0].clone()]
            } else {
                tokens
            }
        })
        .filter(|sequence| !sequence.is_empty())
        .collect();

    WordRepeat::new(exceptions)
}

/// Reads a builtin data list from `<build_dir>/builtin/<name>.txt`. One entry
/// per line; lines starting with `#` and blank lines are ignored. A missing
/// file yields an empty list and a warning.
fn read_list(build_dir: &Path, name: &str) -> Vec<String> {
    let path = build_dir.join("builtin").join(format!("{}.txt", name));
    match fs::read_to_string(&path) {
        Ok(content) => content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_owned)
            .collect(),
        Err(_) => {
            warn!(
                "Builtin data file {} does not exist. Using an empty list.",
                path.display()
            );
            Vec::new()
        }
    }
}
