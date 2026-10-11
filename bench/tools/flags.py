"""Load each checker's raw output from bench/runs/ into one shape: (line, rule, flagged_text, replacements)."""
import json
import re
from pathlib import Path

NOISE = re.compile(
    r"TRAILING|Spaces|NoFrenchSpaces|EN_QUOTES|WHITESPACE|CONSECUTIVE_SPACES|EN_ELLIPSIS|GERMAN_QUOTES"
)


def harper(runs: Path, name: str):
    d = json.loads((runs / f"harper-{name}.json").read_text())
    items = d if isinstance(d, list) else d.get("lints", d)
    if isinstance(items, list) and items and "lints" in items[0]:
        items = items[0]["lints"]
    for x in items:
        repl = [s.replace("Replace with: ", "") for s in x.get("suggestions", [])[:2]]
        yield x["line"], x["rule"], x["matched_text"], repl


def lt(runs: Path, name: str):
    for x in json.loads((runs / f"lt-{name}.json").read_text()):
        yield x["line"], x["rule"], x["text"], x["repl"][:2]


def nlprule(runs: Path, name: str, prefix: str = "nlp"):
    for raw in (runs / f"{prefix}-{name}.log").read_text().splitlines():
        p = raw.split(" | ")
        if len(p) < 4 or not p[0].isdigit():
            continue
        m = re.match(r"(\".*?\"|'.*?') -> (\[.*\])", p[2])
        text = (m.group(1) if m else p[2]).strip('"')
        yield int(p[0]), p[1].split("/")[1], text, (m.group(2) if m else "")


def writemorebetter(runs: Path, name: str):
    return nlprule(runs, name, "wmb")


def fork(runs: Path, name: str):
    d = json.loads((runs / f"fork-{name}.json").read_text())
    for g in d["results"]:
        b = g["text"].encode()
        for rid, st, en, repl in g["hits"]:
            yield g["i"] + 1, rid, b[st:en].decode(errors="replace"), repl[:2]


TOOLS = {
    "LT 6.9 (API)": ("lt", lt),
    "nlprule fork": ("fork", fork),
    "nlprule 0.6.4": ("nlp", nlprule),
    "writemorebetter": ("wmb", writemorebetter),
    "Harper": ("harper", harper),
}


def available(runs: Path, name: str):
    for label, (prefix, fn) in TOOLS.items():
        if any(runs.glob(f"{prefix}-{name}.*")):
            yield label, fn
