use std::iter;

use crate::{tokenizer::tag::Tagger, types::*, utils::regex::Regex};
use enum_dispatch::enum_dispatch;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use unicase::UniCase;

type Context<'a, 't> = (&'a MatchSentence<'t>, &'a MatchGraph<'t>);

/// The three ways a [Matcher] compares: a literal string, the text of another pattern group, or a
/// regular expression.
///
/// Kept compact (regex and string boxed, a graph reference is just an id) so [`Atom`] and
/// `Vec<Part>` stay small — those vectors hold one entry per pattern atom and dominate the loaded
/// rule memory.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MatcherKind {
    Text(Box<str>),
    Graph(GraphId),
    Regex(Box<Regex>),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Matcher {
    pub(crate) matcher: MatcherKind,
    pub negate: bool,
    pub case_sensitive: bool,
    pub empty_always_false: bool,
}

impl Matcher {
    pub fn is_slice_match<S: AsRef<str>>(
        &self,
        input: &[S],
        context: Option<Context>,
        case_sensitive: Option<bool>,
    ) -> bool {
        input
            .iter()
            .any(|x| self.is_match(x.as_ref(), context, case_sensitive))
    }

    pub fn is_match(
        &self,
        input: &str,
        context: Option<Context>,
        case_sensitive: Option<bool>,
    ) -> bool {
        if input.is_empty() {
            return if self.empty_always_false {
                false
            } else {
                self.negate
            };
        }
        let case_sensitive = case_sensitive.unwrap_or(self.case_sensitive);

        let matches = match &self.matcher {
            MatcherKind::Text(string) => {
                if case_sensitive {
                    string.as_ref() == input
                } else {
                    UniCase::new(string.as_ref()) == UniCase::new(input)
                }
            }
            MatcherKind::Graph(id) => {
                let (sentence, graph) =
                    context.expect("context must be set for context-dependent matcher");

                graph
                    .by_id(*id)
                    .tokens(sentence)
                    .next()
                    .map_or(false, |token| {
                        if case_sensitive {
                            token.word().as_str() == input
                        } else {
                            UniCase::new(token.word().as_str()) == UniCase::new(input)
                        }
                    })
            }
            MatcherKind::Regex(regex) => regex.is_match(input),
        };

        if self.negate {
            !matches
        } else {
            matches
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct TextMatcher {
    pub(crate) matcher: Matcher,
    /// The word ids matching `matcher`, sorted so membership is a binary search. `None` when the
    /// matcher depends on the graph or matches too many words. A `HashSet` cost ~8 bytes per id
    /// plus a per-set table; a sorted slice is 4 bytes per id and one allocation.
    pub(crate) set: Option<Box<[WordIdInt]>>,
}

impl TextMatcher {
    pub fn is_match(
        &self,
        word_id: &WordId,
        context: Option<Context>,
        case_sensitive: Option<bool>,
    ) -> bool {
        if let Some(set) = &self.set {
            if let Some(id) = word_id.id() {
                return set.binary_search(&id).is_ok();
            }
        }
        self.matcher
            .is_match(word_id.as_str(), context, case_sensitive)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PosMatcher {
    pub mask: Vec<bool>,
}

impl PosMatcher {
    pub fn is_match(&self, pos: &PosId) -> bool {
        self.mask[pos.id().value() as usize]
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WordDataMatcher {
    pub(crate) pos_matcher: Option<PosMatcher>,
    pub(crate) inflect_matcher: Option<TextMatcher>,
}

impl WordDataMatcher {
    pub fn is_match(
        &self,
        input: &[WordData],
        context: Option<Context>,
        case_sensitive: Option<bool>,
    ) -> bool {
        input.iter().any(|x| {
            let pos_matches = self
                .pos_matcher
                .as_ref()
                .map_or(true, |m| m.is_match(x.pos()));

            // matching part-of-speech tag is faster than inflection, so check POS first and early exit if it doesn't match
            if !pos_matches {
                return false;
            }

            let inflect_matches = self
                .inflect_matcher
                .as_ref()
                .map_or(true, |m| m.is_match(x.lemma(), context, case_sensitive));

            inflect_matches
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Quantifier {
    pub min: u32,
    pub max: u32,
}

#[enum_dispatch]
pub trait Atomable: Send + Sync {
    fn is_match(&self, context: Context, position: usize) -> bool;
}

#[enum_dispatch(Atomable)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Atom {
    ChunkAtom(concrete::ChunkAtom),
    SpaceBeforeAtom(concrete::SpaceBeforeAtom),
    TextAtom(concrete::TextAtom),
    WordDataAtom(concrete::WordDataAtom),
    TrueAtom,
    FalseAtom,
    AndAtom,
    OrAtom,
    NotAtom,
    OffsetAtom,
}

pub mod concrete {
    use super::{Atomable, Context, Matcher, TextMatcher, WordDataMatcher};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub struct TextAtom {
        pub(crate) matcher: TextMatcher,
    }

    impl Atomable for TextAtom {
        fn is_match(&self, context: Context, position: usize) -> bool {
            let (sentence, _) = context;

            self.matcher
                .is_match(&sentence.index(position).word().text(), Some(context), None)
        }
    }

    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub struct ChunkAtom {
        pub(crate) matcher: Matcher,
    }

    impl Atomable for ChunkAtom {
        fn is_match(&self, context: Context, position: usize) -> bool {
            let (sentence, _) = context;

            self.matcher
                .is_slice_match(&sentence.index(position).chunks(), Some(context), None)
        }
    }

    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub struct SpaceBeforeAtom {
        pub(crate) value: bool,
    }

    impl Atomable for SpaceBeforeAtom {
        fn is_match(&self, context: Context, position: usize) -> bool {
            let (sentence, _) = context;

            sentence.index(position).has_space_before() == self.value
        }
    }

    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub struct WordDataAtom {
        pub(crate) matcher: WordDataMatcher,
        pub(crate) case_sensitive: bool,
    }

    impl Atomable for WordDataAtom {
        fn is_match(&self, context: Context, position: usize) -> bool {
            let (sentence, _) = context;
            let tags = &sentence.index(position).word().tags();

            self.matcher
                .is_match(&tags, Some(context), Some(self.case_sensitive))
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct TrueAtom {}

impl Atomable for TrueAtom {
    fn is_match(&self, _context: Context, _position: usize) -> bool {
        true
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct FalseAtom {}

impl Atomable for FalseAtom {
    fn is_match(&self, _context: Context, _position: usize) -> bool {
        false
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AndAtom {
    pub(crate) atoms: Vec<Atom>,
}

impl Atomable for AndAtom {
    fn is_match(&self, context: Context, position: usize) -> bool {
        self.atoms.iter().all(|x| x.is_match(context, position))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrAtom {
    pub(crate) atoms: Vec<Atom>,
}

impl Atomable for OrAtom {
    fn is_match(&self, context: Context, position: usize) -> bool {
        self.atoms.iter().any(|x| x.is_match(context, position))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NotAtom {
    pub(crate) atom: Box<Atom>,
}

impl Atomable for NotAtom {
    fn is_match(&self, context: Context, position: usize) -> bool {
        !self.atom.is_match(context, position)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OffsetAtom {
    pub(crate) atom: Box<Atom>,
    pub(crate) offset: isize,
}

impl Atomable for OffsetAtom {
    fn is_match(&self, context: Context, position: usize) -> bool {
        let (sentence, _) = context;
        let new_position = position as isize + self.offset;

        if new_position < 0 || (new_position as usize) >= sentence.len() {
            false
        } else {
            self.atom.is_match(context, new_position as usize)
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Group {
    pub span: Span,
}

impl Group {
    pub fn new(span: Span) -> Self {
        Group { span }
    }

    pub fn tokens<'a, 't>(
        &'a self,
        sentence: &'t MatchSentence,
    ) -> impl DoubleEndedIterator<Item = &'t Token<'t>> {
        let start = self.span.char().start;
        let end = self.span.char().end;

        sentence.iter().filter(move |x| {
            x.span().char().end > x.span().char().start // special tokens with zero range (e. g. SENT_START) can not be part of groups
                && x.span().char().start >= start
                && x.span().char().end <= end
        })
    }

    pub fn text<'a>(&self, sentence: &'a MatchSentence<'a>) -> &'a str {
        if self.span.char().start >= self.span.char().end {
            return "";
        }

        sentence.slice(self.span.clone())
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct GraphId(pub usize);

impl GraphId {
    /// Returns an iterator from the lower bound (inclusive) to the upper bound (inclusive).
    /// Important: this assumes both ids index into the same graph, otherwise the generated ids
    /// might not be valid!
    pub fn range(lower: &GraphId, upper: &GraphId) -> impl DoubleEndedIterator<Item = GraphId> {
        (lower.0..upper.0 + 1).map(GraphId)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchSentence<'t> {
    sentence: &'t Sentence<'t>,
    sent_start: &'t Token<'t>,
}

impl<'t> MatchSentence<'t> {
    pub fn new(sentence: &'t Sentence<'t>) -> Self {
        MatchSentence {
            sentence,
            sent_start: Token::sent_start(),
        }
    }

    pub fn index(&self, index: usize) -> &Token {
        match index {
            0 => &self.sent_start,
            i => &self.sentence.tokens()[i - 1],
        }
    }

    pub fn iter(&'t self) -> impl DoubleEndedIterator<Item = &'t Token> {
        iter::once(self.sent_start).chain(self.sentence.iter())
    }

    pub fn len(&self) -> usize {
        self.sentence.len() + 1
    }

    pub fn text(&self) -> &str {
        self.sentence.text()
    }

    pub fn slice(&self, span: Span) -> &str {
        let span = span.lshift(self.span().start());
        &self.text()[span.byte().clone()]
    }

    pub fn tagger(&self) -> &'t Tagger {
        self.sentence.tagger()
    }

    pub fn span(&self) -> &Span {
        self.sentence.span()
    }
}

/// How a [`MatchGraph`] maps a [`GraphId`] to a group index.
///
/// Token compositions used to store this as a `HashMap<GraphId, usize>`, which cost ~30 bytes per
/// entry; the mapping is a monotone id → index function, so a `Vec<u32>` (with `u32::MAX` for
/// "absent") is enough. Text engines keep their identity map.
#[derive(Debug, Clone, Copy)]
pub(crate) enum GraphIndex<'t> {
    Slice(&'t [u32]),
    Map(&'t DefaultHashMap<GraphId, usize>),
}

#[derive(Debug, Clone)]
pub struct MatchGraph<'t> {
    groups: Vec<Group>,
    id_to_idx: GraphIndex<'t>,
}

impl<'t> Default for MatchGraph<'t> {
    fn default() -> Self {
        MatchGraph {
            groups: Vec::new(),
            id_to_idx: GraphIndex::Slice(&[]),
        }
    }
}

impl<'t> MatchGraph<'t> {
    pub fn new(groups: Vec<Group>, id_to_idx: &'t [u32]) -> Self {
        MatchGraph {
            groups,
            id_to_idx: GraphIndex::Slice(id_to_idx),
        }
    }

    pub fn new_map(groups: Vec<Group>, id_to_idx: &'t DefaultHashMap<GraphId, usize>) -> Self {
        MatchGraph {
            groups,
            id_to_idx: GraphIndex::Map(id_to_idx),
        }
    }

    pub fn by_index(&self, index: usize) -> &Group {
        &self.groups[index]
    }

    pub fn by_id(&self, id: GraphId) -> &Group {
        &self.groups[self.get_index(id)]
    }

    pub fn get_index(&self, id: GraphId) -> usize {
        match self.id_to_idx {
            GraphIndex::Slice(slice) => {
                let index = slice[id.0];
                assert!(index != u32::MAX, "only valid graph indices exist");
                index as usize
            }
            GraphIndex::Map(map) => *map.get(&id).expect("only valid graph indices exist"),
        }
    }

    /// Like [`MatchGraph::get_index`] but returns `None` for an id that is not
    /// part of this graph (filters may reference tokens that did not match).
    pub fn get_index_opt(&self, id: GraphId) -> Option<usize> {
        match self.id_to_idx {
            GraphIndex::Slice(slice) => slice.get(id.0).and_then(|&index| {
                if index == u32::MAX {
                    None
                } else {
                    Some(index as usize)
                }
            }),
            GraphIndex::Map(map) => map.get(&id).copied(),
        }
    }

    pub fn groups(&self) -> &[Group] {
        &self.groups[..]
    }

    pub fn fill_empty(&mut self, sentence: &MatchSentence) {
        let mut start = self
            .groups
            .iter()
            .find_map(|x| x.tokens(&sentence).next().map(|token| token.span().start()))
            .expect("graph must contain at least one token");

        let mut end = self
            .groups
            .iter()
            .rev()
            .find_map(|x| {
                x.tokens(&sentence)
                    .next_back()
                    .map(|token| token.span().end())
            })
            .expect("graph must contain at least one token");

        let group_tokens: Vec<_> = self
            .groups
            .iter()
            .map(|x| x.tokens(&sentence).collect::<Vec<_>>())
            .collect::<Vec<_>>();

        for (group, tokens) in self.groups.iter_mut().zip(group_tokens.iter()) {
            if !tokens.is_empty() {
                group.span.set_start(tokens[0].span().start());
                group.span.set_end(tokens[tokens.len() - 1].span().end());
                start = tokens[tokens.len() - 1].span().end();
            } else {
                group.span.set_end(start);
            }
        }

        for (group, tokens) in self.groups.iter_mut().zip(group_tokens.iter()).rev() {
            if !tokens.is_empty() {
                end = tokens[0].span().start();
            } else {
                group.span.set_start(end);
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Part {
    pub atom: Atom,
    pub quantifier: Quantifier,
    pub greedy: bool,
    pub visible: bool,
    pub unify: Option<bool>,
}

/// Index of a [`Part`] in a rule set's shared [`PartPool`].
///
/// [`Composition::parts`] stores these instead of owned [`Part`]s: most compositions (especially
/// the per-rulegroup antipatterns cloned into every rule of a group) repeat the same token
/// matchers, so the distinct parts are stored once and referenced by a `u32` index. This turns a
/// 96 B inline `Part` into a 4 B reference.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PartId(pub u32);

/// The distinct [`Part`]s of a rule set, addressed by [`PartId`]. See [`PartId`] for why this
/// exists.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartPool {
    parts: Vec<Part>,
}

impl PartPool {
    pub(crate) fn from_parts(parts: Vec<Part>) -> Self {
        PartPool { parts }
    }

    pub fn get(&self, id: PartId) -> &Part {
        &self.parts[id.0 as usize]
    }

    pub fn len(&self) -> usize {
        self.parts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// All parts, for the memory probe.
    pub(crate) fn parts_mut(&mut self) -> &mut [Part] {
        &mut self.parts
    }

    pub(crate) fn parts(&self) -> &[Part] {
        &self.parts
    }
}

/// Deduplicates [`Part`]s while a rule set is compiled, keyed by their serialized bytes so only
/// truly identical parts (including any baked-in `GraphId`) share.
#[derive(Default)]
pub(crate) struct PartInterner {
    seen: std::collections::HashMap<Vec<u8>, PartId>,
    parts: Vec<Part>,
}

impl PartInterner {
    pub(crate) fn intern(&mut self, part: Part) -> PartId {
        let key = bincode::serialize(&part).expect("part serializes");
        if let Some(id) = self.seen.get(&key) {
            return *id;
        }
        let id = PartId(self.parts.len() as u32);
        self.parts.push(part);
        self.seen.insert(key, id);
        id
    }

    pub(crate) fn get(&self, id: PartId) -> &Part {
        &self.parts[id.0 as usize]
    }

    pub(crate) fn into_pool(self) -> PartPool {
        PartPool::from_parts(self.parts)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Composition {
    /// Indices into the rule set's [`PartPool`].
    pub(crate) parts: Vec<PartId>,
    /// `GraphId(k) -> group index`, as a `Vec` indexed by `k` (`u32::MAX` = absent). See
    /// [`GraphIndex`] for why this is not a map.
    pub(crate) id_to_idx: Vec<u32>,
    pub(crate) can_stop_mask: Vec<bool>,
}

impl Composition {
    fn next_can_match(
        &self,
        pool: &PartPool,
        context: Context,
        position: usize,
        index: usize,
    ) -> bool {
        let next_required_pos = match self.parts[index + 1..]
            .iter()
            .position(|x| pool.get(*x).quantifier.min > 0)
        {
            Some(pos) => index + 1 + pos + 1,
            None => self.parts.len(),
        };

        self.parts[index + 1..next_required_pos]
            .iter()
            .any(|x| pool.get(*x).atom.is_match(context, position))
    }

    fn apply_recursive<'t>(
        &'t self,
        pool: &'t PartPool,
        sentence: &'t MatchSentence,
        mut position: usize,
        mut cur_atom_idx: usize,
        mut graph: MatchGraph<'t>,
    ) -> Option<MatchGraph<'t>> {
        let mut cur_count = 0u32;
        let is_match = loop {
            if cur_atom_idx >= self.parts.len() {
                break true;
            }

            let part = pool.get(self.parts[cur_atom_idx]);

            if cur_count >= part.quantifier.max {
                cur_atom_idx += 1;
                cur_count = 0;
                if cur_atom_idx >= self.parts.len() {
                    break false;
                }
                continue;
            }

            if position >= sentence.len() {
                break false;
            }

            if cur_count >= part.quantifier.min && cur_atom_idx + 1 < self.parts.len() {
                if !part.greedy
                    && self.next_can_match(pool, (sentence, &graph), position, cur_atom_idx)
                {
                    cur_atom_idx += 1;
                    cur_count = 0;
                    continue;
                }
                if part.greedy {
                    if let Some(graph) = self.apply_recursive(
                        pool,
                        sentence,
                        position,
                        cur_atom_idx + 1,
                        graph.clone(),
                    ) {
                        return Some(graph);
                    }
                }
            }

            if part.atom.is_match((sentence, &graph), position) {
                let group = &mut graph.groups[cur_atom_idx + 1];

                // set the group beginning if the char end was zero (i. e. the group was empty)
                if group.span.char().end == 0 {
                    group
                        .span
                        .set_start(sentence.index(position).span().start());
                }
                group.span.set_end(sentence.index(position).span().end());

                position += 1;
                cur_count += 1;
            } else {
                break false;
            }
        };

        // edge case if the last atom is quantified and the minimum has been exceeded
        // TODO to avoid further such undetected edge cases: revisit the entire composition logic
        if cur_atom_idx < self.parts.len()
            && cur_count >= pool.get(self.parts[cur_atom_idx]).quantifier.min
        {
            cur_atom_idx += 1;
        }

        if is_match || cur_atom_idx == self.parts.len() || self.can_stop_mask[cur_atom_idx] {
            graph.fill_empty(sentence);
            Some(graph)
        } else {
            None
        }
    }

    pub fn apply<'t>(
        &'t self,
        pool: &'t PartPool,
        sentence: &'t MatchSentence,
        start: usize,
    ) -> Option<MatchGraph<'t>> {
        // this path is extremely hot so more optimizations are done

        // the first matcher can never rely on the match graph, so we use an empty default graph for the first match
        // then allocate a new graph if the first matcher matched
        lazy_static! {
            static ref DEFAULT_GRAPH: MatchGraph<'static> = MatchGraph::default();
        };

        let first = pool.get(self.parts[0]);
        if first.quantifier.min > 0 && !first.atom.is_match((sentence, &DEFAULT_GRAPH), start) {
            return None;
        }

        let position = start;

        let graph = MatchGraph::new(
            vec![Group::default(); self.parts.len() + 1],
            &self.id_to_idx,
        );

        self.apply_recursive(pool, sentence, position, 0, graph)
    }
}
