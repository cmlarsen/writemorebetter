use clap::Parser;
use nlprule::{rules::Rules, tokenizer::Tokenizer};

#[derive(Parser)]
#[clap(
    version = "1.0",
    author = "Benjamin Minixhofer <bminixhofer@gmail.com>"
)]
struct Opts {
    #[clap(long, short)]
    tokenizer: String,
    #[clap(long, short)]
    rules: String,
    #[clap(long, short)]
    ids: Vec<String>,
    /// Print the sorted IDs of the rules that pass their examples, one per line.
    #[clap(long)]
    list_passing: bool,
    /// Print the sorted IDs of the rules that fail their examples, one per line.
    #[clap(long)]
    list_failing: bool,
    /// Print the sorted IDs of all runnable rules, one per line.
    #[clap(long)]
    list_rules: bool,
}

fn main() {
    env_logger::init();
    let opts = Opts::parse();

    let tokenizer = Tokenizer::new(opts.tokenizer).unwrap();
    let rules_container = Rules::new(opts.rules).unwrap();
    let (mother_tongue_rules, rules): (Vec<_>, Vec<_>) = rules_container
        .rules()
        .iter()
        .partition(|rule| rule.mother_tongue().is_some());

    println!("Runnable rules: {}", rules.len());
    if !mother_tongue_rules.is_empty() {
        println!(
            "Mother-tongue rules (no examples, not tested): {}",
            mother_tongue_rules.len()
        );
    }

    if opts.list_rules {
        let mut ids: Vec<String> = rules.iter().map(|r| r.id().to_string()).collect();
        ids.sort();
        for id in &ids {
            println!("RULE {}", id);
        }
    }

    let mut passes = 0;
    let mut passing_ids: Vec<String> = Vec::new();
    let mut failing_ids: Vec<String> = Vec::new();
    for rule in &rules {
        if opts.ids.is_empty() || opts.ids.contains(&rule.id().to_string()) {
            if rules_container.test_rule(rule, &tokenizer) {
                passes += 1;
                passing_ids.push(rule.id().to_string());
            } else {
                failing_ids.push(rule.id().to_string());
            }
        }
    }

    if opts.list_passing {
        passing_ids.sort();
        for id in &passing_ids {
            println!("PASS {}", id);
        }
    }
    if opts.list_failing {
        failing_ids.sort();
        for id in &failing_ids {
            println!("FAIL {}", id);
        }
    }

    println!("Rules passing tests: {}", passes);
    if passes == rules.len() {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}
