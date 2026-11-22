#!/usr/bin/env python3
"""Labeled accuracy benchmark for Shrike against the JavaScript/TypeScript
dead-code tools.

The performance suite answers "how fast". This suite answers "is the finding
correct" against a corpus where the answer is known before the tools run.

Corpus
------
`fixtures/must-not-report` is Shrike's false-positive gate: 315 minimal
projects, each with framework-derived labels.

  protected    symbols that must never be reported as dead
  mustReport   symbols that must be reported
  unusedFiles  files that must be reported unused
  usedFiles    files that must not be reported unused
  internalOnly Shrike-only advisory expectation

The labels are derived from framework documentation, not from any analyzer's
output, so a tool cannot pass by agreeing with itself. The same corpus is run
by Shrike's CI; this harness runs the competitors on it.

Scoring
-------
For each case and tool, reported symbols are normalized to `file#name` and
reported files to case-relative paths.

  TP = mustReport symbols found + unusedFiles reported
  FP = protected symbols reported + usedFiles reported
  FN = mustReport symbols missed + unusedFiles missed

A mustReport symbol counts as found when the tool reports the symbol or reports
its whole file unused (knip and fallow suppress symbols in unused files).

`usedFiles` reported unused is scored strictly and with a closure allowance:
if every importer of the file is itself reported unused, the file is
transitively dead and is not counted as a false positive. The strict count is
reported alongside. This matters for Shrike's file policy: a file with an
incoming edge is not a file candidate even when that importer is dead; its
export is reported at symbol level instead.

Symbol-level member warnings (unused enum/class/namespace members) are not
scored: the corpus protects the enclosing export, and a member warning is not a
deletion of that export. They are counted separately.

The narrow tools (ts-prune, ts-unused-exports) only see exports, so they are
scored on symbols only; file-level labels are excluded from their metrics.

Usage
-----
    python3 benchmarks/competitors/accuracy.py --work /tmp/opencode/bench-v2
    python3 benchmarks/competitors/accuracy.py --tools shrike-cli,fallow,knip
    python3 benchmarks/competitors/accuracy.py --limit 20 --keep-raw
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CORPUS = ROOT / "fixtures" / "must-not-report"
DEFAULT_WORK = Path(os.environ.get("SHRIKE_BENCH_WORK", "/tmp/opencode/bench-v2"))

FULL_TOOLS = ("shrike-cli", "fallow", "knip")
NARROW_TOOLS = ("ts-prune", "ts-unused-exports")
ALL_TOOLS = FULL_TOOLS + NARROW_TOOLS
EXPECTED_EXITS = {
    "shrike-cli": {0, 1},
    "fallow": {0, 1},
    "knip": {0, 1},
    "ts-prune": {0, 1},
    "ts-unused-exports": {0, 1},
}
CACHE_DIRS = (".shrike", ".fallow", ".knip", "node_modules/.cache/knip")


def which_tool(work, name):
    candidates = [
        work / "tools" / "node_modules" / ".bin" / name,
        Path("/tmp/opencode/bench-v2/tools/node_modules/.bin") / name,
        Path("/tmp/opencode/bench-tools/node_modules/.bin") / name,
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate
    return shutil.which(name)


def tool_paths(work):
    engine = Path(os.environ.get("SHRIKE_ENGINE", ROOT / "target" / "release" / "shrike-engine"))
    cli = Path(os.environ.get("SHRIKE_CLI", ROOT / "packages" / "cli" / "dist" / "main.js"))
    return {
        "node": shutil.which("node"),
        "engine": engine,
        "cli": cli,
        "fallow": which_tool(work, "fallow"),
        "knip": which_tool(work, "knip"),
        "ts-prune": which_tool(work, "ts-prune"),
        "ts-unused-exports": which_tool(work, "ts-unused-exports"),
    }


def tool_command(tool, tools):
    if tool == "shrike-cli":
        return [str(tools["node"]), str(tools["cli"]), "scan", "--json"]
    if tool == "fallow":
        return [str(tools["fallow"]), "dead-code", "--quiet", "--format", "json", "--no-cache"]
    if tool == "knip":
        return [str(tools["knip"]), "--reporter", "json", "--no-progress"]
    if tool == "ts-prune":
        return [str(tools["ts-prune"]), "--project", "tsconfig.json"]
    if tool == "ts-unused-exports":
        return [str(tools["ts-unused-exports"]), "tsconfig.json", "--maxIssues=0"]
    raise ValueError(f"unknown tool: {tool}")


def normalize_path(root, value):
    if not value:
        return value
    value = str(value).replace("\\", "/")
    if os.path.isabs(value):
        try:
            value = os.path.relpath(value, root)
        except ValueError:
            pass
        value = value.replace("\\", "/")
    while value.startswith("./"):
        value = value[2:]
    root_text = str(root).replace("\\", "/").rstrip("/") + "/"
    if value.startswith(root_text):
        value = value[len(root_text):]
    return value


def split_symbol(value):
    if isinstance(value, dict):
        value = value.get("symbol") or value.get("key")
    if not isinstance(value, str) or "#" not in value:
        return None
    file, name = value.rsplit("#", 1)
    return file, name


def load_json(path):
    try:
        text = path.read_text(errors="replace")
    except OSError:
        return None
    decoder = json.JSONDecoder()
    index = 0
    while True:
        start = text.find("{", index)
        if start < 0:
            return None
        try:
            value, _ = decoder.raw_decode(text[start:])
        except json.JSONDecodeError:
            index = start + 1
            continue
        if isinstance(value, dict):
            return value
        index = start + 1


def parse_shrike(data, root):
    snapshot = data.get("snapshot") or {}
    files = set()
    for value in snapshot.get("unusedFiles") or []:
        files.add(normalize_path(root, value))
    symbols = set()
    for value in (data.get("candidates") or snapshot.get("candidates") or []):
        pair = split_symbol(value)
        if pair:
            symbols.add((normalize_path(root, pair[0]), pair[1]))
    internal_only = set()
    for value in snapshot.get("internalOnly") or []:
        pair = split_symbol(value)
        if pair:
            internal_only.add((normalize_path(root, pair[0]), pair[1]))
    return files, symbols, internal_only, 0


def parse_fallow(data, root):
    files = set()
    for item in data.get("unused_files") or []:
        if isinstance(item, dict):
            files.add(normalize_path(root, item.get("path") or item.get("file")))
    symbols = set()
    for key in ("unused_exports", "unused_types", "unused_server_actions"):
        for item in data.get(key) or []:
            if not isinstance(item, dict):
                continue
            path = item.get("path") or item.get("file")
            name = item.get("export_name") or item.get("type_name") or item.get("name") or item.get("action_name")
            if path and name:
                symbols.add((normalize_path(root, path), name))
    members = sum(
        len(data.get(key) or [])
        for key in ("unused_enum_members", "unused_class_members", "unused_namespace_members")
    )
    return files, symbols, set(), members


def parse_knip(data, root):
    files = set()
    symbols = set()
    for issue in data.get("issues") or []:
        if not isinstance(issue, dict):
            continue
        issue_file = issue.get("file")
        for item in issue.get("files") or []:
            if isinstance(item, dict):
                files.add(normalize_path(root, item.get("name") or issue_file))
            elif isinstance(item, str):
                files.add(normalize_path(root, item))
        for key in ("exports", "types"):
            for item in issue.get(key) or []:
                if isinstance(item, dict) and item.get("name") and issue_file:
                    symbols.add((normalize_path(root, issue_file), item["name"]))
    members = sum(
        len(issue.get(key) or [])
        for issue in data.get("issues") or []
        if isinstance(issue, dict)
        for key in ("enumMembers", "namespaceMembers", "classMembers")
    )
    return files, symbols, set(), members


TS_PRUNE_LINE = re.compile(r"^(.*?):(\d+)\s+-\s+(.*)$")
TS_UNUSED_LINE = re.compile(r"^(.*?):\s+(.*)$")


def parse_ts_prune(text, root):
    symbols = set()
    for line in text.splitlines():
        match = TS_PRUNE_LINE.match(line.strip())
        if not match:
            continue
        path, _, name = match.groups()
        name = re.sub(r"\s+\(used in .*\)$", "", name).strip()
        if path and name:
            symbols.add((normalize_path(root, path), name))
    return set(), symbols, set(), 0


def parse_ts_unused_exports(text, root):
    symbols = set()
    for line in text.splitlines():
        line = line.strip()
        if not line or "modules with unused exports" in line:
            continue
        match = TS_UNUSED_LINE.match(line)
        if not match:
            continue
        path, names = match.groups()
        for name in names.split(","):
            name = name.strip()
            name = re.sub(r"\s+\(used in .*\)$", "", name).strip()
            if path and name:
                symbols.add((normalize_path(root, path), name))
    return set(), symbols, set(), 0


def run_tool(tool, case_root, tools, raw_dir, keep_raw):
    for name in CACHE_DIRS:
        shutil.rmtree(case_root / name, ignore_errors=True)
    env = os.environ.copy()
    env.update({"NO_COLOR": "1", "FORCE_COLOR": "0"})
    env["SHRIKE_ENGINE"] = str(tools["engine"])
    command = tool_command(tool, tools)
    output_path = raw_dir / f"{case_root.name}-{tool}.out"
    error_path = raw_dir / f"{case_root.name}-{tool}.err"
    started = time.perf_counter()
    try:
        with open(output_path, "wb") as output, open(error_path, "wb") as error:
            process = subprocess.run(
                command,
                cwd=case_root,
                env=env,
                stdin=subprocess.DEVNULL,
                stdout=output,
                stderr=error,
                timeout=240,
            )
        elapsed = time.perf_counter() - started
        exit_code = process.returncode
    except subprocess.TimeoutExpired:
        elapsed = time.perf_counter() - started
        exit_code = None
    text = output_path.read_text(errors="replace")
    if tool == "shrike-cli":
        data = load_json(output_path)
        parsed = parse_shrike(data, case_root) if data is not None else None
    elif tool == "fallow":
        data = load_json(output_path)
        parsed = parse_fallow(data, case_root) if data is not None else None
    elif tool == "knip":
        data = load_json(output_path)
        parsed = parse_knip(data, case_root) if data is not None else None
    elif tool == "ts-prune":
        parsed = parse_ts_prune(text, case_root)
    elif tool == "ts-unused-exports":
        parsed = parse_ts_unused_exports(text, case_root)
    else:
        parsed = None
    valid = exit_code in EXPECTED_EXITS[tool] and parsed is not None
    if not valid:
        try:
            error_text = error_path.read_text(errors="replace")[-800:]
        except OSError:
            error_text = ""
        result = {
            "valid": False,
            "exit": exit_code,
            "error": error_text,
            "elapsed_s": elapsed,
            "reported_files": [],
            "reported_symbols": [],
            "member_warnings": 0,
            "internal_only": [],
        }
    else:
        files, symbols, internal_only, members = parsed
        result = {
            "valid": True,
            "exit": exit_code,
            "error": None,
            "elapsed_s": elapsed,
            "reported_files": sorted(files),
            "reported_symbols": sorted(f"{file}#{name}" for file, name in symbols),
            "member_warnings": members,
            "internal_only": sorted(f"{file}#{name}" for file, name in internal_only),
        }
    if not keep_raw:
        output_path.unlink(missing_ok=True)
        error_path.unlink(missing_ok=True)
    return result


IMPORT_RE = re.compile(
    r"""(?:from\s+|import\s*\(\s*|require\s*\(\s*)["'](\.{1,2}/[^"']+)["']"""
)
SOURCE_EXTENSIONS = (".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs")


def importers_for(case_root):
    """Map a case-relative file to the case-relative files that import it."""
    importers = defaultdict(set)
    for directory, dirs, files in os.walk(case_root):
        dirs[:] = [name for name in dirs if name not in {"node_modules", ".git", ".shrike", ".fallow", ".knip"}]
        for name in files:
            if not name.endswith(SOURCE_EXTENSIONS):
                continue
            path = Path(directory) / name
            rel = normalize_path(case_root, path)
            try:
                text = path.read_text(errors="replace")
            except OSError:
                continue
            for specifier in IMPORT_RE.findall(text):
                base = (path.parent / specifier).resolve()
                candidates = [base]
                candidates += [Path(str(base) + suffix) for suffix in SOURCE_EXTENSIONS]
                candidates += [base / ("index" + suffix) for suffix in SOURCE_EXTENSIONS]
                for candidate in candidates:
                    if candidate.is_file():
                        importers[normalize_path(case_root, candidate)].add(rel)
                        break
    return importers


def score_case(entry, case_root, reported, importers, full_scope):
    protected = {item["symbol"] for item in entry.get("protected") or []}
    must = set(entry.get("mustReport") or [])
    unused_files = set(entry.get("unusedFiles") or [])
    used_files = set(entry.get("usedFiles") or [])
    reported_symbols = set(reported.get("reported_symbols") or [])
    reported_files = set(reported.get("reported_files") or [])

    fp_symbols = sorted(protected & reported_symbols)
    # A protected symbol is also a false positive when its whole file is
    # reported unused, because deleting the file deletes the symbol.
    for symbol in protected:
        if "#" not in symbol:
            continue
        file, _ = symbol.rsplit("#", 1)
        if file in reported_files and symbol not in fp_symbols:
            fp_symbols.append(symbol)
    fp_symbols = sorted(set(fp_symbols))

    found_symbols = set(must & reported_symbols)
    for symbol in must:
        if "#" in symbol:
            file, _ = symbol.rsplit("#", 1)
            if file in reported_files:
                found_symbols.add(symbol)
    fn_symbols = sorted(must - found_symbols)
    tp_symbols = sorted(found_symbols)
    extra_symbols = sorted(reported_symbols - protected - must)

    if full_scope:
        fp_files_strict = sorted(used_files & reported_files)
        fp_files_closure = []
        for file in fp_files_strict:
            sources = importers.get(file, set())
            if sources and all(source in reported_files for source in sources):
                continue
            fp_files_closure.append(file)
        tp_files = sorted(unused_files & reported_files)
        fn_files = sorted(unused_files - reported_files)
        extra_files = sorted(reported_files - unused_files - used_files)
    else:
        fp_files_strict = []
        fp_files_closure = []
        tp_files = []
        fn_files = []
        extra_files = []

    internal_only_expected = set(entry.get("internalOnly") or [])
    internal_only_reported = set(reported.get("internal_only") or [])
    internal_only_ok = internal_only_expected <= internal_only_reported if internal_only_expected else None

    tp = len(tp_symbols) + len(tp_files)
    fp = len(fp_symbols) + len(fp_files_closure)
    fn = len(fn_symbols) + len(fn_files)
    strict_fp = len(fp_symbols) + len(fp_files_strict)
    return {
        "tp": tp,
        "fp": fp,
        "fn": fn,
        "strict_fp": strict_fp,
        "tp_symbols": tp_symbols,
        "fp_symbols": fp_symbols,
        "fn_symbols": fn_symbols,
        "tp_files": tp_files,
        "fp_files_strict": fp_files_strict,
        "fp_files_closure": fp_files_closure,
        "fn_files": fn_files,
        "extra_symbols": extra_symbols,
        "extra_files": extra_files,
        "member_warnings": reported.get("member_warnings", 0),
        "internal_only_ok": internal_only_ok,
    }


def pct(numerator, denominator):
    return None if denominator == 0 else numerator / denominator


def aggregate(cases):
    tp = sum(case["tp"] for case in cases)
    fp = sum(case["fp"] for case in cases)
    fn = sum(case["fn"] for case in cases)
    strict_fp = sum(case["strict_fp"] for case in cases)
    precision = pct(tp, tp + fp)
    recall = pct(tp, tp + fn)
    f1 = None if precision is None or recall is None or precision + recall == 0 else 2 * precision * recall / (precision + recall)
    strict_precision = pct(tp, tp + strict_fp)
    passed = sum(
        1
        for case in cases
        if not case["fp_symbols"]
        and not case["fn_symbols"]
        and not case["fp_files_closure"]
        and not case["fn_files"]
    )
    return {
        "cases": len(cases),
        "tp": tp,
        "fp": fp,
        "fn": fn,
        "strict_fp": strict_fp,
        "precision": precision,
        "recall": recall,
        "f1": f1,
        "strict_precision": strict_precision,
        "case_pass": passed,
        "case_pass_rate": pct(passed, len(cases)),
        "extra_symbols": sum(len(case["extra_symbols"]) for case in cases),
        "extra_files": sum(len(case["extra_files"]) for case in cases),
        "member_warnings": sum(case["member_warnings"] for case in cases),
    }


def fmt(value, digits=3):
    if value is None:
        return "—"
    if isinstance(value, float):
        return f"{value:.{digits}f}"
    return str(value)


def fmt_pct(value):
    return "—" if value is None else f"{value * 100:.1f}%"


def write_markdown(path, meta, results, cases_by_tool, corpus_count):
    lines = []
    add = lines.append
    add("# Shrike competitor accuracy benchmark — labeled corpus")
    add("")
    add(f"**Date:** {meta['date']}  ")
    add(f"**Corpus:** {corpus_count} cases / {meta['labels']} labeled items from `fixtures/must-not-report`  ")
    add(f"**Tools:** {', '.join(meta['tools'])}  ")
    add(f"**Machine:** {meta['cpu']}, {meta['logical_cpus']} logical CPUs  ")
    add(f"**Shrike:** {meta.get('shrike_commit')} (working tree)  ")
    add("")
    add("---")
    add("")
    add("## 1. What this measures")
    add("")
    add(
        "The performance benchmark measures speed. This benchmark measures whether a "
        "finding is correct on cases where the answer is known before any tool runs. "
        "Each case is a minimal project with framework-derived labels: protected "
        "symbols and files that must never be reported as dead, and must-report "
        "symbols and files that must be found. The labels come from framework "
        "documentation and a two-sided fixture review, not from any analyzer's output."
    )
    add("")
    add("Scoring is at the labeled item level:")
    add("")
    add("```text")
    add("TP = mustReport symbols found + unusedFiles reported")
    add("FP = protected symbols reported + usedFiles reported")
    add("FN = mustReport symbols missed + unusedFiles missed")
    add("precision = TP / (TP + FP)   recall = TP / (TP + FN)")
    add("```")
    add("")
    add(
        "A symbol counts as found when the tool reports it or reports its whole file "
        "unused. `usedFiles` reported unused is scored with a closure allowance: if "
        "every importer of the file is itself reported unused, the file is "
        "transitively dead and is not a false positive. The strict count is shown "
        "separately. The narrow tools (ts-prune, ts-unused-exports) only see exports, "
        "so file-level labels are excluded from their metrics. Member-level warnings "
        "(unused enum/class/namespace members) are not scored; they are not deletions "
        "of the enclosing export."
    )
    add("")
    add("---")
    add("")
    add("## 2. Headline")
    add("")
    add("| Tool | Scope | Valid cases | TP | FP | FN | Precision | Recall | F1 | Case pass |")
    add("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for tool, result in results.items():
        scope = "symbols + files" if tool in FULL_TOOLS else "symbols"
        add(
            f"| {tool} | {scope} | {result['cases']} | {result['tp']} | {result['fp']} | "
            f"{result['fn']} | {fmt_pct(result['precision'])} | {fmt_pct(result['recall'])} | "
            f"{fmt_pct(result['f1'])} | {result['case_pass']}/{result['cases']} |"
        )
    add("")
    add(
        "Precision uses the closure-adjusted file score. Strict precision (counting "
        "every reported `usedFiles` entry as a false positive) is in §3."
    )
    add("")
    add("---")
    add("")
    add("## 3. Strict file scoring and unlabeled findings")
    add("")
    add("| Tool | TP | FP (closure) | FP (strict) | Precision (closure) | Precision (strict) | Extra symbols | Extra files | Member warnings |")
    add("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for tool, result in results.items():
        add(
            f"| {tool} | {result['tp']} | {result['fp']} | {result['strict_fp']} | "
            f"{fmt_pct(result['precision'])} | {fmt_pct(result['strict_precision'])} | "
            f"{result['extra_symbols']} | {result['extra_files']} | {result['member_warnings']} |"
        )
    add("")
    add(
        "Extra findings are reported items that are neither protected nor must-report. "
        "They are not counted in precision because the corpus does not label them. "
        "Member warnings are unused enum/class/namespace members, which are not "
        "deletions of the enclosing export."
    )
    add("")
    add("---")
    add("")
    add("## 4. By category")
    add("")
    categories = sorted(
        {
            case["category"]
            for cases in cases_by_tool.values()
            for case in cases
            if case["valid"]
        }
    )
    add("| Category | Cases | " + " | ".join(f"{tool} P / R / F1" for tool in results) + " |")
    add("|---|---:|" + "---:|" * len(results))
    for category in categories:
        row = [category]
        count = 0
        for tool in results:
            subset = [case for case in cases_by_tool[tool] if case["category"] == category]
            count = max(count, len(subset))
            agg = aggregate(subset)
            row.append(
                f"{fmt_pct(agg['precision'])} / {fmt_pct(agg['recall'])} / {fmt_pct(agg['f1'])}"
            )
        add(f"| {row[0]} | {count} | " + " | ".join(row[1:]) + " |")
    add("")
    add("---")
    add("")
    add("## 5. False positives (protected code reported as dead)")
    add("")
    for tool, cases in cases_by_tool.items():
        valid = [case for case in cases if case["valid"]]
        fps = [(case["id"], case["category"], symbol) for case in valid for symbol in case["fp_symbols"]]
        add(f"### {tool} — {len(fps)} symbol false positives")
        add("")
        if not fps:
            add("None.")
        else:
            add("| Case | Category | Symbol |")
            add("|---|---|---|")
            for case_id, category, symbol in fps[:40]:
                add(f"| {case_id} | {category} | `{symbol}` |")
            if len(fps) > 40:
                add(f"| … | | {len(fps) - 40} more |")
        add("")
        files = [(case["id"], case["category"], file) for case in valid for file in case["fp_files_strict"]]
        add(f"### {tool} — {len(files)} file false positives (strict)")
        add("")
        if not files:
            add("None.")
        else:
            add("| Case | Category | File |")
            add("|---|---|---|")
            for case_id, category, file in files[:30]:
                add(f"| {case_id} | {category} | `{file}` |")
            if len(files) > 30:
                add(f"| … | | {len(files) - 30} more |")
        add("")
    add("---")
    add("")
    add("## 6. Missed must-report items")
    add("")
    for tool, cases in cases_by_tool.items():
        valid = [case for case in cases if case["valid"]]
        fns = [(case["id"], case["category"], symbol) for case in valid for symbol in case["fn_symbols"]]
        add(f"### {tool} — {len(fns)} missed symbols")
        add("")
        if not fns:
            add("None.")
        else:
            add("| Case | Category | Symbol |")
            add("|---|---|---|")
            for case_id, category, symbol in fns[:40]:
                add(f"| {case_id} | {category} | `{symbol}` |")
            if len(fns) > 40:
                add(f"| … | | {len(fns) - 40} more |")
        add("")
        files = [(case["id"], case["category"], file) for case in valid for file in case["fn_files"]]
        add(f"### {tool} — {len(files)} missed unused files")
        add("")
        if not files:
            add("None.")
        else:
            add("| Case | Category | File |")
            add("|---|---|---|")
            for case_id, category, file in files[:30]:
                add(f"| {case_id} | {category} | `{file}` |")
            if len(files) > 30:
                add(f"| … | | {len(files) - 30} more |")
        add("")
    add("---")
    add("")
    add("## 7. Invalid configurations")
    add("")
    invalid = [
        (case["id"], case["category"], tool, case.get("error") or "")
        for tool, cases in cases_by_tool.items()
        for case in cases
        if not case["valid"]
    ]
    if invalid:
        add("| Case | Category | Tool | Error (tail) |")
        add("|---|---|---|---|")
        for case_id, category, tool, error in invalid[:80]:
            error = " ".join(error.split())[-160:]
            add(f"| {case_id} | {category} | {tool} | {error} |")
        if len(invalid) > 80:
            add(f"| … | | | {len(invalid) - 80} more |")
    else:
        add("No invalid configurations.")
    add("")
    add("---")
    add("")
    add("## 8. Caveats")
    add("")
    add(
        "1. **The corpus is Shrike's own CI gate, not a random sample.** It "
        "over-represents framework conventions (Next.js routes, server actions, "
        "barrels, dynamic imports) because those are where false positives are "
        "costly. Shrike's perfect score is therefore expected by construction and "
        "is not independent evidence; the competitor scores on the same labels are "
        "the informative comparison. The per-category table is the right way to "
        "read it; the headline is not a claim about an arbitrary repository."
    )
    add(
        "2. **Different categories.** Knip and fallow's unused exports include de-export "
        "candidates; Shrike's `internalOnly` is advisory and is not scored against them. "
        "Member-level warnings are counted separately."
    )
    add(
        "3. **Framework support differs.** A tool that does not implement a convention "
        "will report the convention's entry as dead. That is a real false positive for "
        "that tool, but it is also a scope difference. The category table makes it visible."
    )
    add(
        "4. **No dependency labels.** The corpus labels symbols and files, not package "
        "dependencies. Dependency accuracy remains the agreement study in the "
        "performance report."
    )
    add(
        "5. **Minimal projects.** Each case is small and has no installed dependencies. "
        "The corpus tests conventions and reachability, not resolver performance or "
        "monorepo scale. Performance is measured separately on 12 real and synthetic "
        "fixtures."
    )
    add(
        "6. **Closure allowance.** A used file whose every importer is reported unused is "
        "transitively dead. The strict column counts it as a false positive; the primary "
        "score does not. Both are shown."
    )
    add("")
    add("---")
    add("")
    add("## 9. Reproduction")
    add("")
    add("```bash")
    add("# Build Shrike")
    add("cargo build --release -p shrike-engine")
    add("pnpm --filter shrike build")
    add("")
    add("# Install competitors (see benchmarks/competitors/setup.sh)")
    add("benchmarks/competitors/setup.sh")
    add("")
    add("# Run the labeled accuracy suite")
    add("python3 benchmarks/competitors/accuracy.py --work /tmp/opencode/bench-v2")
    add("")
    add("# Quick smoke run")
    add("python3 benchmarks/competitors/accuracy.py --limit 20 --tools shrike-cli,fallow,knip")
    add("```")
    add("")
    add("Raw per-case results are in the JSON next to this report. Raw tool output "
        "is deleted unless `--keep-raw` is passed.")
    path.write_text("\n".join(lines) + "\n")


def metadata(tools, cases):
    cpu = None
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                cpu = line.split(":", 1)[1].strip()
                break
    except OSError:
        pass
    try:
        commit = subprocess.run(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            timeout=20,
        ).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        commit = None
    labels = sum(
        len(case.get("protected") or [])
        + len(case.get("mustReport") or [])
        + len(case.get("unusedFiles") or [])
        + len(case.get("usedFiles") or [])
        for case in cases
    )
    return {
        "date": datetime.now(timezone.utc).isoformat(),
        "cpu": cpu,
        "logical_cpus": os.cpu_count(),
        "shrike_commit": commit,
        "labels": labels,
        "corpus": str(DEFAULT_CORPUS),
        "tools": [],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", type=Path, default=DEFAULT_WORK)
    parser.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)
    parser.add_argument("--tools", default=",".join(ALL_TOOLS))
    parser.add_argument("--limit", type=int, default=None, help="first N cases (smoke run)")
    parser.add_argument("--case", default=None, help="comma-separated case ids")
    parser.add_argument("--category", default=None, help="comma-separated categories")
    parser.add_argument("--keep-raw", action="store_true")
    parser.add_argument("--out", type=Path, default=None)
    parser.add_argument("--json-only", action="store_true")
    args = parser.parse_args()

    manifest = json.loads((args.corpus / "manifest.json").read_text())
    entries = manifest["entries"]
    if args.case:
        wanted = {value.strip() for value in args.case.split(",")}
        entries = [entry for entry in entries if entry["id"] in wanted]
    if args.category:
        wanted = {value.strip() for value in args.category.split(",")}
        entries = [entry for entry in entries if entry["category"] in wanted]
    if args.limit:
        entries = entries[: args.limit]
    if not entries:
        print("no cases selected", file=sys.stderr)
        return 2

    tools = tool_paths(args.work)
    tool_names = [value.strip() for value in args.tools.split(",") if value.strip()]
    unknown = [value for value in tool_names if value not in ALL_TOOLS]
    if unknown:
        parser.error(f"unknown tools: {', '.join(unknown)}")
    path_keys = {
        "shrike-cli": "cli",
        "fallow": "fallow",
        "knip": "knip",
        "ts-prune": "ts-prune",
        "ts-unused-exports": "ts-unused-exports",
    }
    missing = []
    for name in tool_names:
        path = tools.get(path_keys[name])
        if path is None or (isinstance(path, Path) and not path.exists()):
            missing.append(name)
    if missing:
        print(f"missing tools/binaries: {', '.join(missing)}", file=sys.stderr)
        print("run benchmarks/competitors/setup.sh and build Shrike first", file=sys.stderr)
        return 2

    out_dir = args.out or (args.work / "results-accuracy")
    out_dir.mkdir(parents=True, exist_ok=True)
    raw_dir = out_dir / "raw"
    if args.keep_raw:
        raw_dir.mkdir(parents=True, exist_ok=True)
    else:
        raw_dir.mkdir(parents=True, exist_ok=True)

    meta = metadata(tools, entries)
    meta["tools"] = tool_names
    cases_by_tool = {tool: [] for tool in tool_names}
    started = time.time()
    for index, entry in enumerate(entries, 1):
        case_root = args.corpus / "cases" / entry["id"]
        if not case_root.exists():
            print(f"skipping {entry['id']}: missing case directory", file=sys.stderr)
            continue
        importers = importers_for(case_root)
        for tool in tool_names:
            reported = run_tool(tool, case_root, tools, raw_dir, args.keep_raw)
            full_scope = tool in FULL_TOOLS
            scored = score_case(entry, case_root, reported, importers, full_scope)
            case = {
                "id": entry["id"],
                "category": entry["category"],
                "tool": tool,
                "valid": reported["valid"],
                "exit": reported["exit"],
                "error": reported["error"],
                "elapsed_s": reported["elapsed_s"],
                **scored,
            }
            cases_by_tool[tool].append(case)
        if index % 10 == 0 or index == len(entries):
            elapsed = time.time() - started
            print(
                f"{index}/{len(entries)} cases, {elapsed:.0f}s elapsed",
                file=sys.stderr,
            )

    results = {}
    for tool in tool_names:
        valid_cases = [case for case in cases_by_tool[tool] if case["valid"]]
        results[tool] = aggregate(valid_cases)
        results[tool]["invalid"] = len(cases_by_tool[tool]) - len(valid_cases)

    report = {
        "metadata": meta,
        "manifest": {"count": manifest.get("count"), "schemaVersion": manifest.get("schemaVersion")},
        "results": results,
        "cases": cases_by_tool,
    }
    json_path = out_dir / "summary.json"
    json_path.write_text(json.dumps(report, indent=2) + "\n")
    print(f"wrote {json_path}")

    if not args.json_only:
        md_path = out_dir / "summary.md"
        write_markdown(md_path, meta, results, cases_by_tool, len(entries))
        print(f"wrote {md_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
