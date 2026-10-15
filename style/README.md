# writemorebetter-style

Whole-document prose analysis for the reports tracked in #241. Engine code is MIT OR Apache-2.0.
The crate bundles no dictionaries, rule resources, phrase data, baseline numbers or prose. It uses
one caller-owned `nlprule::Tokenizer` and its separately loaded data.

```rust
let document = writemorebetter_style::analyze_document(
    &tokenizer,
    &["A heading", "“First paragraph.", "“Second paragraph.” She left."],
);
// Future reports consume &document; they do not tokenize again.
let occurrences = document.lemma_index.get("leave");
```

Swift uses `checker.analyzeDocument(paragraphs: [...])`; JS uses
`JSON.parse(checker.analyzeDocument([...]))`. Both reuse the checker's loaded tokenizer.

## Document contract

- Input strings define the paragraphs, including empty paragraphs. Internal newlines and U+2028
  remain inside their input paragraph. No sentence is allowed to merge across input paragraphs.
- `Document.text` joins paragraphs verbatim with one LF between them, adding no terminal LF.
  All spans are absolute and half-open into that text. Paragraph spans exclude separators.
- Rust retains byte, Unicode scalar and UTF-16 spans. Swift and JSON expose only UTF-16 `start`
  and `end`, so platform string slicing works directly. Sentences and tokens use the #242 API.
- `lemma_index` maps exact, case-sensitive lemmas to zero-based `(paragraph, sentence, token)`
  positions. Whitespace and punctuation are excluded. Several POS readings with the same lemma
  count a token once; genuinely ambiguous lemmas each get a position. Unknown tokens have no
  inferred lemma. This index is an analysis aid, not a claim that ambiguity has been resolved.
- Each call builds a fresh snapshot. Run it on idle/save and replace the previous snapshot after
  edits. There is no serializable incremental-state invalidation protocol in this version.

This selects the whole-document/on-demand strategy from #234. Its grammar frequency rules remain
in `nlprule`; this crate does not duplicate them or change per-paragraph `check()` behavior. A future
frequency-rule adapter can consume positions/counts from this snapshot; #234 is not resolved here.
Report options and caller-supplied baselines belong to the reports that need them, rather than an
unused options object in the document builder.

## Quoted dialogue

`dialogue_spans` contains disjoint outer quoted ranges, including the quotation marks. It recognizes
curly double/single and straight double/single pairs with nesting. Repeated opening marks at the
first non-whitespace position of a later paragraph continue the same quotation. An unmatched
opening extends to document EOF; an unmatched curly closing mark is ignored. Nested quotation
marks do not double-count their content. `Document::is_dialogue` tests span overlap.

Apostrophes inside tokenizer-identified contractions and dropped-g words, internal word apostrophes,
and unpaired trailing possessives are excluded. When an unprotected single mark could either close
an open single quotation or be a possessive, closing the quotation takes precedence. Matching uses
a stack: a mismatched closing mark does not repair an inner quotation. Quotation detection is a
procedural heuristic; it also counts quoted thoughts, scare quotes and cited text, and does not
claim to infer whether a character is speaking.

## Verification and performance

```sh
cargo test --release -p writemorebetter-style
cargo build --release -p writemorebetter-style --example document
NLPRULE_PARALLELISM=off /usr/bin/time -l target/release/examples/document \
    storage/en_tokenizer.bin bench/public/moby150k.txt
# Ten copies of the public-domain excerpt provide a reproducible novel-length input.
NLPRULE_PARALLELISM=off /usr/bin/time -l target/release/examples/document \
    storage/en_tokenizer.bin bench/public/moby150k.txt 10
```

Initial native release budget on the development Apple Silicon machine: 2 seconds for 150 KB and
20 seconds for a 1.5 MB novel-length input, excluding tokenizer load, with pipeline parallelism off.
This is an idle/save operation; the existing pipeline's per-sentence parallel scheduling is more
expensive on this workload (see [analysis measurements](../docs/ANALYSIS.md)). These budgets do not
claim browser or mobile performance. The example prints counts and timings only.

Measured 2026-10-14 on an Apple M5 Pro with release Rust 1.98, English 6.6 data and `NLPRULE_PARALLELISM=off`, after
other builds finished. The example treats blank lines as paragraph separators, preserving internal
hard wraps; the resulting document joins those paragraphs with LF.

