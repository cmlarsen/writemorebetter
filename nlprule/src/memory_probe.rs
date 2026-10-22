//! Test-only helpers to measure the loaded memory breakdown of the English binaries.
//!
//! Enabled with the off-by-default `memory-probe` cargo feature. Nothing here is used by the
//! library at runtime; the module exists so `tests/memory_breakdown.rs` can isolate the memory
//! cost of each field of [Tokenizer] and [Rules].
//!
//! Method: [extract_tokenizer]/[extract_rules] deserialize a binary once and re-serialize each
//! component (or a deliberately blanked copy of it) into its own file under `out_dir`. The test
//! then runs each component in a fresh process, where [load] deserializes only that component
//! and the process measures its own RSS growth (see `tests/memory_breakdown.rs`). Measuring a
//! blanked variant against the full variant attributes the cost of the blanked field.

use std::path::Path;

use serde::{de::DeserializeOwned, Serialize};

use crate::rule::disambiguation::Disambiguation;
use crate::rule::engine::composition::{
    Atom, Composition, GraphId, Matcher, MatcherKind, Part, PartPool, Quantifier, TextMatcher,
    WordDataMatcher,
};
use crate::rule::engine::{Engine, TokenEngine};
use crate::rule::grammar::{Synthesizer, SynthesizerPart};
use crate::rule::{DisambiguationRule, Rule};
use crate::spell::Spell;
use crate::tokenizer::chunk::{
    Chunker, MaxentChunker, MaxentPosTagger, MaxentTokenizer, Pmap, TagDict,
};
use crate::tokenizer::multiword::MultiwordTagger;
use crate::types::PosIdInt;
use crate::utils::regex::Regex;
use crate::{Rules, Tokenizer};

/// A component serialized to its own file by [extract_tokenizer]/[extract_rules].
pub struct Extracted {
    /// The component name, also the file stem of the serialized bytes.
    pub name: String,
    /// Length in bytes of the serialized component.
    pub bytes: u64,
    /// Path of the file holding the serialized component.
    pub path: String,
}

/// An opaque handle keeping a deserialized component alive. The test holds this while it reads
/// its own RSS; dropping it releases the component.
pub struct Held(#[allow(dead_code)] Box<dyn std::any::Any>);

impl Held {
    fn new<T: 'static>(value: T) -> Self {
        Held(Box::new(value))
    }
}

fn load_typed<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    // Stream like `Tokenizer::new` / `Rules::new` do. Reading the whole file into a `Vec` first
    // leaves that buffer resident (macOS RSS does not shrink on free) and would inflate every
    // measurement by the file size.
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    bincode::deserialize_from(std::io::BufReader::new(file))
        .map_err(|e| format!("{}: {}", path.display(), e))
}

/// Loads the tokenizer storage binary through the library's reader, so the probe sees the same
/// self-describing header validation the engine does.
fn load_tokenizer(path: &Path) -> Result<Tokenizer, String> {
    Tokenizer::new(path).map_err(|e| format!("{}: {}", path.display(), e))
}

/// Loads the rules storage binary through the library's reader.
fn load_rules(path: &Path) -> Result<Rules, String> {
    Rules::new(path).map_err(|e| format!("{}: {}", path.display(), e))
}

fn write_blob_bytes(
    dir: &Path,
    name: &str,
    bytes: &[u8],
    out: &mut Vec<Extracted>,
) -> Result<(), String> {
    let path = dir.join(format!("{}.bin", name));
    std::fs::write(&path, bytes).map_err(|e| format!("write {}: {}", path.display(), e))?;
    out.push(Extracted {
        name: name.to_string(),
        bytes: bytes.len() as u64,
        path: path.to_string_lossy().into_owned(),
    });
    Ok(())
}

fn write_blob<T: Serialize>(
    dir: &Path,
    name: &str,
    value: &T,
    out: &mut Vec<Extracted>,
) -> Result<(), String> {
    let bytes = bincode::serialize(value).map_err(|e| format!("serialize {}: {}", name, e))?;
    write_blob_bytes(dir, name, &bytes, out)
}

/// Writes a [Tokenizer] blob through the library's writer so it carries the self-describing
/// header and [load] can read it back the same way the engine would.
fn write_tokenizer_blob(
    dir: &Path,
    name: &str,
    value: &Tokenizer,
    out: &mut Vec<Extracted>,
) -> Result<(), String> {
    let mut bytes = Vec::new();
    value
        .to_writer(&mut bytes)
        .map_err(|e| format!("serialize {}: {}", name, e))?;
    write_blob_bytes(dir, name, &bytes, out)
}

/// Writes a [Rules] blob through the library's writer, with the self-describing header.
fn write_rules_blob(
    dir: &Path,
    name: &str,
    value: &Rules,
    out: &mut Vec<Extracted>,
) -> Result<(), String> {
    let mut bytes = Vec::new();
    value
        .to_writer(&mut bytes)
        .map_err(|e| format!("serialize {}: {}", name, e))?;
    write_blob_bytes(dir, name, &bytes, out)
}

/// An [Engine] with no matcher state, used to blank out a field of a rule.
fn empty_engine() -> Engine {
    Engine::Token(TokenEngine {
        composition: Composition {
            parts: Vec::new(),
            id_to_idx: Vec::new(),
            optional_count: 0,
            start_keys: None,
            required_key: None,
        },
        antipatterns: Vec::new(),
    })
}

fn empty_model() -> crate::tokenizer::chunk::Model {
    crate::tokenizer::chunk::Model {
        outcome_labels: Vec::new(),
        outcomes: Vec::new(),
        parameters: Vec::new(),
        pmap: Pmap::default(),
    }
}

