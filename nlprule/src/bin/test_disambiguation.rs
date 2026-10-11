use clap::Parser;
use nlprule::tokenizer::Tokenizer;

#[derive(Parser)]
#[clap(
    version = "1.0",
    author = "Benjamin Minixhofer <bminixhofer@gmail.com>"
)]
struct Opts {
    #[clap(long)]
    stop_at_error: bool,
    #[clap(long, short)]
    tokenizer: String,
    /// Print the sorted IDs of the rules that pass their examples, one per line.
    #[clap(long)]
    list_passing: bool,
    /// Print the sorted IDs of all runnable rules, one per line.
    #[clap(long)]
    list_rules: bool,
}

fn main() {
    env_logger::init();
    let opts = Opts::parse();

    let tokenizer = Tokenizer::new(opts.tokenizer).unwrap();
    let rules = tokenizer.rules();

    println!("Last ID: {}", rules[rules.len() - 1].id());
    println!("Runnable rules: {}", rules.len());

    if opts.list_rules {
        let mut ids: Vec<String> = rules.iter().map(|r| r.id().to_string()).collect();
        ids.sort();
        for id in &ids {
            println!("RULE {}", id);
        }
    }

    let mut passes = 0;
    let mut passing_ids: Vec<String> = Vec::new();

    for rule in rules {
        if rule.test(&tokenizer) {
            passes += 1;
            passing_ids.push(rule.id().to_string());
        } else if opts.stop_at_error {
            break;
        }
    }

    if opts.list_passing {
        passing_ids.sort();
        for id in &passing_ids {
            println!("PASS {}", id);
        }
    }

    println!("Rules passing tests: {}", passes);
    if passes == rules.len() {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}
