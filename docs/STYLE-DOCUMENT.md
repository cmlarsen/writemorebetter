# Whole-document analysis

`writemorebetter-style` supplies the shared document pass for prose reports. Its code is MIT OR
Apache-2.0 and contains no bundled linguistic data. It takes the application's already loaded
tokenizer, without running grammar or spelling checks.

```rust
use writemorebetter_style::{analyze_document, DocumentOptions};
let options = DocumentOptions::new().with_skip_names(vec!["Mara".into()]);
let document = analyze_document(&tokenizer, text, &options);
```

Public Rust records are `#[non_exhaustive]`; construct options with `new()`/`default()` and the builders
`with_skip_names`, `with_token_details`, and `with_hard_wrapped`. Reports take `&Document`.

## Paragraphs and offsets

The input is one original string. By default, each nonempty line is one paragraph: LF, CRLF,
standalone CR and U+2029 separate paragraphs; blank/whitespace-only paragraphs are omitted. U+2028
stays within a paragraph. This matches editor documents with one paragraph per line. Hard-wrapped
plain-text/Markdown imports must be unwrapped first or use `with_hard_wrapped(true)` (bindings:
`hard_wrapped` / `hardWrapped`). In that mode single line endings stay inside paragraphs and blank
lines or U+2029 separate them. The public Moby excerpt is hard-wrapped CRLF text: its default 2,308
paragraphs are lines, not literary paragraphs. The LitBank probe explicitly uses hard-wrapped mode.

Paragraph spans exclude separating line endings and preserve indentation/trailing spaces. Each
paragraph is analyzed independently, so headings and fragments cannot merge into later paragraphs.
Sentence/token spans retain the tokenizer's whitespace conventions and need not cover trailing
paragraph whitespace. All Rust spans contain absolute, half-open UTF-8 byte, Unicode scalar (`char`)
and UTF-16 ranges into the original input. Hard-wrapped mode preserves the input and all offsets.

## Counts, names and lemmas

A word token contains a Unicode letter or number (`char::is_alphanumeric`). Emoji, symbols and
punctuation do not count. This is a **token count**, including clitics: `doesn't` (`does` + `n't`),
`I'm` (`I` + `'m`), and `dog's` (`dog` + `'s`) each count as two words. It differs from whitespace-word
counts. Tier 3 uses the same word predicate/tokenization, while Ngram/HTRC include punctuation and
use different tokenization/weighting. Rates per 1,000 words must use a baseline calibrated with the
same convention; those published baselines are not interchangeable denominators.

Names match case-sensitively and accept their ALL-CAPS form. A lowercase entry also accepts the form
with its first character capitalized, following the user dictionary: `Will` skips `Will`/`WILL` but
keeps the modal `will`; `will` accepts all three. Curly/straight apostrophes and whitespace in names
are normalized. Multi-token names, including possessive entries with no spaces, match contiguous
whole token sequences within a sentence. An immediately adjacent possessive clitic (`'s`, `’s`,
`'`, `’`) following a skipped name is also skipped. No Unicode canonical normalization is implied.
Skipped tokens remain with `is_skipped`, but contribute neither to `word_count` nor `lemma_index`.

The index maps each distinct lemma to paragraph/sentence/token coordinates, including whitespace
in token indices. Duplicate readings of one lemma contribute one token position; different possible
lemmas each receive that position. Summing across ambiguous lemma keys would double-count words.
Unknown word tokens are indexed under their lowercase surface form and still count in the denominator.
This fallback is document statistics, not an added linguistic reading or an EWT accuracy convention.

Known engine gap #153 remains explicit: in `as soon as possible`, both `as` tokens can retain only the
phrase lemma `as soon as possible` instead of ordinary `as` readings. The index preserves those
readings; reports needing exact word-form frequencies must use token surfaces for this case. Selecting
a primary lemma/POS for ambiguous tokens is deferred to the report issues, not determined by reading
order here.

## Dialogue

Dialogue spans include delimiters and cover outer quoted regions. Nested quotes belong to the outer
region, without overlapping extra spans or speaker attribution. Curly/straight double quotes and
punctuated single-quoted speech are recognized. A single closer requires preceding speech punctuation
(`.!?,:;—…`), so unpunctuated single-quoted fragments and British punctuation-outside forms can be
missed. Internal/trailing apostrophes and common leading elisions (`'em`, `'tis`, `'twas`, `'n'`,
`'cause`, decade forms such as `'90s`/`'eighties`) do not open speech or serve as repeated
opening marks for a continued quote.

At each paragraph boundary, an unfinished quote continues only if the next nonempty paragraph begins
with the same opening mark. Otherwise the **entire unfinished span is dropped**, including any carried
prefix; the final unterminated paragraph is dropped too. Later paragraphs start with fresh state.
A malformed straight quote can still confuse pairing within its own paragraph. Scare quotes can
count as dialogue; dash-led speech is outside this detector's scope. All adversarial review probe rows
are checked in as invented fixtures under `style/tests/fixtures/review-dialogue.json`.

## Bindings and memory

Swift `Checker.analyzeDocument(text:options:)` and JavaScript `Checker.analyzeDocument(text, optionsJSON)`
reuse the loaded tokenizer. Their `start`/exclusive `end` offsets count UTF-16 units, suitable for
`NSRange`/JavaScript `slice`. Both options records default to including readings/chunks and line
paragraphs. Swift options fields have defaults; WASM accepts snake_case or camelCase option names
and rejects unknown keys instead of silently ignoring misspellings.

Set `include_token_details: false` (`includeTokenDetails: false` in Swift/camelCase JSON) to discard
per-token readings/chunks after constructing the index. WASM omits those JSON keys; Swift returns
empty arrays. Counts, lemma positions, text, spans and token flags remain identical. Reports needing
POS/chunk information must request full details. This reduces returned object/JSON size, but tokenization
and index construction still run. See [measured native/WASM novel-length time and memory](STYLE-DOCUMENT-BENCHMARKS.md).
Run document analysis off the UI thread. Spelling dictionary additions do not supply tokenizer readings.

## Shared statistics

This implements #234 option 1: rebuild owned statistics on save/idle, avoiding incremental invalidation.
Future `TOO_OFTEN_USED_*` integration can read `lemma_index[lemma].len()` and `word_count`, inspect POS
readings through `TokenPosition`, and return token spans. Its existing 100-word minimum, POS and
ambiguity conventions must be applied explicitly. Builtins and `suggest()` behavior remain unchanged.
No reports or replacement prose are added here.
