"""Compare writemorebetter with LanguageTool 6.6 (local) and LanguageTool's public API.

Usage:
  python3 bench/tools/parity.py "Some sentence . . . to check."
  python3 bench/tools/parity.py --file snippet.txt [--profile fiction] [--no-api] [--no-local]

Prints one row per flag: line, rule id, flagged text, and which checkers report it
(wmb, 6.6 = the LanguageTool 6.6 command line from data/cache, api = api.languagetool.org,
which runs a newer LanguageTool). Rule ids are compared without category and sub-rule
number, and the speller ids are unified as SPELL.

The API sends the text to a third party: only use it on text you wrote yourself, never on
the private corpus or the published-fiction units. Pass --no-api for anything else.
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PROBE = ROOT / "bench/tools/wmb-probe/target/release/wmb-probe"
LT_DIR = ROOT / "data/cache/LanguageTool-6.6"
SPELL_IDS = {"SPELLCHECK", "MORFOLOGIK_RULE_EN_US", "HUNSPELL_RULE"}


def normalize(rule):
    parts = rule.split("/")
    if parts[0] in ("BUILTIN",):
        name = parts[1]
    elif len(parts) >= 2:
        name = parts[1]
    else:
        name = parts[0]
    return "SPELL" if parts[0] in SPELL_IDS or name in SPELL_IDS else name


def utf16_to_index(text):
    table, units = [], 0
    for i, ch in enumerate(text):
        table.append((units, i))
        units += 2 if ord(ch) > 0xFFFF else 1
    lookup = {u: i for u, i in table}
    lookup[units] = len(text)
    return lambda u: lookup.get(u, len(text))


def from_lt_json(text, matches):
    index = utf16_to_index(text)
    out = []
    for m in matches:
        start, end = index(m["offset"]), index(m["offset"] + m["length"])
        out.append((text[:start].count("\n") + 1, normalize(m["rule"]["id"]), text[start:end]))
    return out


def run_wmb(path, bins, profile):
    env = dict(os.environ, SHOW="1", PROFILE=profile)
    result = subprocess.run([str(PROBE), str(bins), str(path)], env=env, capture_output=True, text=True)
    out = []
    for raw in result.stdout.splitlines():
        parts = raw.split(" | ")
        if len(parts) >= 3 and parts[0].isdigit():
            flagged = json.loads(parts[2].split(" -> ")[0])
            out.append((int(parts[0]), normalize(parts[1].strip()), flagged))
    return out


def run_local(path, text):
    java = next((p for p in (ROOT / "data/cache").glob("temurin21-jre/**/bin/java") if os.access(p, os.X_OK)), None)
    jar = LT_DIR / "languagetool-commandline.jar"
    if java is None or not jar.exists():
        sys.exit("LanguageTool 6.6 or its JRE is missing from data/cache; run scripts/build-data.sh once")
    result = subprocess.run([str(java), "-jar", str(jar), "-l", "en-US", "--json", str(path)],
                            capture_output=True, text=True)
    start = result.stdout.find("{")
    return from_lt_json(text, json.loads(result.stdout[start:])["matches"])


def run_api(text):
    data = urllib.parse.urlencode({"text": text, "language": "en-US"}).encode()
    reply = json.load(urllib.request.urlopen("https://api.languagetool.org/v2/check", data, timeout=60))
    return reply.get("software", {}).get("version", "?"), from_lt_json(text, reply["matches"])


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("text", nargs="?")
    ap.add_argument("--file", type=Path)
    ap.add_argument("--bins", type=Path, default=ROOT / "storage")
    ap.add_argument("--profile", default="default")
    ap.add_argument("--no-api", action="store_true")
    ap.add_argument("--no-local", action="store_true")
    args = ap.parse_args()
    if (args.text is None) == (args.file is None):
        ap.error("give either TEXT or --file")
    text = args.file.read_text(encoding="utf-8") if args.file else args.text.replace("\\n", "\n")
    if args.file and len(text.encode()) > 18000 and not args.no_api:
        ap.error("text is over the API's 18 KB limit; shorten it or pass --no-api")

    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "parity.txt"
        path.write_text(text, encoding="utf-8")
        results = {"wmb": run_wmb(path, args.bins, args.profile)}
        if not args.no_local:
            results["6.6"] = run_local(path, text)
    columns = list(results)
    if not args.no_api:
        version, results["api"] = run_api(text)
        columns.append("api")
        print(f"api: LanguageTool {version}")

    rows = {}
    for name, flags in results.items():
        for flag in flags:
            rows.setdefault(flag, set()).add(name)
    width = max([len(rule) for _, rule, _ in rows] + [4])
    print(f"{'line':>4}  {'rule':<{width}}  " + "  ".join(f"{c:>3}" for c in columns) + "  text")
    for (line, rule, flagged), seen in sorted(rows.items()):
        marks = "  ".join(f"{'x' if c in seen else '.':>3}" for c in columns)
        print(f"{line:>4}  {rule:<{width}}  {marks}  {flagged[:50]!r}")
    if args.profile != "default":
        print(f"(wmb ran with the {args.profile} profile; LanguageTool has no profiles)")


if __name__ == "__main__":
    main()
