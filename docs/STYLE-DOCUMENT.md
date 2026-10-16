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

Besides paragraphs, `dialogue`, `lemma_index` and `word_count`, the document records how it was built
and what every report needs to agree:

- `speech: Vec<bool>`, parallel to `dialogue`: whether each quoted span is speech (see
  [Narration and dialogue](#narration-and-dialogue)).
- `token_details`: whether token readings and chunks were kept. Reports that need readings (echoes,
  repeated phrases, sentence starts, dialogue tags) return `StyleError::NeedsTokenDetails` for a
  compact document instead of silently degrading.
- `hard_wrapped`: the paragraph mode the document was built with. The combined report result echoes
  it as `stats.hardWrapped`.
- `sentences()` and `words()` iterate every sentence and every counted word token in document order;
  `Sentence::word_count()` counts `is_word && !is_skipped` tokens. Sentence indexes in reports are
  positions in `sentences()`.

The Rust `Document` serializes with snake_case keys, like the shipped `analyzeDocument` JSON, which is
unchanged. Report records serialize camelCase (see [STYLE-REPORTS.md](STYLE-REPORTS.md#conventions)).

## Paragraphs and offsets

The input is one original string. By default, each nonempty line is one paragraph: LF, CRLF,
standalone CR and U+2029 separate paragraphs; blank/whitespace-only paragraphs are omitted. U+2028
stays within a paragraph. This matches editor documents with one paragraph per line. Hard-wrapped
plain-text/Markdown imports must be unwrapped first or use `with_hard_wrapped(true)` (bindings:
`hard_wrapped` / `hardWrapped`). In that mode single line endings stay inside paragraphs and blank
lines or U+2029 separate them. The public Moby excerpt is hard-wrapped CRLF text: its default 2,308
paragraphs are lines, not literary paragraphs. The LitBank probe explicitly uses hard-wrapped mode.

Paragraph spans exclude separating line endings and preserve indentation/trailing spaces. Each
paragraph is analyzed independently. Recognized Chapter/Book/Part headings, Markdown headings,
all-capital short lines without sentence punctuation, scene-marker lines, and marked epigraph
citations are excluded from every report. In hard-wrapped mode these lines remain separate before
filtering. Other short fragments are retained: an arbitrary unpunctuated sentence cannot reliably
be distinguished from a title.

Paragraphs record `scene_break_before` and `chapter_start`. Blank lines in line mode, two blank
lines in hard-wrapped mode, U+2029 and excluded heading/marker lines end narration runs. Ordinary
paragraphs remain separate but do not end runs. U+2028 stays inside the paragraph and ends a
sentence after terminal punctuation. Bare speech attributions split after a closing quote are
reattached to that speech sentence; all offsets still point into the original text.
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
keeps lowercase `will`; `will` accepts all three. Matching uses spelling and case, not POS: with
`skip_names: ["Will"]`, sentence-initial `Will you come?` and shouted `I WILL NOT GO` also skip the
modal. A tagger-aware distinction is deferred. Curly/straight apostrophes and whitespace in names
are normalized. Multi-token names, including possessive entries with no spaces, match contiguous
whole token sequences within a sentence. A preceding common honorific (Mr., Mrs., Ms., Dr., M., Monsieur, Madame and related forms) is
skipped with a supplied surname, including its intervening period and whitespace. An immediately adjacent possessive clitic (`'s`, `’s`,
`'`, `’`) following a skipped name is also skipped. No Unicode canonical normalization is implied.
Skipped tokens remain with `is_skipped`, but contribute neither to `word_count` nor `lemma_index`.

The index maps each distinct lemma to paragraph/sentence/token coordinates, including whitespace
in token indices. Duplicate readings of one lemma contribute one token position; different possible
lemmas each receive that position. Summing across ambiguous lemma keys would double-count words.
Unknown word tokens and tokens whose readings all have whitespace-containing lemmas are also indexed
under their lowercase surface form and still count in the denominator. This fallback is document
statistics, not an added linguistic reading or an EWT accuracy convention. A multiword lemma is indexed
once per occurrence, at the first token carrying that reading, rather than once per covered token.
Thus `By the way` retains `by`/`the`/`way` surface entries, and `Herman Melville` retains
`herman`/`melville` alongside one `Herman Melville` phrase entry. Standalone proper-noun readings
can keep their original case: `Melville wrote. Herman Melville wrote. Herman spoke.` indexes standalone
`Melville`/`Herman` separately from covered-token `melville`/`herman`. Reports combining these name
frequencies must explicitly merge case variants; the index does not unify them. Such merging can
also conflate homographs, so each report must choose its convention.

Known engine gap #153 remains explicit: in `as soon as possible`, both `as` tokens have no readings;
`soon` and `possible` carry only the phrase lemma `as soon as possible`. Their surface fallbacks
preserve both `as` occurrences and the `soon`/`possible` entries. The phrase entry currently points to
`soon`, its first reading-bearing token; it will point to the leading `as` if #153 restores that
reading. Original token readings remain unchanged. Selecting a primary lemma/POS for ambiguous tokens
is deferred to the report issues, not determined by reading order here.

## Dialogue

Dialogue spans include delimiters and cover outer quoted regions. Nested quotes belong to the outer
region, without overlapping extra spans or speaker attribution. Curly/straight double quotes and
punctuated single-quoted speech are recognized. A single closer requires preceding speech punctuation
(`.!?,:;—…`), so unpunctuated single-quoted fragments and British punctuation-outside forms can be
missed. Internal/trailing apostrophes do not open speech. Straight single openers before a word are
tentative, including listed and unlisted elisions (`'em`, `'tis`, `'twas`, `'n'`, `'cause`, `'im`, `'90s`).
A tentative opener produces a span only with a punctuated single closer in the same paragraph.
A later double opener takes precedence, but the tentative single outer is recovered if its
punctuated closer follows later in that paragraph; the outer span then replaces the inner double
spans it covers. This preserves British-style `'He said "no," then left,'` while still recovering
`"Stop,"` after `She saw 'im go.` when no single closer follows.
A later same-mark single opener before a word replaces the tentative opener unless the new mark
begins a listed elision. Thus `'Give 'em hell,'` retains its outer span, and `She saw 'im go. 'Stop,'`
recovers `'Stop,'`. Unlisted elisions inside speech remain ambiguous: `'Tell 'im I said so,'` is
truncated to `'im I said so,'` by this recovery rule. Directional left single quotes are ordinary
openers unless their first word is a listed elision (for example `‘Tis`). Tentative openers, including
those temporarily superseded by a double opener, never continue into another paragraph. Consequently,
straight-single speech continued across paragraphs loses its unfinished prefix; only the final
closed paragraph is detected.

At each paragraph boundary, an unfinished quote continues only if the next nonempty paragraph begins
with the same opening mark (excluding listed elisions). This accepts the standard continued-speech
convention, but can join unrelated paragraphs after a missing closer, as in `"Unclosed⏎"Reply," he said.`
Otherwise the **entire unfinished span is dropped**, including any carried prefix; the final
unterminated paragraph is dropped too. Later paragraphs start with fresh state.
A malformed straight quote can still confuse pairing within its own paragraph. Scare quotes are
dialogue spans here and are classified as non-speech below; dash-led speech is outside this
detector's scope. All adversarial review probe rows are checked in as invented fixtures under
`style/tests/fixtures/review-dialogue.json`.

## Narration and dialogue

Every report uses one definition, computed once in the document pass. A dialogue span is **speech**
unless one of two tests marks it as a quoted term:

- **Embedded:** the token right before the quote's opening mark, in the paragraph where it opens, is a
  word other than a lead-in verb (a listed speech or non-speech verb outside
  `style/data/dialogue/object-verbs.txt`). `He called it “grub” and ate.`, `She pressed “Send.”`,
  `She said that “honesty is the best policy” was…` and titles after a determiner are not speech.
- **Copula subject:** the quote closes and the next token starts with a lowercase form of *be*,
  *have* or *do*, and no lead-in verb precedes it: `“Yes” was the answer.`

The tests need POS readings, so they run before a compact document discards them; compact and full
documents have identical `speech`. A span continued across paragraphs is classified once, by its
opening and closing paragraphs. A following listed attribution keeps speech after a conjunction
(`—and ‘better safe than sorry,’ he said`) in dialogue. Comma/exclamation/question endings exempt
passive reported speech from the copula test (`“Go,” was the answer.`); explicit title-noun contexts
still suppress titles. A cry/shout followed by a punctuated quote also counts as speech.

On the public hard-wrapped Moby excerpt, #271 counted 183 of 196 quoted spans as speech. The
usefulness fix restores the four reported attributions, including the Bulkington cry; nine titles,
inn signs and quoted descriptions remain narration. This intentionally does not classify all 196
quoted spans as speech.

There are two granularities, both built on that one rule:

- **Token:** `Document::mode_at(byte)` is `Mode::Dialogue` when the byte lies inside a speech span.
  Word splits (`stats.words` in the combined result, clichés `groups.*.words`) and per-match modes
  (clichés) use it.
- **Sentence:** `Document::sentence_mode(&sentence)` is the token mode of the sentence's first
  `is_word` token. Sentence length and sentence starts group sentences by it, so a mixed
  narration/speech sentence belongs wholly to its opening mode.

Echoes and repeated phrases ignore mode. The dialogue-tag report reads speech spans as its quotes.
`style/tests/cross_report.rs` checks on shared passages that every report's word, sentence and
dialogue counts agree with this definition.

## Bindings and memory

Swift `Checker.analyzeDocument(text:options:)` and JavaScript `Checker.analyzeDocument(text, optionsJSON)`
reuse the loaded tokenizer. Their `start`/exclusive `end` offsets count UTF-16 units, suitable for
`NSRange`/JavaScript `slice`. Both options records default to including readings/chunks and line
paragraphs. Swift options fields have defaults; WASM accepts snake_case or camelCase option names
and rejects unknown keys instead of silently ignoring misspellings.

Set `include_token_details: false` (`includeTokenDetails: false` in Swift/camelCase JSON) to discard
per-token readings/chunks after constructing the index and classifying speech, paragraph by
paragraph. WASM omits those JSON keys; Swift returns empty arrays. Counts, lemma positions, speech
flags, text, spans and token flags remain identical. Reports needing POS/chunk information reject a
compact document with `StyleError::NeedsTokenDetails`. This reduces returned object/JSON size, but
tokenization and index construction still run. The speech classification keeps one paragraph's
readings until that paragraph is classified: on the hard-wrapped Moby excerpt compact peak RSS went
from 59.5 to 62.7 MB (native, shared machine). See [measured native/WASM novel-length time and memory](STYLE-DOCUMENT-BENCHMARKS.md).
Run document analysis off the UI thread. Spelling dictionary additions do not supply tokenizer readings.

## Shared statistics

This implements #234 option 1: rebuild owned statistics on save/idle, avoiding incremental invalidation.
Future `TOO_OFTEN_USED_*` integration can read `lemma_index[lemma].len()` and `word_count`, inspect POS
readings through `TokenPosition`, and return token spans. Its existing 100-word minimum, POS and
ambiguity conventions must be applied explicitly. Builtins and `suggest()` behavior remain unchanged.
No reports or replacement prose are added here.
