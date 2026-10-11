# Builtin data (English)

These word lists are derived from LanguageTool
(https://github.com/languagetool-org/languagetool) and are licensed under
**LGPL-2.1**. They are data, not engine code: they are copied into a build
directory before compiling and end up serialized inside `en_rules.bin`, so they
never enter the MIT/Apache-2.0 engine sources.

## Usage

The compiler reads these lists from `<build_dir>/builtin/<name>.txt`. Copy the
files into the build directory before compiling, e.g.:

```sh
mkdir -p data/en/builtin
cp -r build/builtin-data/en/. data/en/builtin/
```

## Files

- `comma_parenthesis_whitespace_domains.txt` — top level domains that the
  English comma/parenthesis-whitespace rule accepts directly after a full stop
  (e.g. `foo.org`), taken from LanguageTool's `CommaWhitespaceRule`. The rule
  also accepts any two-letter sequence and file-name suffixes such as `.exe`;
  those are shapes and stay in the engine source.
- `a_vs_an_a.txt` — words that take the determiner "a" even though they begin
  with a vowel letter (e.g. "university", "one"). Taken from LanguageTool's
  `det_a.txt`. Lines starting with `*` are case-sensitive; all others are
  matched case-insensitively.
- `a_vs_an_an.txt` — words that take the determiner "an" even though they do not
  begin with a vowel letter (e.g. "hour", "MBA"). Taken from LanguageTool's
  `det_an.txt`. Lines starting with `*` are case-sensitive; all others are
  matched case-insensitively.
- `word_repeat_exceptions.txt` — repetitions that the English word-repeat rule
  should not flag, taken from LanguageTool's `WordRepeatRule` and
  `EnglishWordRepeatRule`. A single word allows that word to be repeated; a
  multi-word entry allows the exact token sequence to contain the repetition
  (used for LanguageTool's phrase- and part-of-speech-based exceptions).
- `word_coherency.txt` — pairs of admitted spellings of the same word (e.g.
  `adviser;advisor`, `analyse;analyze`), one pair per line. Taken from
  LanguageTool's `WordCoherencyRule` data. The English word-coherency builtin
  flags the second spelling when a text mixes both spellings of a pair.