| Input | Document bytes | Sentences | Tokens including whitespace | Analysis | Peak RSS |
|---|---:|---:|---:|---:|---:|
| Moby-Dick excerpt | 147,687 | 1,408 | 55,766 | 0.823 s | 62.5 MiB |
| Ten copies, novel-length stress input | 1,476,879 | 14,080 | 557,660 | 8.314 s | 250.1 MiB |

Tokenizer load was 0.056–0.061 s. Both runs meet the initial time budget. Peak RSS includes the
tokenizer and owned document, but excludes grammar/spelling data. The snapshot's memory grows
with token count. Repeating an excerpt is a throughput/memory test, not quotation-accuracy evidence;
its unmatched opening quotes can continue across copies. Correctness uses the annotated fixtures.

See [the comparison plan](../docs/STYLE-BENCHMARKS.md) for spaCy, BookNLP, prose linters and
ProWritingAid comparisons. Those comparisons have not yet been run.

The published-fiction EPUB corpus and its ingest/export pipeline remain local to the separate
`fiction-baseline` project. No extracted text is needed for these tests or benchmarks. Baseline
aggregation should preserve that project's author weighting and reviewed-catalog filtering.

## Sentence reports (#244)

```rust
use writemorebetter_style::reports::sentences::{report, Options};
let reports = report(&document, &Options::default())?;
```

`reports.sentences` retains sentence spans, word counts, dialogue/narration word counts, opening
word, opening category, context and a `mixed` flag. Word counts group adjacent contraction tokens
and words linked by an internal apostrophe or ASCII hyphen. Em dashes separate words; emoji and
punctuation do not count as words. Sentences with no lexical words are excluded.

`lengths.narration` and `lengths.dialogue` contain exact word-count histograms, sentences **longer
than** `long_sentence_words` (default 40), and monotony runs. A monotony run has at least four
consecutive sentences whose maximum and minimum word counts differ by at most two. Runs are
maximal, can overlap, and do not drift by comparing only adjacent lengths. The implementation is a
linear sliding-window pass, checked against exhaustive short sequences.

`starts.narration` and `starts.dialogue` contain counts and spans for subject, adverb, gerund,
infinitive, subordinating conjunction, coordinating conjunction, past participle, inversion,
introductory, and unclassified openings. They also contain runs of three or more identical opening
words, compared case-insensitively with curly apostrophes normalized. Both kinds of run cross
paragraph boundaries but reset when the sentence's context changes.

A sentence's context is the context of its first lexical word. A sentence combining dialogue and
attribution is counted once, with `mixed = true`; its words are also counted separately by their
quotation overlap. A narration-first sentence containing a later quotation remains a narration
sentence. This convention avoids double-counting one grammatical sentence in both distributions.

Classification uses disambiguated Penn-style readings, with this precedence: explicit introductory
words/phrases; TO + VB; a small subordinating-conjunction list; CC; VBG; a VBN participial opener;
verb + subject inversion; RB; noun phrase/pronoun; introductory preposition; unclassified. Participial
and inversion patterns use nearby tags and punctuation to resolve some surviving ambiguity. Likely
imperatives with ambiguous noun/verb readings remain unclassified. This is a deterministic heuristic,
not a dependency parse; ambiguous openings can still be misclassified. The acceptance fixtures have
five invented examples per requested category, including dialogue in each category.

Each context's shares and baseline differences are `None` below `minimum_sentences` (default 200).
Counts and spans remain available. Empty contexts never return shares, even if the minimum is zero.
Baseline differences are actual minus expected **fractions**, not percentages. Baselines are optional,
may cover only selected buckets, and must be finite shares in [0,1] summing to at most one. Narration
and dialogue have independent baselines; absent length buckets are returned with zero count when
present in the baseline. No numeric comparison baseline is bundled.

Swift's `try checker.sentenceReports(paragraphs: ..., optionsJson: nil)` and JavaScript's
`JSON.parse(checker.sentenceReports(paragraphs, undefined))` return the same report JSON. This
report API retains Rust's full span shape: use `span.utf16.start` and `span.utf16.end` for `NSRange`
and JS slicing. All spans index the input paragraphs joined with one LF. Options are optional JSON:

```json
{
  "long_sentence_words": 40,
  "minimum_sentences": 200,
  "start_baseline": {
    "narration": {"subject": 0.6},
    "dialogue": {"subject": 0.7}
  }
}
```

The numeric values above only illustrate the input format. They are not corpus measurements.
Unknown option fields and invalid values return errors. Both reports reuse a single fresh document
snapshot within the call; Rust callers with an existing snapshot call `report` directly.
