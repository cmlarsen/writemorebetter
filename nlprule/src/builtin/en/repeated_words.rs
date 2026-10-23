//! Flags a word that was already used in one of the preceding sentences and
//! suggests synonyms for it.
//!
//! The behaviour follows LanguageTool's `EN_REPEATEDWORDS` rule, which is
//! tagged `picky` and therefore off by default.

use crate::{
    document::{
        DocumentFix, DocumentMatch, ParagraphDigest, RepeatedReading, RepeatedWord,
        RepeatedWords as RepeatedDigest, TokenSpan,
    },
    types::{DefaultHashMap, Position, Sentence, Token},
    utils::regex::Regex,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "EN_REPEATEDWORDS";

const MESSAGE: &str = "This word has been used in one of the immediately preceding sentences. \
                       Using a synonym could make your text more interesting to read, unless the \
                       repetition is intentional.";

const MAX_WORDS_DISTANCE: usize = 150;

lazy_static! {
    static ref PUNCTUATION: Regex = Regex::new(r"^\p{P}$".to_owned());
}

/// The synonyms of one word and the part-of-speech tag and chunk it needs to
/// be reported.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Synonyms {
    synonyms: Vec<String>,
    pos: Option<Regex>,
    chunk: Option<Regex>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum TokenMatcher {
    Text(String),
    Regex(Regex),
    Pos(Regex),
    Lemma(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PatternToken {
    matcher: TokenMatcher,
    optional: bool,
    skip: usize,
}

/// Flags a word from the synonym list that was used in an earlier sentence
/// less than 150 words before.
///
/// Only sentences that end in `.`, `!` or `?` are checked and counted, so a
/// sentence of dialogue that ends in a closing quote is skipped. Words in the
/// antipatterns (e.g. `need to`), capitalized words that do not start the
/// sentence, words in capitals and proper nouns are ignored.
///
/// # Scope
///
/// The rule remembers words across sentences and paragraphs of the whole text, so it is a
/// whole-text rule (see [`crate::document`]): [`Rules::suggest`](crate::Rules::suggest) looks back
/// within the text it is given, and an app that checks a document paragraph by paragraph gets
/// the look-back across paragraphs from
/// [`Rules::resolve_document`](crate::Rules::resolve_document).
///
/// For whole-document echoes with a configurable window, lemma matching and
/// caller-supplied names, see the `writemorebetter-style` echoes report; how
/// it differs from this rule is in `docs/STYLE-ECHOES.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepeatedWords {
    #[serde(serialize_with = "crate::types::sorted::map")]
    words: DefaultHashMap<String, Synonyms>,
    antipatterns: Vec<Vec<PatternToken>>,
}

impl RepeatedWords {
    /// Creates a new rule from the lines of LanguageTool's synonym list and
    /// the antipattern lines. Malformed lines are returned as errors.
    pub(crate) fn new(
        synonym_lines: &[String],
        antipattern_lines: &[String],
    ) -> Result<Self, String> {
        let mut words = DefaultHashMap::default();
        for line in synonym_lines {
            parse_synonyms(line, &mut words)?;
        }

        let antipatterns = antipattern_lines
            .iter()
            .map(|line| parse_antipattern(line))
            .collect::<Result<_, _>>()?;

        Ok(RepeatedWords {
            words,
            antipatterns,
        })
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Adds what this rule needs from one sentence to `digest`, that of a text which starts at
    /// `start`: the words it counts, and the words it may flag with their readings that have
    /// synonyms. Whether a word is flagged depends on the words before it, so
    /// [`RepeatedWords::resolve`] decides that.
    pub(crate) fn digest(&self, sentence: &Sentence, start: Position, digest: &mut RepeatedDigest) {
        // Only the last sentence of a text decides `last_sentence`.
        digest.last_sentence = None;
        if self.words.is_empty() {
            return;
        }

        let mut tokens: Vec<&Token> = vec![Token::sent_start()];
        tokens.extend(sentence.tokens());

        let last = tokens[tokens.len() - 1].word().text().as_str();
        if !matches!(last, "." | "!" | "?") {
            return;
        }
        // Skipped when it ends the whole text with whitespace, as in LanguageTool.
        if sentence.text().ends_with(is_trailing_token) {
            digest.last_sentence = Some(digest.words.len() as u32);
        }

        let immunized = self.immunized(&tokens);
        let mut sent_start = true;
        let mut starts_sentence = true;

        let kept = tokens.iter().zip(&immunized).filter(|(_, x)| !**x);
        for (i, (token, _)) in kept.enumerate() {
            let text = token.word().text().as_str();
            if !text.is_empty() {
                digest.count += 1;
            }

            let capitalized = is_capitalized_word(text);
            let all_uppercase = !text.chars().any(|c| c.is_alphabetic() && c.is_lowercase());
            let exception = text.is_empty()
                || all_uppercase
                || (capitalized && !sent_start)
                || has_pos_starting_with(tokens[i], "NNP");

            if sent_start && !text.is_empty() && !PUNCTUATION.is_match(text) {
                sent_start = false;
            }
            if exception {
                continue;
            }

            // The readings whose lemma has synonyms: the others are never seen nor flagged.
            let readings: Vec<RepeatedReading> = token
                .word()
                .tags()
                .iter()
                .filter_map(|reading| {
                    let lemma = reading.lemma().as_str();
                    let synonyms = self.words.get(lemma)?;
                    let pos_matches = synonyms
                        .pos
                        .as_ref()
                        .is_none_or(|pos| pos.is_match(reading.pos().as_str()));
                    let chunk_matches = synonyms
                        .chunk
                        .as_ref()
                        .is_none_or(|chunk| token.chunks().iter().any(|tag| chunk.is_match(tag)));
                    Some(RepeatedReading {
                        lemma: lemma.to_owned(),
                        replacements: (pos_matches && chunk_matches).then(|| {
                            self.replacements(
                                sentence,
                                synonyms,
                                reading.pos().as_str(),
                                capitalized,
                            )
                        }),
                    })
                })
                .collect();
            if !readings.is_empty() {
                digest.words.push(RepeatedWord {
                    span: TokenSpan::new(token.span(), start),
                    number: digest.count,
                    starts_sentence,
                    readings,
                });
                starts_sentence = false;
            }
        }
    }

    /// The matches in a document made of `digests` (the current ones, with their indices).
    /// `last` is the index of the last one with text, whose last sentence ends the document.
    pub(crate) fn resolve<'a>(
        &self,
        digests: impl Iterator<Item = (usize, &'a ParagraphDigest)>,
        last: Option<usize>,
    ) -> Vec<DocumentMatch> {
        let mut matches = Vec::new();
        // The number of the word that each lemma was last seen as.
        let mut last_seen: DefaultHashMap<&str, usize> = DefaultHashMap::default();
        let mut lemmas_in_sentence: Vec<&str> = Vec::new();
        let mut before = 0;

        for (paragraph, digest) in digests {
            let digest = &digest.repeated_words;
            let words = match digest.last_sentence {
                Some(end) if Some(paragraph) == last => &digest.words[..end as usize],
                _ => &digest.words[..],
            };
            for word in words {
                if word.starts_sentence {
                    lemmas_in_sentence.clear();
                }
                let number = before + word.number as usize;

                let mut seen = word.readings.len();
                for (i, reading) in word.readings.iter().enumerate() {
                    let lemma = reading.lemma.as_str();
                    let Some(previous) = last_seen.get(lemma) else {
                        continue;
                    };
                    if lemmas_in_sentence.contains(&lemma) || number - previous > MAX_WORDS_DISTANCE
                    {
                        continue;
                    }
                    if let Some(replacements) = &reading.replacements {
                        matches.push(DocumentMatch::new(
                            paragraph,
                            format!("BUILTIN/{}_{}", ID, to_id(lemma)),
                            MESSAGE.to_owned(),
                            word.span.span(),
                            DocumentFix::Replacements(replacements.clone()),
                        ));
                        // The readings after the flagged one are not remembered.
                        seen = i + 1;
                        break;
                    }
                }

                for reading in &word.readings[..seen] {
                    last_seen.insert(&reading.lemma, number);
                    lemmas_in_sentence.push(&reading.lemma);
                }
            }
            before += digest.count as usize;
        }

        matches
    }

    fn replacements(
        &self,
        sentence: &Sentence,
        synonyms: &Synonyms,
        pos: &str,
        capitalized: bool,
    ) -> Vec<String> {
        let mut replacements = Vec::new();
        for synonym in &synonyms.synonyms {
            let forms = sentence.tagger().synthesize(synonym, |tag| tag == pos);
            let forms = if forms.is_empty() {
                vec![synonym.as_str()]
            } else {
                forms
            };
            for form in forms {
                replacements.push(if capitalized {
                    uppercase_first_char(form)
                } else {
                    form.to_owned()
                });
            }
        }
        replacements
    }

    fn immunized(&self, tokens: &[&Token]) -> Vec<bool> {
        let mut immunized = vec![false; tokens.len()];
        for antipattern in &self.antipatterns {
            for start in 0..tokens.len() {
                if let Some(end) = match_at(antipattern, tokens, start) {
                    immunized[start..end].iter_mut().for_each(|x| *x = true);
                }
            }
        }
        immunized
    }
}

/// Returns the end (exclusive) of a match of `pattern` that starts at `start`.
fn match_at(pattern: &[PatternToken], tokens: &[&Token], start: usize) -> Option<usize> {
    let Some((first, rest)) = pattern.split_first() else {
        return Some(start);
    };

    if start < tokens.len() && matches(&first.matcher, tokens[start]) {
        for next in start + 1..=(start + 1 + first.skip).min(tokens.len()) {
            if let Some(end) = match_at(rest, tokens, next) {
                return Some(end);
            }
        }
    }

    if first.optional {
        return match_at(rest, tokens, start);
    }

    None
}

fn matches(matcher: &TokenMatcher, token: &Token) -> bool {
    let text = token.word().text().as_str();
    match matcher {
        TokenMatcher::Text(expected) => text.to_lowercase() == *expected,
        TokenMatcher::Regex(regex) => regex.is_match(text),
        TokenMatcher::Pos(regex) => token
            .word()
            .tags()
            .iter()
            .any(|reading| regex.is_match(reading.pos().as_str())),
        TokenMatcher::Lemma(lemma) => token
            .word()
            .tags()
            .iter()
            .any(|reading| reading.lemma().as_str() == lemma),
    }
}

fn has_pos_starting_with(token: &Token, prefix: &str) -> bool {
    token
        .word()
        .tags()
        .iter()
        .any(|reading| reading.pos().as_str().starts_with(prefix))
}

fn is_capitalized_word(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(char::is_uppercase)
        && chars.all(|c| !c.is_alphabetic() || c.is_lowercase())
}

/// Whether LanguageTool keeps `c` as a token after the last word or
/// punctuation of a text: whitespace, or a no-break space, which it does not
/// count as whitespace and keeps as a token of its own.
fn is_trailing_token(c: char) -> bool {
    c <= ' ' || c == '\u{200B}' || c.is_whitespace()
}

fn uppercase_first_char(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Turns a lemma into the suffix of LanguageTool's specific rule id, e.g.
/// `nice` into `NICE`.
fn to_id(lemma: &str) -> String {
    lemma
        .to_uppercase()
        .trim()
        .replace(' ', "_")
        .replace('\'', "_Q_")
        .chars()
        .map(|c| match c {
            'A'..='Z' | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{DE}' => c,
            _ => '_',
        })
        .collect()
}

/// Parses one line of the synonym list: `word[/pos[/chunk]]=synonym;…`, or
/// `word;word;…` for words that are synonyms of each other.
fn parse_synonyms(line: &str, words: &mut DefaultHashMap<String, Synonyms>) -> Result<(), String> {
    let line = line.split('#').next().unwrap_or("").trim();
    if line.is_empty() {
        return Ok(());
    }

    let error = || format!("malformed synonym line: {}", line);
    let full_match = |pattern: &str| compiled(format!("^(?:{})$", pattern), line);

    let parts = java_split(line, '=');
    match parts.as_slice() {
        [word, synonyms] => {
            let synonyms: Vec<String> = java_split(synonyms, ';')
                .into_iter()
                .map(str::to_owned)
                .collect();
            let mut fields = java_split(word, '/').into_iter();
            let word = fields.next().unwrap_or("");
            let pos = fields.next().map(full_match).transpose()?;
            let chunk = fields.next().map(full_match).transpose()?;
            if word.is_empty() || synonyms.is_empty() || words.contains_key(word) {
                return Err(error());
            }
            words.insert(
                word.to_owned(),
                Synonyms {
                    synonyms,
                    pos,
                    chunk,
                },
            );
        }
        [group] => {
            let group = java_split(group, ';');
            if group.len() < 2 {
                return Err(error());
            }
            for word in &group {
                if words.contains_key(*word) {
                    return Err(error());
                }
                let synonyms = group
                    .iter()
                    .filter(|other| *other != word)
                    .map(|other| (*other).to_owned())
                    .collect();
                words.insert(
                    (*word).to_owned(),
                    Synonyms {
                        synonyms,
                        pos: None,
                        chunk: None,
                    },
                );
            }
        }
        _ => return Err(error()),
    }

    Ok(())
}

/// Splits `text` at `separator` like Java's `String.split`, which drops the
/// empty strings at the end, so `a;b;` gives `a` and `b`.
fn java_split(text: &str, separator: char) -> Vec<&str> {
    let mut parts: Vec<&str> = text.split(separator).collect();
    if parts.len() > 1 {
        while parts.last() == Some(&"") {
            parts.pop();
        }
    }
    parts
}

fn compiled(pattern: String, line: &str) -> Result<Regex, String> {
    let regex = Regex::new(pattern);
    regex
        .try_compile()
        .map_err(|error| format!("invalid regular expression in {}: {}", line, error))?;
    Ok(regex)
}

/// Parses one antipattern line (see `repeated_words_antipatterns.txt`).
fn parse_antipattern(line: &str) -> Result<Vec<PatternToken>, String> {
    let mut pattern: Vec<PatternToken> = Vec::new();

    for part in line.split_whitespace() {
        if let Some(skip) = part.strip_prefix("skip:") {
            let token = pattern
                .last_mut()
                .ok_or_else(|| format!("'skip:' before any token: {}", line))?;
            token.skip = skip
                .parse()
                .map_err(|_| format!("malformed skip in antipattern: {}", line))?;
            continue;
        }

        let (optional, part) = match part.strip_prefix("opt:") {
            Some(part) => (true, part),
            None => (false, part),
        };
        let matcher = if let Some(regex) = part.strip_prefix("re:") {
            TokenMatcher::Regex(compiled(format!("(?i)^(?:{})$", regex), line)?)
        } else if let Some(regex) = part.strip_prefix("pos:") {
            TokenMatcher::Pos(compiled(format!("^(?:{})$", regex), line)?)
        } else if let Some(lemma) = part.strip_prefix("lemma:") {
            TokenMatcher::Lemma(lemma.to_owned())
        } else {
            TokenMatcher::Text(part.to_lowercase())
        };

        pattern.push(PatternToken {
            matcher,
            optional,
            skip: 0,
        });
    }

    if pattern.is_empty() {
        return Err(format!("empty antipattern: {}", line));
    }
    Ok(pattern)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(synonym_lines: &[&str]) -> Result<RepeatedWords, String> {
        let lines: Vec<String> = synonym_lines.iter().map(|x| (*x).to_owned()).collect();
        RepeatedWords::new(&lines, &[])
    }

    #[test]
    fn synonym_lines_split_like_java() {
        let rule = rule(&["nice/JJ=pleasant;charming;", "big;large; # comment"]).unwrap();
        assert_eq!(rule.words["nice"].synonyms, ["pleasant", "charming"]);
        assert_eq!(rule.words["big"].synonyms, ["large"]);
        assert_eq!(rule.words["large"].synonyms, ["big"]);
    }

    #[test]
    fn malformed_synonym_lines_are_errors() {
        assert!(rule(&["nice="]).is_err());
        assert!(rule(&["nice=a=b"]).is_err());
        assert!(rule(&["nice"]).is_err());
        assert!(rule(&["nice=a", "nice=b"]).is_err());
        assert!(rule(&["nice/(=a"]).is_err());
    }
}
