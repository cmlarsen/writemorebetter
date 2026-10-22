use std::ops::Range;

use crate::types::*;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use super::engine::composition::PosMatcher;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PosFilter {
    pub matcher: PosMatcher,
}

impl PosFilter {
    fn is_word_data_match(&self, data: &WordData) -> bool {
        self.matcher.is_match(data.pos())
    }

    fn keep(&self, data: &mut Word) {
        data.retain(|x| self.is_word_data_match(x))
    }

    pub fn and(filters: &[&Self], data: &Word) -> bool {
        data.tags()
            .iter()
            .any(|x| filters.iter().all(|filter| filter.is_word_data_match(x)))
    }

    pub fn apply(filters: &[Vec<&Self>], data: &mut Word) {
        data.retain(|x| {
            filters
                .iter()
                .any(|filter| filter.iter().all(|f| f.is_word_data_match(x)))
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Disambiguation {
    Remove(Vec<either::Either<owned::WordData, PosFilter>>),
    Add(Vec<owned::WordData>),
    Replace(Vec<owned::WordData>),
    Filter(
        Vec<Option<either::Either<owned::WordData, PosFilter>>>,
        bool,
    ),
    Unify(Vec<Vec<PosFilter>>, Vec<Option<PosFilter>>, Vec<bool>),
    Nop,
    IgnoreSpelling,
}

impl Disambiguation {
    pub fn apply<'t>(&'t self, groups: Vec<Vec<&mut IncompleteToken<'t>>>) {
        match self {
            Disambiguation::Remove(data_or_filters) => {
                for (group, data_or_filter) in groups.into_iter().zip(data_or_filters) {
                    for token in group.into_iter() {
                        match data_or_filter {
                            either::Left(data) => {
                                let any_pos = data.pos.as_ref().is_empty();
                                let any_lemma = data.lemma.as_ref().is_empty();

                                token.retain_tags(|x| {
                                    let matches = (any_pos || *x.pos() == data.pos.as_ref_id())
                                        && (any_lemma || *x.lemma() == data.lemma.as_ref_id());

                                    any_pos && any_lemma || !matches
                                });
                            }
                            either::Right(filter) => {
                                token.retain_tags(|x| !filter.is_word_data_match(x));
                            }
                        }
                    }
                }
            }
            Disambiguation::Filter(filters, retain_last) => {
                for (group, maybe_filter) in groups.into_iter().zip(filters) {
                    if let Some(data_or_filter) = maybe_filter {
                        match data_or_filter {
                            either::Left(limit) => {
                                for token in group.into_iter() {
                                    let last = token.word().tags().get(0).map_or_else(
                                        || token.word().text().clone(),
                                        |x| x.lemma().clone(),
                                    );

                                    let kept = token
                                        .word()
                                        .tags()
                                        .iter()
                                        .rposition(|x| *x.pos() == limit.pos.as_ref_id());

                                    if let Some(kept) = kept {
                                        token.put_sent_end_last();
                                        let mut index = 0;
                                        token.word_mut().retain(|_| {
                                            index += 1;
                                            index - 1 == kept
                                        });
                                    } else {
                                        token.word_mut().clear();

                                        if *retain_last {
                                            token
                                                .word_mut()
                                                .push(WordData::new(last, limit.pos.as_ref_id()));
                                        } else {
                                            let lemma = token.word().text().clone();

                                            token
                                                .word_mut()
                                                .push(WordData::new(lemma, limit.pos.as_ref_id()));
                                        }
                                    }
                                }
                            }
                            either::Right(filter) => {
                                // As in LanguageTool, a filter whose tag matches none of the
                                // token's readings leaves it alone: "as" in "as soon as
                                // possible" keeps its multiword reading against a CC|IN filter.
                                // (`filterall` takes each filter from the token's own pattern
                                // tag, so one reading always matches there. `negate_pos` is not
                                // carried into that filter; no English filterall rule uses it.)
                                for token in group.into_iter() {
                                    // LanguageTool filters only a token with a matching real
                                    // reading, and then appends `SENT_END` last.
                                    if token.word().tags().iter().any(|x| {
                                        !x.pos().as_str().is_empty() && filter.is_word_data_match(x)
                                    }) {
                                        token.put_sent_end_last();
                                    }
                                    if PosFilter::and(&[filter], token.word()) {
                                        filter.keep(token.word_mut())
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Disambiguation::Add(datas) => {
                for (group, data) in groups.into_iter().zip(datas) {
                    for token in group.into_iter() {
                        let data = WordData::new(
                            if data.lemma.as_ref().is_empty() {
                                token.word().text().clone()
                            } else {
                                data.lemma.as_ref_id()
                            },
                            data.pos.as_ref_id(),
                        );

                        token.word_mut().push(data);
                        token.retain_tags(|x| !x.pos().as_str().is_empty());
                    }
                }
            }
            Disambiguation::Replace(datas) => {
                for (group, data) in groups.into_iter().zip(datas) {
                    for token in group.into_iter() {
                        let data = WordData::new(
                            if data.lemma.as_ref().is_empty() {
                                token.word().text().clone()
                            } else {
                                data.lemma.as_ref_id()
                            },
                            data.pos.as_ref_id(),
                        );

                        token.word_mut().clear();
                        token.word_mut().push(data);
                        token.put_sent_end_last();
                    }
                }
            }
            Disambiguation::Unify(filters, disambigs, mask) => {
                let filters: Vec<_> = filters.iter().multi_cartesian_product().collect();

                let mut filter_mask: Vec<_> = filters.iter().map(|_| true).collect();

                for (group, use_mask_val) in groups.iter().zip(mask) {
                    for token in group.iter() {
                        if *use_mask_val {
                            let finalized: Token = (*token).clone().into_token();

                            for (mask_val, filter) in filter_mask.iter_mut().zip(filters.iter()) {
                                *mask_val = *mask_val && PosFilter::and(filter, finalized.word());
                            }
                        }
                    }
                }

                if !filter_mask.iter().any(|x| *x) {
                    return;
                }

                let to_apply: Vec<_> = filter_mask
                    .iter()
                    .zip(filters)
                    .filter_map(
                        |(mask_val, filter)| {
                            if *mask_val {
                                Some(filter)
                            } else {
                                None
                            }
                        },
                    )
                    .collect();
                let sentence_end = PosId::special(SpecialPos::SentEnd);

                for ((group, disambig), use_mask_val) in groups.into_iter().zip(disambigs).zip(mask)
                {
                    if *use_mask_val {
                        for token in group.into_iter() {
                            let before = token.word().clone();

                            PosFilter::apply(&to_apply, token.word_mut());

                            if let Some(disambig) = disambig {
                                disambig.keep(token.word_mut());
                            }

                            // The sentence end's SENT_END reading is only added when the token
                            // is finalized. When it is the one reading that unifies, LanguageTool
                            // keeps it alone, so "!" in "He saw the city!" loses its `.` and `PCT`
                            // tags.
                            let keeps_sentence_end = token.is_sentence_end()
                                && to_apply.iter().any(|filter| {
                                    filter.iter().all(|f| f.matcher.is_match(&sentence_end))
                                })
                                && disambig
                                    .as_ref()
                                    .map_or(true, |d| d.matcher.is_match(&sentence_end));

                            if token.word().tags().is_empty() && !keeps_sentence_end {
                                *token.word_mut() = before;
                            }

                            token.put_sent_end_last();
                        }
                    }
                }
            }
            Disambiguation::Nop => {}
            Disambiguation::IgnoreSpelling => {
                for group in groups {
                    for token in group {
                        *token.ignore_spelling_mut() = true;
                    }
                }
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DisambiguationChange {
    pub text: String,
    pub char_span: Range<usize>,
    pub before: owned::Word,
    pub after: owned::Word,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum DisambiguationExample {
    Unchanged(String),
    Changed(DisambiguationChange),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `<disambig action="add" postag="JJS">` compiles to an [`Disambiguation::Add`]
    /// with an empty lemma. At runtime the matched token's text is used as the
    /// lemma, like LanguageTool does when the added reading has no lemma.
    #[test]
    fn add_with_empty_lemma_uses_token_text() {
        let text = WordId("second-largest".into(), None);
        let pos_nn = PosId::regular("NN", PosIdInt::from_value_unchecked(0));

        let mut token = IncompleteToken::new(
            Word::new(text.clone(), vec![WordData::new(text.clone(), pos_nn)]),
            Span::new(0..13, 0..13),
            false,
            false,
            Vec::new(),
        );

        let disambiguation = Disambiguation::Add(vec![owned::WordData::new(
            owned::WordId("".into(), None),
            owned::PosId("JJS".into(), PosIdInt::from_value_unchecked(1)),
        )]);

        disambiguation.apply(vec![vec![&mut token]]);

        let added = token
            .word()
            .tags()
            .iter()
            .find(|x| x.pos().as_str() == "JJS")
            .expect("a JJS reading must have been added");

        assert_eq!(added.lemma().as_str(), "second-largest");
    }

    /// An explicit lemma is kept as-is.
    #[test]
    fn add_with_lemma_keeps_lemma() {
        let text = WordId("second-largest".into(), None);
        let pos_nn = PosId::regular("NN", PosIdInt::from_value_unchecked(0));

        let mut token = IncompleteToken::new(
            Word::new(text.clone(), vec![WordData::new(text.clone(), pos_nn)]),
            Span::new(0..13, 0..13),
            false,
            false,
            Vec::new(),
        );

        let disambiguation = Disambiguation::Add(vec![owned::WordData::new(
            owned::WordId("large".into(), None),
            owned::PosId("JJS".into(), PosIdInt::from_value_unchecked(1)),
        )]);

        disambiguation.apply(vec![vec![&mut token]]);

        let added = token
            .word()
            .tags()
            .iter()
            .find(|x| x.pos().as_str() == "JJS")
            .expect("a JJS reading must have been added");

        assert_eq!(added.lemma().as_str(), "large");
    }

    fn readings(token: &IncompleteToken) -> Vec<String> {
        token
            .clone()
            .into_token()
            .word()
            .tags()
            .iter()
            .map(|x| x.pos().as_str().to_string())
            .filter(|pos| !pos.is_empty())
            .collect()
    }

    /// `SENT_END` stays after the tagger's readings when disambiguation adds or removes readings,
    /// and goes last when it replaces them, as in LanguageTool 6.6 (a final `.` is
    /// `[./., </S>, ./PCT]`, a final `!` is `[</S>, !/PCT]`).
    #[test]
    fn sent_end_stays_after_the_tagger_readings() {
        let text = WordId("\u{2026}".into(), None);
        let pos = |tag: &'static str, id| PosId::regular(tag, PosIdInt::from_value_unchecked(id));
        let data = |tag: &str, id| {
            owned::WordData::new(
                owned::WordId("".into(), None),
                owned::PosId(tag.into(), PosIdInt::from_value_unchecked(id)),
            )
        };
        let token = |tags: Vec<PosId<'static>>| {
            IncompleteToken::new(
                Word::new(
                    text.clone(),
                    tags.into_iter()
                        .map(|x| WordData::new(text.clone(), x))
                        .collect(),
                ),
                Span::new(0..3, 0..1),
                true,
                false,
                Vec::new(),
            )
        };
        let add_pct = Disambiguation::Add(vec![data("PCT", 1)]);

        let mut tagged = token(vec![pos(".", 0)]);
        add_pct.apply(vec![vec![&mut tagged]]);
        assert_eq!(readings(&tagged), vec![".", "SENT_END", "PCT"]);

        let remove_period = Disambiguation::Remove(vec![either::Left(data(".", 0))]);
        remove_period.apply(vec![vec![&mut tagged]]);
        assert_eq!(readings(&tagged), vec!["SENT_END", "PCT"]);

        let mut untagged = token(vec![]);
        assert_eq!(readings(&untagged), vec!["SENT_END", "UNKNOWN"]);
        add_pct.apply(vec![vec![&mut untagged]]);
        assert_eq!(readings(&untagged), vec!["SENT_END", "PCT"]);

        let replace = Disambiguation::Replace(vec![data(":", 2)]);
        replace.apply(vec![vec![&mut untagged]]);
        assert_eq!(readings(&untagged), vec![":", "SENT_END"]);
    }
}
