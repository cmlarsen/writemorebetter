use std::collections::BTreeSet;

use crate::reports::echoes::{echo_families, fold, listed, CONTENT_TAGS};
use crate::{Sentence, Token};

const SPEECH_VERBS: [&str; 7] = ["answer", "ask", "reply", "say", "speak", "talk", "tell"];

fn excluded(word: &str) -> bool {
    listed(word) || SPEECH_VERBS.contains(&word)
}

pub(crate) fn words(sentence: &Sentence) -> impl Iterator<Item = (&Token, bool)> {
    let mut opening = true;
    sentence.tokens.iter().filter_map(move |token| {
        if token.is_whitespace {
            return None;
        }
        let at_start = opening;
        opening = token.is_punctuation
            && (opening || token.text.chars().all(|c| "“\"‘'([—–:".contains(c)));
        (token.is_word && !token.is_skipped).then_some((token, at_start))
    })
}

pub(crate) struct Found<'a, T> {
    pub lemma: String,
    pub value: &'a T,
}

pub(crate) fn lookup<'a, T>(
    token: &Token,
    at_start: bool,
    get: impl Fn(&str) -> Option<&'a T>,
) -> Option<Found<'a, T>> {
    let surface = fold(&token.text);
    let capitalized = token.text.chars().next().is_some_and(char::is_uppercase)
        && token.text.chars().any(char::is_lowercase);
    if excluded(&surface) || (capitalized && !at_start) {
        return None;
    }
    if token.readings.is_empty() {
        return get(&surface).map(|value| Found {
            lemma: surface,
            value,
        });
    }
    if token.readings.iter().all(|r| r.pos.starts_with("NNP")) {
        return None;
    }
    let mut families = echo_families(token);
    if families.is_empty() {
        families = CONTENT_TAGS
            .into_iter()
            .filter(|tag| token.readings.iter().any(|r| r.pos.starts_with(tag)))
            .collect();
    }
    if families.is_empty() {
        return None;
    }
    let lemmas: BTreeSet<String> = token
        .readings
        .iter()
        .filter(|r| families.iter().any(|tag| r.pos.starts_with(tag)))
        .map(|r| fold(&r.lemma))
        .filter(|lemma| !lemma.is_empty() && !lemma.contains(char::is_whitespace))
        .collect();
    if lemmas.iter().any(|lemma| excluded(lemma)) {
        return None;
    }
    let mut hits: Vec<_> = lemmas
        .into_iter()
        .filter_map(|lemma| get(&lemma).map(|value| (lemma, value)))
        .collect();
    let (lemma, value) = match hits.len() {
        0 => get(&surface).map(|value| (surface.clone(), value)),
        1 => hits.pop(),
        _ => hits
            .iter()
            .position(|(lemma, _)| *lemma == surface)
            .map(|index| hits.swap_remove(index)),
    }?;
    Some(Found { lemma, value })
}
