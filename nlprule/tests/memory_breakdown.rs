//! Measures which parts of the loaded English tokenizer and rules use memory.
//!
//! Test-only and ignored by default. Run with a release build and the `memory-probe` feature:
//!
//! ```sh
//! RUST_LOG=WARN cargo run --release --all-features --bin compile -- \
//!     --build-dir data/en --tokenizer-out storage/en_tokenizer.bin --rules-out storage/en_rules.bin
//! cargo test --release --all-features --test memory_breakdown -- --ignored --nocapture
//! ```
//!
//! The probe (`nlprule::memory_probe`, behind the `memory-probe` feature) deserializes each binary
//! once and re-serializes every component (or a blanked copy of it) into `storage/memory_probe/`.
//! To get a clean RSS reading per component, this test re-executes its own test binary once per
//! component (and per repetition) so each measurement starts from a fresh process, then reports
//! the median of `M2B_REPETITIONS` runs. Override the storage directory with `M2B_STORAGE`.
#![cfg(feature = "memory-probe")]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use nlprule::memory_probe;

/// Components that partition the tokenizer (no nesting), in reporting order.
const TOKENIZER_TOP: &[&str] = &[
    "tagger",
    "disambiguation_rules",
    "chunker",
    "multiword_tagger",
    "sentencizer",
];

/// Components that partition the rules (no nesting).
const RULES_TOP: &[&str] = &["grammar_rules", "spell"];

/// All components to measure, in the order they should be reported.
const COMPONENTS: &[&str] = &[
    // --- totals, for comparison with the 67 MB / 87 MB headline numbers ---
    "tokenizer_full",
    "rules_full",
    // --- in-context partition: full minus one component ---
    "tokenizer_no_multiword",
    "tokenizer_no_chunker",
    "tokenizer_no_disambiguation",
    "tokenizer_no_sentencizer",
    "tokenizer_no_chunker_token_model",
    "tokenizer_no_chunker_pos_model",
    "tokenizer_no_chunker_chunk_model",
    "tokenizer_no_pos_parameters",
    "tokenizer_no_pos_outcomes",
    "tokenizer_no_pos_pmap",
    "tokenizer_no_pos_tagdict",
    "tokenizer_no_chunk_parameters",
    "tokenizer_no_chunk_outcomes",
    "tokenizer_no_chunk_pmap",
    "rules_no_grammar",
    "rules_no_spell",
    "rules_no_articles",
    // --- tokenizer ---
    "tagger",
    "tagger_word_store_fst",
    "tagger_tag_fst",
    "tagger_synth_fst",
    "tagger_tag_store",
    "tagger_word_buf",
    "tagger_word_offsets",
    "disambiguation_rules",
    "disambig_no_examples",
    "disambig_no_filter",
    "disambig_no_unification",
    "disambig_no_disambiguations",
    "disambig_no_engine",
    "disambig_no_text_engine",
    "disambig_no_token_engine",
    "chunker",
    "chunker_token_model",
    "chunker_pos_model",
    "chunker_pos_tagdict",
    "chunker_chunk_model",
    "chunker_model_parameters",
    "chunker_model_outcomes",
    "chunker_model_pmap",
    "chunker_model_outcome_labels",
    "chunker_token_model_parameters",
    "chunker_token_model_pmap",
    "chunker_pos_model_parameters",
    "chunker_pos_model_pmap",
    "multiword_tagger",
    "sentencizer",
    // --- rules ---
    "grammar_rules",
    "grammar_no_examples",
    "grammar_no_suggesters",
    "grammar_no_engine",
    "spell",
    "spell_en_us",
    // --- per-field grammar-rule breakdown, inside the full `Rules` ---
    "rules_no_gr_id",
    "rules_no_gr_examples",
    "rules_no_gr_message",
    "rules_no_gr_suggesters",
    "rules_no_gr_suggester_parts",
    "rules_no_gr_engine",
    "rules_no_gr_unification",
    "rules_no_gr_filter",
    "rules_no_gr_strings",
    "rules_no_gr_antipatterns",
    "rules_no_gr_parts",
    "rules_no_gr_part_pool",
    "rules_no_gr_id_to_idx",
    "rules_no_gr_can_stop_mask",
    "rules_no_gr_text_regex",
    "rules_no_gr_text_id_to_idx",
    "rules_no_gr_atoms",
    "rules_no_gr_quantifiers",
    "rules_no_gr_regexes",
    "rules_no_gr_matcher_strings",
    "rules_no_gr_sets",
];

