# Whole-document analysis

`writemorebetter-style` supplies the shared document pass for prose reports. Its code is MIT OR
Apache-2.0 and contains no bundled linguistic data. It takes the tokenizer already loaded by the
application; rules and spelling checks are not run.

```rust
use writemorebetter_style::{analyze_document, DocumentOptions};
let options = DocumentOptions { skip_names: vec!["Mara".into()] };
let document = analyze_document(&tokenizer, text, &options);
```

The input is one original string, preserving offsets and paragraph separators. LF, CRLF, standalone
CR and U+2029 separate paragraphs; blank and whitespace-only paragraphs are omitted. U+2028 remains
inside the paragraph. Paragraph spans exclude their separators and preserve indentation and trailing
spaces. Each paragraph is analyzed independently, so an unterminated heading or fragment cannot merge
with the following paragraph. Sentence/token spans retain the tokenizer's whitespace conventions;
they need not cover trailing paragraph whitespace. All Rust spans contain absolute, half-open UTF-8
byte, Unicode scalar (`char`) and UTF-16 ranges into the original input.

A token is a word if its surface contains a Unicode letter or number (`char::is_alphanumeric`).
Emoji, symbols and punctuation do not contribute to `word_count`. The name skip-list matches whole
token surfaces or contiguous multi-token names within a sentence case-insensitively, equating curly
and straight apostrophes and collapsing whitespace in names. Skipped tokens remain in
the document with `is_skipped`, but contribute neither to `word_count` nor to `lemma_index`. Unknown
word tokens count in the denominator even when they have no readings. The lemma index retains the
engine's lemma spelling and maps to paragraph/sentence/token coordinates, including whitespace token
indices. Multiple readings of the same lemma contribute one position; different possible lemmas each
receive that token's position. Summing counts across ambiguous lemmas would double-count words.

Dialogue spans include delimiters and cover outer quoted regions. Nested quotes are part of their
outer region, rather than extra overlapping spans; there is no speaker attribution. Curly and straight
double quotes and punctuated single-quoted speech are recognized. A single closing quote requires
preceding speech punctuation (`.!?,:;—…`); ambiguous word-final marks remain apostrophes, so
unpunctuated single-quoted words are deliberately not classified as dialogue. A repeated opening quote at the beginning
of a subsequent line continues the same unfinished speech. An unmatched opener produces no span.
Apostrophes inside words and unpaired trailing apostrophes are excluded. These are procedural quotation
spans: scare quotes may count, and dash-led dialogue is outside this detector's scope.

This implements #234 option 1: rebuild owned document statistics on save or idle, avoiding incremental
state and invalidation. Future `TOO_OFTEN_USED_*` integration can read `lemma_index[lemma].len()` and
`word_count`, inspect candidate POS readings via `TokenPosition`, and return source token spans. Its
existing 100-word minimum and POS/ambiguity conventions must be applied explicitly; raw index totals
must not be summed across ambiguous lemmas. The current builtins and `suggest()` behavior are unchanged.
Future reports take `&Document` and share this pass.

Swift and WASM expose the same call on `Checker`, reusing its tokenizer. Binding `start`/exclusive
`end` offsets count UTF-16 units, matching `analyze()`; use `NSRange` or JavaScript `slice`. A document
call does not update the spelling user dictionary, and dictionary additions do not supply readings.
Run document analysis off the UI thread.

The invented hand-annotated fixture in `style/tests/fixtures/document.json` covers paragraph/sentence
boundaries, nested quotes, continued speech, apostrophes, emoji, em dashes, ellipses and U+2028.
Additional tests exercise three-offset slicing, empty input, paragraph separators, word denominators,
name exclusion and duplicate readings. Benchmark results and reproduction commands are recorded in
[STYLE-DOCUMENT-BENCHMARKS.md](STYLE-DOCUMENT-BENCHMARKS.md).