/// Which part of the chunker to blank.
#[derive(Clone, Copy)]
enum ChunkerBlank {
    TokenModel,
    PosModel,
    ChunkModel,
    PosParameters,
    PosOutcomes,
    PosPmap,
    PosTagdict,
    ChunkParameters,
    ChunkOutcomes,
    ChunkPmap,
}

fn blanked_chunker(chunker: &Chunker, blank: ChunkerBlank) -> Chunker {
    let mut out = chunker.clone();
    match blank {
        ChunkerBlank::TokenModel => {
            out.token_model.model = empty_model();
        }
        ChunkerBlank::PosModel => {
            out.pos_model.model = empty_model();
            out.pos_model.tagdict = TagDict::default();
        }
        ChunkerBlank::ChunkModel => {
            out.chunk_model.model = empty_model();
        }
        ChunkerBlank::PosParameters => out.pos_model.model.parameters = Vec::new(),
        ChunkerBlank::PosOutcomes => out.pos_model.model.outcomes = Vec::new(),
        ChunkerBlank::PosPmap => out.pos_model.model.pmap = Pmap::default(),
        ChunkerBlank::PosTagdict => out.pos_model.tagdict = TagDict::default(),
        ChunkerBlank::ChunkParameters => out.chunk_model.model.parameters = Vec::new(),
        ChunkerBlank::ChunkOutcomes => out.chunk_model.model.outcomes = Vec::new(),
        ChunkerBlank::ChunkPmap => out.chunk_model.model.pmap = Pmap::default(),
    }
    out
}

/// Which field of a disambiguation rule to blank.
#[derive(Clone, Copy)]
enum DisambigBlank {
    Examples,
    Filter,
    Unification,
    Disambiguations,
    Engine,
    TextEngine,
    TokenEngine,
}

fn blanked_disambiguation_rules(
    rules: &[DisambiguationRule],
    blank: DisambigBlank,
) -> Vec<DisambiguationRule> {
    let mut out = rules.to_vec();
    for rule in out.iter_mut() {
        match blank {
            DisambigBlank::Examples => rule.examples.clear(),
            DisambigBlank::Filter => rule.filter = None,
            DisambigBlank::Unification => rule.unification = None,
            DisambigBlank::Disambiguations => rule.disambiguations = Disambiguation::Nop,
            DisambigBlank::Engine => rule.engine = empty_engine(),
            DisambigBlank::TextEngine => {
                if matches!(rule.engine, Engine::Text(..)) {
                    rule.engine = empty_engine();
                }
            }
            DisambigBlank::TokenEngine => {
                if matches!(rule.engine, Engine::Token(..)) {
                    rule.engine = empty_engine();
                }
            }
        }
    }
    out
}

/// Which field of a grammar rule to blank.
///
/// Each variant removes exactly one serialized part of [Rule] (or of its
/// [Engine]) so the probe can measure that part's serialized size and loaded
/// cost. See `tests/memory_breakdown.rs` for the reporting side.
#[derive(Clone, Copy)]
pub enum GrammarBlank {
    Id,
    Examples,
    Message,
    Suggesters,
    SuggesterParts,
    Engine,
    Unification,
    Filter,
    /// `url`, `short`, `name`, `category_name`, `category_type`.
    Strings,
    // --- inside the engine ---
    /// `TokenEngine.antipatterns` (whole compositions).
    Antipatterns,
    /// `Composition.parts` (the `PartId` references), main and antipattern.
    Parts,
    /// The shared `Rules.parts` pool (the distinct parts themselves).
    PartPool,
    /// `Composition.id_to_idx` maps.
    IdToIdx,
    /// The `Box<Regex>` of a text engine.
    TextRegex,
    /// The `id_to_idx` map of a text engine.
    TextIdToIdx,
    /// Every `Part.atom` replaced by `FalseAtom` (the atom payload alone).
    Atoms,
    /// Every `Quantifier` zeroed (the quantifier payload alone).
    Quantifiers,
    /// Every regex pattern string reachable from the engine and synthesizers.
    Regexes,
    /// Every literal `Matcher` string / graph id.
    MatcherStrings,
    /// Every `TextMatcher.set` (the word-id sets).
    Sets,
}

fn empty_synthesizer() -> Synthesizer {
    Synthesizer {
        use_titlecase_adjust: false,
        parts: Vec::new(),
        suppress_misspelled: false,
    }
}

/// Which part of an [Atom] tree a recursive walk should blank.
#[derive(Clone, Copy)]
enum AtomBlank {
    Regexes,
    MatcherStrings,
    Sets,
}

fn blank_matcher(matcher: &mut Matcher, blank: AtomBlank) {
    match blank {
        AtomBlank::Regexes => {
            if let MatcherKind::Regex(regex) = &mut matcher.matcher {
                **regex = Regex::new(String::new());
            }
        }
        AtomBlank::MatcherStrings => match &mut matcher.matcher {
            MatcherKind::Text(string) => *string = "".into(),
            MatcherKind::Graph(id) => *id = GraphId(0),
            MatcherKind::AffixedGraph(graph) => {
                graph.id = GraphId(0);
                graph.prefix = "".into();
                graph.suffix = "".into();
            }
            MatcherKind::Synthesis(..) | MatcherKind::Regex(_) => {}
        },
        AtomBlank::Sets => {}
    }
}

fn blank_text_matcher(matcher: &mut TextMatcher, blank: AtomBlank) {
    blank_matcher(&mut matcher.matcher, blank);
    if matches!(blank, AtomBlank::Sets) {
        matcher.set = None;
    }
}

