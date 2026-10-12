#!/usr/bin/env python3
"""Build the `suggest` seed corpus.

Paragraphs of the committed public texts (bench/public) plus the handcrafted boundary
cases in the fuzz harness. The handcrafted strings live in `src/lib.rs`; this script
asks the stress binary to write them so there is a single source of truth.

Usage: python3 make_seed_corpus.py [--outcorpus DIR] [--stress BIN]
"""
import argparse
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]  # repo root


def paragraphs(text: str, max_bytes: int = 4000):
    for block in text.split("\n\n"):
        block = block.strip("\r\n")
        if not block.strip():
            continue
        if len(block.encode("utf-8")) <= max_bytes:
            yield block
            continue
        # Cut an over-long paragraph on whitespace near the cap.
        words = block.split(" ")
        chunk = []
        size = 0
        for word in words:
            size += len(word.encode("utf-8")) + 1
            if size > max_bytes and chunk:
                yield " ".join(chunk)
                chunk = []
                size = len(word.encode("utf-8")) + 1
            chunk.append(word)
        if chunk:
            yield " ".join(chunk)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--outcorpus", type=Path, default=Path(__file__).resolve().parent / "corpus" / "suggest")
    parser.add_argument("--stress", type=Path, default=Path(__file__).resolve().parent / "target" / "release" / "stress")
    args = parser.parse_args()

    out = args.outcorpus
    out.mkdir(parents=True, exist_ok=True)

    written = 0
    public = ROOT / "bench" / "public"
    for path in sorted(public.glob("*.txt")):
        text = path.read_text(encoding="utf-8")
        for i, para in enumerate(paragraphs(text)):
            (out / f"pub_{path.stem}_{i:04d}").write_bytes(para.encode("utf-8"))
            written += 1

    if args.stress.exists():
        subprocess.run(
            [str(args.stress), "--dump-handcrafted", str(out)],
            check=True,
            env={**__import__("os").environ, "WMB_FUZZ_DIR": str(ROOT / "storage" / "66")},
        )
    else:
        print(f"warning: {args.stress} not built; skipping handcrafted dump", file=sys.stderr)

    print(f"wrote {written} public-text paragraphs to {out}")


if __name__ == "__main__":
    main()
