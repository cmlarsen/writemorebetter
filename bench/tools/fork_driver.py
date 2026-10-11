"""Run the alexvinasdev nlprule fork's check_server over a file, one paragraph per line,
restarting past any paragraph that panics the process (it does, on curly quotes and U+2028).

Usage: python3 bench/tools/fork_driver.py <check_server> <en_tokenizer.bin> <en_rules.bin> <input.txt> <output.json>
"""
import json
import subprocess
import sys


def main():
    server, tokenizer, rules, path, out = sys.argv[1:6]
    lines = open(path, encoding="utf-8").read().split("\n")
    results, crashes, start = [], [], 0
    while start < len(lines):
        p = subprocess.run(
            [server, tokenizer, rules, "en"],
            input="\n".join(lines[start:]) + "\n",
            capture_output=True,
            text=True,
        )
        got = [json.loads(l) for l in p.stdout.split("\n") if l.strip()]
        for g in got:
            g["i"] += start
            results.append(g)
        done = start + len(got)
        if p.returncode == 0:
            break
        panic = next((l for l in p.stderr.split("\n") if "panicked" in l or "byte index" in l), "?")
        crashes.append({"line": done + 1, "error": panic.strip()[:200]})
        start = done + 1
    json.dump({"results": results, "crashes": crashes}, open(out, "w"))
    print(f"{path}: {len(crashes)} paragraph crashes {[c['line'] for c in crashes]}")


if __name__ == "__main__":
    main()
