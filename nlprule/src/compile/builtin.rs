//! Constructs the builtin rules for a language from the build directory.
//!
//! Builtin data lists derived from LanguageTool are read from
//! `<build_dir>/builtin/<name>.txt` and serialized into the rules binary, which
//! keeps that LGPL data out of the engine source.

use std::path::Path;

use fs_err as fs;
use log::{info, warn};

use crate::{
    builtin::{en::*, Builtin, BuiltinRule},
    tokenizer::Tokenizer,
    types::DefaultHashSet,
    utils::regex::Regex,
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
            BuiltinRule::new(Builtin::WordRepeat(word_repeat(build_dir, tokenizer))),
            BuiltinRule::new(Builtin::UppercaseSentenceStart(uppercase_sentence_start(
                build_dir,
            ))),
            BuiltinRule::new(Builtin::UnpairedBrackets(UnpairedBrackets::new())),
            BuiltinRule::new(Builtin::UnpairedQuotes(UnpairedQuotes::new())),
            BuiltinRule::new(Builtin::CommaParenthesisWhitespace(
                comma_parenthesis_whitespace(build_dir),
            )),
            BuiltinRule::new(Builtin::AVsAn(a_vs_an(build_dir))),
            BuiltinRule::new(Builtin::SimpleReplace(simple_replace(
                build_dir,
                "simple_replace_contractions",
                "EN_CONTRACTION_SPELLING",
                MatchMode::Single,
                true,
                "Possible spelling mistake found.",
                ", ",
            ))),
            BuiltinRule::new(Builtin::SimpleReplace(simple_replace(
                build_dir,
                "simple_replace_general",
                "EN_SIMPLE_REPLACE",
                MatchMode::Phrase,
                false,
                "Did you mean $suggestions?",
                ", ",
            ))),
            BuiltinRule::new(Builtin::SimpleReplace(simple_replace(
                build_dir,
                "simple_replace_us",
                "EN_US_SIMPLE_REPLACE",
                MatchMode::Phrase,
                false,
                "'$match' is a common British expression. Consider using expressions more \
                 common to American English.",
                ", ",
            ))),
            BuiltinRule::new(Builtin::SimpleReplace(simple_replace(
                build_dir,
                "simple_replace_diacritics",
                "EN_DIACRITICS_REPLACE_ORTHOGRAPHY",
                MatchMode::Phrase,
                false,
                "'$match' is an imported foreign name or expression, which originally has a \
                 diacritic.",
                " or ",
            ))),
            BuiltinRule::new(Builtin::WrongWordInContext(wrong_word_in_context(
                build_dir,
            ))),
            BuiltinRule::new(Builtin::WordCoherency(word_coherency(build_dir))),
        ]),
        _ => Ok(Vec::new()),
    }
}

fn uppercase_sentence_start(build_dir: &Path) -> UppercaseSentenceStart {
    UppercaseSentenceStart::new(read_list(build_dir, "uppercase_sentence_start_exceptions"))
}

fn comma_parenthesis_whitespace(build_dir: &Path) -> CommaParenthesisWhitespace {
    CommaParenthesisWhitespace::new(read_list(build_dir, "comma_parenthesis_whitespace_domains"))
}

/// Reads and normalizes the two article word lists. Entries from a
/// `*`-prefixed line are kept case-sensitively; all other entries are lowercased.
fn a_and_an_words(build_dir: &Path) -> (DefaultHashSet<String>, DefaultHashSet<String>) {
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

    (
        normalize(read_list(build_dir, "a_vs_an_a")),
        normalize(read_list(build_dir, "a_vs_an_an")),
    )
}

fn a_vs_an(build_dir: &Path) -> AVsAn {
    let (requires_a, requires_an) = a_and_an_words(build_dir);

    AVsAn::new(requires_a, requires_an)
}

