# Independent English analysis probes

These inputs are invented for this repository. Expected readings in `tests/analysis.rs` were taken
from the **LanguageTool 6.6** distribution's own `--taggeronly` analysis, not from nlprule output.
The adjacent `*.tags.txt` files contain that CLI's stdout, with trailing spaces removed. Inputs
retain their exact original punctuation and whitespace; `fiction_unicode.txt` includes an emoji,
curly quotes/apostrophe, an em dash, an ellipsis and U+2028. `paragraphs.txt` has a blank line.

Reproduce from the repository root with the pinned distribution downloaded by `scripts/build-data.sh`:

```sh
LT_DIR=data/cache/LanguageTool-6.6
# Set LT_JAVA to a Java 17+ runtime, e.g. the portable JRE under data/cache/temurin21-jre/.
for probe in multiword dropped_g fiction_unicode paragraphs; do
  "$LT_JAVA" -jar "$LT_DIR/languagetool-commandline.jar" --taggeronly -l en -c utf-8 \
    "nlprule/tests/fixtures/analysis/$probe.txt" |
    sed 's/[[:blank:]]*$//' > "nlprule/tests/fixtures/analysis/$probe.tags.txt"
done
```

The distribution zip's SHA-256 is `53600506b399bb5ffe1e4c8dec794fd378212f14aaf38ccef9b6f89314d11631`.

The reference normalizes curly apostrophes in its printed text. The public API preserves original
surface text and offsets while exporting the post-disambiguation lemma/POS readings. Internal
matcher sentinels (empty POS, UNKNOWN, SENT_START, SENT_END) are deliberately omitted. This is an
export of the existing tokenizer, not a new claim of complete tagger parity: the tests assert selected
independent readings and a chunk tag, alongside span coverage, punctuation and whitespace behavior.