fn walk_atom(atom: &mut Atom, blank: AtomBlank) {
    match atom {
        Atom::ChunkAtom(chunk) => blank_matcher(&mut chunk.matcher, blank),
        Atom::TextAtom(text) => blank_text_matcher(&mut text.matcher, blank),
        Atom::WordDataAtom(data) => {
            if let Some(inflect) = data.matcher.inflect_matcher.as_mut() {
                blank_text_matcher(inflect, blank);
            }
        }
        Atom::AndAtom(and) => and.atoms.iter_mut().for_each(|x| walk_atom(x, blank)),
        Atom::OrAtom(or) => or.atoms.iter_mut().for_each(|x| walk_atom(x, blank)),
        Atom::NotAtom(not) => walk_atom(&mut not.atom, blank),
        Atom::OffsetAtom(offset) => walk_atom(&mut offset.atom, blank),
        _ => {}
    }
}

/// Runs `f` over every part of a rule set's shared part pool.
fn for_each_pool_part(pool: &mut PartPool, mut f: impl FnMut(&mut Part)) {
    for part in pool.parts_mut() {
        f(part);
    }
}

fn blank_synth_regexes(synthesizer: &mut Synthesizer) {
    for part in synthesizer.parts.iter_mut() {
        if let SynthesizerPart::Match(match_) = part {
            if let Some(replacer) = match_.pos_replacer.as_mut() {
                if let Some(regex) = replacer.regex.as_mut() {
                    *regex = Regex::new(String::new());
                }
            }
            if let Some((regex, _)) = match_.regex_replacer.as_mut() {
                *regex = Regex::new(String::new());
            }
        }
    }
}

fn blank_rule(rule: &mut Rule, blank: GrammarBlank, pool: &mut PartPool) {
    match blank {
        GrammarBlank::Id => rule.id = Default::default(),
        GrammarBlank::Examples => rule.examples = Default::default(),
        GrammarBlank::Message => rule.message = empty_synthesizer(),
        GrammarBlank::Suggesters => rule.suggesters.clear(),
        GrammarBlank::SuggesterParts => {
            for suggester in rule.suggesters.iter_mut() {
                suggester.parts.clear();
            }
        }
        GrammarBlank::Engine => rule.engine = empty_engine(),
        GrammarBlank::Unification => rule.unification = None,
        GrammarBlank::Filter => rule.filter = None,
        GrammarBlank::Strings => {
            rule.url = None;
            rule.short = None;
            rule.name.clear();
            rule.category_name.clear();
            rule.category_type = None;
        }
        GrammarBlank::Antipatterns => {
            if let Engine::Token(token) = &mut rule.engine {
                token.antipatterns.clear();
            }
        }
        GrammarBlank::Parts => {
            if let Engine::Token(token) = &mut rule.engine {
                token.composition.parts.clear();
            }
        }
        GrammarBlank::PartPool => {
            *pool = PartPool::default();
        }
        GrammarBlank::IdToIdx => {
            if let Engine::Token(token) = &mut rule.engine {
                token.composition.id_to_idx.clear();
            }
        }
        GrammarBlank::TextRegex => {
            if let Engine::Text(regex, _) = &mut rule.engine {
                **regex = Regex::new(String::new());
            }
        }
        GrammarBlank::TextIdToIdx => {
            if let Engine::Text(_, id_to_idx) = &mut rule.engine {
                id_to_idx.clear();
            }
        }
        GrammarBlank::Atoms => {
            for_each_pool_part(pool, |part| {
                part.atom = Atom::FalseAtom(Default::default());
            });
        }
        GrammarBlank::Quantifiers => {
            for_each_pool_part(pool, |part| {
                part.quantifier = Quantifier { min: 0, max: 0 };
            });
        }
        GrammarBlank::Regexes => {
            if let Engine::Text(regex, _) = &mut rule.engine {
                **regex = Regex::new(String::new());
            }
            for_each_pool_part(pool, |part| {
                walk_atom(&mut part.atom, AtomBlank::Regexes);
            });
            for suggester in rule.suggesters.iter_mut() {
                blank_synth_regexes(suggester);
            }
            blank_synth_regexes(&mut rule.message);
        }
        GrammarBlank::MatcherStrings => {
            for_each_pool_part(pool, |part| {
                walk_atom(&mut part.atom, AtomBlank::MatcherStrings);
            });
        }
        GrammarBlank::Sets => {
            for_each_pool_part(pool, |part| {
                walk_atom(&mut part.atom, AtomBlank::Sets);
            });
        }
    }
}

/// The blanked variants used by [extract_rules] to attribute the grammar-rule
/// load to individual fields. The name is the component name in the probe.
pub const GRAMMAR_FIELD_BLANKS: &[(&str, GrammarBlank)] = &[
    ("rules_no_gr_id", GrammarBlank::Id),
    ("rules_no_gr_examples", GrammarBlank::Examples),
    ("rules_no_gr_message", GrammarBlank::Message),
    ("rules_no_gr_suggesters", GrammarBlank::Suggesters),
    ("rules_no_gr_suggester_parts", GrammarBlank::SuggesterParts),
    ("rules_no_gr_engine", GrammarBlank::Engine),
    ("rules_no_gr_unification", GrammarBlank::Unification),
    ("rules_no_gr_filter", GrammarBlank::Filter),
    ("rules_no_gr_strings", GrammarBlank::Strings),
    ("rules_no_gr_antipatterns", GrammarBlank::Antipatterns),
    ("rules_no_gr_parts", GrammarBlank::Parts),
    ("rules_no_gr_part_pool", GrammarBlank::PartPool),
    ("rules_no_gr_id_to_idx", GrammarBlank::IdToIdx),
    ("rules_no_gr_text_regex", GrammarBlank::TextRegex),
    ("rules_no_gr_text_id_to_idx", GrammarBlank::TextIdToIdx),
    ("rules_no_gr_atoms", GrammarBlank::Atoms),
    ("rules_no_gr_quantifiers", GrammarBlank::Quantifiers),
    ("rules_no_gr_regexes", GrammarBlank::Regexes),
    ("rules_no_gr_matcher_strings", GrammarBlank::MatcherStrings),
    ("rules_no_gr_sets", GrammarBlank::Sets),
];