/// The article word lists for the `+DT`/`+INDT` synthesis in grammar rules
/// (LanguageTool's `EnglishSynthesizer`), read from the same build-dir lists as
/// the a-vs-an builtin.
pub(crate) fn article_data(build_dir: &Path) -> ArticleData {
    let (requires_a, requires_an) = a_and_an_words(build_dir);
    ArticleData::new(requires_a, requires_an)
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

/// Builds a table-driven replacement rule from the data list `<name>.txt`.
#[allow(clippy::too_many_arguments)]
fn simple_replace(
    build_dir: &Path,
    name: &str,
    id: &str,
    mode: MatchMode,
    case_sensitive: bool,
    message: &str,
    suggestions_separator: &str,
) -> SimpleReplace {
    SimpleReplace::new(
        id,
        mode,
        case_sensitive,
        message,
        suggestions_separator,
        read_list(build_dir, name),
    )
}

/// Reads the `other;spelling` pairs of words that admit two spellings.
fn word_coherency(build_dir: &Path) -> WordCoherency {
    let pairs = read_list(build_dir, "word_coherency")
        .into_iter()
        .filter_map(|line| {
            // A trailing semicolon is allowed, as it is by the corresponding
            // LanguageTool loader; anything else with the wrong number of
            // fields is skipped.
            let mut parts: Vec<&str> = line.split(';').collect();
            while parts.last() == Some(&"") {
                parts.pop();
            }
            if parts.len() == 2 {
                Some((parts[0].to_owned(), parts[1].to_owned()))
            } else {
                warn!("Ignoring malformed word coherency line: {}", line);
                None
            }
        })
        .collect();

    WordCoherency::new(pairs)
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

/// Reads the wrong-word-in-context table and translates its Java patterns to
/// Rust `regex` syntax.
///
/// The table is tab separated with the columns
/// `word1 word2 match1 match2 context1 context2 [explanation1 explanation2]`.
/// Lines whose patterns cannot be translated are skipped and reported with
/// their line number, and the number of skipped lines is logged.
fn wrong_word_in_context(build_dir: &Path) -> WrongWordInContext {
    let path = build_dir.join("builtin").join("wrong_word_in_context.txt");
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => {
            warn!(
                "Builtin data file {} does not exist. Using an empty list.",
                path.display()
            );
            return WrongWordInContext::new(Vec::new());
        }
    };

    let mut confusions = Vec::new();
    let mut skipped = 0;
    let mut total = 0;

    for (index, line) in content.lines().enumerate() {
        let line_number = index + 1;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }

        total += 1;
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() < 6 {
            warn!(
                "wrong_word_in_context.txt:{}: expected at least 6 tab-separated columns, found \
                 {}. Skipping the line.",
                line_number,
                columns.len()
            );
            skipped += 1;
            continue;
        }

        match build_confusion(&columns) {
            Ok(confusion) => confusions.push(confusion),
            Err(reason) => {
                warn!(
                    "wrong_word_in_context.txt:{}: {}. Skipping the line.",
                    line_number, reason
                );
                skipped += 1;
            }
        }
    }

    info!(
        "wrong_word_in_context.txt: read {} data lines, compiled {}, skipped {}.",
        total,
        confusions.len(),
        skipped
    );

    WrongWordInContext::new(confusions)
}

/// Builds one confusion row from its table columns, translating the four Java
/// patterns to Rust `regex` syntax.
fn build_confusion(columns: &[&str]) -> Result<Confusion, String> {
    let words = [
        translate_pattern(columns[0], "word1")?,
        translate_pattern(columns[1], "word2")?,
    ];
    let contexts = [
        translate_pattern(columns[4], "context1")?,
        translate_pattern(columns[5], "context2")?,
    ];
    let matches = [columns[2].to_owned(), columns[3].to_owned()];
    let explanations = [
        columns.get(6).copied().unwrap_or("").to_owned(),
        columns.get(7).copied().unwrap_or("").to_owned(),
    ];

    Ok(Confusion::new(words, matches, contexts, explanations))
}

/// Translates one Java pattern, wraps it in word boundaries and compiles it.
fn translate_pattern(pattern: &str, name: &str) -> Result<Regex, String> {
    let translated = translate_java_regex(pattern)
        .map_err(|reason| format!("{} {:?}: {}", name, pattern, reason))?;
    let bounded = with_word_boundaries(&translated);

    regex_syntax::ast::parse::Parser::new()
        .parse(&bounded)
        .map_err(|error| {
            format!(
                "{} {:?}: not valid Rust regex syntax: {}",
                name, pattern, error
            )
        })?;

    let regex = Regex::new(bounded);
    regex.try_compile().map_err(|error| {
        format!(
            "{} {:?}: the engine cannot compile the translated pattern: {}",
            name, pattern, error
        )
    })?;

    Ok(regex)
}

