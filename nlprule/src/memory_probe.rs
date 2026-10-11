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

use half::bf16;
use serde::{de::DeserializeOwned, Serialize};

use crate::rule::disambiguation::Disambiguation;
use crate::rule::engine::composition::Composition;
use crate::rule::engine::{Engine, TokenEngine};
use crate::rule::{DisambiguationRule, Rule};
use crate::spell::Spell;
use crate::tokenizer::chunk::{
    Chunker, MaxentChunker, MaxentPosTagger, MaxentTokenizer, Pmap, TagDict,
};
use crate::tokenizer::multiword::MultiwordTagger;
use crate::types::{DefaultHashMap, PosIdInt};
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

fn write_blob<T: Serialize>(
    dir: &Path,
    name: &str,
    value: &T,
    out: &mut Vec<Extracted>,
) -> Result<(), String> {
    let bytes = bincode::serialize(value).map_err(|e| format!("serialize {}: {}", name, e))?;
    let path = dir.join(format!("{}.bin", name));
    std::fs::write(&path, &bytes).map_err(|e| format!("write {}: {}", path.display(), e))?;
    out.push(Extracted {
        name: name.to_string(),
        bytes: bytes.len() as u64,
        path: path.to_string_lossy().into_owned(),
    });
    Ok(())
}