fn blank_antipattern_pool(pool: &mut PartPool, blank: GrammarBlank) {
    if matches!(blank, GrammarBlank::Engine | GrammarBlank::Antipatterns) {
        pool.antipatterns_mut().clear();
    } else {
        for antipattern in pool.antipatterns_mut() {
            match blank {
                GrammarBlank::Parts => antipattern.parts.clear(),
                GrammarBlank::IdToIdx => antipattern.id_to_idx.clear(),
                _ => {}
            }
        }
    }
}

fn blanked_grammar_rules(
    rules: &[Rule],
    parts: &PartPool,
    blank: GrammarBlank,
) -> (Vec<Rule>, PartPool) {
    let mut out = rules.to_vec();
    let mut pool = parts.clone();
    blank_antipattern_pool(&mut pool, blank);
    for rule in out.iter_mut() {
        blank_rule(rule, blank, &mut pool);
    }
    (out, pool)
}

/// Deserializes `tokenizer_bin` once and writes each tokenizer component to `out_dir`.
pub fn extract_tokenizer(tokenizer_bin: &Path, out_dir: &Path) -> Result<Vec<Extracted>, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{}: {}", out_dir.display(), e))?;
    let tokenizer: Tokenizer = load_tokenizer(tokenizer_bin)?;
    let mut out = Vec::new();

    write_tokenizer_blob(out_dir, "tokenizer_full", &tokenizer, &mut out)?;

    // Partition variants: same tokenizer with one component removed. The loaded cost of that
    // component in context is `tokenizer_full - tokenizer_no_<component>`.
    {
        let mut no_multiword = tokenizer.clone();
        no_multiword.multiword_tagger = None;
        write_tokenizer_blob(out_dir, "tokenizer_no_multiword", &no_multiword, &mut out)?;

        let mut no_chunker = tokenizer.clone();
        no_chunker.chunker = None;
        write_tokenizer_blob(out_dir, "tokenizer_no_chunker", &no_chunker, &mut out)?;

        let mut no_disambiguation = tokenizer.clone();
        no_disambiguation.rules = Vec::new();
        no_disambiguation.parts = PartPool::default();
        write_tokenizer_blob(
            out_dir,
            "tokenizer_no_disambiguation",
            &no_disambiguation,
            &mut out,
        )?;

        let mut no_sentencizer = tokenizer.clone();
        no_sentencizer.sentencizer = srx::Rules::default();
        write_tokenizer_blob(
            out_dir,
            "tokenizer_no_sentencizer",
            &no_sentencizer,
            &mut out,
        )?;
    }

    // In-context sub-partition of the chunker: full minus one sub-model or model field.
    if let Some(chunker) = &tokenizer.chunker {
        for (name, blank) in [
            ("tokenizer_no_chunker_token_model", ChunkerBlank::TokenModel),
            ("tokenizer_no_chunker_pos_model", ChunkerBlank::PosModel),
            ("tokenizer_no_chunker_chunk_model", ChunkerBlank::ChunkModel),
            ("tokenizer_no_pos_parameters", ChunkerBlank::PosParameters),
            ("tokenizer_no_pos_outcomes", ChunkerBlank::PosOutcomes),
            ("tokenizer_no_pos_pmap", ChunkerBlank::PosPmap),
            ("tokenizer_no_pos_tagdict", ChunkerBlank::PosTagdict),
            (
                "tokenizer_no_chunk_parameters",
                ChunkerBlank::ChunkParameters,
            ),
            ("tokenizer_no_chunk_outcomes", ChunkerBlank::ChunkOutcomes),
            ("tokenizer_no_chunk_pmap", ChunkerBlank::ChunkPmap),
        ] {
            let mut variant = tokenizer.clone();
            variant.chunker = Some(blanked_chunker(chunker, blank));
            write_tokenizer_blob(out_dir, name, &variant, &mut out)?;
        }
    }

    let tagger = &*tokenizer.tagger;
    write_blob(out_dir, "tagger", tagger, &mut out)?;
    write_blob(
        out_dir,
        "tagger_word_store_fst",
        &tagger.word_store_fst.as_fst().as_bytes().to_vec(),
        &mut out,
    )?;
    write_blob(
        out_dir,
        "tagger_tag_fst",
        &tagger.tag_fst.as_fst().as_bytes().to_vec(),
        &mut out,
    )?;
    write_blob(
        out_dir,
        "tagger_synth_fst",
        &tagger.synth_fst.as_fst().as_bytes().to_vec(),
        &mut out,
    )?;
    write_blob(out_dir, "tagger_tag_store", &tagger.tag_store, &mut out)?;
    write_blob(out_dir, "tagger_word_buf", &tagger.word_buf, &mut out)?;
    write_blob(
        out_dir,
        "tagger_word_offsets",
        &tagger.word_offsets,
        &mut out,
    )?;

    write_blob(
        out_dir,
        "disambiguation_rules",
        &(&tokenizer.rules, &tokenizer.parts),
        &mut out,
    )?;
    for (name, blank) in [
        ("disambig_no_examples", DisambigBlank::Examples),
        ("disambig_no_filter", DisambigBlank::Filter),
        ("disambig_no_unification", DisambigBlank::Unification),
        (
            "disambig_no_disambiguations",
            DisambigBlank::Disambiguations,
        ),
        ("disambig_no_engine", DisambigBlank::Engine),
        ("disambig_no_text_engine", DisambigBlank::TextEngine),
        ("disambig_no_token_engine", DisambigBlank::TokenEngine),
    ] {
        write_blob(
            out_dir,
            name,
            &(
                blanked_disambiguation_rules(&tokenizer.rules, blank),
                &tokenizer.parts,
            ),
            &mut out,
        )?;
    }

    write_blob(out_dir, "chunker", &tokenizer.chunker, &mut out)?;
    if let Some(chunker) = &tokenizer.chunker {
        write_blob(
            out_dir,
            "chunker_token_model",
            &chunker.token_model,
            &mut out,
        )?;
        write_blob(out_dir, "chunker_pos_model", &chunker.pos_model, &mut out)?;
        write_blob(
            out_dir,
            "chunker_pos_tagdict",
            &chunker.pos_model.tagdict,
            &mut out,
        )?;
        write_blob(
            out_dir,
            "chunker_chunk_model",
            &chunker.chunk_model,
            &mut out,
        )?;

        let model = &chunker.chunk_model.model;
        write_blob(
            out_dir,
            "chunker_model_parameters",
            &model.parameters,
            &mut out,
        )?;
        write_blob(out_dir, "chunker_model_outcomes", &model.outcomes, &mut out)?;
        write_blob(out_dir, "chunker_model_pmap", &model.pmap, &mut out)?;
        write_blob(
            out_dir,
            "chunker_model_outcome_labels",
            &model.outcome_labels,
            &mut out,
        )?;
        // The same fields of the two tagger models for comparison.
        write_blob(
            out_dir,
            "chunker_token_model_parameters",
            &chunker.token_model.model.parameters,
            &mut out,
        )?;
        write_blob(
            out_dir,
            "chunker_token_model_pmap",
            &chunker.token_model.model.pmap,
            &mut out,
        )?;
        write_blob(
            out_dir,
            "chunker_pos_model_parameters",
            &chunker.pos_model.model.parameters,
            &mut out,
        )?;
        write_blob(
            out_dir,
            "chunker_pos_model_pmap",
            &chunker.pos_model.model.pmap,
            &mut out,
        )?;
    }

    write_blob(
        out_dir,
        "multiword_tagger",
        &tokenizer.multiword_tagger,
        &mut out,
    )?;
    write_blob(out_dir, "sentencizer", &tokenizer.sentencizer, &mut out)?;

    Ok(out)
}

