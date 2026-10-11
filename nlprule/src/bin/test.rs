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
    /// Print the id of every rule that passed its examples, one per line.
    #[clap(long)]
    list_passing: bool,
    /// Print the id of every rule that failed its examples, one per line.
    #[clap(long)]
    list_failing: bool,
}

fn main() {
    env_logger::init();
    let opts = Opts::parse();

    let tokenizer = Tokenizer::new(opts.tokenizer).unwrap();
    let rules_container = Rules::new(opts.rules).unwrap();
    let rules = rules_container.rules();

    println!("Runnable rules: {}", rules.len());

    let mut passes = 0;
    for rule in rules {
        if opts.ids.is_empty() || opts.ids.contains(&rule.id().to_string()) {
            let passed = rule.test(&tokenizer);
            passes += passed as usize;

            if passed && opts.list_passing {
                println!("{}", rule.id());
            }
            if !passed && opts.list_failing {
                println!("{}", rule.id());
            }
        }
    }

    println!("Rules passing tests: {}", passes);
    if passes == rules.len() {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}
