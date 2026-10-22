"""Extract LanguageTool's curated English spelling suggestions into a builtin data list.

LanguageTool's English speller rule puts hand-written answers before (or instead of) the
Morfologik speller's suggestions for common typos. They live in Java source, not in a
resource file:

- ``AbstractEnglishSpellerRule.getTopSuggestions()``: ``s.put(word, Arrays.asList(...))``
  entries, matched on the word as written;
- ``AbstractEnglishSpellerRule.getTopSuggestionsIgnoreCase()``: the same, matched on the
  lowercased word;
- ``AbstractEnglishSpellerRule.getAdditionalTopSuggestions()``: its suffix rewrite for a
  word with no curated entry (``word.endsWith(X)`` / ``replaceFirst("X$", Y)``);
- ``AbstractEnglishSpellerRule.getOnlySuggestions()``: patterns whose suggestions replace
  the speller's (``topMatch(...)``, ``StringUtils.replaceOnce(word, A, B)`` or a list);
- ``MorfologikAmericanSpellerRule.getAdditionalTopSuggestions()``: American-only entries
  (``"x".equals(word)`` -> ``Arrays.asList(...)``), which replace the shared ones.

The output is the tab-separated list described at the top of the generated file. The
generated file is LanguageTool-derived data (LGPL-2.1); this script holds no entries itself.

Usage:
  python3 build/extract_speller_suggestions.py --ref v6.6 > out.txt
  python3 build/extract_speller_suggestions.py --abstract A.java --american B.java > out.txt

Without ``--abstract``/``--american`` the files are downloaded from GitHub at ``--ref``.
"""
import argparse
import re
import sys
import urllib.request

REPO = "https://raw.githubusercontent.com/languagetool-org/languagetool"
DIR = "languagetool-language-modules/en/src/main/java/org/languagetool/rules/en"
ABSTRACT = "AbstractEnglishSpellerRule.java"
AMERICAN = "MorfologikAmericanSpellerRule.java"

STRING = r'"((?:[^"\\]|\\.)*)"'


def unescape(literal):
    """Decodes the escapes a Java string literal may use."""
    out, i = [], 0
    while i < len(literal):
        c = literal[i]
        if c != "\\":
            out.append(c)
            i += 1
            continue
        nxt = literal[i + 1]
        if nxt == "u":
            out.append(chr(int(literal[i + 2:i + 6], 16)))
            i += 6
            continue
        out.append({"n": "\n", "t": "\t", "\\": "\\", '"': '"', "'": "'"}[nxt])
        i += 2
    return "".join(out)


def strings(text):
    return [unescape(s) for s in re.findall(STRING, text)]


def method_body(source, signature):
    """The text of the method whose declaration contains `signature`, braces balanced."""
    start = source.index(signature)
    i = source.index("{", start)
    depth = 0
    for j in range(i, len(source)):
        if source[j] == "{":
            depth += 1
        elif source[j] == "}":
            depth -= 1
            if depth == 0:
                return source[i + 1:j]
    raise ValueError(f"unbalanced method {signature}")


def strip_comments(text):
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    return re.sub(r"//[^\n]*", "", text)


def check(field):
    if "\t" in field or "\n" in field:
        raise ValueError(f"field {field!r} contains a tab or newline")
    return field


def row(*fields):
    return "\t".join(check(f) for f in fields)


def puts(body):
    """`s.put(word, Arrays.asList(...))` entries; a later put of a word replaces the earlier."""
    entries = {}
    for key, values in re.findall(r"s\.put\(\s*" + STRING + r"\s*,\s*Arrays\.asList\((.*?)\)\s*\)\s*;",
                                  body, flags=re.S):
        entries[unescape(key)] = strings(values)
    return entries


def only_suggestions(source):
    patterns = {name: unescape(regex) for name, regex in re.findall(
        r"Pattern\s+(\w+)\s*=\s*compile\(\s*" + STRING + r"\s*\)\s*;", source)}
    body = strip_comments(method_body(source, "List<SuggestedReplacement> getOnlySuggestions(String word)"))
    out = []
    statement = re.compile(
        r"if\s*\((?P<cond>(?:[^(){}]|\([^(){}]*\))*)\)\s*"
        r"(?:\{(?P<block>.*?)return\s+l\s*;\s*\}|return\s+topMatch\((?P<top>.*?)\)\s*;)",
        flags=re.S)
    for m in statement.finditer(body):
        names = re.findall(r"(\w+)\.matcher\(word\)\.matches\(\)", m["cond"])
        if not names:
            raise ValueError(f"unexpected condition {m['cond']!r}")
        if m["block"] is not None:
            action = ["list"] + [unescape(s) for s in re.findall(
                r"new\s+SuggestedReplacement\(\s*" + STRING + r"\s*\)", m["block"])]
        else:
            top = m["top"].strip()
            once = re.match(r"StringUtils\.replaceOnce\(\s*word\s*,\s*" + STRING + r"\s*,\s*" + STRING + r"\s*\)",
                            top)
            if once:
                # Any further argument is a short description shown next to the suggestion.
                action = ["replace", unescape(once[1]), unescape(once[2])]
            else:
                lit = re.match(STRING, top)
                if not lit:
                    raise ValueError(f"unexpected topMatch argument {top!r}")
                action = ["list", unescape(lit[1])]
        for name in names:
            out.append(row("only", patterns[name], *action))
    return out


