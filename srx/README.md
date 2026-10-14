# SRX

> **Vendored.** This is the `srx` crate, vendored from crates.io `srx` 0.1.4 with
> modifications by the writemorebetter project: the per-rule regexes are built with
> `regex-automata` with the one-pass DFA disabled, and the compiled regex is stored
> alongside its pattern string so serialization is unchanged. See `Cargo.toml`.
> Rules match like the Java SRX implementation LanguageTool uses: whitespace inside
> `<beforebreak>`/`<afterbreak>` is kept, matches of one rule may overlap, a `break="no"`
> rule holds at every index where its `beforebreak` can end and its `afterbreak` matches, the
> split index is a named group so capture groups in `beforebreak` do not move it, and
> patterns with look-around fall back to `fancy-regex`.
> The upstream tests, benchmark and `data/` samples were removed because that data is
> LanguageTool-derived and is not committed here; the remaining tests are data-free.

[![Crates.io](https://img.shields.io/crates/v/srx)](https://crates.io/crates/srx)
[![Docs.rs](https://docs.rs/srx/badge.svg)](https://docs.rs/srx)
![MIT OR Apache 2.0 license](https://img.shields.io/crates/l/srx)

A simple and reasonably fast Rust implementation of the [Segmentation Rules eXchange 2.0 standard](https://www.unicode.org/uli/pas/srx/srx20.html) for text segmentation. `srx` is *not* fully compliant with the standard.

This crate is intended for segmentation of plaintext so markup information (`<formathandle>` and `segmentsubflows`) is ignored.

Not complying with the SRX spec, overlapping matches of the same `<rule>` are not found which could lead to different behavior in a few edge cases.

## A note on regular expressions

This crate uses the [`regex` crate](https://github.com/rust-lang/regex) for parsing and executing regular expressions. The `regex` crate is mostly compatible with the [regular expression standard](https://www.unicode.org/uli/pas/srx/srx20.html#Intro_RegExp) from the SRX specification. However, some metacharacters such as `\Q` and `\E` are not supported.

To still be able to use files containing unsupported rules and to parse useful SRX files such as [`segment.srx` from LanguageTool](https://github.com/languagetool-org/languagetool/blob/master/languagetool-core/src/main/resources/org/languagetool/resource/segment.srx) which does not comply with the standard by e. g. using look-ahead and look-behind, `srx` ignores `<rule>` elements with invalid regular expressions and provides information about them via the `srx.errors()` function.