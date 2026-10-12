//! Shared harness for the `suggest` fuzz target and the deterministic stress replay.
//!
//! The engine is loaded once from the LT 6.6 binaries with the fiction profile and the
//! en_US speller enabled, matching what the writing app runs. Inputs are arbitrary
//! UTF-8; the harness calls [`Rules::suggest`] and then [`apply_suggestions`], which is
//! the pair the app uses to turn a text into corrected text.

use nlprule::{profile::Profile, Rules, Tokenizer};
use std::sync::OnceLock;

/// Default binary directory, relative to the `nlprule/fuzz` crate root (the cwd cargo-fuzz uses).
const DEFAULT_DIR: &str = "../../storage/66";

/// The directory holding `en_tokenizer.bin` and `en_rules.bin`.
pub fn engine_dir() -> String {
    std::env::var("WMB_FUZZ_DIR").unwrap_or_else(|_| DEFAULT_DIR.to_owned())
}

struct Engine {
    tokenizer: Tokenizer,
    rules: Rules,
}

fn load() -> Engine {
    let dir = engine_dir();
    let tokenizer = Tokenizer::new(format!("{dir}/en_tokenizer.bin"))
        .unwrap_or_else(|e| panic!("loading {dir}/en_tokenizer.bin: {e}"));
    let mut rules = Rules::new(format!("{dir}/en_rules.bin"))
        .unwrap_or_else(|e| panic!("loading {dir}/en_rules.bin: {e}"));
    let unmatched = rules.apply_profile(&Profile::fiction());
    assert!(
        unmatched.is_empty(),
        "unmatched profile selectors: {unmatched:?}"
    );
    let variant = rules
        .spell()
        .variant("en_US")
        .expect("en_US speller variant");
    rules.spell_mut().options_mut().variant = Some(variant);
    Engine { tokenizer, rules }
}

/// The process-wide engine, loaded on first use.
pub fn engine() -> (&'static Tokenizer, &'static Rules) {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    let engine = ENGINE.get_or_init(load);
    (&engine.tokenizer, &engine.rules)
}

/// Runs one input through the same path the app uses. Returns the suggestion count.
pub fn check(text: &str) -> usize {
    let (tokenizer, rules) = engine();
    let suggestions = rules.suggest(text, tokenizer);
    let _ = nlprule::rules::apply_suggestions(text, &suggestions);
    suggestions.len()
}

/// xorshift64* — small, fast, deterministic across platforms and Rust versions.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next_u64() % items.len() as u64) as usize]
    }
}

/// Characters chosen to exercise every parser, tokenizer and builtin boundary: ASCII prose
/// and punctuation, the curly-quote and dash family, line/paragraph separators, combining
/// marks, ZWJ, RTL, CJK, control characters and NUL.
const ALPHABET: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L',
    'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '0', '1', '2', '3', '4',
    '5', '6', '7', '8', '9', ' ', ' ', ' ', '\t', '\n', '\r', '.', ',', '!', '?', ';', ':', '-',
    '\'', '"', '(', ')', '[', ']', '{', '}', '<', '>', '/', '\\', '*', '#', '@', '%', '&', '+',
    '=', '~', '`', '|', '_', '$', '^', '’', '‘', '“', '”', '–', '—', '…', '\u{00a0}', '\u{2028}',
    '\u{2029}', '\u{200b}', '\u{200d}', '\u{0301}', '\u{0640}', '\u{05d0}', '\u{4e2d}', '\u{202e}',
    '\u{0000}', '\u{0001}', '\u{001b}', '\u{007f}', 'a', 'e', 'i', 'o', 'u', ' ', '\n',
];

fn random_string(rng: &mut Rng, max_chars: usize) -> String {
    let len = (rng.next_u64() as usize) % (max_chars + 1);
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        out.push(*rng.pick(ALPHABET));
    }
    out
}

/// Words and fragments that actually steer rules and builtins, so the generator
/// produces text a rule can act on rather than only noise.
const WORDS: &[&str] = &[
    "the",
    "a",
    "an",
    "and",
    "but",
    "is",
    "was",
    "were",
    "be",
    "been",
    "I",
    "you",
    "he",
    "she",
    "it",
    "they",
    "we",
    "not",
    "don't",
    "can't",
    "won't",
    "isn't",
    "didn't",
    "hello",
    "world",
    "sentence",
    "break",
    "here",
    "said",
    "quietly",
    "door",
    "then",
    "window",
    "matter",
    "bazaar",
    "bizarre",
    "prescribe",
    "proscribe",
    "heroin",
    "heroine",
    "alot",
    "definately",
    "recieve",
    "seperate",
    "apple",
    "hour",
    "university",
    "MBA",
    "one-way",
    "e.g.",
    "i.e.",
    "etc.",
    "Dr.",
    "12\"",
    "don’t",
    "it’s",
    "she’s",
    "Marta’s",
    "’90s",
    "wait",
    "may",
    "May",
    "will",
    "Will",
    "(((",
    ")))",
    "“",
    "”",
    "…",
    "—",
    "\u{2028}",
    "\u{200d}",
    "\u{0301}",
    "\u{0000}",
];

