use crate::{
    types::*,
    utils::regex::{CaptureMatches, Regex},
};
use serde::{Deserialize, Serialize};
pub mod composition;

use composition::{
    AntipatternId, Composition, GraphId, Group, MatchGraph, MatchSentence, PartPool,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokenEngine {
    pub(crate) composition: Composition,
    pub(crate) antipatterns: Vec<AntipatternId>,
}

struct Blockers {
    starts: Vec<usize>,
    max_ends: Vec<usize>,
    immune: Vec<bool>,
}

impl Blockers {
    fn new(
        engine: &TokenEngine,
        pool: &PartPool,
        sentence: &MatchSentence,
        immunize: bool,
    ) -> Self {
        let mut spans = Vec::new();
        let mut immune = vec![false; if immunize { sentence.len() } else { 0 }];
        for antipattern in &engine.antipatterns {
            for m in sentence.antipattern_matches(pool, *antipattern) {
                if immunize {
                    immune[m.tokens.clone()].fill(true);
                } else {
                    spans.push((m.chars.start, m.chars.end));
                }
            }
        }
        Blockers {
            immune,
            ..Self::from_spans(spans)
        }
    }

    fn from_spans(mut spans: Vec<(usize, usize)>) -> Self {
        spans.sort_unstable();

        let mut max_end = 0;
        let max_ends = spans
            .iter()
            .map(|(_, end)| {
                max_end = max_end.max(*end);
                max_end
            })
            .collect();

        Blockers {
            starts: spans.into_iter().map(|(start, _)| start).collect(),
            max_ends,
            immune: Vec::new(),
        }
    }

    fn overlaps(&self, start: usize, end: usize) -> bool {
        let n = self.starts.partition_point(|anti_start| *anti_start <= end);
        n > 0 && self.max_ends[n - 1] >= start
    }
}

impl TokenEngine {
    fn get_match<'t>(
        &'t self,
        pool: &'t PartPool,
        sentence: &'t MatchSentence,
        i: usize,
        blockers: &mut Option<Blockers>,
        immunize: bool,
    ) -> Option<MatchGraph<'t>> {
        if self.antipatterns.is_empty() {
            return self.composition.apply(pool, sentence, i);
        }

        if !self.composition.can_start(pool, sentence, i) {
            return None;
        }

        if immunize {
            if !self.composition.may_match_immunized(pool, sentence, i) {
                return None;
            }
            let blockers =
                blockers.get_or_insert_with(|| Blockers::new(self, pool, sentence, true));
            return self
                .composition
                .apply_from(pool, sentence, i, Some(&blockers.immune))
                .map(|(graph, _)| graph);
        }

        let (graph, _) = self.composition.apply_from(pool, sentence, i, None)?;

        let blockers = blockers.get_or_insert_with(|| Blockers::new(self, pool, sentence, false));
        let (start, end) = self
            .composition
            .parts
            .iter()
            .enumerate()
            .filter(|(_, part)| pool.get(**part).visible)
            .map(|(idx, _)| graph.by_index(idx + 1).span.char())
            .fold((usize::MAX, 0), |(start, end), span| {
                (start.min(span.start), end.max(span.end))
            });

        if blockers.overlaps(start, end) {
            None
        } else {
            Some(graph)
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Engine {
    Token(TokenEngine),
    // regex with the `fancy_regex` backend is large on the stack
    Text(
        Box<Regex>,
        #[serde(serialize_with = "crate::types::sorted::map")] DefaultHashMap<GraphId, usize>,
    ),
}

struct TokenMatches<'a, 't> {
    engine: &'a TokenEngine,
    pool: &'a PartPool,
    index: usize,
    /// Marks the character ranges already claimed by a match of this rule, so a later
    /// starting position cannot emit an overlapping match. Starts empty and grows on the
    /// first match only: most rules never match a given sentence, and eagerly allocating a
    /// `Vec` sized to the sentence's character count for every rule made the matcher allocate
    /// once per (rule, sentence) regardless.
    mask: Vec<bool>,
    /// The token positions a match can start at, when the composition's first part is a required
    /// literal (see [`Composition::start_keys`]).
    starts: Starts<'t>,
    cursor: usize,
    blockers: Option<Blockers>,
    immunize: bool,
}

/// Which token positions a rule can start a match at.
enum Starts<'t> {
    /// Every position (no prefilter).
    All,
    /// The positions of a single required literal, borrowed from the sentence's index.
    Positions(&'t [u32]),
    /// The merged positions of several required literals.
    Multi(Box<[u32]>),
}

impl Starts<'_> {
    fn as_slice(&self) -> Option<&[u32]> {
        match self {
            Starts::All => None,
            Starts::Positions(positions) => Some(positions),
            Starts::Multi(positions) => Some(positions),
        }
    }
}

struct TextMatches<'a, 't> {
    byte_idx_to_char_idx: DefaultHashMap<usize, usize>,
    id_to_idx: &'a DefaultHashMap<GraphId, usize>,
    captures: CaptureMatches<'a, 't>,
}

enum InnerMatches<'a: 't, 't> {
    Token(TokenMatches<'a, 't>),
    Text(TextMatches<'a, 't>),
}

pub struct EngineMatches<'a, 't> {
    sentence: &'t MatchSentence<'t>,
    start: GraphId,
    end: GraphId,
    inner: InnerMatches<'a, 't>,
}

impl<'a, 't> Iterator for EngineMatches<'a, 't> {
    type Item = MatchGraph<'t>;

