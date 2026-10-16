use std::collections::BTreeSet;

use crate::reports::echoes::{echo_families, fold, listed, CONTENT_TAGS};
use crate::{Sentence, Token};

const SPEECH_VERBS: [&str; 7] = ["answer", "ask", "reply", "say", "speak", "talk", "tell"];

fn excluded(word: &str) -> bool {
    listed(word) || SPEECH_VERBS.contains(&word)
}

#[derive(Clone, Copy)]
pub(crate) struct Word<'a> {
    pub token: &'a Token,
    pub at_start: bool,
    pub follows_word: bool,
}

pub(crate) fn words(sentence: &Sentence) -> impl Iterator<Item = Word<'_>> {
    let mut opening = true;
    let mut after_word = false;
    sentence.tokens.iter().filter_map(move |token| {
        if token.is_whitespace {
            return None;
        }
        let at_start = opening;
        let follows_word = after_word;
        opening = token.is_punctuation
            && (opening || token.text.chars().all(|c| "“\"‘'([—–:".contains(c)));
        let counted = token.is_word && !token.is_skipped;
        after_word = counted;
        counted.then_some(Word {
            token,
            at_start,
            follows_word,
        })
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
    prefer: impl Fn(&T) -> bool,
) -> Option<Found<'a, T>> {
    let capitalized = token.text.chars().next().is_some_and(char::is_uppercase)
        && token.text.chars().any(char::is_lowercase);
    if capitalized && !at_start {
        return None;
    }
    lookup_word(token, &get, prefer).or_else(|| lookup_parts(token, &get))
}

fn lookup_parts<'a, T>(token: &Token, get: impl Fn(&str) -> Option<&'a T>) -> Option<Found<'a, T>> {
    if !token.readings.is_empty() && token.readings.iter().all(|r| r.pos.starts_with("NNP")) {
        return None;
    }
    let surface = fold(&token.text);
    let parts: Vec<_> = surface.split('-').filter(|part| !part.is_empty()).collect();
    if parts.len() < 2 {
        return None;
    }
    let last = parts[parts.len() - 1];
    if matches!(
        last,
        "blooded"
            | "footed"
            | "handed"
            | "headed"
            | "hearted"
            | "heartedness"
            | "minded"
            | "mindedness"
            | "mouthed"
            | "nosed"
            | "outs"
            | "tempered"
    ) {
        return None;
    }
    let candidates: &[&str] = if matches!(last, "eyed" | "looking" | "tinted" | "clothed") {
        &[parts[0]]
    } else {
        &[last, parts[0]]
    };
    for &part in candidates {
        if let Some(value) = get(part).filter(|_| !excluded(part)) {
            return Some(Found {
                lemma: part.to_owned(),
                value,
            });
        }
    }
    None
}

fn lookup_word<'a, T>(
    token: &Token,
    get: impl Fn(&str) -> Option<&'a T>,
    prefer: impl Fn(&T) -> bool,
) -> Option<Found<'a, T>> {
    let surface = fold(&token.text);
    if excluded(&surface) {
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
    let readings: Vec<_> = token
        .readings
        .iter()
        .filter(|r| families.iter().any(|tag| r.pos.starts_with(tag)))
        .map(|r| (r.pos.as_str(), fold(&r.lemma)))
        .filter(|(_, lemma)| !lemma.is_empty() && !lemma.contains(char::is_whitespace))
        .collect();
    if readings.iter().any(|(_, lemma)| excluded(lemma)) {
        return None;
    }
    let verb_forms: BTreeSet<_> = readings
        .iter()
        .filter(|(pos, lemma)| pos.starts_with("VB") && *lemma != surface)
        .map(|(_, lemma)| lemma)
        .collect();
    let regular_form = verb_forms.iter().any(|lemma| {
        ["s", "ed", "ing"].iter().any(|suffix| {
            surface == format!("{lemma}{suffix}")
                || lemma
                    .strip_suffix('e')
                    .is_some_and(|stem| surface == format!("{stem}{suffix}"))
        })
    });
    let candidates = if verb_forms.is_empty() {
        readings.iter().map(|(_, lemma)| lemma).collect()
    } else {
        verb_forms
    };
    let mut hits: Vec<_> = candidates
        .into_iter()
        .filter_map(|lemma| get(lemma).map(|value| (lemma.clone(), value)))
        .collect();
    let surface_reading = readings.iter().any(|(_, lemma)| *lemma == surface);
    let (lemma, value) = match hits.len() {
        0 if !surface_reading || regular_form => {
            get(&surface).map(|value| (surface.clone(), value))
        }
        1 => hits.pop(),
        _ => hits
            .iter()
            .position(|(lemma, _)| *lemma == surface)
            .or_else(|| {
                let mut preferred = (0..hits.len()).filter(|&index| prefer(hits[index].1));
                preferred.next().filter(|_| preferred.next().is_none())
            })
            .map(|index| hits.swap_remove(index)),
    }?;
    Some(Found { lemma, value })
}
