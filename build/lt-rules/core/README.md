# Forked sentence segmentation rules

`segment.srx` is LanguageTool's SRX sentence segmentation file
(https://github.com/languagetool-org/languagetool), licensed under **LGPL-2.1**.
It is data, not engine code: `scripts/build-data.sh` passes it to
`build/make_build_dir.py --segment_path` in place of the copy inside
`languagetool-core.jar`, and it ends up serialized inside `en_tokenizer.bin`.
`README.md` itself is writemorebetter documentation.

Provenance: LanguageTool 6.6, `org/languagetool/resource/segment.srx` from
`libs/languagetool-core.jar` in
<https://languagetool.org/download/LanguageTool-6.6.zip> (sha256
`53600506b399bb5ffe1e4c8dec794fd378212f14aaf38ccef9b6f89314d11631`), source tag
<https://github.com/languagetool-org/languagetool/tree/v6.6>.

## Local edits

Every change from the 6.6 file is listed here. To see the edits as a diff,
extract the original from the release and diff it against this file:

```sh
unzip -p data/cache/LanguageTool-6.6/libs/languagetool-core.jar \
    org/languagetool/resource/segment.srx | diff - build/lt-rules/core/segment.srx
```

- English: two no-break rules for the spaced-out ellipsis (`word . . . word`),
  backported from LanguageTool commit
  [`458ff9e655`](https://github.com/languagetool-org/languagetool/commit/458ff9e655)
  (after 6.6). One keeps the dots of the run in one sentence; the other keeps the
  run and a following word that is not uppercase in one sentence. LanguageTool
  6.6 split the sentence at every dot, so the run was flagged as a space before
  a full stop and the next lowercase word as a sentence start. The same
  commit's `IMMUNIZE_ELLIPSIS` disambiguation rule is implemented in the
  `COMMA_PARENTHESIS_WHITESPACE` builtin instead, and its new `EN_ELLIPSIS`
  style rule is not ported. The builtin also skips a quote mark or comma
  separated from the run by a space (`. . . ”`), which LanguageTool still flags.