/// Grammar-rule fields for the field table, in reporting order: `(label, component)`.
const GRAMMAR_FIELDS: &[(&str, &str)] = &[
    ("id (rule index)", "rules_no_gr_id"),
    ("examples", "rules_no_gr_examples"),
    ("message synthesizer", "rules_no_gr_message"),
    ("suggesters", "rules_no_gr_suggesters"),
    ("  suggester parts", "rules_no_gr_suggester_parts"),
    ("engine (whole)", "rules_no_gr_engine"),
    ("unification", "rules_no_gr_unification"),
    ("filter", "rules_no_gr_filter"),
    ("strings (name/url/cat)", "rules_no_gr_strings"),
    ("articles (rule set)", "rules_no_articles"),
    ("  antipatterns", "rules_no_gr_antipatterns"),
    ("  composition parts", "rules_no_gr_parts"),
    ("  part pool (unique parts)", "rules_no_gr_part_pool"),
    ("  composition id_to_idx", "rules_no_gr_id_to_idx"),
    ("  composition can_stop_mask", "rules_no_gr_can_stop_mask"),
    ("  text engine regex", "rules_no_gr_text_regex"),
    ("  text engine id_to_idx", "rules_no_gr_text_id_to_idx"),
    ("    part atoms", "rules_no_gr_atoms"),
    ("    part quantifiers", "rules_no_gr_quantifiers"),
    ("    all regex strings", "rules_no_gr_regexes"),
    ("    matcher strings", "rules_no_gr_matcher_strings"),
    ("    text-matcher sets", "rules_no_gr_sets"),
];

const REPETITIONS: usize = 3;
const MARKER: &str = "###M2B###";

fn rss_mb() -> f64 {
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps must run");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .expect("ps must print an integer")
        / 1024.0
}

/// One process, one component: measure RSS before and after loading it.
fn run_child() {
    let name = std::env::var("M2B_COMPONENT").unwrap();
    let file = std::env::var("M2B_FILE").unwrap();
    let bytes: u64 = std::env::var("M2B_BYTES").unwrap().parse().unwrap();

    let before = rss_mb();
    let started = std::time::Instant::now();
    let mut held = memory_probe::load(&name, Path::new(&file)).expect("component must load");
    let load_ms = started.elapsed().as_secs_f64() * 1e3;
    let mid = rss_mb();
    let mut after = mid;
    if name == "spell_en_us" {
        memory_probe::spell_set_variant(&mut held, "en_US").expect("en_US variant must exist");
        after = rss_mb();
    }
    std::hint::black_box(&held);

    // `before`-`after` is the total load growth; `mid`-`after` is the en_US selection growth.
    println!(
        "{} name={} bytes={} before={:.3} mid={:.3} after={:.3} load_ms={:.3}",
        MARKER, name, bytes, before, mid, after, load_ms
    );
}

struct Measurement {
    bytes: u64,
    /// Total load growth (`after - before`), one value per repetition.
    growths: Vec<f64>,
    /// Growth up to `mid` (`mid - before`); differs from `growths` only for `spell_en_us`.
    mid_growths: Vec<f64>,
    /// Wall time of the deserialization alone, in milliseconds, one value per repetition.
    load_ms: Vec<f64>,
}

impl Measurement {
    fn median(v: &[f64]) -> f64 {
        let mut v = v.to_vec();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    }

    fn spread_pct(v: &[f64]) -> f64 {
        let median = Self::median(v);
        if median <= 0.0 {
            return 0.0;
        }
        let min = v.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        (max - min) / median * 100.0
    }
}

