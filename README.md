# writemorebetter

An embeddable, LanguageTool-grade grammar and spelling checker in Rust — for the browser (WASM), desktop, and
native apps, with no Java server. English first.

writemorebetter will build on [nlprule](https://github.com/bminixhofer/nlprule), which compiles
[LanguageTool](https://github.com/languagetool-org/languagetool)'s open rule set into a fast Rust engine.
It aims to bring that work up to current LanguageTool rules, add the speller and LanguageTool's Java-coded rules,
cut memory to something a browser tab or phone can carry, and tune a default profile for fiction.

**Status:** early. The engine is upstream nlprule (`main`, LT 5.2 rules) with its unmerged spellcheck branch
merged in. Start with [`docs/FINDINGS.md`](docs/FINDINGS.md) for the evaluation behind this project and the plan.

## Repository layout

- `docs/FINDINGS.md` — evaluation of LanguageTool, Harper, nlprule, and a recent nlprule fork, with the plan.
- `nlprule/` — the engine crate (tokenizer, tagger, disambiguation, rules, speller), imported from nlprule with its
  history. `build/` makes build directories and fetches binaries; `python/` holds upstream's Python bindings.
- `swift/` — UniFFI bindings and the Swift package (`Package.swift`, `scripts/build-xcframework.sh`); see
  [`swift/README.md`](swift/README.md).
- `docs/NLPRULE.md` — upstream nlprule's README.
- `bench/` — the comparison harness: run each checker over a corpus and score recall against verified errors.
  See [`bench/README.md`](bench/README.md).

## License

Engine code: MIT or Apache-2.0, at your option. It includes nlprule, copyright Benjamin Minixhofer, under the same
licenses; see [`NOTICE`](NOTICE). Compiled rule and dictionary data derived from LanguageTool are LGPL-2.1, as in
nlprule. writemorebetter is not affiliated with LanguageTool or LanguageTooler GmbH.
