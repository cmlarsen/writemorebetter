use nlprule::{profile::Profile, Rules, Tokenizer};
use std::time::Instant;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn rss_mib() -> f64 {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        let kib: f64 = status
            .lines()
            .find(|line| line.starts_with("VmRSS:"))
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        kib / 1024.0
    }
    #[cfg(not(target_os = "linux"))]
    {
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse::<f64>()
            .unwrap()
            / 1024.0
    }
}

fn peak_rss_mib() -> f64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage fully initializes the struct when it returns 0.
    let maxrss = unsafe {
        assert_eq!(libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()), 0);
        usage.assume_init().ru_maxrss as f64
    };
    maxrss / if cfg!(target_os = "macos") { 1048576.0 } else { 1024.0 }
}

struct Phases(Vec<(String, f64, f64)>);

impl Phases {
    fn run<T>(&mut self, name: &str, f: impl FnOnce() -> T) -> T {
        let before = rss_mib();
        let start = Instant::now();
        let value = f();
        self.0
            .push((name.to_string(), start.elapsed().as_secs_f64() * 1e3, rss_mib() - before));
        value
    }
}

fn load(dir: &str, profile_name: &str, spell: &str, phases: &mut Phases) -> (Tokenizer, Rules) {
    let tokenizer = phases.run("tokenizer deserialize", || {
        Tokenizer::new(format!("{dir}/en_tokenizer.bin")).unwrap()
    });
    let mut rules = phases.run("rules deserialize", || {
        Rules::new(format!("{dir}/en_rules.bin")).unwrap()
    });
    let profile = Profile::by_name(profile_name).expect("unknown profile");
    phases.run("profile apply", || {
        rules.apply_profile(&profile);
    });
    if !spell.is_empty() {
        phases.run("speller variant select", || {
            let variant = rules.spell().variant(spell).unwrap();
            rules.spell_mut().options_mut().variant = Some(variant);
        });
    }
    (tokenizer, rules)
}

fn print_phases(phases: &Phases) {
    for (name, ms, mib) in &phases.0 {
        println!("PHASE\t{name}\t{ms:.2}\t{mib:.2}");
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().expect("mode: phases | stages | heap");
    let dir = args.next().expect("storage dir");
    let text_path = args.next();
    let profile_name = std::env::var("PROFILE").unwrap_or_else(|_| "fiction".into());
    let spell = std::env::var("SPELL").unwrap_or_else(|_| "en_US".into());

    #[cfg(feature = "dhat-heap")]
    let _profiler = if mode == "heap" {
        Some(
            dhat::Profiler::builder()
                .file_name(std::env::var("DHAT_OUT").unwrap_or_else(|_| "dhat-heap.json".into()))
                .build(),
        )
    } else {
        None
    };

    let mut phases = Phases(Vec::new());
    println!("BASELINE_RSS\t{:.2}", rss_mib());
    let (tokenizer, rules) = load(&dir, &profile_name, &spell, &mut phases);
    println!("LOADED_RSS\t{:.2}", rss_mib());
    #[cfg(feature = "dhat-heap")]
    if mode == "heap" {
        let stats = dhat::HeapStats::get();
        println!("HEAP_AFTER_LOAD\t{}\t{}", stats.curr_bytes, stats.max_bytes);
    }
    match mode.as_str() {
        "phases" => print_phases(&phases),
        "stages" => {
            print_phases(&phases);
            let text = std::fs::read_to_string(text_path.expect("text path")).unwrap();
            let t = Instant::now();
            let splits = tokenizer.sentencize(&text);
            let split_ms = t.elapsed().as_secs_f64() * 1e3;
            drop(splits);
            let t = Instant::now();
            let sentences: Vec<_> = tokenizer.sentencize(&text).collect();
            let tokenize_ms = t.elapsed().as_secs_f64() * 1e3 - split_ms;
            let n_sentences = sentences.len();
            let t = Instant::now();
            let disambiguated: Vec<_> = sentences
                .into_iter()
                .map(|sentence| tokenizer.disambiguate(sentence))
                .collect();
            let disambiguate_ms = t.elapsed().as_secs_f64() * 1e3;
            let tokens: usize = disambiguated
                .into_iter()
                .map(|s| s.into_sentence().tokens().len())
                .sum();
            let t = Instant::now();
            let suggestions = rules.suggest(&text, &tokenizer);
            let suggest_ms = t.elapsed().as_secs_f64() * 1e3;
            println!("STAGE\tsentence split\t{split_ms:.1}");
            println!("STAGE\ttokenize+tag+chunk+multiword\t{tokenize_ms:.1}");
            println!("STAGE\tdisambiguation\t{disambiguate_ms:.1}");
            println!("STAGE\tsuggest total (includes the three above)\t{suggest_ms:.1}");
            println!("COUNTS\t{n_sentences}\t{tokens}\t{}", suggestions.len());
            println!("PEAK_RSS\t{:.2}", peak_rss_mib());
        }
        "heap" => {
            print_phases(&phases);
            let text = std::fs::read_to_string(text_path.expect("text path")).unwrap();
            let suggestions = rules.suggest(&text, &tokenizer);
            #[cfg(feature = "dhat-heap")]
            {
                let stats = dhat::HeapStats::get();
                println!(
                    "HEAP_AFTER_CHECK\t{}\t{}\t{}",
                    stats.curr_bytes, stats.max_bytes, stats.total_bytes
                );
            }
            println!("COUNTS\t{}", suggestions.len());
            println!("PEAK_RSS\t{:.2}", peak_rss_mib());
        }
        other => panic!("unknown mode {other}"),
    }
}
