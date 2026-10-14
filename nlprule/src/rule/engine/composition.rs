use std::sync::OnceLock;
use std::{convert::TryFrom, iter, ops::Range};

use crate::{tokenizer::tag::Tagger, types::*, utils::regex::Regex};
use enum_dispatch::enum_dispatch;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use unicase::UniCase;

type Context<'a, 't> = (&'a MatchSentence<'t>, &'a MatchGraph<'t>);

/// The ways a [Matcher] compares: a literal string, the text of another pattern group, the word
/// forms synthesized from another pattern group, or a regular expression.
///
/// Kept compact (regex and string boxed, a graph reference is just an id) so [`Atom`] and
/// `Vec<Part>` stay small — those vectors hold one entry per pattern atom and dominate the loaded
/// rule memory.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MatcherKind {
    Text(Box<str>),
    Graph(GraphId),
    Regex(Box<Regex>),
    Synthesis(Box<crate::rule::grammar::Match>, bool),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Matcher {
    pub(crate) matcher: MatcherKind,
    pub negate: bool,
    pub case_sensitive: bool,
    pub empty_always_false: bool,
    /// For a reference to another pattern token (`<match no="…"/>` inside a
    /// `<token>`), the case conversion applied to the referenced token's text
    /// before comparing, mirroring LanguageTool's `case_conversion` attribute.
    /// `None` for every other matcher.
    #[serde(default)]
    pub reference_conversion: Option<crate::rule::grammar::Conversion>,
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
                        let reference = token.word().as_str();
                        if let Some(conversion) = &self.reference_conversion {
                            let converted = conversion.convert(reference, reference);
                            if case_sensitive {
                                converted == input
                            } else {
                                UniCase::new(converted.as_str()) == UniCase::new(input)
                            }
                        } else if case_sensitive {
                            reference == input
                        } else {
                            UniCase::new(reference) == UniCase::new(input)
                        }
                    })
            }
            MatcherKind::Synthesis(reference, is_regex) => {
                let (sentence, graph) =
                    context.expect("context must be set for context-dependent matcher");
                let joined = reference
                    .forms(sentence, graph, None)
                    .map_or_else(String::new, |forms| forms.join("|"));

                if joined.is_empty() {
                    return true;
                }

                if *is_regex {
                    Regex::is_match_uncached(
                        &format!(
                            "{}^(?:{})$",
                            if case_sensitive { "" } else { "(?i)" },
                            joined
                        ),
                        input,
                    )
                } else if case_sensitive {
                    joined == input
                } else {
                    UniCase::new(joined.as_str()) == UniCase::new(input)
                }
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
    use crate::types::SpecialPos;
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
        pub(crate) first_reading: bool,
    }

    impl Atomable for WordDataAtom {
        fn is_match(&self, context: Context, position: usize) -> bool {
            let (sentence, _) = context;
            let tags = sentence.index(position).word().tags();
            let tags = if self.first_reading {
                tags.iter()
                    .position(|x| x.pos().id().value() != SpecialPos::None as u16)
                    .map_or(&tags[..0], |i| &tags[i..i + 1])
            } else {
                tags
            };

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

        let tokens = sentence.sentence.tokens();
        let lo = tokens.partition_point(|x| x.span().char().start < start);
        let hi = tokens
            .partition_point(|x| x.span().char().start < end)
            .max(lo);

        tokens[lo..hi].iter().filter(move |x| {
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
    /// Token positions by token text, built on first use (see [`MatchSentence::start_positions`]).
    pub(crate) start_positions: OnceLock<StartPositions>,
    antipattern_matches: OnceLock<Box<[AntipatternMatches]>>,
}

type AntipatternMatches = OnceLock<Box<[AntipatternMatch]>>;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AntipatternMatch {
    pub(crate) tokens: Range<usize>,
    pub(crate) chars: Range<usize>,
}

/// Token positions in a sentence, keyed by token text (and its lowercase form), used to jump
/// straight to the positions a rule can start at (see [`Composition::start_keys`]).
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct StartPositions {
    /// Lowercased token text -> positions, for case-insensitive keys.
    lower: fnv::FnvHashMap<Box<str>, Vec<u32>>,
    /// Exact token text -> positions, for case-sensitive keys.
    exact: fnv::FnvHashMap<Box<str>, Vec<u32>>,
    has_non_ascii: bool,
}

impl StartPositions {
    pub(crate) fn may_contain(&self, key: &StartKey) -> bool {
        // Lowercasing is not full Unicode case folding, so absence is conclusive only for ASCII.
        (!key.case_sensitive
            && (self.has_non_ascii || !key.text.is_ascii())
            && key.text.chars().any(char::is_alphabetic))
            || !self.get(key).is_empty()
    }

    pub(crate) fn get(&self, key: &StartKey) -> &[u32] {
        let map = if key.case_sensitive {
            &self.exact
        } else {
            &self.lower
        };
        map.get(key.text.as_ref()).map_or(&[], Vec::as_slice)
    }
}

impl<'t> MatchSentence<'t> {
    pub fn new(sentence: &'t Sentence<'t>) -> Self {
        MatchSentence {
            sentence,
            sent_start: Token::sent_start(),
            start_positions: OnceLock::new(),
            antipattern_matches: OnceLock::new(),
        }
    }

    /// The positions of every token by text, built once per sentence.
    pub(crate) fn start_positions(&self) -> &StartPositions {
        self.start_positions.get_or_init(|| {
            let mut positions = StartPositions::default();
            for i in 0..self.len() {
                let text = self.index(i).word().text().as_str();
                positions.has_non_ascii |= !text.is_ascii();
                if text.is_empty() {
                    continue;
                }
                positions
                    .exact
                    .entry(text.into())
                    .or_insert_with(Vec::new)
                    .push(i as u32);
                positions
                    .lower
                    .entry(text.to_lowercase().into_boxed_str())
                    .or_insert_with(Vec::new)
                    .push(i as u32);
            }
            positions
        })
    }

    pub(crate) fn antipattern_matches(
        &self,
        pool: &PartPool,
        id: AntipatternId,
    ) -> &[AntipatternMatch] {
        let cache = self.antipattern_matches.get_or_init(|| {
            (0..pool.antipatterns().len())
                .map(|_| OnceLock::new())
                .collect()
        });
        cache[id.0 as usize].get_or_init(|| {
            let antipattern = pool.antipattern(id);
            let positions = self.start_positions();
            if let Some(key) = antipattern.required_key.as_deref() {
                if !positions.may_contain(key) {
                    return Box::new([]);
                }
            }
            let starts: Vec<usize> = match antipattern.start_keys.as_deref() {
                Some(keys) => {
                    let mut starts: Vec<usize> = keys
                        .iter()
                        .flat_map(|key| positions.get(key))
                        .map(|&i| i as usize)
                        .collect();
                    starts.sort_unstable();
                    starts.dedup();
                    starts
                }
                None => (0..self.len()).collect(),
            };
            starts
                .into_iter()
                .filter(|&i| antipattern.can_start(pool, self, i))
                .filter_map(|i| {
                    let (graph, end) = antipattern.apply_from(pool, self, i, None)?;
                    let start = graph.by_index(0).span.char().start;
                    let end_char = graph.by_index(graph.groups().len() - 1).span.char().end;
                    let tokens = match pool.antipattern_marker(id) {
                        Some((marker_start, marker_end)) => {
                            let from = graph.by_id(marker_start).span.char().start;
                            let to = graph.by_id(marker_end).span.char().end;
                            let inside = |k: &usize| {
                                let span = self.index(*k).span().char();
                                !span.is_empty() && span.start >= from && span.end <= to
                            };
                            match (i..end).find(inside) {
                                Some(first) => first..(i..end).rfind(inside).unwrap() + 1,
                                None => i..i,
                            }
                        }
                        None => i..end,
                    };
                    Some(AntipatternMatch {
                        tokens,
                        chars: start..end_char,
                    })
                })
                .collect()
        })
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
    groups: SmallVec<[Group; 8]>,
    id_to_idx: GraphIndex<'t>,
}

impl<'t> Default for MatchGraph<'t> {
    fn default() -> Self {
        MatchGraph {
            groups: SmallVec::new(),
            id_to_idx: GraphIndex::Slice(&[]),
        }
    }
}

impl<'t> MatchGraph<'t> {
    pub fn new_map(groups: Vec<Group>, id_to_idx: &'t DefaultHashMap<GraphId, usize>) -> Self {
        MatchGraph {
            groups: groups.into(),
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

    /// Gives a position to the groups that matched no token. A group that matched
    /// already carries its span from the matcher, so "empty" is read from the span
    /// rather than by walking the sentence once per group per match. That walk is
    /// O(sentence) and, called once per match, is quadratic for a rule that matches
    /// at every position of a long run of tokens.
    pub fn fill_empty(&mut self, _sentence: &MatchSentence) {
        let empty: Vec<bool> = self.groups.iter().map(|x| x.span.is_empty()).collect();

        let mut start = self
            .groups
            .iter()
            .zip(empty.iter())
            .find(|(_, empty)| !**empty)
            .map(|(group, _)| group.span.start())
            .expect("graph must contain at least one token");

        let mut end = self
            .groups
            .iter()
            .rev()
            .zip(empty.iter().rev())
            .find(|(_, empty)| !**empty)
            .map(|(group, _)| group.span.end())
            .expect("graph must contain at least one token");

        for (group, empty) in self.groups.iter_mut().zip(empty.iter()) {
            if *empty {
                group.span.set_end(start);
            } else {
                start = group.span.end();
            }
        }

        for (group, empty) in self.groups.iter_mut().zip(empty.iter()).rev() {
            if *empty {
                group.span.set_start(end);
            } else {
                end = group.span.start();
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
    pub next_exception: Option<Box<NextExceptions>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NextExceptions {
    pub all: Atom,
    pub first: Atom,
}

struct Immunity<'a> {
    immune: Option<&'a [bool]>,
    stopped: bool,
}

impl Part {
    fn admits(&self, position: usize, immune: Option<&[bool]>) -> bool {
        !self.visible || !immune.is_some_and(|immune| immune[position])
    }
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct AntipatternId(u32);

/// The distinct [`Part`]s of a rule set, addressed by [`PartId`]. See [`PartId`] for why this
/// exists.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartPool {
    parts: Vec<Part>,
    antipatterns: Vec<Composition>,
    antipattern_markers: Vec<Option<(GraphId, GraphId)>>,
}

impl PartPool {
    pub(crate) fn antipattern(&self, id: AntipatternId) -> &Composition {
        &self.antipatterns[id.0 as usize]
    }

    pub(crate) fn antipattern_marker(&self, id: AntipatternId) -> Option<(GraphId, GraphId)> {
        self.antipattern_markers[id.0 as usize]
    }

    pub(crate) fn antipatterns_mut(&mut self) -> &mut Vec<Composition> {
        &mut self.antipatterns
    }

    pub(crate) fn antipatterns(&self) -> &[Composition] {
        &self.antipatterns
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
    seen_antipatterns: std::collections::HashMap<Vec<u8>, AntipatternId>,
    antipatterns: Vec<Composition>,
    antipattern_markers: Vec<Option<(GraphId, GraphId)>>,
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

    pub(crate) fn intern_antipattern(
        &mut self,
        composition: Composition,
        marker: Option<(GraphId, GraphId)>,
    ) -> AntipatternId {
        let key = bincode::serialize(&(&composition, marker)).expect("composition serializes");
        if let Some(id) = self.seen_antipatterns.get(&key) {
            return *id;
        }
        let id = AntipatternId(
            u32::try_from(self.antipatterns.len()).expect("antipattern pool index exhausted"),
        );
        self.antipatterns.push(composition);
        self.antipattern_markers.push(marker);
        self.seen_antipatterns.insert(key, id);
        id
    }

    pub(crate) fn into_pool(self) -> PartPool {
        PartPool {
            parts: self.parts,
            antipatterns: self.antipatterns,
            antipattern_markers: self.antipattern_markers,
        }
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
    /// Literal token texts such that a match of this composition can only *start* at a token
    /// whose text is one of them. `None` when the first part is not a required literal (a regex,
    /// a POS constraint, an optional token, …). The matcher uses this to skip the whole position
    /// scan for a sentence that contains none of the texts. Computed at compile time.
    #[serde(default)]
    pub(crate) start_keys: Option<Box<[StartKey]>>,
    /// A mandatory literal anywhere in the pattern; absence can reject the whole sentence.
    pub(crate) required_key: Option<Box<StartKey>>,
}

/// A literal token text a match must start with, see [`Composition::start_keys`].
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StartKey {
    /// The text to look for; already lowercased when `case_sensitive` is `false`.
    pub(crate) text: Box<str>,
    pub(crate) case_sensitive: bool,
}

impl Atom {
    /// Collects the literal texts such that, if this atom matches a token, that token's text is
    /// one of them. Returns `false` when the atom cannot be constrained this way (a regex, a POS
    /// constraint, a negation, or an offset that moves the check away from the start position).
    ///
    /// The returned set is a *superset* of the texts that can match, so using it to skip whole
    /// rules is sound as long as at least one collected text must match.
    pub(crate) fn start_keys(&self, out: &mut Vec<StartKey>) -> bool {
        match self {
            Atom::TextAtom(atom) => {
                let matcher = &atom.matcher.matcher;
                match &matcher.matcher {
                    MatcherKind::Text(text) if !matcher.negate => {
                        out.push(StartKey {
                            text: if matcher.case_sensitive {
                                text.clone()
                            } else {
                                text.to_lowercase().into_boxed_str()
                            },
                            case_sensitive: matcher.case_sensitive,
                        });
                        true
                    }
                    _ => false,
                }
            }
            // A conjunction matches every branch at the same position, so any single branch that
            // yields keys is enough.
            Atom::AndAtom(and) => {
                let mut best: Option<Vec<StartKey>> = None;
                for atom in &and.atoms {
                    let mut keys = Vec::new();
                    if atom.start_keys(&mut keys)
                        && !keys.is_empty()
                        && best.as_ref().map_or(true, |b| keys.len() < b.len())
                    {
                        best = Some(keys);
                    }
                }
                match best {
                    Some(keys) => {
                        out.extend(keys);
                        true
                    }
                    None => false,
                }
            }
            // A disjunction matches one branch; every branch must be constrained for the union to
            // be a sound prefilter.
            Atom::OrAtom(or) => {
                let mut keys = Vec::new();
                for atom in &or.atoms {
                    if !atom.start_keys(&mut keys) {
                        return false;
                    }
                }
                out.extend(keys);
                true
            }
            // An offset moves the check to another position, so it cannot pin the start token.
            _ => false,
        }
    }
}

impl Composition {
    fn element_after(&self, pool: &PartPool, index: usize) -> (Option<usize>, usize) {
        match self.parts.get(index + 1) {
            Some(id) if !pool.get(*id).visible => (Some(index + 1), index + 2),
            _ => (None, index + 1),
        }
    }

    fn test(
        &self,
        pool: &PartPool,
        context: Context,
        index: usize,
        position: usize,
        immune: Option<&[bool]>,
    ) -> bool {
        let part = pool.get(self.parts[index]);
        part.admits(position, immune) && part.atom.is_match(context, position)
    }

    #[allow(clippy::too_many_arguments)]
    fn test_after(
        &self,
        pool: &PartPool,
        context: Context,
        index: usize,
        position: usize,
        prev: Option<usize>,
        prev_skip: i64,
        immune: Option<&[bool]>,
        excepted: &mut bool,
    ) -> bool {
        if !*excepted && prev_skip > 0 {
            *excepted = prev
                .and_then(|prev| pool.get(self.parts[prev]).next_exception.as_ref())
                .is_some_and(|exception| exception.all.is_match(context, position));
        }
        if !*excepted && prev_skip == 0 && position + 1 < context.0.len() {
            *excepted = pool
                .get(self.parts[index])
                .next_exception
                .as_ref()
                .is_some_and(|exception| exception.first.is_match(context, position + 1));
        }
        !*excepted && self.test(pool, context, index, position, immune)
    }

    #[allow(clippy::too_many_arguments)]
    fn next_element_matches(
        &self,
        pool: &PartPool,
        context: Context,
        current: usize,
        position: usize,
        prev_skip: i64,
        immune: Option<&[bool]>,
        excepted: &mut bool,
    ) -> bool {
        let mut index = self.element_after(pool, current).1;
        while index < self.parts.len() {
            if self.test_after(
                pool,
                context,
                index,
                position,
                Some(current),
                prev_skip,
                immune,
                excepted,
            ) {
                return true;
            }
            if pool.get(self.parts[index]).quantifier.min > 0 {
                return false;
            }
            index = self.element_after(pool, index).1;
        }
        false
    }

    fn match_elements<'t>(
        &'t self,
        pool: &'t PartPool,
        sentence: &'t MatchSentence,
        start: usize,
        mut graph: MatchGraph<'t>,
        immunity: &mut Immunity,
    ) -> Option<(MatchGraph<'t>, usize)> {
        let len = sentence.len() as i64;
        let pattern_size = (self.id_to_idx.len() - 1) as i64;
        let min_occur_correction = self
            .parts
            .iter()
            .filter(|id| {
                let part = pool.get(**id);
                part.visible && part.quantifier.min == 0
            })
            .count() as i64;

        let mut next_pos = start as i64;
        let mut prev_skip = 0i64;
        let mut prev_element: Option<usize> = None;
        let mut prev_present_skip: Option<usize> = None;
        let mut index = 0;
        let mut k = 0i64;

        while index < self.parts.len() {
            let part = pool.get(self.parts[index]);
            let (skip_index, next_index) = self.element_after(pool, index);

            if prev_skip + next_pos >= len || prev_skip < 0 {
                prev_skip = len - (next_pos + 1);
            }
            let max_tok =
                (next_pos + prev_skip).min(len - (pattern_size - k) + min_occur_correction);

            let mut found = None;
            let mut excepted = false;
            let mut m = next_pos;
            while m <= max_tok {
                let position = m as usize;
                let matched = self.test_after(
                    pool,
                    (sentence, &graph),
                    index,
                    position,
                    prev_element,
                    prev_skip,
                    immunity.immune,
                    &mut excepted,
                );
                if part.quantifier.min == 0
                    && self.next_element_matches(
                        pool,
                        (sentence, &graph),
                        index,
                        position,
                        prev_skip,
                        immunity.immune,
                        &mut excepted,
                    )
                {
                    immunity.stopped = true;
                    break;
                }
                if matched {
                    immunity.stopped |= part.quantifier.min == 0 || m < max_tok;
                    found = Some(position);
                    break;
                }
                if excepted {
                    return None;
                }
                m += 1;
            }

            if m > max_tok {
                return None;
            }

            if let Some(first) = found {
                let remaining = pattern_size - k - 1;
                let mut last = first;
                let mut count = 1;
                while count < part.quantifier.max
                    && (last as i64 + 1) < len - remaining
                    && self.test_after(
                        pool,
                        (sentence, &graph),
                        index,
                        last + 1,
                        prev_element,
                        prev_skip,
                        immunity.immune,
                        &mut excepted,
                    )
                {
                    last += 1;
                    count += 1;
                }
                if count < part.quantifier.min {
                    return None;
                }
                immunity.stopped |= count > 1;

                if first as i64 > next_pos {
                    if let Some(skip) = prev_present_skip {
                        let group = &mut graph.groups[skip + 1];
                        group
                            .span
                            .set_start(sentence.index(next_pos as usize).span().start());
                        group.span.set_end(sentence.index(first - 1).span().end());
                    }
                }
                let group = &mut graph.groups[index + 1];
                group.span.set_start(sentence.index(first).span().start());
                group.span.set_end(sentence.index(last).span().end());

                prev_skip = skip_index.map_or(0, |skip| {
                    let max = pool.get(self.parts[skip]).quantifier.max;
                    if max == u32::MAX {
                        -1
                    } else {
                        max as i64
                    }
                });
                prev_present_skip = skip_index;
                next_pos = last as i64 + 1;
            }

            prev_element = Some(index);
            index = next_index;
            k += 1;
        }

        graph.fill_empty(sentence);
        Some((graph, next_pos as usize))
    }

    pub fn apply<'t>(
        &'t self,
        pool: &'t PartPool,
        sentence: &'t MatchSentence,
        start: usize,
    ) -> Option<MatchGraph<'t>> {
        // this path is extremely hot so more optimizations are done
        if !self.can_start(pool, sentence, start) {
            return None;
        }
        self.apply_from(pool, sentence, start, None)
            .map(|(graph, _)| graph)
    }

    pub(crate) fn can_start(
        &self,
        pool: &PartPool,
        sentence: &MatchSentence,
        start: usize,
    ) -> bool {
        // the first matcher can never rely on the match graph, so we use an empty default graph for the first match
        // then allocate a new graph if the first matcher matched
        lazy_static! {
            static ref DEFAULT_GRAPH: MatchGraph<'static> = MatchGraph::default();
        };

        let first = pool.get(self.parts[0]);
        first.quantifier.min == 0 || first.atom.is_match((sentence, &DEFAULT_GRAPH), start)
    }

    pub(crate) fn apply_from<'t>(
        &'t self,
        pool: &'t PartPool,
        sentence: &'t MatchSentence,
        start: usize,
        immune: Option<&[bool]>,
    ) -> Option<(MatchGraph<'t>, usize)> {
        let position = start;

        let graph = MatchGraph {
            groups: smallvec![Group::default(); self.parts.len() + 1],
            id_to_idx: GraphIndex::Slice(&self.id_to_idx),
        };

        let mut immunity = Immunity {
            immune,
            stopped: false,
        };
        self.match_elements(pool, sentence, position, graph, &mut immunity)
    }

    pub(crate) fn may_match_immunized(
        &self,
        pool: &PartPool,
        sentence: &MatchSentence,
        start: usize,
    ) -> bool {
        let graph = MatchGraph {
            groups: smallvec![Group::default(); self.parts.len() + 1],
            id_to_idx: GraphIndex::Slice(&self.id_to_idx),
        };
        let mut immunity = Immunity {
            immune: None,
            stopped: false,
        };
        self.match_elements(pool, sentence, start, graph, &mut immunity)
            .is_some()
            || immunity.stopped
    }
}

#[cfg(test)]
mod prefilter_tests {
    use super::*;

    #[test]
    fn required_literal_absence_is_conservative_for_unicode_case_folding() {
        for (pattern, token) in [("ss", "ß"), ("ſ", "s"), ("k", "K")] {
            assert_eq!(UniCase::new(pattern), UniCase::new(token));
            let mut positions = StartPositions {
                has_non_ascii: !token.is_ascii(),
                ..Default::default()
            };
            positions.lower.insert(token.to_lowercase().into(), vec![1]);
            let key = StartKey {
                text: pattern.to_lowercase().into(),
                case_sensitive: false,
            };
            assert!(positions.may_contain(&key));
        }
        for token in ["“", "’", "—", "…", "\u{2028}"] {
            let positions = StartPositions {
                has_non_ascii: !token.is_ascii(),
                ..Default::default()
            };
            let mut key = StartKey {
                text: "absent".into(),
                case_sensitive: false,
            };
            assert!(positions.may_contain(&key));
            key.case_sensitive = true;
            assert!(!positions.may_contain(&key));
            for text in ["\"", "--", "\u{2019}"] {
                let key = StartKey {
                    text: text.into(),
                    case_sensitive: false,
                };
                assert!(!positions.may_contain(&key));
            }
        }
    }
}