/// Translates a Java regular expression to the syntax of the Rust `regex`
/// crate.
///
/// Only constructs supported by the Rust `regex` crate are accepted. Anything
/// else (lookaround assertions, backreferences, possessive quantifiers, atomic
/// groups and Java-only escapes) yields an error so that the caller can report
/// the offending table line.
fn translate_java_regex(pattern: &str) -> Result<String, String> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut output = String::with_capacity(pattern.len());
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c == '\\' {
            let Some(&escaped) = chars.get(i + 1) else {
                return Err("the pattern ends with a dangling backslash".to_owned());
            };
            i += 2;

            match escaped {
                'Q' => {
                    let mut literal = String::new();
                    let mut terminated = false;
                    while i < chars.len() {
                        if chars[i] == '\\' && chars.get(i + 1) == Some(&'E') {
                            i += 2;
                            terminated = true;
                            break;
                        }
                        literal.push(chars[i]);
                        i += 1;
                    }
                    if !terminated {
                        return Err("an unterminated \\Q...\\E quotation".to_owned());
                    }
                    escape_literal(&literal, &mut output);
                }
                'E' => return Err("\\E without a preceding \\Q".to_owned()),
                '1'..='9' | 'k' => {
                    return Err(
                        "backreferences are not supported by the Rust regex crate".to_owned()
                    );
                }
                'R' | 'X' | 'G' | 'Z' | 'h' | 'H' | 'v' | 'V' => {
                    return Err(format!(
                        "the Java escape \\{} has no equivalent in the Rust regex crate",
                        escaped
                    ));
                }
                _ => {
                    output.push('\\');
                    output.push(escaped);
                }
            }

            continue;
        }

        if c == '(' && chars.get(i + 1) == Some(&'?') {
            match chars.get(i + 2).copied() {
                Some('=') | Some('!') => {
                    return Err(
                        "lookahead assertions are not supported by the Rust regex crate".to_owned(),
                    );
                }
                Some('>') => {
                    return Err(
                        "atomic groups are not supported by the Rust regex crate".to_owned()
                    );
                }
                Some('<') if matches!(chars.get(i + 3), Some('=') | Some('!')) => {
                    return Err(
                        "lookbehind assertions are not supported by the Rust regex crate"
                            .to_owned(),
                    );
                }
                Some('<') => {
                    // Java names a group `(?<name>`; Rust spells it `(?P<name>`.
                    output.push_str("(?P<");
                    i += 3;
                    continue;
                }
                _ => {}
            }
        }

        if is_possessive_quantifier(&chars, i) {
            return Err(
                "possessive quantifiers are not supported by the Rust regex crate".to_owned(),
            );
        }

        output.push(c);
        i += 1;
    }

    Ok(output)
}

/// Whether the character at `i` is a quantifier followed by `+`, which Java
/// reads as a possessive quantifier and the Rust `regex` crate rejects.
fn is_possessive_quantifier(chars: &[char], i: usize) -> bool {
    matches!(chars[i], '*' | '+' | '?' | '}') && chars.get(i + 1) == Some(&'+')
}

/// Appends `literal` to `output`, escaping every character that has a special
/// meaning in a Rust regular expression.
fn escape_literal(literal: &str, output: &mut String) {
    for c in literal.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == ' ' {
            output.push(c);
        } else {
            output.push('\\');
            output.push(c);
        }
    }
}

/// Wraps a translated pattern in word boundaries. A leading `(?i)` is hoisted
/// in front of the boundaries, exactly like LanguageTool's `addBoundaries`.
fn with_word_boundaries(pattern: &str) -> String {
    match pattern.strip_prefix("(?i)") {
        Some(rest) => format!("(?i)\\b({})\\b", rest),
        None => format!("\\b({})\\b", pattern),
    }
}
