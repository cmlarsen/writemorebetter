"""Check a text file against LanguageTool's public API (free tier: ~20 KB/request, 20 req/min).

Usage: python3 bench/tools/lt_api.py <input.txt> <output.json> [--language en-US]
Sends the text to api.languagetool.org; do not run it on anything that must stay private.
"""
import argparse
import json
import time
import urllib.parse
import urllib.request

CHUNK_BYTES = 18000


def chunks(text):
    out, cur, first_line = [], [], 1
    for i, line in enumerate(text.split("\n")):
        if cur and sum(len(x.encode()) + 1 for x in cur) + len(line.encode()) > CHUNK_BYTES:
            out.append((first_line, "\n".join(cur)))
            cur, first_line = [], i + 1
        cur.append(line)
    out.append((first_line, "\n".join(cur)))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("input")
    ap.add_argument("output")
    ap.add_argument("--language", default="en-US")
    args = ap.parse_args()

    text = open(args.input, encoding="utf-8").read()
    matches, version = [], None
    for first_line, chunk in chunks(text):
        data = urllib.parse.urlencode({"text": chunk, "language": args.language}).encode()
        r = json.load(urllib.request.urlopen("https://api.languagetool.org/v2/check", data, timeout=60))
        version = r.get("software", {}).get("version")
        for m in r["matches"]:
            matches.append({
                "line": first_line + chunk[: m["offset"]].count("\n"),
                "rule": m["rule"]["id"],
                "cat": m["rule"]["category"]["id"],
                "text": chunk[m["offset"] : m["offset"] + m["length"]],
                "repl": [x["value"] for x in m["replacements"][:3]],
                "msg": m["shortMessage"] or m["message"][:80],
            })
        time.sleep(4)
    json.dump(matches, open(args.output, "w"), indent=0)
    print(f"{len(matches)} matches; LanguageTool {version}")


if __name__ == "__main__":
    main()
