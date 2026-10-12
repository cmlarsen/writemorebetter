//! Isolated chunker benchmark and golden checker.
//!
//! Loads an LT English tokenizer, runs the pipeline up to (but not including) chunking on a
//! fixed text set once, then runs only [`nlprule`]'s chunker over those prepared sentences N
//! times. A counting global allocator reports allocation volume. `--golden` dumps or compares
//! every token's chunk tags byte-for-byte.
//!
//! This is measurement tooling, not part of the library's public API. It needs nlprule's
//! `bench-harness` feature, which exposes the pre-chunk pipeline stages.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use std::time::Instant;

/// Counts allocations made anywhere in the process. Relaxed ordering is enough: the benchmark
/// runs the chunker on one thread and reads the counters after joining it.
struct CountingAllocator;

static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static DEALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicIsize = AtomicIsize::new(0);

#[allow(unused_unsafe)]
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        LIVE_BYTES.fetch_add(layout.size() as isize, Ordering::Relaxed);
        System.alloc(layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        LIVE_BYTES.fetch_add(layout.size() as isize, Ordering::Relaxed);
        System.alloc_zeroed(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        LIVE_BYTES.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(new_size, Ordering::Relaxed);
        LIVE_BYTES.fetch_add(new_size as isize - layout.size() as isize, Ordering::Relaxed);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn rss_mib() -> f64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps runs");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .unwrap()
        / 1024.0
}

/// A `(calls, bytes)` snapshot of the counting allocator.
#[derive(Clone, Copy)]
struct AllocSnapshot {
    alloc_calls: usize,
    alloc_bytes: usize,
}

impl AllocSnapshot {
    fn take() -> Self {
        AllocSnapshot {
            alloc_calls: ALLOC_CALLS.load(Ordering::Relaxed),
            alloc_bytes: ALLOC_BYTES.load(Ordering::Relaxed),
        }
    }

    fn delta(&self, earlier: &AllocSnapshot) -> (usize, usize) {
        (
            self.alloc_calls - earlier.alloc_calls,
            self.alloc_bytes - earlier.alloc_bytes,
        )
    }
}

/// Compares the harness path (`prepared_sentences` + `chunk_apply`) against the real
/// `Tokenizer::pipe` pipeline for one text. The chunk tags must be identical.
fn verify_pipeline(tokenizer: &nlprule::Tokenizer, name: &str, text: &str) {
    let real: Vec<Vec<Vec<String>>> = tokenizer
        .pipe(text)
        .map(|s| s.iter().map(|t| t.chunks().to_vec()).collect())
        .collect();

    let mut harness: Vec<Vec<Vec<String>>> = Vec::new();
    for mut sentence in tokenizer.prepared_sentences(text) {
        tokenizer.chunk_apply(&mut sentence);
        harness.push(sentence.iter().map(|t| t.chunks().to_vec()).collect());
    }

    if real == harness {
        println!("verify ok {} ({} sentences)", name, real.len());
    } else {
        eprintln!(
            "VERIFY MISMATCH {}: real {} sentences, harness {} sentences",
            name,
            real.len(),
            harness.len()
        );
        std::process::exit(1);
    }
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

struct Args {
    storage: String,
    runs: usize,
    golden: Option<String>,
    out: String,
    label: String,
    texts: Vec<String>,
}

fn parse_args() -> Args {
    let mut args = std::env::args().skip(1);
    let storage = args.next().expect("usage: wmb-chunkbench <storage-dir> [texts...]");
    let mut runs = 10usize;
    let mut golden = None;
    let mut out = "bench/golden/chunker".to_string();
    let mut label = "unknown".to_string();
    let mut texts = Vec::new();
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--runs=") {
            runs = value.parse().expect("--runs is a number");
        } else if arg == "--runs" {
            runs = args.next().expect("--runs needs a value").parse().unwrap();
        } else if let Some(value) = arg.strip_prefix("--golden=") {
            golden = Some(value.to_string());
        } else if arg == "--golden" {
            golden = Some(args.next().expect("--golden needs a value"));
        } else if let Some(value) = arg.strip_prefix("--out=") {
            out = value.to_string();
        } else if let Some(value) = arg.strip_prefix("--label=") {
            label = value.to_string();
        } else if let Some(value) = arg.strip_prefix("--texts=") {
            texts.extend(value.split(',').map(|s| s.to_string()));
        } else {
            texts.push(arg);
        }
    }
    Args {
        storage,
        runs,
        golden,
        out,
        label,
        texts,
    }
}

