use nlprule::{Rules, Tokenizer};
use std::time::Instant;
fn rss() -> f64 {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        let line = status
            .lines()
            .find(|line| line.starts_with("VmRSS:"))
            .unwrap();
        line.split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<f64>()
            .unwrap()
            / 1024.0
    }
    #[cfg(not(target_os = "linux"))]
    {
        let o = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .unwrap();
        String::from_utf8_lossy(&o.stdout)
            .trim()
            .parse::<f64>()
            .unwrap()
            / 1024.0
    }
}
fn main() {
    let d = std::env::args().nth(1).unwrap();
    println!("baseline {:.0} MB", rss());
    let load_start = Instant::now();
    let t = Instant::now();
    let tok = Tokenizer::new(format!("{}/en_tokenizer.bin", d)).unwrap();
    println!("tokenizer loaded {:.0} MB in {:.2?}", rss(), t.elapsed());
    let t = Instant::now();
    let mut rules = Rules::new(format!("{}/en_rules.bin", d)).unwrap();
    println!("rules loaded {:.0} MB in {:.2?}", rss(), t.elapsed());
    let profile_name = std::env::var("PROFILE").unwrap_or_else(|_| "default".into());
    let profile = nlprule::profile::Profile::by_name(&profile_name)
        .unwrap_or_else(|| panic!("unknown profile: {}", profile_name));
    let enabled = |rules: &Rules| {
        rules.rules().iter().filter(|r| r.enabled()).count()
            + rules.builtins().iter().filter(|r| r.enabled()).count()
    };
    let before = enabled(&rules);
    let unmatched = rules.apply_profile(&profile);
    let after = enabled(&rules);
    println!(
        "profile {}: {} rules disabled",
        profile.name,
        before - after
    );
    if !unmatched.is_empty() {
        eprintln!(
            "profile {}: unmatched selectors: {:?}",
            profile.name, unmatched
        );
    }
    let variant = std::env::var("SPELL").unwrap_or_else(|_| "en_US".into());
    if !variant.is_empty() {
        let t = Instant::now();
        rules.spell_mut().options_mut().variant = Some(rules.spell().variant(&variant).unwrap());
        println!(
            "speller {} enabled {:.0} MB in {:.2?}",
            variant,
            rss(),
            t.elapsed()
        );
    }
    let load_seconds = load_start.elapsed().as_secs_f64();
    let loaded_rss = rss();
    let text = std::fs::read_to_string(std::env::args().nth(2).unwrap()).unwrap();
    let t = Instant::now();
    let sugg = rules.suggest(&text, &tok);
    let check_seconds = t.elapsed().as_secs_f64();
    if std::env::var("METRICS").is_ok() {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
        // getrusage initializes the entire output on success; macOS reports bytes, Linux KiB.
        let peak_mib = unsafe {
            assert_eq!(libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()), 0);
            let divisor = if cfg!(target_os = "macos") {
                1048576.0
            } else {
                1024.0
            };
            usage.assume_init().ru_maxrss as f64 / divisor
        };
        eprintln!(
            "METRICS {}",
            serde_json::json!({
                "check_seconds": check_seconds, "load_seconds": load_seconds,
                "loaded_rss_mib": loaded_rss, "peak_rss_mib": peak_mib,
                "bytes": text.len(), "suggestions": sugg.len()
            })
        );
    }
    println!(
        "checked {} chars, {} suggestions, {:.0} MB in {:.2?}",
        text.len(),
        sugg.len(),
        rss(),
        t.elapsed()
    );
    let chars: Vec<char> = text.chars().collect();
    if std::env::var("SHOW").is_ok() {
        for s in &sugg {
            let line = chars[..s.span().char().start]
                .iter()
                .filter(|c| **c == '\n')
                .count()
                + 1;
            let flagged: String = chars[s.span().char().clone()].iter().collect();
            println!(
                "{} | {} | {:?} -> {:?} | {}",
                line,
                s.source(),
                flagged,
                s.replacements(),
                s.message()
            );
        }
    }
}
