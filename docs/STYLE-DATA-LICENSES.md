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
- **Changes** (recorded in the file header and NOTICE): single words only, lowercased; only words
  whose strongest perceptual modality rates ≥ 3.5 and leads the next by ≥ 0.5, not counting
  interoception as the next modality of a haptic word; columns reduced to that modality, its mean
  strength and that lead, in tenths. 11,450 of 39,707 entries; 192,986 bytes. The build is
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
