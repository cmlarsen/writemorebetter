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
- **Concreteness** stays caller-supplied. Tests use invented ratings only, and the usefulness review
  used a proxy derived locally from the Lancaster norms, never Brysbaert's ratings.
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
  OSF trial-level data node <https://osf.io/qpmf4/> has no license set. The concreteness table must be a
  caller-supplied input.
- **Lancaster Sensorimotor Norms (Lynott, Connell, Brysbaert, Brand & Carney, 2020): CC BY 4.0,
  redistributable with attribution.** The OSF project <https://osf.io/7emr6/> and its Data component
  <https://osf.io/rwhs6/> (`Lancaster_sensorimotor_norms_for_39707_words.csv`) both record "CC-By
  Attribution 4.0 International" (legal code <https://creativecommons.org/licenses/by/4.0/legalcode>);
  the open-access article (doi 10.3758/s13428-019-01316-z) is CC BY 4.0. The aggregate CSV is about
  17 MB, so a trimmed table (word plus the six perceptual columns) with a NOTICE entry is the likely
  form if #250 vendors it.

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
