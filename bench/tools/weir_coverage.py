"""Estimate how many active LanguageTool rules Harper's Weir language could express,
and how coverage grows as Weir gains each LT matcher feature.

Usage: python3 bench/tools/weir_coverage.py grammar.xml style.xml [--rule ID ...]
Get the XML from languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/.
"""
import argparse
import collections
import itertools
import re
import xml.etree.ElementTree as ET

COARSE_POS = re.compile(r"^(NN\.\*|NN.?\*|NNS?|NNP\.?\*?|DT|PRP|PRP\$|JJ\.\*|JJ|RB\.\*|RB|VB\.\*|IN|CC|CD|UH|MD|TO)$")
WORD_ALTERNATION = re.compile(r"^[\w' -]+(\|[\w' -]+)*$")
FEATURE_ORDER = ["case", "regex", "backref", "lemma", "penn", "skip", "synth", "chunk", "spacing", "unify", "regexp-rule"]


def token_features(t, out):
    a = t.attrib
    if "skip" in a or "min" in a or "max" in a:
        out.add("skip")
    if "inflected" in a:
        out.add("lemma")
    if "chunk" in a or "chunk_re" in a:
        out.add("chunk")
    if "spacebefore" in a:
        out.add("spacing")
    if a.get("case_sensitive") == "yes":
        out.add("case")
    if "postag" in a and not COARSE_POS.match(a["postag"]):
        out.add("penn")
    if a.get("regexp") == "yes" and not WORD_ALTERNATION.match((t.text or "").strip()):
        out.add("regex")
    if t.find("match") is not None:
        out.add("backref")
    for e in t.findall("exception"):
        token_features(e, out)


def rule_features(r, group):
    out = set()
    p = r.find("pattern")
    if p is None:
        out.add("regexp-rule")
    else:
        if p.attrib.get("case_sensitive") == "yes":
            out.add("case")
        if any(p.find(f".//{x}") is not None for x in ("unify", "or", "and")):
            out.add("unify")
        for t in p.iter("token"):
            token_features(t, out)
    antipatterns = list(r.findall("antipattern")) + (list(group.findall("antipattern")) if group is not None else [])
    for a in antipatterns:
        for t in a.iter("token"):
            token_features(t, out)
    for s in itertools.chain(r.iter("suggestion"), r.iter("message")):
        if s.find("match") is not None:
            out.add("synth")
        elif re.search(r"\\\d", "".join(s.itertext())):
            out.add("backref")
    return out


def active_rules(paths):
    for path in paths:
        for cat in ET.parse(path).getroot().iter("category"):
            if cat.attrib.get("default") == "off":
                continue
            for el in cat:
                if el.tag == "rule":
                    pairs = [(el, None)]
                elif el.tag == "rulegroup":
                    pairs = [(r, el) for r in el.findall("rule")]
                else:
                    continue
                for r, g in pairs:
                    default = r.attrib.get("default") or (g.attrib.get("default") if g is not None else "") or ""
                    if default in ("off", "temp_off"):
                        continue
                    yield (g if g is not None else r).attrib.get("id"), rule_features(r, g)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("xml", nargs="+")
    ap.add_argument("--rule", nargs="*", default=[])
    args = ap.parse_args()

    rules = list(active_rules(args.xml))
    by_id = collections.defaultdict(set)
    for rid, feats in rules:
        by_id[rid] |= feats
    n = len(rules)
    usage = collections.Counter(f for _, feats in rules for f in feats)

    print(f"active rules: {n}")
    print("rules using each feature:", usage.most_common())
    have = set()
    k = sum(1 for _, f in rules if not f)
    print(f"Weir today      {k:5d} ({100 * k / n:.0f}%)")
    for feat in FEATURE_ORDER:
        have.add(feat)
        k = sum(1 for _, f in rules if f <= have)
        print(f"+{feat:14s} {k:5d} ({100 * k / n:.0f}%)")
    for rid in args.rule:
        print(f"{rid:28s}", sorted(by_id[rid]) if rid in by_id else "not an XML rule (Java builtin or n-gram)")


if __name__ == "__main__":
    main()
