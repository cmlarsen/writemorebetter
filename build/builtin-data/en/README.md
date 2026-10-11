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

- `word_repeat_exceptions.txt` — repetitions that the English word-repeat rule
  should not flag, taken from LanguageTool's `WordRepeatRule` and
  `EnglishWordRepeatRule`. A single word allows that word to be repeated; a
  multi-word entry allows the exact token sequence to contain the repetition
  (used for LanguageTool's phrase- and part-of-speech-based exceptions).
- `uppercase_sentence_start_exceptions.txt` — tokens that may start a sentence
  in lowercase without being an error, taken from the exceptions of the
  corresponding capitalization rule. One token per line.
