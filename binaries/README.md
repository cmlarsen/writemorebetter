# writemorebetter data binaries

`en_tokenizer.bin` and `en_rules.bin` are the compiled English data for
writemorebetter. They are **not** part of the engine source: the engine is
MIT/Apache-2.0, the data is derived from LanguageTool and licensed under
**LGPL-2.1**. A copy of that license ships with the data as
[`LICENSE-LGPL-2.1.txt`](LICENSE-LGPL-2.1.txt).

## Source

The data is derived from LanguageTool 6.6:

- Release download: <https://languagetool.org/download/LanguageTool-6.6.zip>
  (sha256 `53600506b399bb5ffe1e4c8dec794fd378212f14aaf38ccef9b6f89314d11631`)
- Source tag: <https://github.com/languagetool-org/languagetool/tree/v6.6>

Derived from that release:

- the grammar and style rule XML (`grammar.xml`, `style.xml`); `grammar.xml`
  is a fork with the local edits listed in `build/lt-rules/en/README.md`,
- the sentence segmentation rules (`segment.srx`), a fork with the local edits
  listed in `build/lt-rules/core/README.md`,
- the disambiguation rule XML (`disambiguation.xml`),
- the tagger dictionary (`english.dict` / `english.info`) and the Morfologik
  speller dictionaries (`hunspell/*.dict` / `*.info`),
- the `builtin-data` word lists (article lists, word-repeat exceptions,
  wrong-word/phrase replacement tables, word coherency, wrong-word-in-context,
  uppercase-sentence-start exceptions, adverb→adjective table, compound
  antipatterns) and the builtin lists copied from the release (compounds,
  specific-case phrases, wordiness, redundancy and profanity lists), and
- the OpenNLP chunker model, taken from nlprule's published `en.zip`
  (<https://f000.backblazeb2.com/file/nlprule/en.zip>, sha256
  `57a0ded6f3becbb669835c0ef9a8841c642cd809c005357ff48bf5c6319adf01`), which
  LanguageTool 6.6 reuses unchanged.

The binaries are built from pinned inputs by `scripts/build-data.sh` in the
[writemorebetter repository](https://github.com/cmlarsen/writemorebetter); the same
script runs in CI and in the release workflow.

## Licensing

LanguageTool is licensed under the LGPL-2.1. Under LGPL-2.1 §6 an application
that uses this data must let the user replace it. Load `en_tokenizer.bin` and
`en_rules.bin` as **separate files** at runtime (for example from a resource
directory the user can write to) rather than linking them into the application
binary, so a modified copy of the data can be substituted.

writemorebetter is not affiliated with LanguageTool or LanguageTooler GmbH.
LanguageTool is a trademark of LanguageTooler GmbH.