fn measure(name: &str, bytes: u64, path: &Path) -> Measurement {
    let exe = std::env::current_exe().expect("current test binary");
    let mut growths = Vec::new();
    let mut mid_growths = Vec::new();
    let mut load_ms = Vec::new();
    for _ in 0..REPETITIONS {
        let output = Command::new(&exe)
            .args(["--ignored", "--nocapture"])
            .env("M2B_COMPONENT", name)
            .env("M2B_FILE", path)
            .env("M2B_BYTES", bytes.to_string())
            .output()
            .expect("child test binary must run");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout
            .lines()
            .find(|line| line.starts_with(MARKER))
            .unwrap_or_else(|| {
                panic!(
                    "no result for {}; stdout:\n{}\nstderr:\n{}",
                    name,
                    stdout,
                    String::from_utf8_lossy(&output.stderr)
                )
            });
        let field = |key: &str| -> f64 {
            line.split_whitespace()
                .find_map(|tok| tok.strip_prefix(&format!("{}=", key)))
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or_else(|| panic!("no {} in {}", key, line))
        };
        growths.push(field("after") - field("before"));
        mid_growths.push(field("mid") - field("before"));
        load_ms.push(field("load_ms"));
    }
    Measurement {
        bytes,
        growths,
        mid_growths,
        load_ms,
    }
}

/// Finds the directory holding `en_tokenizer.bin` / `en_rules.bin`.
fn storage_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("M2B_STORAGE") {
        return PathBuf::from(dir);
    }
    for candidate in ["storage", "../storage"] {
        if Path::new(candidate).join("en_tokenizer.bin").exists() {
            return PathBuf::from(candidate);
        }
    }
    PathBuf::from("storage")
}