/// Deserializes `rules_bin` once and writes each rules component to `out_dir`.
pub fn extract_rules(rules_bin: &Path, out_dir: &Path) -> Result<Vec<Extracted>, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{}: {}", out_dir.display(), e))?;
    let rules: Rules = load_rules(rules_bin)?;
    let mut out = Vec::new();

    write_rules_blob(out_dir, "rules_full", &rules, &mut out)?;

    // Partition variants: same rules with one component removed.
    {
        let mut no_grammar = rules.clone();
        no_grammar.rules = Vec::new();
        write_rules_blob(out_dir, "rules_no_grammar", &no_grammar, &mut out)?;

        let mut no_spell = rules.clone();
        no_spell.spell = Spell::default();
        write_rules_blob(out_dir, "rules_no_spell", &no_spell, &mut out)?;

        let mut no_articles = rules.clone();
        no_articles.articles = None;
        write_rules_blob(out_dir, "rules_no_articles", &no_articles, &mut out)?;
    }

    write_blob(
        out_dir,
        "grammar_rules",
        &(&rules.rules, &rules.parts),
        &mut out,
    )?;
    for (name, blank) in [
        ("grammar_no_examples", GrammarBlank::Examples),
        ("grammar_no_suggesters", GrammarBlank::Suggesters),
        ("grammar_no_engine", GrammarBlank::Engine),
    ] {
        write_blob(
            out_dir,
            name,
            &blanked_grammar_rules(&rules.rules, &rules.parts, blank),
            &mut out,
        )?;
    }

    // The same blanked variants, but inside the full `Rules` so their loaded cost can be
    // attributed in context (`rules_full` minus the variant).
    for (name, blank) in GRAMMAR_FIELD_BLANKS {
        let mut variant = rules.clone();
        let pool = &mut variant.parts;
        blank_antipattern_pool(pool, *blank);
        for rule in variant.rules.iter_mut() {
            blank_rule(rule, *blank, pool);
        }
        write_rules_blob(out_dir, name, &variant, &mut out)?;
    }

    write_blob(out_dir, "spell", &rules.spell, &mut out)?;
    // Same bytes as `spell`; the test enables the en_US variant after loading.
    write_blob(out_dir, "spell_en_us", &rules.spell, &mut out)?;

    Ok(out)
}

/// Convenience: extract both binaries into one directory.
pub fn extract_all(
    tokenizer_bin: &Path,
    rules_bin: &Path,
    out_dir: &Path,
) -> Result<Vec<Extracted>, String> {
    let mut all = extract_tokenizer(tokenizer_bin, out_dir)?;
    all.extend(extract_rules(rules_bin, out_dir)?);
    Ok(all)
}