/// Dumps every token's chunk tags for one text to a `String`.
fn golden_text(tokenizer: &nlprule::Tokenizer, name: &str, text: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# text={}\n", name));

    let chunker = tokenizer.chunker().as_ref().expect("tokenizer has a chunker");
    for mut sentence in tokenizer.prepared_sentences(text) {
        out.push_str("## sentence\n");
        // The chunker's own tokenizer, POS tagger and chunk model on the same text
        // (with the apostrophe replacement `Chunker::apply` performs).
        let normalized = sentence.text().replace('\u{2019}', "'");
        let (tokens, pos, chunks) = chunker.predict_internal(&normalized);
        for ((tok, pos), chunk) in tokens.iter().zip(pos.iter()).zip(chunks.iter()) {
            out.push_str(&format!("@\t{}\t{}\t{}\n", tok, pos, chunk));
        }
        // The chunk tags assigned to the pipeline's own tokens.
        tokenizer.chunk_apply(&mut sentence);
        for token in sentence.iter() {
            let span = token.span();
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\n",
                span.byte().start,
                span.byte().end,
                token.chunks().join(","),
                token.word().text().as_str(),
            ));
        }
    }
    out
}

fn main() {
    let args = parse_args();
    let tokenizer = nlprule::Tokenizer::new(format!("{}/en_tokenizer.bin", args.storage))
        .expect("tokenizer loads");

    if let Some(mode) = &args.golden {
        let dir = std::path::PathBuf::from(&args.out).join(&args.label);
        std::fs::create_dir_all(&dir).unwrap();
        for name in &args.texts {
            let text = std::fs::read_to_string(name).expect("text reads");
            let text_name = std::path::Path::new(name)
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .to_string();
            let dump = golden_text(&tokenizer, &text_name, &text);
            let path = dir.join(format!("{}.txt", text_name));
            if mode == "write" {
                std::fs::write(&path, dump).unwrap();
                println!("wrote {}", path.display());
            } else if mode == "verify" {
                verify_pipeline(&tokenizer, &text_name, &text);
            } else {
                let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
                    panic!("missing golden {}", path.display());
                });
                if expected == dump {
                    println!("ok {}", path.display());
                } else {
                    eprintln!("MISMATCH {}", path.display());
                    // Show a short diff.
                    for (i, (a, b)) in expected.lines().zip(dump.lines()).enumerate() {
                        if a != b {
                            eprintln!("  line {}: golden {:?} != current {:?}", i + 1, a, b);
                            break;
                        }
                    }
                    std::process::exit(1);
                }
            }
        }
        return;
    }

    // Read every text first so the `Vec<String>` never reallocates while sentences borrow it.
    let texts: Vec<(String, String)> = args
        .texts
        .iter()
        .map(|path| {
            let name = std::path::Path::new(path)
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .to_string();
            (name, std::fs::read_to_string(path).expect("text reads"))
        })
        .collect();

    let load_heap = LIVE_BYTES.load(Ordering::Relaxed);
    println!(
        "loaded {} in memory: live heap {:.2} MiB, rss {:.1} MiB",
        args.storage,
        load_heap as f64 / 1048576.0,
        rss_mib()
    );

    let mut prepared: Vec<nlprule::types::IncompleteSentence> = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    for (name, text) in &texts {
        let before = prepared.len();
        prepared.extend(tokenizer.prepared_sentences(text));
        labels.push(format!("{} ({} sent, {} tok)", name, prepared.len() - before,
            prepared[before..].iter().map(|s| s.len()).sum::<usize>()));
    }
    let token_count: usize = prepared.iter().map(|s| s.len()).sum();
    let sentence_count = prepared.len();
    println!("prepared {} sentences, {} tokens", sentence_count, token_count);

    let mut run_ms = Vec::with_capacity(args.runs);
    let mut run_allocs = Vec::with_capacity(args.runs);
    let mut run_bytes = Vec::with_capacity(args.runs);
    for _ in 0..args.runs {
        let before = AllocSnapshot::take();
        let start = Instant::now();
        for sentence in prepared.iter_mut() {
            tokenizer.chunk_apply(sentence);
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let after = AllocSnapshot::take();
        let (calls, bytes) = after.delta(&before);
        run_ms.push(elapsed);
        run_allocs.push(calls as f64);
        run_bytes.push(bytes as f64);
    }

    let median_ms = median(run_ms.clone());
    let median_allocs = median(run_allocs.clone());
    let median_bytes = median(run_bytes.clone());
    println!(
        "runs={} texts={:?}",
        args.runs,
        labels
    );
    println!(
        "chunker: median {:.3} ms total, {:.1} ns/token, {:.0} alloc calls, {:.2} MiB requested",
        median_ms,
        median_ms * 1_000_000.0 / token_count as f64,
        median_allocs,
        median_bytes / 1048576.0,
    );
}

