#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let (tokenizer, rules) = wmb_fuzz::engine();
    let suggestions = rules.suggest(text, tokenizer);
    let _ = nlprule::rules::apply_suggestions(text, &suggestions);
});