/// Deserializes a single component (identified by the name used in [extract_all]) from `path`.
pub fn load(name: &str, path: &Path) -> Result<Held, String> {
    let held = match name {
        "tokenizer_full" => Held::new(load_tokenizer(path)?),
        "tokenizer_no_multiword"
        | "tokenizer_no_chunker"
        | "tokenizer_no_disambiguation"
        | "tokenizer_no_sentencizer"
        | "tokenizer_no_chunker_token_model"
        | "tokenizer_no_chunker_pos_model"
        | "tokenizer_no_chunker_chunk_model"
        | "tokenizer_no_pos_parameters"
        | "tokenizer_no_pos_outcomes"
        | "tokenizer_no_pos_pmap"
        | "tokenizer_no_pos_tagdict"
        | "tokenizer_no_chunk_parameters"
        | "tokenizer_no_chunk_outcomes"
        | "tokenizer_no_chunk_pmap" => Held::new(load_tokenizer(path)?),
        "tagger" => Held::new(load_typed::<crate::tokenizer::tag::Tagger>(path)?),
        "tagger_word_store_fst" | "tagger_tag_fst" | "tagger_synth_fst" => {
            Held::new(load_typed::<Vec<u8>>(path)?)
        }
        "tagger_tag_store" => Held::new(load_typed::<bimap::BiMap<String, PosIdInt>>(path)?),
        "tagger_word_buf" => Held::new(load_typed::<String>(path)?),
        "tagger_word_offsets" => Held::new(load_typed::<Vec<u32>>(path)?),
        "disambiguation_rules"
        | "disambig_no_examples"
        | "disambig_no_filter"
        | "disambig_no_unification"
        | "disambig_no_disambiguations"
        | "disambig_no_engine"
        | "disambig_no_text_engine"
        | "disambig_no_token_engine" => {
            Held::new(load_typed::<(Vec<DisambiguationRule>, PartPool)>(path)?)
        }
        "chunker" => Held::new(load_typed::<Option<Chunker>>(path)?),
        "chunker_token_model" => Held::new(load_typed::<MaxentTokenizer>(path)?),
        "chunker_pos_model" => Held::new(load_typed::<MaxentPosTagger>(path)?),
        "chunker_pos_tagdict" => Held::new(load_typed::<TagDict>(path)?),
        "chunker_chunk_model" => Held::new(load_typed::<MaxentChunker>(path)?),
        "chunker_model_parameters"
        | "chunker_token_model_parameters"
        | "chunker_pos_model_parameters" => Held::new(load_typed::<Vec<f32>>(path)?),
        "chunker_model_outcomes" => Held::new(load_typed::<Vec<u8>>(path)?),
        "chunker_model_pmap" | "chunker_token_model_pmap" | "chunker_pos_model_pmap" => {
            Held::new(load_typed::<Pmap>(path)?)
        }
        "chunker_model_outcome_labels" => Held::new(load_typed::<Vec<String>>(path)?),
        "multiword_tagger" => Held::new(load_typed::<Option<MultiwordTagger>>(path)?),
        "sentencizer" => Held::new(load_typed::<srx::Rules>(path)?),
        "rules_full" => Held::new(load_rules(path)?),
        "rules_no_grammar" | "rules_no_spell" | "rules_no_articles" => Held::new(load_rules(path)?),
        name if name.starts_with("rules_no_gr_") => Held::new(load_rules(path)?),
        "grammar_rules" | "grammar_no_examples" | "grammar_no_suggesters" | "grammar_no_engine" => {
            Held::new(load_typed::<(Vec<Rule>, PartPool)>(path)?)
        }
        "spell" | "spell_en_us" => Held::new(load_typed::<Spell>(path)?),
        other => return Err(format!("unknown component {:?}", other)),
    };
    Ok(held)
}

/// Enables the spellchecker variant on a [Spell] handle (built by [load] with `spell_en_us`).
pub fn spell_set_variant(held: &mut Held, variant: &str) -> Result<(), String> {
    let spell = held
        .0
        .downcast_mut::<Spell>()
        .ok_or_else(|| "held value is not a Spell".to_string())?;
    let variant = spell.variant(variant).map_err(|e| e.to_string())?;
    spell.options_mut().variant = Some(variant);
    Ok(())
}

/// Structural counts over the grammar rules, printed by [describe].
#[derive(Default)]
struct GrammarStats {
    compositions: u64,
    main_parts: u64,
    antipattern_parts: u64,
    antipatterns: u64,
    id_to_idx: u64,
    chunk: u64,
    text: u64,
    word: u64,
    space: u64,
    and: u64,
    or: u64,
    not: u64,
    offset: u64,
    true_atom: u64,
    false_atom: u64,
    regexes: u64,
    matcher_strings: u64,
    sets: u64,
    set_entries: u64,
    examples: u64,
    main_bytes: u64,
    main_unique_bytes: u64,
    anti_bytes: u64,
    anti_unique_bytes: u64,
    regex_total: u64,
    regex_unique: u64,
    matcher_total: u64,
    matcher_unique: u64,
    regex_seen: std::collections::HashSet<String>,
    matcher_seen: std::collections::HashSet<String>,
    rules_with_articles: u64,
    article_bytes: u64,
    unique_part_seen: std::collections::HashSet<u64>,
    unique_atom_seen: std::collections::HashSet<u64>,
    unique_part_bytes: u64,
    unique_atom_bytes: u64,
    unique_parts: u64,
    unique_atoms: u64,
    part_instances: u64,
    atom_instances: u64,
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

impl GrammarStats {
    fn count_matcher(&mut self, matcher: &Matcher) {
        match &matcher.matcher {
            MatcherKind::Text(string) => {
                self.matcher_strings += 1;
                self.matcher_total += string.len() as u64;
                if self.matcher_seen.insert(string.to_string()) {
                    self.matcher_unique += string.len() as u64;
                }
            }
            MatcherKind::Graph(_) | MatcherKind::AffixedGraph(_) | MatcherKind::Synthesis(..) => {}
            MatcherKind::Regex(regex) => {
                self.regexes += 1;
                self.count_regex(regex);
            }
        }
    }

    fn count_regex(&mut self, regex: &Regex) {
        let string = regex.as_str();
        self.regex_total += string.len() as u64;
        if self.regex_seen.insert(string.to_string()) {
            self.regex_unique += string.len() as u64;
        }
    }

    fn count_text_matcher(&mut self, matcher: &TextMatcher) {
        self.count_matcher(&matcher.matcher);
        if let Some(set) = &matcher.set {
            self.sets += 1;
            self.set_entries += set.len() as u64;
        }
    }

