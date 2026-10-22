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
                                    // LT's lemma when no reading has the tag.
                                    let first_lemma = token.word().tags().get(0).map_or_else(
                                        || token.word().text().clone(),
                                        |x| x.lemma().clone(),
                                    );

                                    let kept = token
                                        .word()
                                        .tags()
                                        .iter()
                                        .rposition(|x| *x.pos() == limit.pos.as_ref_id());

                                    if let Some(kept) = kept {
                                        let mut index = 0;
                                        token.word_mut().retain(|_| {
                                            index += 1;
                                            index - 1 == kept
                                        });
                                    } else {
                                        token.word_mut().clear();

                                        if *retain_last {
                                            token.word_mut().push(WordData::new(
                                                first_lemma,
                                                limit.pos.as_ref_id(),
                                            ));
                                        } else {
                                            let lemma = token.word().text().clone();

                                            token
                                                .word_mut()
                                                .push(WordData::new(lemma, limit.pos.as_ref_id()));
                                        }
                                    }

                                    // Either way LanguageTool rebuilds the token and appends
                                    // `SENT_END` after the new reading.
                                    token.put_sent_end_last();
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
                                    let rebuilt = token.word().tags().iter().any(|x| {
                                        !x.pos().as_str().is_empty() && filter.is_word_data_match(x)
                                    });
                                    if PosFilter::and(&[filter], token.word()) {
                                        filter.keep(token.word_mut());
                                    }
                                    if rebuilt {
                                        token.put_sent_end_last();
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

                for ((group, disambig), use_mask_val) in groups.into_iter().zip(disambigs).zip(mask)
                {
                    if *use_mask_val {
                        for token in group.into_iter() {
                            let before = token.word().clone();

                            // In LanguageTool `SENT_END` is one of the readings the unify
                            // filters, so a filter like `SENT_END|VB.` keeps it.
                            let keeps_sent_end = token.is_sentence_end() && {
                                let sent_end = WordData::new(
                                    token.word().text().clone(),
                                    PosId::special(SpecialPos::SentEnd),
                                );
                                to_apply.iter().any(|filter| {
                                    filter.iter().all(|f| f.is_word_data_match(&sent_end))
                                }) && disambig
                                    .as_ref()
                                    .map_or(true, |d| d.is_word_data_match(&sent_end))
                            };

                            PosFilter::apply(&to_apply, token.word_mut());

                            if let Some(disambig) = disambig {
                                disambig.keep(token.word_mut());
                            }

                            if !token.word().tags().is_empty() {
                                token.put_sent_end_last();
                            } else if keeps_sent_end {
                                token.keep_only_sent_end();
                            } else {
                                *token.word_mut() = before;
                            }
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

    fn pos(tag: &str, id: u16) -> PosId<'_> {
        PosId::regular(tag, PosIdInt::from_value_unchecked(id))
    }

    fn token<'t>(text: &'t str, readings: &[(&'t str, PosId<'t>)]) -> IncompleteToken<'t> {
        let chars = text.chars().count();
        IncompleteToken::new(
            Word::new(
                WordId(text.into(), None),
                readings
                    .iter()
                    .map(|(lemma, pos)| WordData::new(WordId((*lemma).into(), None), pos.clone()))
                    .collect(),
            ),
            Span::new(0..text.len(), 0..chars),
            false,
            false,
            Vec::new(),
        )
    }

    fn lemma_pos_readings(token: &IncompleteToken) -> Vec<(String, String)> {
        token
            .word()
            .tags()
            .iter()
            .map(|x| (x.lemma().as_str().to_string(), x.pos().as_str().to_string()))
            .collect()
    }

    fn owned_data(lemma: &str, tag: &str, id: u16) -> owned::WordData {
        owned::WordData::new(
            owned::WordId(lemma.into(), None),
            owned::PosId(tag.into(), PosIdInt::from_value_unchecked(id)),
        )
    }

    /// A default-action `<disambig>` with `<wd>` children compiles to
    /// [`Disambiguation::Replace`]: like LT, every reading is replaced by the `<wd>`
    /// reading, whose lemma is the token text when the `<wd>` has none.
    #[test]
    fn replace_sets_the_wd_reading() {
        let replace_with_lemma = Disambiguation::Replace(vec![owned_data("hound", "UH", 2)]);
        let replace_without_lemma = Disambiguation::Replace(vec![owned_data("", "UH", 2)]);

        for text in ["“dog’s", "it’s—", "dog…", "dog\u{2028}"] {
            let readings_before = [("dog", pos("NN", 0)), ("dog", pos("VB", 1))];

            let mut with_lemma = token(text, &readings_before);
            replace_with_lemma.apply(vec![vec![&mut with_lemma]]);
            assert_eq!(
                lemma_pos_readings(&with_lemma),
                vec![("hound".to_string(), "UH".to_string())]
            );

            let mut without_lemma = token(text, &readings_before);
            replace_without_lemma.apply(vec![vec![&mut without_lemma]]);
            assert_eq!(
                lemma_pos_readings(&without_lemma),
                vec![(text.to_string(), "UH".to_string())]
            );
        }
    }

    /// `<disambig postag="…">` keeps the last reading with that tag, or else makes one
    /// whose lemma is the first reading's (with `retain_last`) or the token text.
    #[test]
    fn filter_by_word_data_pos_keeps_or_makes_one_reading() {
        let filter = |retain_last| {
            Disambiguation::Filter(
                vec![Some(either::Left(owned_data("", "NN", 0)))],
                retain_last,
            )
        };
        let (retaining, not_retaining) = (filter(true), filter(false));

        for text in ["“Walks", "walks’", "walks—", "walks…", "walks\u{2028}"] {
            let mut kept = token(
                text,
                &[
                    ("walk", pos("NN", 0)),
                    ("walk", pos("VB", 1)),
                    ("walks", pos("NN", 0)),
                ],
            );
            retaining.apply(vec![vec![&mut kept]]);
            assert_eq!(
                lemma_pos_readings(&kept),
                vec![("walks".to_string(), "NN".to_string())]
            );

            for (filter, lemma) in [(&retaining, "walk"), (&not_retaining, text)] {
                let mut made = token(text, &[("walk", pos("VB", 1)), ("walker", pos("UH", 2))]);
                filter.apply(vec![vec![&mut made]]);
                assert_eq!(
                    lemma_pos_readings(&made),
                    vec![(lemma.to_string(), "NN".to_string())]
                );
            }
        }
    }

    /// `<disambig action="remove"><wd …/>` removes the readings matching every given field;
    /// an empty `<wd/>` removes nothing, like LT's `AnalyzedToken.matches`.
    #[test]
    fn remove_by_word_data_matches_given_fields() {
        let before = [
            ("dog", pos("NN", 0)),
            ("dog", pos("VB", 1)),
            ("hound", pos("NN", 0)),
        ];
        let cases = [
            (owned_data("dog", "", 3), vec![("hound", "NN")]),
            (owned_data("", "NN", 0), vec![("dog", "VB")]),
            (
                owned_data("dog", "NN", 0),
                vec![("dog", "VB"), ("hound", "NN")],
            ),
            (
                owned_data("", "", 3),
                vec![("dog", "NN"), ("dog", "VB"), ("hound", "NN")],
            ),
        ]
        .map(|(data, expected)| (Disambiguation::Remove(vec![either::Left(data)]), expected));

        for (remove, expected) in &cases {
            for text in ["“dog’s", "dog—", "dog…", "dog\u{2028}"] {
                let mut token = token(text, &before);
                remove.apply(vec![vec![&mut token]]);
                let expected: Vec<_> = expected
                    .iter()
                    .map(|(lemma, pos)| (lemma.to_string(), pos.to_string()))
                    .collect();
                assert_eq!(lemma_pos_readings(&token), expected, "{:?}", remove);
            }
        }
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
