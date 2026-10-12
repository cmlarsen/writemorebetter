//! Deterministic stress run: a seeded generator over boundary-heavy inputs plus a replay
//! of the saved seed and crash corpus. Used by `.github/workflows/fuzz.yml`; it needs no
//! nightly toolchain and no libFuzzer, so it runs on stable Rust.
//!
//! Usage: `stress [--iterations N] [--seed S] [--max-chars N] [--corpus DIR]...`
//! Also: `stress --dump-handcrafted DIR` and `stress --timings DIR` for maintaining the
//! seed corpus and finding slow inputs. Exits non-zero on the first panic, printing the
//! exact input.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use wmb_fuzz::{check, generated_with, handcrafted};

fn main() {
    let mut iterations = 3_000usize;
    let mut seed = 0x5eed_1234_abcd_0001u64;
    let mut max_chars = 200usize;
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut dump_handcrafted: Option<PathBuf> = None;
    let mut timings_dir: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--iterations" => iterations = args.next().unwrap().parse().unwrap(),
            "--seed" => seed = args.next().unwrap().parse().unwrap(),
            "--max-chars" => max_chars = args.next().unwrap().parse().unwrap(),
            "--corpus" => dirs.push(PathBuf::from(args.next().unwrap())),
            "--dump-handcrafted" => dump_handcrafted = Some(PathBuf::from(args.next().unwrap())),
            "--timings" => timings_dir = Some(PathBuf::from(args.next().unwrap())),
            other => panic!("unknown argument: {other}"),
        }
    }

    if let Some(dir) = dump_handcrafted {
        std::fs::create_dir_all(&dir).unwrap();
        let inputs = handcrafted();
        for (i, input) in inputs.iter().enumerate() {
            std::fs::write(dir.join(format!("handcrafted_{i:03}")), input.as_bytes()).unwrap();
        }
        println!(
            "wrote {} handcrafted inputs to {}",
            inputs.len(),
            dir.display()
        );
        return;
    }

    if let Some(dir) = timings_dir {
        // Load before timing.
        let _ = check("warm up");
        timings(&dir);
        return;
    }

    if dirs.is_empty() {
        // The crash corpus only: the full seed corpus is for cargo-fuzz, and some of its
        // paragraph-sized handcrafted cases exercise the known long-sentence quadratic
        // (docs/FUZZ.md), which would blow the CI time budget. Pass --corpus explicitly
        // to replay the seeds too.
        dirs.push(PathBuf::from("corpus/crashes"));
    }

    // Load the engine before timing so a load failure is unambiguous.
    let _ = check("warm up");

    let start = Instant::now();
    let mut replayed = 0usize;
    for dir in &dirs {
        replayed += replay(dir);
    }
    let mut checked = 0usize;
    for input in generated_with(seed, iterations, max_chars) {
        run(&input);
        checked += 1;
    }
    let elapsed = start.elapsed();
    println!(
        "ok: {replayed} corpus inputs + {checked} generated inputs in {:.1}s",
        elapsed.as_secs_f64()
    );
    if elapsed > Duration::from_secs(120) {
        eprintln!("warning: stress run exceeded 120s");
    }
}

fn replay(dir: &Path) -> usize {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return 0,
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    paths.sort();
    for path in &paths {
        let bytes = std::fs::read(path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let label = format!("corpus file {}", path.display());
        run_labeled(&text, &label);
    }
    paths.len()
}

/// Times every file in `dir` and reports any input under 10 KB that takes over 1 s,
/// plus the slowest inputs overall.
fn timings(dir: &Path) {
    let mut rows: Vec<(Duration, PathBuf, usize)> = Vec::new();
    let entries = std::fs::read_dir(dir).unwrap();
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let start = Instant::now();
        let _ = check(&text);
        rows.push((start.elapsed(), path, bytes.len()));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    let mut slow = 0usize;
    for (elapsed, path, bytes) in &rows {
        if *bytes < 10_000 && *elapsed >= Duration::from_secs(1) {
            slow += 1;
            println!(
                "SLOW {:.3}s {} bytes: {}",
                elapsed.as_secs_f64(),
                bytes,
                path.display()
            );
        }
    }
    println!("{} files, {} under 10KB over 1s", rows.len(), slow);
    for (elapsed, path, bytes) in rows.iter().take(15) {
        println!(
            "{:>8.3}s {:>7} bytes {}",
            elapsed.as_secs_f64(),
            bytes,
            path.display()
        );
    }
}

fn run(input: &str) {
    run_labeled(input, "generated input");
}

fn run_labeled(input: &str, label: &str) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = check(input);
    }));
    if result.is_err() {
        // Print the input in a form that survives non-UTF-8 terminals and is easy to
        // paste into a replay file.
        eprintln!("panic on {label}:");
        eprintln!("{input:?}");
        std::fs::write("stress_crash.txt", input.as_bytes()).ok();
        std::process::exit(1);
    }
}