    fn count_synth(&mut self, synthesizer: &Synthesizer) {
        for part in &synthesizer.parts {
            if let SynthesizerPart::Match(match_) = part {
                if let Some(regex) = match_.pos_replacer.as_ref().and_then(|x| x.regex.as_ref()) {
                    self.regexes += 1;
                    self.count_regex(regex);
                }
                if let Some((regex, _)) = match_.regex_replacer.as_ref() {
                    self.regexes += 1;
                    self.count_regex(regex);
                }
            }
        }
    }

    fn count_part(&mut self, part: &Part) {
        self.part_instances += 1;
        let bytes = bincode::serialize(part).expect("part serializes");
        if self.unique_part_seen.insert(hash_bytes(&bytes)) {
            self.unique_part_bytes += bytes.len() as u64;
            self.unique_parts += 1;
        }

        self.atom_instances += 1;
        let abytes = bincode::serialize(&part.atom).expect("atom serializes");
        if self.unique_atom_seen.insert(hash_bytes(&abytes)) {
            self.unique_atom_bytes += abytes.len() as u64;
            self.unique_atoms += 1;
        }
    }

    fn count_atom(&mut self, atom: &Atom) {
        match atom {
            Atom::ChunkAtom(chunk) => {
                self.chunk += 1;
                self.count_matcher(&chunk.matcher);
            }
            Atom::SpaceBeforeAtom(_) => self.space += 1,
            Atom::TextAtom(text) => {
                self.text += 1;
                self.count_text_matcher(&text.matcher);
            }
            Atom::WordDataAtom(data) => {
                self.word += 1;
                if let Some(inflect) = data.matcher.inflect_matcher.as_ref() {
                    self.count_text_matcher(inflect);
                }
            }
            Atom::TrueAtom(_) => self.true_atom += 1,
            Atom::FalseAtom(_) => self.false_atom += 1,
            Atom::AndAtom(and) => {
                self.and += 1;
                for atom in &and.atoms {
                    self.count_atom(atom);
                }
            }
            Atom::OrAtom(or) => {
                self.or += 1;
                for atom in &or.atoms {
                    self.count_atom(atom);
                }
            }
            Atom::NotAtom(not) => {
                self.not += 1;
                self.count_atom(&not.atom);
            }
            Atom::OffsetAtom(offset) => {
                self.offset += 1;
                self.count_atom(&offset.atom);
            }
        }
    }
}

/// Structural facts (counts, on-disk vs loaded element sizes) that explain the measurements.
pub fn describe(tokenizer_bin: &Path, rules_bin: &Path) -> Result<Vec<(String, String)>, String> {
    let tokenizer: Tokenizer = load_tokenizer(tokenizer_bin)?;
    let rules: Rules = load_rules(rules_bin)?;
    let mut facts: Vec<(String, String)> = Vec::new();

    let tagger = &*tokenizer.tagger;
    facts.push((
        "tagger.words".into(),
        tagger.word_offsets.len().saturating_sub(1).to_string(),
    ));
    facts.push((
        "tagger.word_buf bytes".into(),
        tagger.word_buf.len().to_string(),
    ));
    facts.push((
        "tagger.tag_store entries".into(),
        tagger.tag_store.len().to_string(),
    ));

    let text_engines = tokenizer
        .rules
        .iter()
        .filter(|rule| matches!(rule.engine, Engine::Text(..)))
        .count();
    facts.push((
        "disambiguation rules (text-engine rules)".into(),
        format!("{} ({})", tokenizer.rules.len(), text_engines),
    ));

    if let Some(chunker) = &tokenizer.chunker {
        let models = [
            ("token", &chunker.token_model.model),
            ("pos", &chunker.pos_model.model),
            ("chunk", &chunker.chunk_model.model),
        ];
        for (name, model) in models {
            facts.push((
                format!("chunker.{}.parameters (loaded f32 bytes)", name),
                format!("{}", model.parameters.len() * 4),
            ));
            facts.push((
                format!("chunker.{}.outcomes (loaded u8 bytes)", name),
                format!("{}", model.outcomes.len()),
            ));
            facts.push((
                format!("chunker.{}.pmap entries (disk bytes)", name),
                format!("{} ({})", model.pmap.len(), model.pmap.len() * 9),
            ));
            facts.push((
                format!("chunker.{}.outcome_labels", name),
                model.outcome_labels.len().to_string(),
            ));
        }
        facts.push((
            "chunker.pos.tagdict entries".into(),
            chunker.pos_model.tagdict.len().to_string(),
        ));
    }

    facts.push(("grammar rules".into(), rules.rules.len().to_string()));

    let mut stats = GrammarStats::default();
    stats.article_bytes = rules
        .articles
        .as_ref()
        .map(|articles| {
            bincode::serialize(articles)
                .expect("articles serialize")
                .len() as u64
        })
        .unwrap_or(0);
    // Do identical compositions repeat across rules (e.g. a rulegroup's shared antipattern cloned
    // into every rule)? A byte hash of each composition answers whether interning would pay off.
    let mut seen_compositions: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let hash_composition = |composition: &Composition,
                            seen: &mut std::collections::HashSet<u64>| {
        use std::hash::{Hash, Hasher};
        let bytes = bincode::serialize(composition).expect("composition serializes");
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        let unique = seen.insert(hasher.finish());
        (bytes.len() as u64, unique)
    };
    for rule in &rules.rules {
        match &rule.engine {
            Engine::Token(engine) => {
                stats.compositions += 1;
                stats.main_parts += engine.composition.parts.len() as u64;
                stats.id_to_idx += engine.composition.id_to_idx.len() as u64;
                for id in &engine.composition.parts {
                    let part = rules.parts.get(*id);
                    stats.count_atom(&part.atom);
                    stats.count_part(part);
                }
                let (bytes, unique) = hash_composition(&engine.composition, &mut seen_compositions);
                stats.main_bytes += bytes;
                if unique {
                    stats.main_unique_bytes += bytes;
                }
                stats.antipatterns += engine.antipatterns.len() as u64;
                for antipattern in &engine.antipatterns {
                    let antipattern = rules.parts.antipattern(*antipattern);
                    stats.compositions += 1;
                    stats.antipattern_parts += antipattern.parts.len() as u64;
                    stats.id_to_idx += antipattern.id_to_idx.len() as u64;
                    for id in &antipattern.parts {
                        let part = rules.parts.get(*id);
                        stats.count_atom(&part.atom);
                        stats.count_part(part);
                    }
                    let (bytes, unique) = hash_composition(antipattern, &mut seen_compositions);
                    stats.anti_bytes += bytes;
                    if unique {
                        stats.anti_unique_bytes += bytes;
                    }
                }
            }
            Engine::Text(_, _) => {}
        }
        stats.examples += rule.examples.get().len() as u64;
        if rule.uses_articles {
            stats.rules_with_articles += 1;
        }
        for suggester in &rule.suggesters {
            stats.count_synth(suggester);
        }
        stats.count_synth(&rule.message);
    }
    for (key, value) in [
        ("grammar token compositions", stats.compositions),
        (
            "grammar rules with later required literal",
            rules.rules.iter().filter(|rule| {
                matches!(&rule.engine, Engine::Token(engine) if engine.composition.required_key.is_some())
            }).count() as u64,
        ),
        ("grammar composition parts (main)", stats.main_parts),
        (
            "grammar composition parts (antipattern)",
            stats.antipattern_parts,
        ),
        ("grammar antipattern compositions", stats.antipatterns),
        (
            "grammar stored antipattern compositions",
            rules.parts.antipatterns().len() as u64,
        ),
        ("grammar id_to_idx entries", stats.id_to_idx),
        ("grammar atom ChunkAtom", stats.chunk),
        ("grammar atom TextAtom", stats.text),
        ("grammar atom WordDataAtom", stats.word),
        ("grammar atom SpaceBeforeAtom", stats.space),
        (
            "grammar atom And/Or/Not/Offset",
            stats.and + stats.or + stats.not + stats.offset,
        ),
        (
            "grammar atom True/False",
            stats.true_atom + stats.false_atom,
        ),
        ("grammar regexes (atoms+suggesters)", stats.regexes),
        ("grammar matcher strings", stats.matcher_strings),
        ("grammar text-matcher sets", stats.sets),
        ("grammar text-matcher set entries", stats.set_entries),
        ("grammar examples", stats.examples),
    ] {
        facts.push((key.into(), value.to_string()));
    }
    facts.push((
        "grammar main composition bytes (total/unique)".into(),
        format!("{}/{}", stats.main_bytes, stats.main_unique_bytes),
    ));
    facts.push((
        "grammar antipattern bytes (total/unique)".into(),
        format!("{}/{}", stats.anti_bytes, stats.anti_unique_bytes),
    ));
    facts.push((
        "grammar parts (instances/unique bytes)".into(),
        format!("{}/{}", stats.part_instances, stats.unique_part_bytes),
    ));
    facts.push((
        "grammar unique parts / unique atoms".into(),
        format!("{}/{}", stats.unique_parts, stats.unique_atoms),
    ));
    facts.push((
        "grammar atoms (instances/unique bytes)".into(),
        format!("{}/{}", stats.atom_instances, stats.unique_atom_bytes),
    ));
    facts.push((
        "grammar rules using articles".into(),
        stats.rules_with_articles.to_string(),
    ));
    facts.push((
        "grammar article data bytes (all copies)".into(),
        stats.article_bytes.to_string(),
    ));
    facts.push((
        "grammar regex string bytes (total/unique)".into(),
        format!("{}/{}", stats.regex_total, stats.regex_unique),
    ));
    facts.push((
        "grammar matcher string bytes (total/unique)".into(),
        format!("{}/{}", stats.matcher_total, stats.matcher_unique),
    ));
    for (key, value) in [
        ("size_of Rule", std::mem::size_of::<Rule>()),
        ("size_of Engine", std::mem::size_of::<Engine>()),
        ("size_of Composition", std::mem::size_of::<Composition>()),
        ("size_of Part", std::mem::size_of::<Part>()),
        ("size_of Atom", std::mem::size_of::<Atom>()),
        ("size_of TextMatcher", std::mem::size_of::<TextMatcher>()),
        ("size_of Matcher", std::mem::size_of::<Matcher>()),
        (
            "size_of WordDataMatcher",
            std::mem::size_of::<WordDataMatcher>(),
        ),
        (
            "size_of Example",
            std::mem::size_of::<crate::rule::grammar::Example>(),
        ),
        (
            "size_of Index",
            std::mem::size_of::<crate::rule::id::Index>(),
        ),
    ] {
        facts.push((key.into(), value.to_string()));
    }
    facts.push(("spell.fst bytes".into(), rules.spell.fst.len().to_string()));
    facts.push((
        "spell.variants".into(),
        rules.spell.variants().len().to_string(),
    ));
    facts.push((
        "spell.multiwords".into(),
        rules.spell.multiwords.len().to_string(),
    ));
    facts.push((
        "spell.multitoken phrases".into(),
        rules.spell.multitoken.len().to_string(),
    ));
    facts.push((
        "spell.map entries".into(),
        rules.spell.map.len().to_string(),
    ));
    facts.push((
        "sentencizer rules".into(),
        tokenizer.sentencizer.len().to_string(),
    ));
    facts.push((
        "size_of nlprule Regex".into(),
        std::mem::size_of::<crate::utils::regex::Regex>().to_string(),
    ));

    Ok(facts)
}
