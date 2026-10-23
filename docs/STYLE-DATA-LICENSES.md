# Style data license checks

Licenses checked on 2026-10-15 from each source itself: the repository's license file at a pinned
commit, the OSF license record, or the authors' and publisher's pages.

## Phrase lists (#246)

| Candidate | Repository and commit | License file | Decision |
|---|---|---|---|
| no-cliches | [duereg/no-cliches](https://github.com/duereg/no-cliches) `61d6c543b971d72f9f559b99d2452368c35bac41` (v0.3.6) | `LICENSE`: MIT, © 2021 Matt Blair | vendored |
| retext-redundant-acronyms | [retextjs/retext-redundant-acronyms](https://github.com/retextjs/retext-redundant-acronyms) `e670637aa13a223f7d6505349f4a474695e2c115` (v5.0.0) | `license`: MIT, © 2016 Titus Wormer | vendored |
| proselint | [amperser/proselint](https://github.com/amperser/proselint) `dbed789caae662d06c7c8a5a13dd31f1acd36f5c` | `LICENSE.md`: BSD-3-Clause, © 2014–2015 Suchow, Pacer and Ross | `hell.py` and `ras_syndrome.py` only |
| write-good | [btford/write-good](https://github.com/btford/write-good) `6940b034c6f5a7e101c01a24d651a778fc3fe435` (master; package version 1.0.8, the v1.0.8 tag is `f4bd563`) | `LICENSE`: MIT, © 2014–2019 Brian Ford | nothing to vendor |

- write-good has no cliché or redundancy list of its own. Its cliché check is the `no-cliches`
  dependency; its other lists (wordiness, weasel words, E-Prime) are outside #246.
- `dunckr/no-cliches` is a 2016 GitHub fork of `duereg/no-cliches` (frozen at 0.1.0); npm's
  `repository` field names `duereg/no-cliches`, so that is the vendored upstream.
- proselint's repository is BSD-3-Clause, but its own source names a third-party origin for most of
  its data files. A relicense by proselint does not clear the original authors' rights, so those were
  dropped:

  | proselint file | Entries | Origin named by proselint | Origin license |
  |---|---:|---|---|
  | `cliches/write-good` | 697 | write-good → no-cliches | MIT (duplicate of no-cliches) |
  | `cliches/garner`, `redundancy/garner`, `garner` regexes | 79 + 88 + 2 | *Garner's Modern American Usage* | all rights reserved |
  | `cliches/diction` | 27 | GNU diction | GPL-3.0 (`COPYING`, diction-1.11) |
  | `redundancy/after-the-deadline` | 361 | After the Deadline | GPL-2.0 (`LICENSE.txt`, Automattic/atd-server-next `15b4a25`) |
  | `wallace`, `nordquist` in `redundancy/misc.py` | 2 + 4 | David Foster Wallace; Richard Nordquist | not stated |
  | `cliches/hell.py`, `redundancy/ras_syndrome.py` | 1 + 20 | none | proselint, BSD-3-Clause: **vendored** |

## Word norms (#250)

Licenses checked 2026-10-15 (recorded in #246, below). #250 vendors one derived table:

| File | Source | Version | License | Retrieved |
|---|---|---|---|---|
| `style/data/norms/lancaster-perceptual.tsv` | Lancaster Sensorimotor Norms, OSF project <https://osf.io/7emr6/>, Data component <https://osf.io/rwhs6/>, file `Lancaster_sensorimotor_norms_for_39707_words.csv` (<https://osf.io/download/48wsc/>) | the 39,707-word aggregate release accompanying Lynott et al. (2020); sha256 `445d363fb1f9f3e50b86d88e2f46cdc9a22b5dd8a713ce4e7be2a773d57f43c5`, 17,196,336 bytes | CC BY 4.0 | 2026-10-16 |

- **How it was obtained.** On 2026-10-16 the OSF API and download routes returned HTTP 500 or timed
  out for several hours. The CSV was taken from a verbatim copy in
  [BergelsonLab/BlindToddlerVocabulary](https://github.com/BergelsonLab/BlindToddlerVocabulary/blob/d8db5afa7e480b0eaadc01f8df83d6007568a766/data/Norms/Lancaster_sensorimotor_norms_for_39707_words.csv)
  (commit `d8db5afa`, added 2023-01-03), whose size matches OSF's published listing for `48wsc`.
  `scripts/build-lancaster-norms.py` refuses any input with a different sha256. Re-verify against
  OSF's own download when it is reachable.
- **OSF recheck (2026-10-16, #281).** OSF was still unreachable from the build machine:
  `https://osf.io/download/48wsc/` timed out after 90 s and `https://api.osf.io/v2/files/48wsc/`
  returned HTTP 500, so the pinned sha256 could not be compared with OSF's own copy. The #281 review
  corroborated the mirror independently: concepticon/norare-data's `Lynott-2020-Sensorimotor.tsv`,
  downloaded by that project from `https://osf.io/48wsc/download`, agrees on all six perceptual
  means for every one of the 2,708 shared words, and a third-party listing of OSF's files gives the
  same 17,196,336-byte size. The BergelsonLab copy fetched again for #281 still has the pinned sha256.
- **OSF recheck (2026-10-16 20:12 and 20:57 UTC, #282).** Both times
  `https://osf.io/download/48wsc/` and `https://api.osf.io/v2/files/48wsc/` returned HTTP 503
  (and `https://osf.io/48wsc/download` at 20:57).
  The mirror was downloaded again and matches the pinned sha256
  `445d363fb1f9f3e50b86d88e2f46cdc9a22b5dd8a713ce4e7be2a773d57f43c5`
  (17,196,336 bytes). Direct OSF hash verification remains open.
- **OSF verification (2026-10-20 16:36 UTC, #289).** `https://osf.io/download/48wsc/` returned
  HTTP 200 with 17,196,336 bytes and sha256
  `445d363fb1f9f3e50b86d88e2f46cdc9a22b5dd8a713ce4e7be2a773d57f43c5`, identical to the pinned
  mirror copy. The vendored table is built from OSF's own file.
- **Changes** (recorded in the file header and NOTICE): single words only, lowercased; only words
  whose strongest perceptual modality rates ≥ 3.5 and leads the next by ≥ 0.5, not counting
  interoception as the next modality of a haptic word; columns reduced to that modality, its mean
  strength and that lead, in floored tenths. Decimal CSV arithmetic and flooring (#282) prevent
  subthreshold values from rounding into the defaults; exact 0.5 boundaries add six entries that
  binary floating-point subtraction previously lost. 11,456 of 39,707 entries; 193,102 bytes. The build is
  byte-reproducible from the pinned CSV.
  The raw CSV is not committed.
- **Concreteness** is writemorebetter's own table since #256; see
  [Concreteness (#256)](#concreteness-256). The earlier usefulness review used a proxy derived
  locally from the Lancaster norms, never Brysbaert's ratings.
- `style/data/norms/not-sensory.txt` is writemorebetter's own list (MIT OR Apache-2.0).

### License research (#246)

- **Brysbaert, Warriner & Kuperman (2014) concreteness ratings: CC BY-NC-ND 3.0, so not
  redistributable here.** The authors' release page (Center for Reading Research, Ghent University;
  the live site is retired, archived at
  <https://web.archive.org/web/20230925032140/http://crr.ugent.be/archives/1330>) says, directly under
  the Excel and text download links: "This work is licensed under a Creative Commons
  Attribution-NonCommercial-NoDerivs 3.0 Unported License." The article
  (<https://link.springer.com/article/10.3758/s13428-013-0403-5>) is not open access: its metadata gives
  `dc.rights` "2013 Psychonomic Society, Inc.", and its supplementary XLSX carries no other license. The
  OSF trial-level data node <https://osf.io/qpmf4/> has no license set. Brysbaert's table can only be a
  caller-supplied input; the bundled table is writemorebetter's own ([Concreteness (#256)](#concreteness-256)).
- **Lancaster Sensorimotor Norms (Lynott, Connell, Brysbaert, Brand & Carney, 2020): CC BY 4.0,
  redistributable with attribution.** The OSF project <https://osf.io/7emr6/> and its Data component
  <https://osf.io/rwhs6/> (`Lancaster_sensorimotor_norms_for_39707_words.csv`) both record "CC-By
  Attribution 4.0 International" (legal code <https://creativecommons.org/licenses/by/4.0/legalcode>);
  the open-access article (doi 10.3758/s13428-019-01316-z) is CC BY 4.0. The aggregate CSV is about
  17 MB, so a trimmed table (word plus the six perceptual columns) with a NOTICE entry is the likely
  form if #250 vendors it.

## Concreteness (#256)

`style/data/norms/concreteness.tsv` is writemorebetter's own table, licensed MIT OR Apache-2.0 like
the style crate. Owner decision (Caleb, 2026-10-20): build it from LLM ratings over our own fiction
lemma list, bundle it as diction's default, let a caller's table override it, and pay for no human
ratings for now. The research behind the decision is summarised below; the build, validation and
usefulness numbers are in [STYLE-NORMS.md](STYLE-NORMS.md#bundled-concreteness-table-256).

**How it was made.** `bench/style/concreteness/rate.py` sent the words, 50 per request in a seeded
shuffled order, to Claude models through the `claude` CLI on the owner's Claude Team subscription
(`claude -p`, no tools, a fixed system prompt, CLI 2.1.304 on macOS and 2.1.302 on Linux, default
temperature and effort), with
the instructions frozen in `bench/style/concreteness/prompt.txt` (sha256
`0317ef4beee2b11db9719e811af732839d40536bfaac54b1d20266922b990871`) before any scoring. The
instructions and their anchor words (*theory*, *although* / *spoon*, *brick*; *kick*, *fair*) were
written for this project and reused from the pilot; no wording or anchor comes from Brysbaert,
Warriner & Kuperman (2014) or from Martínez et al. (2025). Every reply, including rejected ones, is
committed under `bench/style/concreteness/raw/`, and `scripts/build-concreteness-norms.py` rebuilds
the table byte for byte from those files. Generation itself is not reproducible. The table uses
`claude-opus-5-5`, `claude-sonnet-5-5` and `claude-fable-5-1`, three samples each, all rated on
2026-10-20. `claude-haiku-5-5` was rated too and its files are committed, but the build excludes it
because its sample-to-sample Spearman (0.933) is below the 0.97 stability gate; it also needed 152
retries for malformed replies.

**What was never used.** The Brysbaert, Warriner & Kuperman (2014) ratings were not downloaded,
opened or compared against at any step: not for the word list, the prompt, the model choice, the
scale fit, the threshold or validation. No private fiction was sent to any model; the prompts carry
only lemmas from the bundled Ngram baseline.

**Glasgow Norms, for calibration and validation only.** `GlasgowNorms.xlsx` comes from OSF
(<https://osf.io/download/ud367/>, project <https://osf.io/42s65/>, sha256
`9e3295f4c804289fdd5fa36f2c5fe415826bef3d839702deedd51641b3ff9a39`, 1,159,865 bytes, retrieved
2026-10-20), whose licence field reads "CC-By Attribution 4.0 International". It is downloaded at
bench and build time into the git-ignored `bench/style/.cache`, pinned in
`bench/style/sources.json`, and never committed. A seeded 30% split of its single-word items fits
the linear map from the model consensus to the 1–5 scale; the other 70% is the held-out test set.
The held-out set was also scored on interim builds, one of them with Haiku in the consensus; the
Haiku exclusion rests only on the stability gate, and STYLE-NORMS.md records the interim numbers.
No Glasgow value is copied into the table. NOTICE credits the Glasgow Norms.

**Lancaster and WordNet, for QA only.** The QA ridge (Lancaster 11 dimensions plus WordNet noun
features, fitted on the calibration split) only chooses which words a person reviews and supplies
the convergent check. Its values are not blended into the table, so neither dataset's terms reach
the shipped file. The full Lancaster CSV (`osf.io/download/48wsc/`, the pinned sha256 above) and
Princeton WordNet 3.0's database files (<https://wordnetcode.princeton.edu/3.0/WNdb-3.0.tar.gz>,
sha256 `658b1ba191f5f98c2e9bae3e25c186013158f30ef779f191d2a44e5d25046dc8`) are downloaded into
the same cache and pinned the same way.

**Terms on the model outputs** (checked 2026-10-20):

| Provider | Terms | Operative text | Decision |
|---|---|---|---|
| Anthropic (Claude Team plan) | [Commercial Terms of Service](https://www.anthropic.com/legal/commercial-terms), effective 2025-06-17 | "Customer (a) retains all rights to its Inputs, and (b) owns its Outputs. Anthropic hereby assigns to Customer its right, title and interest (if any) in and to Outputs." Restriction: Customer may not "build a competing product or service, including to train competing AI models". | used: a lookup table is neither; the table is never used to train a model |
| OpenCode Go (`opencode-go/deepseek-v4-pro`, `opencode-go/muse-spark-1.3-contributor`) | [OpenCode Terms of Service](https://opencode.ai/legal/terms-of-service), effective 2026-08-15 | Output is assigned to the user, but "You will only use the Services for your own internal use, and not on behalf of or for the benefit of any third party", and users may not use the Services in a way that "automatically or programmatically extracts data or Output". | **not used**: a scripted run whose output ships in a public library falls under both clauses. The Go API key had also expired (`api_key_expired`), so headless runs failed |
| DeepSeek (upstream of the above) | [Open Platform Terms](https://cdn.deepseek.com/policies/en-US/deepseek-open-platform-terms-of-service.html), effective 2026-04-29 | "We assign any rights, title, and interests—if any—in the Outputs of the Services to you"; §8.1 requires disclosing that output is AI-generated | permissive, but reachable here only through OpenCode |
| Meta (Muse Spark, upstream of the above) | [Meta Model API terms](https://dev.meta.ai/legal/terms-of-service), updated 2026-10-09 | "you own all rights, title, and interests in and to your Inputs and Outputs"; the Geographic Use Policy bars making Outputs available in listed territories, and the Contributor tier trains on prompts and completions | not used (OpenCode terms; the geography clause leaves redistribution unclear) |

So the table is built from Claude models only, and the second-family check the research plan wanted
is not in this release. The table is labelled as AI-generated in its header and in NOTICE.

**Copyright.** The US Copyright Office (Part 2, January 2025) treats purely AI-generated material
as uncopyrightable. MIT OR Apache-2.0 covers the selection, the build and the adjudicated edits;
the numbers themselves may not be protectable.

**Licence research** (2026-10-20, read from each dataset's own page, OSF licence field, repository
licence or article):

| Dataset | Licence | Commercial | Redistribution | Size | Derived from Brysbaert? |
|---|---|---|---|---|---|
| Brysbaert, Warriner & Kuperman 2014 | CC BY-NC-ND 3.0 | no | no | ~40k lemmas | — |
| **Glasgow Norms** (Scott et al. 2019) | **CC BY 4.0** | yes | yes, with attribution | 4,682 words + 871 senses | no |
| Lancaster Sensorimotor Norms (Lynott et al. 2020) | CC BY 4.0 | yes | yes | 39,707 lemmas (a proxy, not concreteness) | word list only |
| MRC Psycholinguistic Database (CONC) | CC BY-NC-SA 3.0 | no | no | 8,228 entries | no |
| Muraki et al. 2023 (multiword expressions) | CC BY 4.0, data files no longer on OSF | in principle | in principle | 62,889 MWEs | design |
| Muraki & Pexman 2025 (verbs); Pexman et al. 2019 (BOI); Calgary SDP | none | no | no | 2,937–10,000 | partly |
| Paetzold & Specia 2016; Köper & Schulte im Walde 2017; Hollis et al. 2017; Thompson & Lupyan 2018; Charbonnier & Wartena 2019 | none or NC | no | no | 78k–3M | most yes |
| MEGA.HR-Crossling (Ljubešić et al. 2018) | CC BY-SA 4.0 | yes | share-alike | 100k forms | partly (tuning) |
| Tsvetkov et al. 2014; IBM Debater abstractness | ODbL 1.0; CC BY-SA 3.0 | yes | share-alike | 150k; 300k | no |
| Martínez, Brysbaert et al. 2025 (GPT-4o) | CC BY-NC-SA | no | no | 126k words | anchors |
| Conde et al. 2025 (8 LLMs on Glasgow words) | CC BY 4.0 | yes | yes | Glasgow's words only | no |
| Hugging Face concreteness re-uploads (MIT / Apache tags) | tags set by re-uploaders of Brysbaert | no | no | — | yes |
| Princeton WordNet 3.0 | WordNet licence (commercial use allowed with notice) | yes | yes, with notice | lexical resource | no |

## Word frequencies (#248)

- **Google Books Ngram, English Fiction 1-grams, version 20200217 ("v3"): CC BY 3.0, redistributable
  with attribution.** The dataset page
  <https://storage.googleapis.com/books/ngrams/books/datasetsv3.html> says, above the download links:
  "This compilation is licensed under a Creative Commons Attribution 3.0 Unported License" (linking
  <http://creativecommons.org/licenses/by/3.0/>). The file used is
  <https://storage.googleapis.com/books/ngrams/books/20200217/eng-fiction/1-00000-of-00001.gz>
  (940,045,192 bytes), retrieved 2026-10-16 without sign-up or click-through. It is not committed.
- `style/data/baselines/overused-words.json` is derived from it by `bench/style/overused_baseline.py`:
  per-lemma rates per million words for edition years 1990–2019, with British spelling variants
  merged into their American lemma, rounded to three significant digits.
  It holds counts, not text. Its source record names the dataset, version, URL, license, retrieval
  date and build rules, and NOTICE credits Google Books Ngram.

## Pacing cognition lemmas (#251)

`style/data/pacing/cognition.txt`, version 1 (2026-10-16), is an original authored list of
22 cognition/interior verb lemmas, including British and American spellings. It is MIT OR
Apache-2.0 with the engine, contains no third-party data, and is credited in NOTICE.

## Transition inventory (#247)

`style/data/transitions.tsv` is an original list written by this project's contributors in 2026,
licensed MIT OR Apache-2.0 like the style crate. No external word list, corpus or rule data was used.

## QuoteLi3 (#256)

**Decision: no license found.** The owner decided on 2026-10-16 to use QuoteLi3 for local/CI
benchmarking only ([recorded on #256](https://github.com/cmlarsen/writemorebetter/issues/256#issuecomment-6228890723)). Its annotations are downloaded at bench time, pinned by sha256 and never
committed or redistributed. Only the *Pride and Prejudice* annotations are used. *Emma* and *The
Steppe* are excluded because the paper says they were taken from the Columbia Quoted Speech
Corpus, which Columbia distributes under a non-commercial license (below). The coordinator applied
the UD-GUM precedent on 2026-10-16. QuoteLi3 is not public domain, and this project does not treat
it as licensed.

QuoteLi3 is the dataset of Muzny, Fang, Chang and Jurafsky, "A Two-stage Sieve Approach for Quote
Attribution", EACL 2017 (<https://aclanthology.org/E17-1044/>). All sources were checked on
2026-10-17 UTC, which was 2026-10-16 in US time.

| Where | What was checked | License text found |
|---|---|---|
| [Release page](https://nlp.stanford.edu/~muzny/quoteli.html) | Live page, `Last-Modified: Tue, 27 Apr 2021 21:08:47 GMT`. It reads "We release our quote/mention annotations for the texts of Pride and Prejudice, Emma, and The Steppe." It links the full XML, chapter zips, the P&P test set, CoreNLP and the annotator. | none, and no copyright notice or terms of use |
| Wayback Machine, `nlp.stanford.edu/~muzny/*` | CDX listing with digest collapse. `quoteli.html` has two distinct versions: [2020-09-20](https://web.archive.org/web/20200920082415/https://nlp.stanford.edu/~muzny/quoteli.html), before the test-set link, and [2021-08-05](https://web.archive.org/web/20210805175015/https://nlp.stanford.edu/~muzny/quoteli.html), identical to today's page and recaptured 2022-01-20, 2023-01-27 and 2025-05-11. The earliest capture (2019-10-20) is a redirect. The [2021-05-12 home page](https://web.archive.org/web/20210512142742/https://nlp.stanford.edu/~muzny/) lists "QuoteLi3 (includes data, annotation tool, and attribution code)". | none in any capture |
| Downloads | Six files under `nlp.stanford.edu/data/quoteattribution/` redirect (301) to `downloads.cs.stanford.edu/nlp/data/quoteattribution/` and are `Last-Modified` 2017-03-18. `~muzny/data/pp_test.xml` is from 2021-04-27. Each XML has only `<characters>` and `<text>`. The three chapter zips contain only annotated XML (plus `__MACOSX` resource forks in `pp_chapters.zip`), with no README or LICENSE. A case-insensitive search for "licen", "copyright" and "creative" matched only novel text. | none |
| [muzny/quoteannotator](https://github.com/muzny/quoteannotator) | GitHub license API: `null`, and the repository has no LICENSE file. Created 2016-03-02; last commit `8444634` (2017-03-19, "Update README.md"). The README only asks users to cite the paper. The repository has character lists and conversion scripts (`convertPP.py` for "the Pride and Prejudice Corpus", `convertCQSC.py` for "the Columbia Quoted Speech Corpus") but no annotation files. | none |
| [muzny/academic_website](https://github.com/muzny/academic_website), [muzny/muzny.github.io](https://github.com/muzny/muzny.github.io) and the live [muzny.github.io/quoteli.html](https://muzny.github.io/quoteli.html) | These are the author's website sources, and GitHub's license API returns `null` for both repositories. Each has the same `quoteli.html` text and copies of `data/{pp_test,austen_emma,chekhov_steppe}.xml`. `pp_test.xml` matches the pinned sha256. `academic_website` added them in `51d68b7` (2021-04-27, "add quote xml data"), 19 seconds before the Stanford page's `Last-Modified`. `muzny.github.io` added them in `d9ab55e` (2022-11-16). Their READMEs only say "academic website". `muzny.github.io` also added `data/quote_extraction_annotated.zip` in `f5c05dc` (2026-06-25, "add quote extraction evaluation data"). That archive is a different set, not QuoteLi3: five annotated novels, one of them a modern in-copyright title, with no license file. It is not used. The other 25 public `muzny` repositories have no quote data in their names or descriptions. | none |
| [stanfordnlp/CoreNLP](https://github.com/stanfordnlp/CoreNLP) | GPL-3.0. Quote attribution entered the code base in `a15eca7` (2017-04-02, "initial move of quote attribution into core complete"). `data/edu/stanford/nlp/quoteattribution/` holds only three lexicons (`animate.unigrams.txt`, `family_words.txt`, `gender_filtered.txt`). No path on `main` contains QuoteLi3 annotations (no `pp_`, `austen` or `steppe` files). The documentation page linked from the release page (`stanfordnlp.github.io/CoreNLP/quoteattribution.html`) returns 404; its successor [`quote.html`](https://stanfordnlp.github.io/CoreNLP/quote.html) does not mention the data. | CoreNLP's GPL covers its code, not QuoteLi3 |
| [EACL 2017 paper](https://aclanthology.org/E17-1044.pdf) | It reads "We release QuoteLi3" with a footnote to the release page. It calls QuoteLi3 "a unified combination of data from Elson and McKeown (2010) and He et al. (2013) with the addition of more than 3,000 new labels". It says: "For Emma and The Steppe, which were taken from the CQSC corpus, we have considerably more quotes because of our added annotations"; "Emma and The Steppe come from CQSC"; and "There is no overlap in the data provided by He et al. (2013) and CQSC". | none for the data |
| [Columbia Quoted Speech Attribution Corpus](https://inventions.techventures.columbia.edu/licenses/quoted-speech-attribution--1) (CQSC; Elson and McKeown 2010), the upstream source of *Emma* and *The Steppe* | Columbia Technology Ventures lists the file `Columbia_QSA_Corpus_1.01.tar.gz` under "Quoted Speech Attribution Corpus, Free Academic". It requires signing in to order and was not downloaded. | "This is the non-commercial license. All interested commercial parties should go back to the previous page and select "Inquire About This Technology"." |
| He, Barbosa and Kondrak 2013, "Identification of Speakers in Novels" ([P13-1129](https://aclanthology.org/P13-1129/)), the upstream source of *Pride and Prejudice* | Paper record and a web search. | none found |
| [Hugging Face `Felix-ML/quoteli3`](https://huggingface.co/datasets/Felix-ML/quoteli3) | The card metadata says `cc-by-4.0`. The uploader added it in their first README commit `2a6f40d` (2021-06-11, as `licenses: - cc-by-4.0`) and cited no source. Hugging Face staff PR #1 (opened 2022-07-01, merged as `a8e3cae` on 2022-10-25) only renamed the key to `license`. The loader `quoteli3.py` sets `_LICENSE = ""` and downloads the Stanford files rather than hosting data. The account was created 2021-06-10, has display name "Felix", belongs to no organization and has this one dataset. It describes the dataset as "a representation of Muzny et al.'s QuoteLi3 dataset" and shows no affiliation with the authors. | an unsourced tag by a third party, **not accepted** |
| [PDNC](https://github.com/Priya22/project-dialogism-novel-corpus) (Vishnubhotla, Hammond and Hirst, LREC 2022) | The paper chose novels to overlap "the LitBank and QuoteLi corpora", but PDNC does not redistribute QuoteLi3. | PDNC's own annotations: "PDNC is free to use under the CC-BY 4.0 License", added to its ReadMe on 2026-10-11 (`fabe7b9`). This does not apply to QuoteLi3. |
| LitBank quotation paper ([arXiv 2004.13980](https://arxiv.org/abs/2004.13980)), [BookNLP](https://github.com/booknlp/booknlp) (MIT) | Both cite or compare with Muzny et al. BookNLP's `examples/158_emma/158_emma.quotes` is BookNLP's own output, not QuoteLi3. Neither redistributes QuoteLi3. | not applicable |
| Web search for QuoteLi3 license or redistribution | No other mirror, aggregator entry or license statement was found. | none |

The underlying novels are public domain: Austen's *Pride and Prejudice* (1813) and *Emma* (1815),
and Chekhov's *The Steppe* (1888) in an early English translation. The XML text appears to be a
Project Gutenberg-style transcription, with `_italic_` underscores and headings such as "CHAPTER
I (1)". The quote spans, mention spans, speaker links and character lists are what carry any
rights, and those rights belong to the annotators and to the upstream corpora.

Pinned files, with no upstream tag or commit (the `Last-Modified` date stands in for a version):

| File | URL | sha256 | Bytes |
|---|---|---|---:|
| `pp_full.xml` | <https://downloads.cs.stanford.edu/nlp/data/quoteattribution/pp_full.xml> | `d4f0c83d73085d02db6db938eb3e2371d4b082d4a8194345e3e527c9a0959a4f` | 791,441 |
| `pp_test.xml` | <https://nlp.stanford.edu/~muzny/data/pp_test.xml> | `55056da60b81fa35d0b42214451d106248d36d14aa8fd92547fdc49d192590bf` | 107,337 |

If the Stanford user directory disappears, the author's website repository serves a byte-identical
`pp_test.xml` with the same sha256 at
<https://raw.githubusercontent.com/muzny/academic_website/master/data/pp_test.xml> (checked
2026-10-16; the `main` branch path returns 404). It is a known fallback, not the pin.

`pp_full.xml` holds 53 chapters and `pp_test.xml` holds the other 8 (19–26). Together they cover
all 61 chapters and 1,750 quotes.

## Wave 3 candidates (#296: #299, #300, #304, #307)

Checked 2026-10-20 from each source itself: release archives and git objects at pinned commits,
license files, READMEs, data-file comments and PR descriptions. Nothing is vendored by this check;
each issue's PR vendors its own table and adds the NOTICE entry and the license text under
`style/data/licenses/`. The standard is the one used above: a list's own origin must be permissive,
not only its repository's license. Every usable source below stays in a separately loaded data file
under its own license, never inlined into engine source.

### Decisions

| Source | Pin | License found | Origin of the content | Size | Decision |
|---|---|---|---|---|---|
| VarCon 2020.12.07 | SourceForge `wordlist/VarCon/2020.12.07/varcon-2020.12.07.tar.gz`, sha256 `3b0720c5718008f37c02658a83d51d5598dd86531f0a2206d410c854006cb184` (215,978 bytes); same files as en-wl/wordlist tag `rel-2020.12.07` → `5ef55f9c42730ebe4394a78b77855468a6f15dd2` | `README` "Copyright": Atkinson and Titze permissive notices, plus the Ispell BSD-style notice with clause 4 (advertising) removed | Ispell American/British lists, Garst R. Reese's Canadian list, then Atkinson's additions checked entry by entry against M-W and Oxford dictionaries; Titze's Australian data | `varcon.txt`: 7,165 clusters, 845,998 bytes | **usable with conditions** |
| SCOWL v2 ("ESDB") variant data | en-wl/wordlist `v2` tag `rel-2026.02.25` → `7e99edab8e32f9f9ea2b15f249ca8d4d67237410` | `Copyright`: Atkinson permissive notice; AU (Titze) and UKACD sections apply depending on use; "the WordNet copyright MIGHT apply"; the VarCon/Ispell notice moved to `docs/sources.md` | VarCon, plus 12dicts, ENABLE2K, UKACD, WordNet 1.6 and COCA (under NDA) for sizes | `data/scowl-pre.txt` 15,613,777 bytes; ≈6,300 variant groups | **usable with conditions; VarCon preferred** |
| Harper `regionalisms.rs` | Automattic/harper `v2.13.0` → `f4ebc61d77df1fc00b6aa8bc604e91af3534dad1` | `LICENSE`: Apache-2.0, "Copyright 2024 Automattic Inc."; no NOTICE file | Contributors' own knowledge, per PRs #1475, #1888, #2360, #2397, #3472, #3619; no outside list cited | 95 terms in 47 concepts, 24,022 bytes | **usable with conditions** |
| Harper `boring_words.rs`, `hedging.rs`, `filler_words.rs` | same | same | Maintainer's own picks (no source stated); fillers from a transcript the contributor was checking (PR #1626) | 5 + 3 + 2 entries | **usable, not worth vendoring** |
| CMUdict | cmusphinx/cmudict `74790861f652b15e4ac49015a90074ad62a27690` (no tags; successor to 0.7b) | `LICENSE`: BSD-2-Clause-style, © 1993–2015 Carnegie Mellon University; README asks that redistributors "acknowledge its origin" | CMU Speech Group and volunteers | `cmudict.dict` 126,052 headwords, 3,618,488 bytes, sha256 `81917843c7f44ce2b094ac63873c2c7a4cf802040792c455ba3ca406891c3d22` | **usable with conditions** |
| Princeton WordNet 3.0 / 3.1 | `WordNet-3.0.tar.gz` sha256 `640db279c949a88f61f851dd54ebbb22d003f8b90b85267042ef85a3781d3a52`; `wn3.1.dict.tar.gz` sha256 `3f7d8be8ef6ecc7167d39b10d66954ec734280b5bdcd57f7d9eafe429d11c22a` | WordNet License (SPDX `WordNet`): any purpose, notice and disclaimer on all copies including modifications, no use of Princeton's name in advertising | Princeton lexicographers | 3.0: 428 `noun.feeling` synsets (771 lemmas), 17,741 noun→verb derivational pointers | **usable with conditions** |
| Open English Wordnet 2025 | globalwordnet/english-wordnet release `2025-edition` → `dc343f2683279ecbb13fab4e2fd778d7b162d287`; `english-wordnet-2025.zip` sha256 `73355e48f8117a24ca9ebc23ed75b35434e6cd21cc9dd3984e80aff5a5f63636` | `LICENSE.md`: CC BY 4.0, "attribution is given to both Princeton WordNet and the Open English Wordnet team"; `WNDB_License.txt` repeats the Princeton notice | Fork of Princeton WordNet 3.1 | 107,519 synsets; 432 `noun.feeling` synsets | **usable with conditions; Princeton preferred** |
| Roget's Thesaurus 1911 | Project Gutenberg #22 `pg22.txt` sha256 `b4ba0c94a1212c8f92245f2c408e7272a9b0b2dcc1613fb315c8a3e04e6cd3be`; #10681 `pg10681.txt` sha256 `20de698e5db9d5c8e9dc9a50e0c8be53483b14f4cb3d0c879bf85c6d17d32a12` (both edited in place by PG; fetched 2026-10-20) | "Public domain in the USA."; MICRA "makes no proprietary claims regarding this electronic version"; PG terms apply only if PG references are kept | #22 is MICRA's transcription of the 1911 edition "supplemented with over 1,000 words not present in the original"; #10681 is L. John Old's reformat of #22 | 1,043 numbered entries, 1,539,641 bytes | **usable with conditions** |
| words/fillers | words/fillers tag 2.0.1 → `d1924b589e6b76d7cd8c4ee3c6d4859aadae8fc7` | `license`: MIT, © 2014 Titus Wormer | `data.txt` header names "Wikipedia" and mikpanko/expresso (`82b800f6`, MIT with the holder placeholder unfilled); all 80 expresso fillers are in the list, 3 entries are unattributed | 83 entries, 972 bytes | **usable with conditions** |
| words/weasels | words/weasels tag 2.0.1 → `cf8212ec0df90730571b6891e511e0d19f6fc738` | MIT, © 2014 Titus Wormer | `data.txt` header names matt.might.net (no license), Wikipedia, grammar.about.com, changingminds.org (© Changing Works), melissajagears.com (©) | 116 entries, 1,207 bytes | **rejected** |
| words/hedges | words/hedges tag 2.0.1 → `0e972f034671bc9fde086abd701a8a55adfd4e0c` | MIT, © 2014 Titus Wormer | `data.txt` header names ohEmily/HedgeCounter `ba029812` (no license; 98 of its 115 entries are here) and web pages including readwriteandedit.com ("All rights reserved") | 162 entries, 2,207 bytes | **rejected** |
| NRC EmoLex | v0.92, <https://saifmohammad.com/WebPages/NRC-Emotion-Lexicon.htm> | "can be used freely for non-commercial research and educational purposes"; "Do not redistribute the data" | Mechanical Turk annotation by NRC | 14,182 words | **rejected: non-commercial, no redistribution** |
| WordNet-Affect | distributed inside WordNet Domains 3.2 (FBK), no public pin | Conflicting: archived download page says CC BY 3.0 (2013–2024), but the download still requires an agreement whose text could not be retrieved; the earlier licence PDF says "limited to non-commercial use" | FBK annotation of WordNet 1.6 synsets | not downloaded | **rejected: license not verifiable** |
| DepecheMood / DepecheMood++ | marcoguerini/DepecheMood `c12a1780a200b629a23c575ae5ae1977efbdd7c1` | No LICENSE file; README: "This resource can be used for research purposes." | Reader mood votes harvested from rappler.com and corriere.it news articles | DM 2014: 37,770 rows; DM++ English: 176K–285K rows | **rejected: research only, scraped news** |

### VarCon 2020.12.07 (#299)

- **Notices.** The `README` "Copyright" section (lines 526–582) has three notices: Kevin Atkinson's
  ("Permission to use, copy, modify, distribute and sell this array, the associated software, and
  its documentation for any purpose is hereby granted without fee, provided that the above
  copyright notice appears in all copies and that both that copyright notice and this permission
  notice appear in supporting documentation"), Benjamin Titze's (same text, for the Australian
  data), and Geoff Kuenning's 1993 Ispell notice, which applies "since the original words lists
  come from the Ispell distribution".
- **Ispell conditions.** Retain the notice in source; reproduce it in the documentation of binary
  redistributions; "All modifications to the source code must be clearly marked as such. Binary
  redistributions based on modified source code must be clearly marked as modified versions in the
  documentation"; Kuenning's name may not endorse derived products. The notice reads "(clause 4
  removed with permission from Geoff Kuenning)", and the README changelog for revision 4.1 (2004)
  records the removal. The removed clause was the advertising clause, so no advertising
  acknowledgment is owed. Kuenning's permission is known only from Atkinson's statement.
- **Origin.** Garst R. Reese's Canadian list carries no stated license; it is from about 2000 and
  its entries have since been rechecked against dictionaries. The dictionaries were used to check
  spelling facts, not copied as lists. `voc.tab` (vocabulary differences) cites web pages as
  sources; use `varcon.txt` only.
- **Conditions.** Ship all three notices verbatim in a license file and NOTICE, including in binary
  documentation; mark the converted table as modified ("Derived from VarCon 2020.12.07; converted
  and filtered by writemorebetter"); keep it in a separate data file.
- **Not in the release.** `v1` has unreleased VarCon fixes from 2024-07-22; there is no later
  release, so pin 2020.12.07. The README's `abbc.tab`, `variant-infl.tab` and `variant-wroot.tab`
  are not in the tarball; "abbc" is the pre-5.0 name of `varcon.txt`, not a separate source.

### SCOWL v2 / ESDB (#299 alternative)

`v2` is now the default branch of en-wl/wordlist, renamed "English Speller Database" in `bc45348`
(2026-05-08). Variant data lives in the spelling and variant-level fields of `data/scowl-pre.txt`,
which the README says comes from "a far more complicated, and unreleased, process". The tag has no
data release asset, and variant-level meanings changed after it (HEAD `1e5b7d3a`, "Swap variant
level for archaic and uncommon variants"). Using it would mean the Atkinson ESDB notice, the Titze
notice whenever `D` codes are used, the UKACD notice above size 80, the VarCon/Ispell notice, the
WordNet notice to be safe, and a credit to Alan Beale's 12dicts. VarCon gives the same spelling
variants with three notices and a fixed release.

### Harper lists (#299, #307)

- `harper-core/src/linting/regionalisms.rs` (sha256
  `61a33117d615f59e09c252aea2337914312a4e2e3f1be456baaee8b8b6782566`) was created in PR #1475
  (`bb84be83`, 2025-07-03) and grown in PRs #1624, #1888, #2360, #2397, #3429, #3472 and #3619. The
  PRs cite no outside list; contributors added terms from their own knowledge ("popped into my
  head", "watching an Australian YouTuber"). Dialects: American, British, Canadian, Australian,
  Indian. Conditions: Apache-2.0 §4 — include the license text and the "Copyright 2024 Automattic
  Inc." line, state that the table was converted and modified, record the commit. Review individual
  rows before use (for example *duvet* is tagged Australian only, and *Xerox* is a trademark).
- `boring_words.rs` (very, interesting, several, most, many), `hedging.rs` ("I would argue that",
  ", so to speak", "to a certain degree") and `filler_words.rs` (uh, um) are Apache-2.0 with no
  outside origin, but ten everyday entries are not worth an attribution chain.

### CMUdict (#300, #307 alliteration)

- `LICENSE` is BSD-2-Clause with a funding acknowledgment: "Redistributions of source code must
  retain the above copyright notice … The contents of this file are deemed to be source code" and
  "Redistributions in binary form must reproduce the above copyright notice … in the documentation
  and/or other materials provided with the distribution." GitHub reports `NOASSERTION` because the
  text is not a verbatim SPDX match. The `README` adds: "Use of this dictionary for any research or
  commercial purpose is completely unrestricted."
- The repository has no tags; `2f4c83d` (2014-11-20) is "Updates from cmudict-0.7b", and the last
  change to `cmudict.dict` is `0f8072f8` (2024-12-17). Cite by commit, not as 0.7b.
- **Size.** A syllable table (stress-marked phones per headword, minimum across variants) has
  126,052 rows: 1,322,203 bytes as `word\tcount` TSV, 396,798 bytes gzipped. 1,715 headwords have
  variants that disagree. A packed FST or trie would be smaller.
- **Conditions.** Separate data file, `LICENSE` verbatim in `style/data/licenses/`, NOTICE entry
  naming CMU and the commit, and the notice in the documentation of WASM and native builds that
  embed it.
- **Vendored by #300 phase 2 (2026-10-20).** `style/data/pronunciation/cmudict-syllables.tsv`:
  syllable counts only (vowel phones of the first pronunciation), no phonemes, for the 8,437 of
  125,159 keys the heuristic counter gets wrong; 92,749 bytes, 32,885 gzipped, instead of the
  1.3 MB full table above. Built by `scripts/build-cmudict.py`, which pins the sha256 above. The
  file header carries CMU's copyright line and marks it as modified; `LICENSE` is verbatim in
  `style/data/licenses/cmudict.txt`; NOTICE has the entry with the full notice and says binary
  redistributions must reproduce it; the root, `wasm/` and `swift/` READMEs point to it. The
  release workflow ships `NOTICE` and `style/data/licenses/` inside the XCFramework and data
  zips and attaches `NOTICE` as a release asset (#321 review).
- **Vendored by #307 alliteration (2026-10-21).** `style/data/pronunciation/cmudict-onsets.tsv`:
  each headword's first phoneme (first pronunciation; one lowercase ARPAbet consonant or `-` for
  a vowel), never a whole pronunciation, for the 1,943 of 125,159 keys whose first sound the
  spelling guess `spelled_onset` gets wrong; 20,888 bytes, 7,018 gzipped, instead of 1,324,850
  (358,104) for every key. Same script, pinned file, header, license file and NOTICE entry as the
  syllable table; CI's staleness check covers the whole `style/data/pronunciation/` directory.
- **Cargo metadata.** `license = "MIT OR Apache-2.0"` on `writemorebetter-style`,
  `writemorebetter-swift` and `writemorebetter-wasm` covers the code only; the crates also compile
  in this table and the CC BY data above. Their `description` says so, but notice generators that
  read only the `license` field (`cargo-about`, `cargo-deny`) will not list CMU or the CC BY
  sources. Consumers building a notice screen add CMU's notice and the CC BY credits from
  `NOTICE` by hand. #307's
  alliteration table should be another output of the same script. See
  [STYLE-READABILITY.md](STYLE-READABILITY.md#dictionary-phase-2).

### Princeton WordNet 3.0 / 3.1 (#304, #307 hidden verbs)

- **License** (`WordNet-3.0/LICENSE`; the same text heads every 3.1 `data.*` file): "Permission to
  use, copy, modify and distribute this software and database and its documentation for any
  purpose and without fee or royalty is hereby granted, provided that you agree to comply with the
  following copyright notice and statements, including the disclaimer, and that the same appear on
  ALL copies of the software, database and documentation, including modifications that you make
  for internal use or for distribution." Princeton's name "may not be used in advertising or
  publicity", and title to copyright stays with Princeton. The license page (read from the Wayback
  snapshot of 2025-12-26 because the live page returned a Cloudflare 403) says WordNet "may be
  used in commercial applications".
- **Derived lists.** An emotion list (the `noun.feeling` lexicographer file, optionally combined
  with the hyponyms of `feeling.n.01` and `emotion.n.01`) and noun↔verb derivational pairs are
  modifications of the database, so the notice must travel with them and they cannot be relicensed
  as MIT/Apache. The license is permissive and compatible with the engine's licenses; it only needs
  the derived tables to be **separate data files** carrying `style/data/licenses/WordNet-LICENSE`
  (verbatim, with the version line of the release used) and a NOTICE entry with the version,
  tarball sha256, derivation, and the citation Princeton asks for (Miller 1995; Fellbaum 1998).
  Don't use "WordNet" or "Princeton" in product names.
- **3.0 vs 3.1.** The terms are the same. 3.1 is distributed as database files only, with no
  separate LICENSE file and no `lexnames`; prefer 3.0 unless a 3.1 correction matters.

### Open English Wordnet (#304, #307 alternative)

Release `2025-edition` was re-uploaded after publication (assets re-uploaded 2026-02-04, files
added 2026-06-26), so pin by asset sha256, not by tag. `LICENSE.md`: "This resource is derived from
Princeton WordNet under the WordNet License and further developed under the Creative Commons
Attribution 4.0 International License. You may share and adapt this resource providing attribution
is given to both Princeton WordNet and the Open English Wordnet team." The release archives contain
no standalone license file; both texts come from the repository at the pinned commit. It has
`noun.feeling` (432 synsets) and derivation relations. It is usable with CC BY attribution plus the
Princeton notice, but Princeton 3.0 needs one notice instead of two attribution chains, so it is the
alternative, not the first choice.

### Roget's Thesaurus 1911 (#304 fallback)

- #22 is MICRA, Inc.'s electronic transcription (released 1991-12-01); #10681 (2004-04-01) is "Taken
  from Project Gutenberg's #22" by L. John Old and drops MICRA's preface.
- MICRA's preface: the text was "supplemented with over 1,000 words not present in the original
  1911 edition" and "does not contain any material published commercially after 1911"; MICRA
  "makes no proprietary claims regarding this electronic version … If the 1911 work is currently
  public domain, this electronic version can also be treated as public domain." The additions are
  not marked.
- PG's license (section 1.C) does not restrict copying "as long as all references to Project
  Gutenberg are removed"; keeping PG's name brings in its trademark terms.
- **Conditions.** Derive from #22 with all PG references stripped, pin by sha256 and fetch date,
  record the MICRA supplement and that public-domain status is confirmed for the US only. The 1911
  editor is not named in either file, so non-US status is unverified. Filter obsolete (`|`-marked)
  and foreign words before fiction-facing use.

### words/fillers, words/weasels, words/hedges (#307)

The READMEs credit no source, but each `data.txt` names its sources in `%` comments, unchanged since
the first commit (2014-08).

- **fillers: usable with conditions.** Its sources are Wikipedia and mikpanko/expresso, whose
  `LICENSE.txt` is MIT with the placeholder "Copyright (c) <year> <copyright holders>" unfilled and
  whose `static/corpora/fillers` (added by Mikhail Panko, `81e8c4a`, 2014-01-12) names no source.
  Vendor only the 80 entries shared with expresso and credit Titus Wormer and expresso (Mikhail
  Panko), both MIT. The 3 other entries are unattributed.
- **weasels: rejected.** All 25 words of Matt Might's default list ("Shell scripts to improve your
  writing", which has no license) are in it, in his order, alongside entries from copyrighted web
  pages. write-good's weasel check comes from the `weasel-words` npm package (MIT), which says it is
  "Based on this shell script" (Might's), so write-good does not clear it either.
- **hedges: rejected.** Its main source, ohEmily/HedgeCounter, has no license, and the others are
  web pages, one marked "All rights reserved".

A vague-words list for #307 should be authored by this project's contributors, as
`transitions.tsv` and `cognition.txt` were, plus the expresso-backed fillers.

### Emotion lexicons (#304)

- **NRC EmoLex v0.92: rejected.** "Copyright (C) 2011 National Research Council Canada (NRC)". The
  terms read "can be used freely for non-commercial research and educational purposes", "Do not
  redistribute the data", and "You may not rent or license the use of the lexicon nor otherwise
  permit third parties to use it". Commercial use needs a license from NRC.
- **WordNet-Affect: rejected (not verifiable).** wndomains.fbk.eu and hltdistributor.fbk.eu did not
  respond on 2026-10-20. Archived `download.html` captures from 2013-12-16 to 2024-10-10 say
  "WordNet Domains is licensed under a Creative Commons Attribution 3.0 Unported License" and that
  the 3.2 distribution "includes WordNet Affect", but downloading still requires accepting an
  agreement whose text could not be retrieved; earlier captures and `licenceWND.pdf` (2011) say
  "limited to non-commercial use". It is also keyed to WordNet 1.6 synsets. Reconsider only if FBK
  confirms CC BY 3.0 in writing.
- **DepecheMood / DepecheMood++: rejected.** No LICENSE file; the README says "This resource can be
  used for research purposes" and `DepecheMood/README.txt` "freely available for research
  purposes". The DM++ paper (arXiv 1810.03660) says the scores come from news articles and reader
  "Mood Meter" votes "harvested" from rappler.com (53,226 articles, about 1.15M votes) and
  corriere.it. A CC BY-NC-SA statement for the 2014 release could not be checked (the Internet
  Archive was offline), but it would be non-commercial either way.

### Recommended source per issue

| Issue | Recommendation | Rejected or not preferred |
|---|---|---|
| #299 US/UK spelling | VarCon 2020.12.07 `varcon.txt`, three notices, marked modified; optionally Harper `regionalisms.rs` for vocabulary (Apache-2.0, US/UK/CA/AU rows only) | SCOWL v2/ESDB (more notices, no data release); VarCon `voc.tab` (web-page sources) |
| #300 readability phase 2 | CMUdict `74790861` syllable-count table, heuristic fallback; vendored as the heuristic's 8,437 misses (92,749 bytes, 32,885 gzipped) | — |
| #304 emotion tells | Lancaster norms (already vendored) ∩ Princeton WordNet 3.0 `noun.feeling` plus `feeling.n.01`/`emotion.n.01` hyponyms, as a separate file under the WordNet License; Roget 1911 (#22, PG references stripped) as fallback | NRC EmoLex, WordNet-Affect, DepecheMood; OEWN acceptable but needs two attribution chains |
| #307 vague words | Project-authored list plus words/fillers' 80 expresso-backed entries | words/weasels, words/hedges (unlicensed origins); Harper boring/hedging/filler (usable but 10 entries) |
| #307 hidden verbs | Princeton WordNet 3.0 noun→verb derivational pointers, separate file under the WordNet License | CatVar (OSL-1.1, per #307) |
| #307 alliteration | CMUdict `74790861`, a second derived table from the same file as #300: each headword's first phoneme, vendored as the spelling guess's 1,943 misses (20,888 bytes, 7,018 gzipped) | — |

#### Still unverified

- Princeton's live license page (Cloudflare 403; the Wayback copy matches the tarball's `LICENSE`).
- FBK's current WordNet Domains download agreement.
- Whether the 2014 DepecheMood page stated a CC license.
- The license of Garst R. Reese's Canadian list in VarCon's original table.
- The 1911 Roget editor, and so its public-domain status outside the US.

## Regional spelling (#299)

Vendored 2026-10-20 under the conditions recorded in the wave 3 check (#316).

| Source | Pin | License | Decision |
|---|---|---|---|
| VarCon 2020.12.07 | `varcon-2020.12.07.tar.gz` from SourceForge `wordlist/VarCon/2020.12.07/`, sha256 `3b0720c5718008f37c02658a83d51d5598dd86531f0a2206d410c854006cb184`; `varcon.txt` inside it sha256 `340858b990255ec409adcf7faaf2b992aa2a76a16f30b4b9e2663863191dba33` | `README` "Copyright": Kevin Atkinson's and Benjamin Titze's permissive notices and Geoff Kuenning's 1993 Ispell notice (clause 4 removed) | **vendored as a modified, derived table** |
| Harper `regionalisms.rs` | Automattic/harper `f4ebc61d77df1fc00b6aa8bc604e91af3534dad1` (v2.13.0) | Apache-2.0 | **not vendored** |

- `style/data/spelling/regional.txt` (153,303 bytes, 5,308 rows, 10,689 spellings) is built by
  `scripts/build-regional-spelling.py` from the archive, which checks both hashes. Only
  `varcon.txt` is read; `voc.tab` cites web pages and is not used.
- **Modified version.** The table's header and `style/data/licenses/varcon.txt` say "derived from
  VarCon 2020.12.07; converted and filtered by writemorebetter", as Ispell clause 3 requires for
  modified source and binary redistributions. The changes: clusters at SCOWL level 70 or below;
  lowercase single words without apostrophes; American (`A`), British -ise (`B`) and British -ize
  (`Z`) categories only, applying VarCon's rule that `B` implies `Z` when a line has no `Z` tag;
  a spelling accepts a system when tagged preferred, equal (`.`) or variant (`v`); usages marked
  rare or archaic (`(-)`) ignored; each spelling marked with the union of systems over all its
  usages, and `~` where usages differ; rows whose spellings all suit the same systems dropped.
- **Notices.** All three notices are reproduced verbatim in `style/data/licenses/varcon.txt` and
  NOTICE, so they ship in source and in binary documentation (Ispell clauses 1 and 2). NOTICE says
  the authors do not endorse writemorebetter (clause 5).
- **Harper regionalisms** are usable (Apache-2.0, contributors' own knowledge) but not vendored:
  most of the vocabulary pairs are ordinary American words in another sense (*pavement*, *lift*,
  *flat*, *boot*, *jumper*), so flagging them needs word-sense information the report does not
  have, and the 95 terms would add an attribution chain for a lower-confidence category. Revisit
  with a sense filter.

## Emotion lexicon (#304)

| File | Source | Version | License | Checked |
|---|---|---|---|---|
| `style/data/emotions/emotions.tsv` | Princeton WordNet 3.0, <https://wordnetcode.princeton.edu/3.0/WordNet-3.0.tar.gz> | release 3.0; sha256 `640db279c949a88f61f851dd54ebbb22d003f8b90b85267042ef85a3781d3a52`, 11,537,239 bytes | WordNet License (SPDX `WordNet`) | 2026-10-20 |

Vendored as the [Princeton WordNet 3.0 decision](#princeton-wordnet-30--31-304-307-hidden-verbs)
above requires: a separate data file, never inlined into engine source, with the copyright line
in its header, `style/data/licenses/WordNet-LICENSE` copied verbatim from the tarball's `LICENSE`
(including its "WordNet Release 3.0" version line), and a NOTICE entry with the version, sha256,
derivation and Princeton's requested citation (Miller 1995; Fellbaum 1998). The table is not
relicensed: the style crate's MIT/Apache license covers its code, not this file. The Lancaster
norms are used only as a filter (dominant interoception), so no Lancaster values are copied into
the table. They remain credited under CC BY 4.0.

**Derivation.** `scripts/build-emotion-lexicon.py` reads the tarball in place, checks the sha256
and rebuilds the table byte for byte: 760 lemmas (438 nouns, 235 adjectives, 112 verbs whose
participle names a feeling). It takes `noun.feeling` plus the hyponyms of `feeling.n.01` and
`emotion.n.01`, those nouns' derived and attribute adjectives with their satellites, and
`verb.emotion` or noun-derived verbs with frame 10. It keeps a lemma when its most frequent sense
is in that set or the bundled Lancaster table rates it dominantly interoceptive, then applies
reviewed inclusion and exclusion lists. Every lemma comes from WordNet. Details:
[STYLE-EMOTION-TELLS.md](STYLE-EMOTION-TELLS.md#lexicon).

## Vague words and hidden verbs (#307)

| File | Source | Version | License | Checked |
|---|---|---|---|---|
| `style/data/hidden-verbs/nominalizations.tsv` | Princeton WordNet 3.0, <https://wordnetcode.princeton.edu/3.0/WordNet-3.0.tar.gz> | release 3.0; sha256 `640db279c949a88f61f851dd54ebbb22d003f8b90b85267042ef85a3781d3a52` | WordNet License (SPDX `WordNet`) | 2026-10-21 |
| `style/data/vague/fillers.txt` | words/fillers `data.txt`, <https://github.com/words/fillers>; mikpanko/expresso `corpora/fillers`, <https://github.com/mikpanko/expresso> | words/fillers tag 2.0.1 → `d1924b589e6b76d7cd8c4ee3c6d4859aadae8fc7`, `data.txt` sha256 `d35af8f76a8989a0fa96ae20a784a20561838008bc90bf32033c8cc57c15d4af`; expresso `2d58bbbcda716ba1e3acc4615c522ad538effe06` (last commit with the file), sha256 `011f59dc812b0d7b92a3937ce1d4f092605211ea32e422dd6ead65e6a49e4c03` | MIT (Titus Wormer, 2014); MIT (expresso, copyright line unfilled) | 2026-10-21 |
| `style/data/vague/vague-words.tsv` | written for this project | — | MIT OR Apache-2.0 | 2026-10-21 |

- **Hidden verbs.** Vendored as the [Princeton WordNet 3.0 decision](#princeton-wordnet-30--31-304-307-hidden-verbs)
  requires, in the same way as the emotion lexicon: a separate data file with the copyright line
  in its header, the existing `style/data/licenses/WordNet-LICENSE` (verbatim from the release, so
  its text is unchanged), and the existing NOTICE entry, which now names both WordNet-derived
  files and describes both derivations. The table is not relicensed. `scripts/build-hidden-verbs.py`
  derives it from WordNet's noun→verb derivational pointers alone; no CatVar (OSL-1.1) or other
  source is used. It reads `emotions.tsv` only to leave its nouns out. Details:
  [STYLE-HIDDEN-VERBS.md](STYLE-HIDDEN-VERBS.md#table).
- **Fillers.** Only the 80 words/fillers entries that expresso also lists are vendored, as the
  [wave 3 check](#wordsfillers-wordsweasels-wordshedges-307) requires; the three unattributed
  entries (*amazing*, *stuff*, *things*) are dropped by `scripts/build-filler-words.py`, which
  checks both files' sha256s. expresso deleted `corpora/fillers` in `e4ee555` (2018); the pinned
  commit is its parent. The file is Mikhail Panko's 2014 list: 79 words from `81e8c4a`
  (2014-01-12) plus *too* from `55d0ae8` (2014-01-25), and no later change. Both MIT notices are
  reproduced in `style/data/licenses/fillers.txt` and NOTICE credits Titus Wormer and Mikhail
  Panko. words/hedges and words/weasels stay rejected; nothing from them is used.
- **Project list.** `vague-words.tsv` was written by this project's contributors, as
  `transitions.tsv` and `cognition.txt` were. *amazing* and *stuff* are in it as two of the
  commonest vague words, chosen independently; single common words are not a selection taken from
  words/fillers, whose other unattributed entry (*things*) is matched only as the lemma *thing*.
  Seven of its common phrases also appear in the rejected lists (*sort of*, *kind of*, *a bit*,
  *somewhat* and *more or less* in words/hedges; *a lot* and *various* in words/weasels). They were
  chosen independently, as the ordinary English for hedging and vague amounts, not taken from those
  lists.

## Thought verbs (#353)

- **Files.** `style/data/thought/thought-verbs.txt` (15 lemmas) and
  `style/data/thought/reflexive-verbs.txt` (12 lemmas) were written by this project's
  contributors and are MIT OR Apache-2.0, like the engine code. They are not LanguageTool data and
  are not LGPL.
- **WordNet.** Princeton WordNet 3.0 verb synsets and hypernyms of *think* were consulted for
  candidates only. No WordNet file, gloss, sense key or derived table is copied, so no WordNet
  notice is needed for these files.
- **Speech verbs.** Unquoted speech reuses `style/data/dialogue/speech-verbs.txt` and
  `showy-verbs.txt` (#249), already project-authored.
- **Bench data.** `bench/style/thought-templates.json` is CC0, written for this repository. The
  voice labels in `bench/style/labels/thought-*.jsonl` hold short context windows of
  public-domain Standard Ebooks texts (CC0 dedication) and the CC0 `bench/public` stories, and
  were produced by Codex `gpt-6-sol` and DeepSeek V4 Pro through OpenCode Go, the same route as
  the pacing labels (#251). The concreteness section above records that OpenCode Go's terms bar
  output that ships in the library; these labels are benchmark data in `bench/`, not shipped
  data, but the owner should confirm that the same reading covers them.
