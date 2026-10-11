# writemorebetter

An embeddable, LanguageTool-grade grammar and spelling checker in Rust — for the browser (WASM), desktop, and
native apps, with no Java server. English first.

writemorebetter will build on [nlprule](https://github.com/bminixhofer/nlprule), which compiles
[LanguageTool](https://github.com/languagetool-org/languagetool)'s open rule set into a fast Rust engine.
It aims to bring that work up to current LanguageTool rules, add the speller and LanguageTool's Java-coded rules,
cut memory to something a browser tab or phone can carry, and tune a default profile for fiction.

**Status:** research and scaffolding. No engine code yet. Start with [`docs/FINDINGS.md`](docs/FINDINGS.md)
for the evaluation behind this project and the plan.

## Repository layout

- `docs/FINDINGS.md` — evaluation of LanguageTool, Harper, nlprule, and a recent nlprule fork, with the plan.
- `bench/` — the comparison harness: run each checker over a corpus and score recall against verified errors.
  See [`bench/README.md`](bench/README.md).

## License

Engine code: MIT or Apache-2.0, at your option. Compiled rule and dictionary data derived from LanguageTool
will be LGPL-2.1, as in nlprule. writemorebetter is not affiliated with LanguageTool or LanguageTooler GmbH.
