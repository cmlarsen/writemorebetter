# Forked rule XML (English)

`grammar.xml` is LanguageTool's English grammar rule file
(https://github.com/languagetool-org/languagetool), licensed under **LGPL-2.1**.
It is data, not engine code: `scripts/build-data.sh` passes it to
`build/make_build_dir.py --grammar_path` in place of the copy in the
LanguageTool release, and it ends up serialized inside `en_rules.bin`.
`README.md` itself is writemorebetter documentation.

Provenance: LanguageTool 6.6, `org/languagetool/rules/en/grammar.xml` from
<https://languagetool.org/download/LanguageTool-6.6.zip> (sha256
`53600506b399bb5ffe1e4c8dec794fd378212f14aaf38ccef9b6f89314d11631`), source tag
<https://github.com/languagetool-org/languagetool/tree/v6.6>.

## Local edits

Every change from the 6.6 file is listed here. Each one adds `<example>`
sentences, so LanguageTool's conformance suite covers it. To see the edits as a
diff, extract the original from the release zip and diff it against this file:

```sh
unzip -p data/cache/LanguageTool-6.6.zip \
    LanguageTool-6.6/org/languagetool/rules/en/grammar.xml | diff - build/lt-rules/en/grammar.xml
```

- `OTHER_OTHERS` (first rule): an antipattern for the idiom "X or other"
  ("one way or other", "something or other", "some fool or other"), which the
  rule flagged when a verb followed it ("one way or other served" → "others").
- `YOUR_YOU_RE`: two rules appended to the group, for "your welcome
  (here) anytime" and "your welcome to <verb>" (→ "you're"). LanguageTool's
  rule only fires when "Your welcome" is the whole clause at a sentence start.
- `HE_THE`: a rule appended to the group for "he" written for "the" or "his"
  after a gerund ("Popping he SD card out").
- `A_INFINITIVE` (second rule): an antipattern for "the heal(s) of", so that
  `HEAL_OF` reports it with the right suggestion.
- New rules `PEAKED_THROUGH` ("peaked through" → "peeked"), `HEAL_OF` ("the
  heal of her shoe" → "heel") and `HEAL_POCKET` ("heal pocket/cup" → "heel"),
  after `WEATHER_WHETHER` in the `TYPOS` category. LanguageTool catches these
  pairs with its n-gram data, which the on-device engine does not ship.
- New rule `EACH_OF_NNS_VBP` ("…that each of the dogs are" → "is"), after
  `AGREEMENT_SENT_START`, whose version only fires at a sentence start. It
  skips "each of…" in object position (after a noun, a modal, "to", or verbs
  such as "let", "make" and "call"), which misfired on published fiction.
- New rule group `THERE_ARE_WHAT` ("There are what I thought was" → "is"),
  after `EACH_OF_NNS_VBP`.

Rules appended to an existing group keep the sub-rule numbers of the rules
before them, so ids such as `YOUR_YOU_RE/8` do not change.

When moving to a newer LanguageTool release, take its `grammar.xml` and
re-apply the edits above.