/// An [Engine] with no matcher state, used to blank out a field of a rule.
fn empty_engine() -> Engine {
    Engine::Token(TokenEngine {
        composition: Composition {
            parts: Vec::new(),
            id_to_idx: DefaultHashMap::new(),
            can_stop_mask: Vec::new(),
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
#[derive(Clone, Copy)]
enum GrammarBlank {
    Examples,
    Suggesters,
    Engine,
}

fn blanked_grammar_rules(rules: &[Rule], blank: GrammarBlank) -> Vec<Rule> {
    let mut out = rules.to_vec();
    for rule in out.iter_mut() {
        match blank {
            GrammarBlank::Examples => rule.examples.clear(),
            GrammarBlank::Suggesters => rule.suggesters.clear(),
            GrammarBlank::Engine => rule.engine = empty_engine(),
        }
    }
    out
}

/// Deserializes `tokenizer_bin` once and writes each tokenizer component to `out_dir`.
pub fn extract_tokenizer(tokenizer_bin: &Path, out_dir: &Path) -> Result<Vec<Extracted>, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{}: {}", out_dir.display(), e))?;
    let tokenizer: Tokenizer = load_typed(tokenizer_bin)?;
    let mut out = Vec::new();

    write_blob(out_dir, "tokenizer_full", &tokenizer, &mut out)?;

    // Partition variants: same tokenizer with one component removed. The loaded cost of that
    // component in context is `tokenizer_full - tokenizer_no_<component>`.
    {
        let mut no_multiword = tokenizer.clone();
        no_multiword.multiword_tagger = None;
        write_blob(out_dir, "tokenizer_no_multiword", &no_multiword, &mut out)?;

        let mut no_chunker = tokenizer.clone();
        no_chunker.chunker = None;
        write_blob(out_dir, "tokenizer_no_chunker", &no_chunker, &mut out)?;

        let mut no_disambiguation = tokenizer.clone();
        no_disambiguation.rules = Vec::new();
        write_blob(
            out_dir,
            "tokenizer_no_disambiguation",
            &no_disambiguation,
            &mut out,
        )?;

        let mut no_sentencizer = tokenizer.clone();
        no_sentencizer.sentencizer = srx::Rules::default();
        write_blob(
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
            write_blob(out_dir, name, &variant, &mut out)?;
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
    write_blob(out_dir, "tagger_tag_store", &tagger.tag_store, &mut out)?;
    write_blob(out_dir, "tagger_word_buf", &tagger.word_buf, &mut out)?;
    write_blob(
        out_dir,
        "tagger_word_offsets",
        &tagger.word_offsets,
        &mut out,
    )?;

    write_blob(out_dir, "disambiguation_rules", &tokenizer.rules, &mut out)?;
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
            &blanked_disambiguation_rules(&tokenizer.rules, blank),
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
    let rules: Rules = load_typed(rules_bin)?;
    let mut out = Vec::new();

    write_blob(out_dir, "rules_full", &rules, &mut out)?;

    // Partition variants: same rules with one component removed.
    {
        let mut no_grammar = rules.clone();
        no_grammar.rules = Vec::new();
        write_blob(out_dir, "rules_no_grammar", &no_grammar, &mut out)?;

        let mut no_spell = rules.clone();
        no_spell.spell = Spell::default();
        write_blob(out_dir, "rules_no_spell", &no_spell, &mut out)?;
    }

    write_blob(out_dir, "grammar_rules", &rules.rules, &mut out)?;
    for (name, blank) in [
        ("grammar_no_examples", GrammarBlank::Examples),
        ("grammar_no_suggesters", GrammarBlank::Suggesters),
        ("grammar_no_engine", GrammarBlank::Engine),
    ] {
        write_blob(
            out_dir,
            name,
            &blanked_grammar_rules(&rules.rules, blank),
            &mut out,
        )?;
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
        "tokenizer_full" => Held::new(load_typed::<Tokenizer>(path)?),
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
        | "tokenizer_no_chunk_pmap" => Held::new(load_typed::<Tokenizer>(path)?),
        "tagger" => Held::new(load_typed::<crate::tokenizer::tag::Tagger>(path)?),
        "tagger_word_store_fst" | "tagger_tag_fst" => Held::new(load_typed::<Vec<u8>>(path)?),
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
        | "disambig_no_token_engine" => Held::new(load_typed::<Vec<DisambiguationRule>>(path)?),
        "chunker" => Held::new(load_typed::<Option<Chunker>>(path)?),
        "chunker_token_model" => Held::new(load_typed::<MaxentTokenizer>(path)?),
        "chunker_pos_model" => Held::new(load_typed::<MaxentPosTagger>(path)?),
        "chunker_pos_tagdict" => Held::new(load_typed::<TagDict>(path)?),
        "chunker_chunk_model" => Held::new(load_typed::<MaxentChunker>(path)?),
        "chunker_model_parameters"
        | "chunker_token_model_parameters"
        | "chunker_pos_model_parameters" => Held::new(load_typed::<Vec<bf16>>(path)?),
        "chunker_model_outcomes" => Held::new(load_typed::<Vec<u8>>(path)?),
        "chunker_model_pmap" | "chunker_token_model_pmap" | "chunker_pos_model_pmap" => {
            Held::new(load_typed::<Pmap>(path)?)
        }
        "chunker_model_outcome_labels" => Held::new(load_typed::<Vec<String>>(path)?),
        "multiword_tagger" => Held::new(load_typed::<Option<MultiwordTagger>>(path)?),
        "sentencizer" => Held::new(load_typed::<srx::Rules>(path)?),
        "rules_full" => Held::new(load_typed::<Rules>(path)?),
        "rules_no_grammar" | "rules_no_spell" => Held::new(load_typed::<Rules>(path)?),
        "grammar_rules" | "grammar_no_examples" | "grammar_no_suggesters" | "grammar_no_engine" => {
            Held::new(load_typed::<Vec<Rule>>(path)?)
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

/// Structural facts (counts, on-disk vs loaded element sizes) that explain the measurements.
pub fn describe(tokenizer_bin: &Path, rules_bin: &Path) -> Result<Vec<(String, String)>, String> {
    let tokenizer: Tokenizer = load_typed(tokenizer_bin)?;
    let rules: Rules = load_typed(rules_bin)?;
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
                format!("chunker.{}.parameters (loaded bf16 bytes)", name),
                format!("{}", model.parameters.len() * 2),
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
