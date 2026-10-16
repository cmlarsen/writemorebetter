# #256 requirement audit

Audited main: `d0fe086f97cb38752a3eb7f99e7959eb1c3b48b0` (2026-10-16).
PR #259 delivered the initial three-tier harness. This audit includes every tier and gate bullet
from #256; additions here concern #248/#250 and calibration, while #247/#251 own their parallel suites.
CI evidence below is job `english` (displayed as English), steps **Style benchmark accuracy gates**
and **Style lexical report accuracy gates (256)**. “Owner owed” is deliberately not a completion claim.

| Requirement | Main evidence / gap | Result after this PR |
|---|---|---|
| Tier 1: UD English-EWT sentence-boundary F1, Penn XPOS, lemma accuracy | `tier1.py`, `equivalence.py`, `tier1-baseline.json`; CI `tier1 --gate` | Done; existing fixed test floors retained |
| Tier 1: LitBank direct-speech exact-span F1 | `tier1.py`, `splits.json`; CI same gate | Done; existing product dialogue span floor retained |
| Tier 1: QuoteLi3 second quotation set, confirm original license | `README.md` Download provenance; Stanford release has no confirmed license | Explicitly excluded pending owner license confirmation; third-party card is not permission |
| Tier 1: download at bench time, pinned tag/commit + SHA-256, no vendored dataset | `common.py:fetch`, `sources.json`, ignored `.cache/` | Done; checksum checked on cache hits too |
| Tier 1: align tokens by character offset despite tokenization differences | `tier1.py` exact scalar endpoint alignment and concatenating multiword-token offsets; coverage floors | Done |
| Tier 1: fixed dev/test, tune dev only, report test | EWT upstream split; LitBank `splits.json`; baseline records test | Done for harness; README discloses later development reuse of report-label test sets |
| Tier 1: exclude GUM and PDNC | README exclusion/license explanations | Done |
| Tier 2: injected Standard Ebooks CC0 prose, pinned and fetched at bench time, generator + answer key | `tier2.py`, `sources.json`, existing report adapters | Done; new `lexical.py` uses same pinned generator and shared scorer |
| Tier 2: echoes | `echoes.py`, `echoes-baseline.json`; CI `echoes.py --gate` | Done |
| Tier 2: repeated phrases | Same echo adapter/baseline, separate phrase cases and categories | Done |
| Tier 2: clichés | `cliches.py`, `tier2-cliches-baseline.json`; CI `cliches --gate` | Done |
| Tier 2: sentence length and sentence-start runs/categories | `sentences.py`, `sentence-floors.json`, `sentence-measurements.json`; CI `tier2 sentences --gate` | Done; injection and labeled regression suites |
| Tier 2: dialogue tags | `dialogue_tags.py`, `dialogue-tags-baseline.json`; CI `dialogue-tags tier2` | Done; injected + model-labeled suites |
| Tier 2: overused words (#248) | No report accuracy suite (the similarly named `overused_baseline.py` builds the reference data) | Added `lexical.py`, `lexical_reports.rs`, `lexical-baseline.json`; new CI step. Covers both halves of the report: overused lemmas (five-occurrence boundary positives, four-occurrence and below-ratio controls) and fillers (*very*, *just*, clause *that*, with adjective/determiner controls) |
| Tier 2: sensory balance and diction (#250) | No report accuracy suite | Added per-word diagnostics and the reports' flags: six modalities plus figurative, flat-adverb, excluded-lemma and sight-margin controls; single-sense scenes (90% positive, 80% and below-minimum controls; removed with the flag in #282); abstract sentences and abstract runs (near-threshold, dialogue, two-rated-word and broken-run cases) with a caller-supplied invented table; new CI step |
| Tier 2: transitions / pronoun starts (#247) and pacing (#251) | Not merged on audited main | Separate active lanes; this PR does not add their suites |
| Tier 2: recall = planted patterns found; precision = no new flags on untouched text | Shared `tier2.py:score` and report adapters; legacy smoke uses toy detector | Every prediction on the untouched public paragraphs is frozen (per-category counts + SHA-256) and gated for equality, so natural-text precision regressions fail; paired prefixes are compared as multisets and all suffix predictions are scored; targeted negative controls for each filter and threshold; duplicate/wrong-label false positives. all 21 report mutations tried fail the tier 2 gate (see README) |
| Tier 2: about 300 sentences per judgment report, offsets + JSONL, two independent labelers + owner adjudication | 332 sentence-start rows: two models/coordinator; 34 modern rows: one model; 238 dialogue records over 186 spans: two model sessions/coordinator; no transitions/pacing sets | Owner owed: independent hand labels/owner adjudication and acceptance, held-out evaluation; see README inventory; no fabricated second labeler |
| Tier 2: per-category precision, recall, F1 and confusion matrix | `tier2.py:score`, existing suite baselines | Done for added suites too, full test/dev matrices committed |
| Tier 3: every report over export units, path argument, local only | `tier3.py`, `flag_rates.py`, `dialogue_tags.py`; old flag-rate probe covers six reports | Extended `flag_rates.rs`/`.py` to all nine merged reports; real diction requires caller table |
| Tier 3: per-report distributions across authors, share of authors flagged | Author-pooled manuscript distributions plus equally weighted chapter windows in `flag_rates.py` | Done for new counts, six modality counts and rated-word-weighted diction means |
| Tier 3: gaps versus Ngram/HTRC baselines (tagger mismatch) | `tier3.py` universal-start shares and supplied `published-baseline.json`; `flag_rates.py` shipped length/start comparisons | Existing comparisons retained. Overused words has no separate gap measurement: the report itself is defined relative to the bundled Ngram lemma rates, and tier 3 emits flag counts, not a measured-vs-Ngram distribution. Sensory norms and diction have no Ngram/HTRC comparison by design |
| Tier 3: never print text, owner runs real private inputs, Codex never does | Numeric-only output, key allowlist, stderr/runtime/argument suppression in `run.py` and `flag_rates.py`; synthetic `--self-test` CI | Done; extended output uses fixed numeric keys, no lemmas/table vocabulary; no private input accessed |
| Gates: tiers 1 and 2 in CI, first measured floors must not drop | Existing accuracy step gates six reports | Added new lexical CI step: test-split P/R/F1 floors per category, gold and case coverage, paired-prefix count and frozen natural-paragraph predictions |
| Gates: tier 3 reported in each report PR | Owner-supplied prior aggregate provenance documented in README; new report calibration absent | Owner owed: actual new-report calibration, diction table choice, publication/acceptance of aggregate numbers; only synthetic tests here |
| Gates: baselines in STYLE-REPORTS or linked benchmark docs | `docs/STYLE-REPORTS.md` Accuracy benchmarks links README | Done; added lexical numbers and owner-debt summary |
| First PR: tier 1 harness/scores, tier 2 generator/key/scorer smoke, tier 3 skeleton | PR #259; `tier1.py`, `tier2.py`, `tier3.py`, CI | Done before this PR; retained |

#256 remains open because the owner-owed license/label/calibration decisions above are unresolved.
All executable work for the nine reports merged on audited main is covered without accessing private
inputs or changing `style/src/reports/`.
