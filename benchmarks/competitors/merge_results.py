#!/usr/bin/env python3
"""Merge chunked benchmark summaries into one results directory.

Usage:
    python3 benchmarks/competitors/merge_results.py \
      --out benchmarks/results-full \
      results-final-A results-final-B
"""

import argparse
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("chunks", nargs="+", type=Path, help="chunk result directories")
    parser.add_argument("--out", type=Path, required=True, help="destination directory")
    args = parser.parse_args()

    merged = None
    chunks = []
    for chunk in args.chunks:
        path = chunk / "summary.json"
        data = json.loads(path.read_text())
        chunks.append({
            "source": str(path),
            "date": data["metadata"].get("date"),
            "fixtures": [row["name"] for row in data["fixtures"]],
            "load_average_start": data["metadata"].get("load_average_start"),
            "load_average_end": data["metadata"].get("load_average_end"),
        })
        if merged is None:
            merged = data
        else:
            merged["fixtures"].extend(data["fixtures"])

    merged["metadata"]["chunks"] = chunks
    merged["metadata"]["load_average_end"] = chunks[-1]["load_average_end"]
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "summary.json").write_text(json.dumps(merged, indent=2) + "\n")
    (args.out / "summary.md").write_text(
        "# Competitor benchmark (merged chunks)\n\n"
        "See `summary.json` for the per-fixture paired results and metadata.\n"
    )
    print(f"wrote {args.out / 'summary.json'} with {len(merged['fixtures'])} fixtures")


if __name__ == "__main__":
    main()