    fn next(&mut self) -> Option<Self::Item> {
        let sentence = self.sentence;
        let start_id = self.start;
        let end_id = self.end;

        match &mut self.inner {
            InnerMatches::Token(inner) => loop {
                let filtered = !matches!(&inner.starts, Starts::All);
                let i = if let Some(starts) = inner.starts.as_slice() {
                    if inner.cursor >= starts.len() {
                        return None;
                    }
                    starts[inner.cursor] as usize
                } else {
                    if inner.index >= sentence.len() {
                        return None;
                    }
                    inner.index
                };
                if filtered {
                    inner.cursor += 1;
                } else {
                    inner.index += 1;
                }

                let graph = match inner.engine.get_match(
                    inner.pool,
                    sentence,
                    i,
                    &mut inner.blockers,
                    inner.immunize,
                ) {
                    Some(graph) => graph,
                    None => continue,
                };

                let start_group = graph.by_id(start_id);
                let end_group = graph.by_id(end_id);

                let start = start_group.span.char().start - sentence.span().char().start;
                let end = end_group.span.char().end - sentence.span().char().start;

                if inner.mask.len() < end {
                    inner.mask.resize(end, false);
                }

                if inner.mask[start..end].iter().all(|x| !x) {
                    inner.mask[start..end].iter_mut().for_each(|x| *x = true);
                    return Some(graph);
                }
                // Overlaps a match already emitted by this rule: try the next candidate.
            },
            InnerMatches::Text(inner) => inner.captures.next().map(|captures| {
                let bi_to_ci = &inner.byte_idx_to_char_idx;
                let mut groups = Vec::new();

                for group in captures.iter() {
                    if let Some(group) = group {
                        let byte_span = group.start()..group.end();

                        let char_start = *bi_to_ci
                            .get(&byte_span.start)
                            .expect("byte index is at char boundary");
                        let char_end = *bi_to_ci
                            .get(&byte_span.end)
                            .expect("byte index is at char boundary");

                        groups.push(Group::new(
                            Span::new(byte_span, char_start..char_end)
                                .rshift(sentence.span().start()),
                        ));
                    } else {
                        groups.push(Group::new(Span::default()));
                    }
                }

                MatchGraph::new_map(groups, inner.id_to_idx)
            }),
        }
    }
}

impl Engine {
    pub fn get_matches<'a, 't>(
        &'a self,
        pool: &'a PartPool,
        sentence: &'t MatchSentence,
        start: GraphId,
        end: GraphId,
        immunize: bool,
    ) -> EngineMatches<'a, 't> {
        let inner = match &self {
            Engine::Token(engine) => {
                // If the composition's first part is a required literal, only the positions of
                // tokens with that text can start a match, so jump straight to them (and skip the
                // rule entirely when there are none).
                let starts = match engine.composition.required_key.as_deref() {
                    Some(key) if !sentence.start_positions().may_contain(key) => {
                        Starts::Positions(&[])
                    }
                    _ => match engine.composition.start_keys.as_deref() {
                        None => Starts::All,
                        Some([key]) => Starts::Positions(sentence.start_positions().get(key)),
                        Some(keys) => {
                            let positions = sentence.start_positions();
                            let mut starts: Vec<u32> = Vec::new();
                            for key in keys {
                                starts.extend_from_slice(positions.get(key));
                            }
                            starts.sort_unstable();
                            starts.dedup();
                            Starts::Multi(starts.into_boxed_slice())
                        }
                    },
                };

                InnerMatches::Token(TokenMatches {
                    engine,
                    pool,
                    index: 0,
                    mask: Vec::new(),
                    starts,
                    cursor: 0,
                    blockers: None,
                    immunize,
                })
            }

            Engine::Text(regex, id_to_idx) => {
                let mut bi_to_ci: DefaultHashMap<usize, usize> = sentence
                    .text()
                    .char_indices()
                    .enumerate()
                    .map(|(ci, (bi, _))| (bi, ci))
                    .collect();
                bi_to_ci.insert(sentence.text().len(), bi_to_ci.len());

                InnerMatches::Text(TextMatches {
                    byte_idx_to_char_idx: bi_to_ci,
                    id_to_idx,
                    captures: regex.captures_iter(sentence.text()),
                })
            }
        };

        EngineMatches {
            sentence,
            start,
            end,
            inner,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Blockers;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn blockers_overlap_like_a_linear_scan(spans: Vec<(u8, u8)>, start: u8, len: u8) -> bool {
        let spans: Vec<(usize, usize)> = spans
            .into_iter()
            .map(|(a, b)| (a.min(b) as usize, a.max(b) as usize))
            .collect();
        let (start, end) = (start as usize, start as usize + len as usize);
        let expected = spans
            .iter()
            .any(|(anti_start, anti_end)| *anti_start <= end && start <= *anti_end);

        Blockers::from_spans(spans).overlaps(start, end) == expected
    }

    #[test]
    fn touching_blockers_overlap() {
        let blockers = Blockers::from_spans(vec![(0, 1), (10, 11)]);
        assert!(blockers.overlaps(1, 4));
        assert!(blockers.overlaps(5, 10));
        assert!(blockers.overlaps(1, 1));
        assert!(!blockers.overlaps(2, 9));
        assert!(blockers.overlaps(0, 4));
        assert!(blockers.overlaps(5, 11));
        assert!(blockers.overlaps(0, 0));
    }
}