def suffix_rewrites(source):
    body = method_body(source, "List<SuggestedReplacement> getAdditionalTopSuggestions(")
    out = []
    for suffix, pattern, replacement in re.findall(
            r"word\.endsWith\(\s*" + STRING + r"\s*\)\s*\)\s*\{\s*String\s+\w+\s*=\s*word\.replaceFirst\(\s*"
            + STRING + r"\s*,\s*" + STRING + r"\s*\)", body):
        suffix, pattern, replacement = unescape(suffix), unescape(pattern), unescape(replacement)
        if pattern != re.escape(suffix) + "$":
            raise ValueError(f"suffix rewrite {pattern!r} is not anchored on {suffix!r}")
        out.append(row("suffix", suffix, replacement))
    return out


def american(source):
    body = method_body(source, "List<SuggestedReplacement> getAdditionalTopSuggestions(")
    return [row("top:en_US", unescape(word), *strings(values)) for word, values in re.findall(
        STRING + r"\.equals\(word\)\s*\)\s*\{\s*s\s*=\s*Arrays\.asList\((.*?)\)\s*;", body, flags=re.S)]


def read(path, name, ref):
    if path:
        with open(path, encoding="utf-8") as f:
            return f.read()
    with urllib.request.urlopen(f"{REPO}/{ref}/{DIR}/{name}", timeout=60) as reply:
        return reply.read().decode("utf-8")


HEADER = """\
# Curated spelling suggestions for the English speller.
#
# Generated by build/extract_speller_suggestions.py from LanguageTool's
# AbstractEnglishSpellerRule.java and MorfologikAmericanSpellerRule.java
# (languagetool-language-modules/en/src/main/java/org/languagetool/rules/en/)
# at {ref}. LanguageTool-derived data, LGPL-2.1. Do not edit by hand; see
# build/builtin-data/en/README.md.
#
# One tab-separated entry per line:
#   top<TAB>word<TAB>suggestion...              word as written
#   top-ignore-case<TAB>word<TAB>suggestion...  lowercased word
#   top:<variant><TAB>word<TAB>suggestion...    one variant only; replaces the entries above
#   suffix<TAB>ending<TAB>replacement           with no entry above: the word with its ending
#                                               replaced, if that is a correct word
#   only<TAB>regex<TAB>list<TAB>suggestion...   whole word matches the (Java) regex: these
#   only<TAB>regex<TAB>replace<TAB>from<TAB>to  suggestions, or the word with the first `from`
#                                               replaced by `to`, instead of the speller's.
#                                               The first matching line wins.
# The top, top-ignore-case and suffix suggestions go before the speller's.
"""


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--ref", default="v6.6", help="LanguageTool git ref (tag or commit)")
    ap.add_argument("--abstract", help=f"local {ABSTRACT} instead of downloading it")
    ap.add_argument("--american", help=f"local {AMERICAN} instead of downloading it")
    args = ap.parse_args()
    abstract = read(args.abstract, ABSTRACT, args.ref)
    us = read(args.american, AMERICAN, args.ref)

    lines = [HEADER.format(ref=args.ref).rstrip("\n")]
    top = puts(strip_comments(method_body(abstract, "Map<String, List<String>> getTopSuggestions()")))
    ignore_case = puts(strip_comments(method_body(abstract, "Map<String, List<String>> getTopSuggestionsIgnoreCase()")))
    lines += [row("top", k, *v) for k, v in top.items()]
    lines += [row("top-ignore-case", k, *v) for k, v in ignore_case.items()]
    lines += american(strip_comments(us))
    lines += suffix_rewrites(strip_comments(abstract))
    lines += only_suggestions(abstract)
    sys.stdout.write("\n".join(lines) + "\n")
    counts = {kind: sum(1 for l in lines[1:] if l.split("\t", 1)[0] == kind)
              for kind in ("top", "top-ignore-case", "top:en_US", "suffix", "only")}
    print(f"extracted {counts}", file=sys.stderr)


if __name__ == "__main__":
    main()