fn word_string(rng: &mut Rng, max_chars: usize) -> String {
    let words = 1 + (rng.next_u64() as usize) % 24;
    let mut out = String::new();
    for i in 0..words {
        if i > 0 && rng.next_u64() % 4 != 0 {
            out.push(' ');
        }
        out.push_str(rng.pick(WORDS));
        if out.len() > max_chars {
            break;
        }
    }
    out.truncate(char_boundary_at_or_before(&out, max_chars));
    out
}

fn char_boundary_at_or_before(s: &str, max_bytes: usize) -> usize {
    if s.len() <= max_bytes {
        return s.len();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    end
}

/// Deterministic generated inputs: a mix of word sequences that steer rules and
/// random boundary-heavy noise, bounded by `max_chars`.
pub fn generated_with(seed: u64, count: usize, max_chars: usize) -> Vec<String> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let max = match i % 10 {
            0 => (max_chars / 8).max(8),
            1..=7 => (max_chars / 3).max(16),
            _ => max_chars,
        };
        if i % 3 == 0 {
            out.push(word_string(&mut rng, max));
        } else {
            out.push(random_string(&mut rng, max));
        }
    }
    out
}

/// Handcrafted boundary cases, mirrored into the seed corpus by `make_seed_corpus.py`.
pub fn handcrafted() -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    let mut push = |s: &str| v.push(s.to_owned());

    // Empty and whitespace.
    push("");
    push(" ");
    push("   \t\n\r ");
    push("\n\n\n");

    // Line / paragraph separators, byte-boundary traps.
    push("word\u{2028}word");
    push("word\u{2029}word");
    push("a\u{2028}");
    push("\u{2028}b");
    push("x\u{2029}y\u{2029}z");

    // Lone combining marks and ZWJ emoji sequences.
    push("\u{0301}");
    push("a\u{0301}\u{0301}\u{0301}");
    push("\u{0301}\u{0301}");
    push("\u{200d}");
    push("\u{200d}\u{200d}\u{200d}");
    push("👨\u{200d}👩\u{200d}👧\u{200d}👦");
    push("🏳️\u{200d}🌈");
    push("👩\u{200d}💻");
    push("\u{200d}");

    // RTL and bidi controls.
    push("שלום עולם");
    push("مرحبا بالعالم");
    push("abc\u{202e}def");
    push("\u{202b}reversed\u{202c}");
    push("א\u{0301}ב");

    // CJK, Hangul, hiragana, emoji.
    push("中文测试。这是第二句。");
    push("日本語のテスト。");
    push("한국어 테스트");
    push("こんにちは、世界。");
    push("Emoji 😀🎉 test.");

    // NUL and control characters.
    push("a\u{0000}b");
    push("\u{0000}");
    push("\u{0001}\u{0002}\u{0003}");
    push("line\u{000b}break");
    push("\u{001b}[31mred\u{001b}[0m");

    // Very long words, single tokens.
    push(&"a".repeat(10_000));
    push(&"x".repeat(10_000));
    push(&format!("{}.", "a".repeat(10_000)));
    push(&"supercalifragilistic".repeat(600));
    push(&"’".repeat(10_000));

    // 100k chars with no sentence break.
    push(&"word ".repeat(20_000));
    push(&"a".repeat(100_000));
    push(&"no sentence break here ".repeat(5_000));

    // Deep nested quotes / parentheses / brackets.
    push(&"(".repeat(2_000));
    push(&")".repeat(2_000));
    push(&("(".repeat(2_000) + &")".repeat(2_000)));
    push(&"“".repeat(2_000));
    push(&"”".repeat(2_000));
    push(&"“a".repeat(1_000));
    push(&("[{<(".repeat(500) + &")>}]".repeat(500)));
    push("(((deep)))");

    // Straight / curly quote mixing and apostrophes.
    push("“It’s a ‘test’,” she said.");
    push("\"It's a 'test',\" she said.");
    push("He said, \"She said, 'hi'.\"");
    push("don’t can’t won’t shouldn’t");
    push("don't can't won't shouldn't");
    push("’");
    push("‘");
    push("“");
    push("”");
    push("'");
    push("\"");

    // Repeated punctuation.
    push("!!!!!!");
    push("?????????");
    push("..........");
    push(",,,;:;:;:");
    push("-- — – -----");
    push("...…...…");
    push("!!!!!?");
    push("word?????another");

    // Fiction-flavored text with all the typography.
    push("“I don’t know,” she said—quietly, too quietly.");
    push("He left… then returned. “Why?”");
    push("The door—then the window… it didn’t matter.");

    // Builtin trigger texts.
    push("I saw a apple and an banana.");
    push("This is a hour and an university.");
    push("hello ,world");
    push("word(word)");
    push("word )");
    push("( word");
    push("a,b");
    push(". hello");
    push("! lower");
    push("? lower");
    push("the the");
    push("is is");
    push("had had");
    push("I I");
    push("wait wait");
    push("may May");
    push("will Will");
    push("b a s i c a l l y");
    push("don’t don’t");
    push("alot");
    push("definately");
    push("thier");
    push("recieve");
    push("seperate");
    push("absinth");
    push("bazaar");
    push("the bazaar was bizarre");

    // Speller traps (curly contractions, em-dash joins, short fragments).
    push("weren’t didn’t doesn’t");
    push("door—then quiet—too");
    push("Dr. Jane");
    push("e.g. i.e. etc.");

    v
}