fn run_parent() {
    let storage = storage_dir();
    let tokenizer_bin = storage.join("en_tokenizer.bin");
    let rules_bin = storage.join("en_rules.bin");
    let probe_dir = storage.join("memory_probe");

    println!(
        "extracting components from {} and {} into {}",
        tokenizer_bin.display(),
        rules_bin.display(),
        probe_dir.display()
    );
    let extracted = memory_probe::extract_all(&tokenizer_bin, &rules_bin, &probe_dir)
        .expect("extraction must succeed");
    let by_name: HashMap<String, memory_probe::Extracted> = extracted
        .into_iter()
        .map(|extracted| (extracted.name.clone(), extracted))
        .collect();

    let mut measurements: HashMap<String, Measurement> = HashMap::new();
    for name in COMPONENTS {
        let extracted = by_name
            .get(*name)
            .unwrap_or_else(|| panic!("component {} was not extracted", name));
        eprintln!("measuring {} ({} bytes) ...", name, extracted.bytes);
        measurements.insert(
            (*name).to_string(),
            measure(name, extracted.bytes, Path::new(&extracted.path)),
        );
    }

    println!();
    println!(
        "| component | serialized bytes | RSS growth MB (median of {}) | min | max | spread | load ms (median) |",
        REPETITIONS
    );
    println!("|---|---:|---:|---:|---:|---:|---:|");
    for name in COMPONENTS {
        let m = &measurements[*name];
        let min = m.growths.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = m.growths.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        println!(
            "| {} | {} | {:.2} | {:.2} | {:.2} | {:.0}% | {:.1} |",
            name,
            m.bytes,
            Measurement::median(&m.growths),
            min,
            max,
            Measurement::spread_pct(&m.growths),
            Measurement::median(&m.load_ms)
        );
    }

    let sum = |names: &[&str]| -> f64 {
        names
            .iter()
            .map(|name| Measurement::median(&measurements[*name].growths))
            .sum()
    };
    let median = |name: &str| -> f64 { Measurement::median(&measurements[name].growths) };
    println!();
    println!("tokenizer full:       {:.1} MB", median("tokenizer_full"));
    println!("  component sum:      {:.1} MB", sum(TOKENIZER_TOP));
    println!("rules full:           {:.1} MB", median("rules_full"));
    println!("  component sum:      {:.1} MB", sum(RULES_TOP));
    println!();
    println!("in-context partition (full minus that component):");
    let full = median("tokenizer_full");
    for (label, name) in [
        ("tagger (residual)", None),
        ("disambiguation rules", Some("tokenizer_no_disambiguation")),
        ("chunker", Some("tokenizer_no_chunker")),
        ("multiword tagger", Some("tokenizer_no_multiword")),
        ("sentencizer", Some("tokenizer_no_sentencizer")),
    ] {
        match name {
            Some(name) => println!("  {:<22} {:.1} MB", label, full - median(name)),
            None => println!(
                "  {:<22} {:.1} MB",
                label,
                full - (full - median("tokenizer_no_disambiguation"))
                    - (full - median("tokenizer_no_chunker"))
                    - (full - median("tokenizer_no_multiword"))
                    - (full - median("tokenizer_no_sentencizer"))
            ),
        }
    }
    let rules_full = median("rules_full");
    println!(
        "  grammar rules         {:.1} MB",
        rules_full - median("rules_no_grammar")
    );
    println!(
        "  spell (variant none)  {:.1} MB",
        rules_full - median("rules_no_spell")
    );
    println!();

    println!("grammar-rule field breakdown (in-context = rules_full minus the field blanked):");
    println!("| field | serialized MB | in-context MB |");
    println!("|---|---:|---:|");
    let rules_full_bytes = measurements["rules_full"].bytes;
    for (label, name) in GRAMMAR_FIELDS {
        let m = &measurements[*name];
        println!(
            "| {} | {:.2} | {:.2} |",
            label,
            (rules_full_bytes - m.bytes) as f64 / 1e6,
            rules_full - Measurement::median(&m.growths)
        );
    }
    println!();
    println!("chunker in-context breakdown (full minus that sub-field):");
    for (label, name) in [
        ("token model", "tokenizer_no_chunker_token_model"),
        ("pos model", "tokenizer_no_chunker_pos_model"),
        ("chunk model", "tokenizer_no_chunker_chunk_model"),
    ] {
        println!("  {:<22} {:.1} MB", label, full - median(name));
    }
    println!("  pos model fields:");
    for (label, name) in [
        ("parameters", "tokenizer_no_pos_parameters"),
        ("outcomes", "tokenizer_no_pos_outcomes"),
        ("pmap", "tokenizer_no_pos_pmap"),
        ("tagdict", "tokenizer_no_pos_tagdict"),
    ] {
        println!("    {:<20} {:.1} MB", label, full - median(name));
    }
    println!("  chunk model fields:");
    for (label, name) in [
        ("parameters", "tokenizer_no_chunk_parameters"),
        ("outcomes", "tokenizer_no_chunk_outcomes"),
        ("pmap", "tokenizer_no_chunk_pmap"),
    ] {
        println!("    {:<20} {:.1} MB", label, full - median(name));
    }
    println!();
    println!("structural facts:");
    for (key, value) in memory_probe::describe(&tokenizer_bin, &rules_bin).expect("describe") {
        println!("  {:<52} {}", key, value);
    }
    println!();

    let spell = &measurements["spell"];
    let spell_en_us = &measurements["spell_en_us"];
    let variant_growths: Vec<f64> = spell_en_us
        .growths
        .iter()
        .zip(spell_en_us.mid_growths.iter())
        .map(|(total, raw)| total - raw)
        .collect();
    println!(
        "spell (variant none): {:.1} MB",
        Measurement::median(&spell.growths)
    );
    println!(
        "spell en_US total:    {:.1} MB (raw load {:.1} MB + variant selection {:.1} MB)",
        Measurement::median(&spell_en_us.growths),
        Measurement::median(&spell_en_us.mid_growths),
        Measurement::median(&variant_growths)
    );
}

/// Entry point. Without `M2B_COMPONENT` it extracts components and spawns one child process per
/// component/repetition; with it set, it measures the single component in this fresh process.
#[test]
#[ignore]
fn memory_breakdown() {
    if std::env::var("M2B_COMPONENT").is_ok() {
        run_child();
        return;
    }
    run_parent();
}

/// Prints only the structural facts (fast: one load of each binary, no per-component children).
#[test]
#[ignore]
fn memory_describe() {
    // The measurement children are spawned with `--ignored` and `M2B_COMPONENT` set; they must
    // only run `run_child`, otherwise their extra load of both binaries corrupts the RSS reading.
    if std::env::var("M2B_COMPONENT").is_ok() {
        return;
    }
    let storage = storage_dir();
    for (key, value) in memory_probe::describe(
        &storage.join("en_tokenizer.bin"),
        &storage.join("en_rules.bin"),
    )
    .expect("describe")
    {
        println!("{:<52} {}", key, value);
    }
}
