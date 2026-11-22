#!/usr/bin/env python3
"""Paired competitor benchmark for Shrike.

The method follows fallow's public benchmark suite (pinned real-world
repositories plus deterministic synthetic projects, cold and warm caches,
warmups, medians, peak RSS) and adds paired/interleaved rounds, p95, and
output validation.

Usage:
    python3 benchmarks/competitors/bench.py --work /tmp/opencode/bench-v2
    python3 benchmarks/competitors/bench.py --quick --fixture requo,vite
"""

import argparse
import json
import math
import os
import platform
import shutil
import statistics
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_WORK = Path(os.environ.get("SHRIKE_BENCH_WORK", "/tmp/opencode/bench-v2"))
DEFAULT_FIXTURES = Path(__file__).resolve().parent / "fixtures.json"
SOURCE_EXTENSIONS = (".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs")

TOOL_ORDER = [
    "shrike-cli",
    "shrike-engine",
    "fallow",
    "knip",
    "depcheck",
    "ts-prune",
    "ts-unused-exports",
]
WARM_TOOLS = {"shrike-cli", "shrike-engine", "fallow", "knip"}
EXPECTED_EXITS = {
    "shrike-cli": {0, 1},
    "shrike-engine": {0},
    "fallow": {0, 1},
    "knip": {0, 1},
    "depcheck": {0, 255},
    "ts-prune": {0, 1},
    "ts-unused-exports": {0, 1},
}


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
        "depcheck": which_tool(work, "depcheck"),
        "ts-prune": which_tool(work, "ts-prune"),
        "ts-unused-exports": which_tool(work, "ts-unused-exports"),
    }


def package_version(work, name):
    for base in (work / "tools", Path("/tmp/opencode/bench-v2/tools"), Path("/tmp/opencode/bench-tools")):
        path = base / "node_modules" / name / "package.json"
        if path.exists():
            try:
                return json.loads(path.read_text()).get("version")
            except (OSError, json.JSONDecodeError):
                pass
    return None


def command_output(command):
    try:
        return subprocess.run(command, capture_output=True, text=True, timeout=20).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return None


def workspace_version():
    try:
        return json.loads((ROOT / "packages" / "cli" / "package.json").read_text()).get("version")
    except (OSError, json.JSONDecodeError):
        return None


def metadata(work, tools):
    cpu = None
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                cpu = line.split(":", 1)[1].strip()
                break
    except OSError:
        pass
    load = os.getloadavg() if hasattr(os, "getloadavg") else (None, None, None)
    fallow_version = command_output([str(tools["fallow"]), "--version"]) if tools["fallow"] else None
    return {
        "date": datetime.now(timezone.utc).isoformat(),
        "host": platform.node(),
        "platform": platform.platform(),
        "kernel": platform.release(),
        "cpu": cpu,
        "logical_cpus": os.cpu_count(),
        "load_average_start": load,
        "shrike_commit": command_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"]),
        "shrike_version": workspace_version()
        or command_output([str(tools["node"]), str(tools["cli"]), "--version"]),
        "rustc": command_output(["rustc", "--version"]),
        "node": command_output([str(tools["node"]), "--version"]),
        "fallow": fallow_version.splitlines()[0] if fallow_version else None,
        "knip": command_output([str(tools["knip"]), "--version"]) if tools["knip"] else None,
        "depcheck": command_output([str(tools["depcheck"]), "--version"]) if tools["depcheck"] else None,
        "ts-prune": package_version(work, "ts-prune"),
        "ts-unused-exports": package_version(work, "ts-unused-exports"),
        "work": str(work),
        "rounds": None,
        "warmups": None,
    }


def source_files(root):
    count = 0
    for directory, dirs, files in os.walk(root):
        dirs[:] = [name for name in dirs if name not in {"node_modules", ".git", "dist"}]
        count += sum(1 for name in files if name.endswith(SOURCE_EXTENSIONS))
    return count


def remove(path):
    shutil.rmtree(path, ignore_errors=True)


def cache_paths(tool, fixture, work):
    root = fixture["root"]
    if tool == "shrike-cli":
        return [root / ".shrike" / "cache"]
    if tool == "shrike-engine":
        return [work / "cache" / "engine" / fixture["name"]]
    if tool == "fallow":
        return [root / ".fallow"]
    if tool == "knip":
        return [
            root / ".knip",
            root / "node_modules" / ".cache" / "knip",
            root / "node_modules" / ".cache" / ".knip",
        ]
    return []


def clear_cache(tool, fixture, work):
    for path in cache_paths(tool, fixture, work):
        remove(path)


def tool_command(tool, fixture, work, warm, tools):
    if tool == "shrike-cli":
        return [str(tools["node"]), str(tools["cli"]), "scan", "--json"], None
    if tool == "shrike-engine":
        request = {
            "protocolVersion": "1.0",
            "id": "bench",
            "command": "analyze",
            "payload": {
                "root": str(fixture["root"]),
                "config": {},
                "cacheDir": str(work / "cache" / "engine" / fixture["name"]),
            },
        }
        return [str(tools["engine"])], (json.dumps(request) + "\n").encode()
    if tool == "fallow":
        args = [str(tools["fallow"]), "dead-code", "--quiet", "--format", "json"]
        if not warm:
            args.append("--no-cache")
        return args, None
    if tool == "knip":
        args = [str(tools["knip"]), "--reporter", "json"]
        if warm:
            args.append("--cache")
        return args, None
    if tool == "depcheck":
        return [str(tools["depcheck"]), "--json", "--ignore-bin-package"], None
    if tool == "ts-prune":
        return [str(tools["ts-prune"]), "--project", "tsconfig.json", "--error"], None
    if tool == "ts-unused-exports":
        return [str(tools["ts-unused-exports"]), "tsconfig.json", "--maxIssues=0"], None
    raise ValueError(f"unknown tool: {tool}")


class TreeSampler(threading.Thread):
    """Sample RSS of the process tree so a wrapper's child is counted.

    Two artifacts are filtered out:

    * Until the child calls exec, it is a fork of this Python process and
      its RSS is the benchmark harness's own memory, not the tool's. A
      137.24 MB value was repeatedly measured this way once the harness
      had grown; samples are skipped until /proc/<pid>/exe differs from
      the parent's executable.
    * A PID can be reused by an unrelated process as soon as the child is
      reaped by wait4(). Every sample is therefore verified against the
      process start time captured when the child was started.

    For the same reason run_once() reports the sampler's peak rather than
    wait4()'s ru_maxrss: ru_maxrss also includes the pre-exec fork image.
    """

    def __init__(self, pid, interval=0.005, parent_exe=None):
        super().__init__(daemon=True)
        self.pid = pid
        self.interval = interval
        self.parent_exe = parent_exe
        self.stop_event = threading.Event()
        self.max_total_mb = 0.0
        self.max_root_mb = 0.0
        self.samples = 0
        self.exec_seen = parent_exe is None
        self.start_times = {pid: self.proc_start_time(pid)}

    @staticmethod
    def exe(pid):
        try:
            return os.path.realpath(f"/proc/{pid}/exe")
        except OSError:
            return None

    @staticmethod
    def proc_info(pid):
        try:
            # Field 3 is state, field 22 is starttime. The command name in
            # field 2 can contain spaces and parentheses, so split on the
            # final ") " delimiter.
            fields = Path(f"/proc/{pid}/stat").read_text().rsplit(") ", 1)[1].split()
            return fields[0], fields[19]
        except (OSError, ValueError, IndexError):
            return None, None

    def proc_start_time(self, pid):
        return self.proc_info(pid)[1]

    def alive(self, pid):
        state, start = self.proc_info(pid)
        known = self.start_times.get(pid)
        if known is None:
            self.start_times[pid] = start
            known = start
        if start is None or start != known or state in (None, "Z", "X"):
            return False
        return True

    def children(self, pid):
        try:
            text = Path(f"/proc/{pid}/task/{pid}/children").read_text()
            return [int(value) for value in text.split()]
        except (OSError, ValueError):
            return []

    def tree(self, pid):
        found = [pid]
        index = 0
        while index < len(found):
            if self.alive(found[index]):
                found.extend(self.children(found[index]))
            index += 1
        return found

    def rss_mb(self, pid):
        if not self.alive(pid):
            return 0.0
        try:
            for line in Path(f"/proc/{pid}/status").read_text().splitlines():
                if line.startswith("VmRSS:"):
                    return int(line.split()[1]) / 1024
        except (OSError, ValueError):
            pass
        return 0.0

    def run(self):
        while not self.stop_event.is_set():
            if not self.exec_seen:
                current = self.exe(self.pid)
                if current is None or current == self.parent_exe:
                    # Still the pre-exec fork image (or already gone, in
                    # which case wait4() has the final measurement).
                    self.stop_event.wait(self.interval)
                    continue
                self.exec_seen = True
            tree = self.tree(self.pid)
            total = sum(self.rss_mb(pid) for pid in tree)
            self.max_total_mb = max(self.max_total_mb, total)
            self.max_root_mb = max(self.max_root_mb, self.rss_mb(self.pid))
            self.samples += 1
            self.stop_event.wait(self.interval)


def run_once(tool, fixture, work, warm, tools, raw_dir, tag):
    command, stdin_data = tool_command(tool, fixture, work, warm, tools)
    output_path = raw_dir / f"{tool}-{tag}.out"
    error_path = raw_dir / f"{tool}-{tag}.err"
    env = os.environ.copy()
    env.update({"NO_COLOR": "1", "FORCE_COLOR": "0"})
    env["SHRIKE_ENGINE"] = str(tools["engine"])
    stdin = subprocess.PIPE if stdin_data is not None else subprocess.DEVNULL
    start = time.perf_counter()
    with open(output_path, "wb") as output, open(error_path, "wb") as error:
        process = subprocess.Popen(
            command,
            cwd=fixture["root"],
            env=env,
            stdin=stdin,
            stdout=output,
            stderr=error,
        )
        try:
            parent_exe = os.path.realpath("/proc/self/exe")
        except OSError:
            parent_exe = None
        sampler = TreeSampler(process.pid, parent_exe=parent_exe)
        sampler.start()
        if stdin_data is not None:
            process.stdin.write(stdin_data)
            process.stdin.close()
        _, status, usage = os.wait4(process.pid, 0)
        sampler.stop_event.set()
        sampler.join(timeout=1)
    elapsed = time.perf_counter() - start
    exit_code = os.waitstatus_to_exitcode(status)
    wait4_rss = usage.ru_maxrss / 1024
    if sampler.exec_seen and sampler.samples:
        peak_rss = sampler.max_total_mb
        rss_source = "sampler"
    else:
        # The sampler never observed the post-exec process (it exited
        # immediately, or exec could not be verified). wait4()'s value may
        # include the inherited fork image, so mark it as suspect.
        peak_rss = wait4_rss
        rss_source = "wait4-suspect"
    result = {
        "wall_s": elapsed,
        "peak_rss_mb": peak_rss,
        "rss_source": rss_source,
        "rss_samples": sampler.samples,
        "wait4_maxrss_mb": wait4_rss,
        "exit": exit_code,
        "output_bytes": output_path.stat().st_size,
        "output": output_path,
        "error": error_path,
    }
    return result


def load_json(path):
    try:
        text = path.read_text(errors="replace")
    except OSError:
        return None
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        pass
    # Tools may print an env banner or a warning before the report. Find the
    # first position that decodes as a JSON object instead of assuming the
    # first `{` is the report (a banner can contain braces too).
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


def count_findings(tool, path):
    if tool == "ts-prune":
        try:
            return sum(1 for line in path.read_text(errors="replace").splitlines() if line.strip())
        except OSError:
            return None
    if tool == "ts-unused-exports":
        try:
            return sum(
                1
                for line in path.read_text(errors="replace").splitlines()
                if line.strip() and "unused export" not in line.lower() and "module" not in line.lower()
            )
        except OSError:
            return None
    data = load_json(path)
    if data is None:
        return None
    if tool == "shrike-cli":
        snapshot = data.get("snapshot") or {}
        dependencies = data.get("dependencies") or {}
        packages = dependencies.get("packages") or []
        deps = sum(len(pkg.get("unused") or []) + len(pkg.get("unlisted") or []) for pkg in packages)
        return len(data.get("candidates") or []) + len(snapshot.get("unusedFiles") or []) + deps
    if tool == "shrike-engine":
        snapshot = data.get("result", {}).get("snapshot") or {}
        return len(snapshot.get("candidates") or []) + len(snapshot.get("unusedFiles") or [])
    if tool == "fallow":
        return sum(
            len(value)
            for key, value in data.items()
            if key.startswith("unused_") and isinstance(value, list)
        )
    if tool == "knip":
        total = len(data.get("files") or [])
        for issue in data.get("issues") or []:
            for value in issue.values():
                if isinstance(value, list):
                    total += len(value)
        return total
    if tool == "depcheck":
        return len(data.get("dependencies") or []) + len(data.get("devDependencies") or [])
    return None


def percentile(values, fraction):
    ordered = sorted(values)
    if not ordered:
        return None
    index = max(0, min(len(ordered) - 1, math.ceil(fraction * len(ordered)) - 1))
    return ordered[index]


def summarize(samples):
    walls = [sample["wall_s"] for sample in samples]
    rss = [sample["peak_rss_mb"] for sample in samples]
    return {
        "runs": len(samples),
        "median_wall_s": statistics.median(walls),
        "p95_wall_s": percentile(walls, 0.95),
        "min_wall_s": min(walls),
        "max_wall_s": max(walls),
        "mean_wall_s": statistics.mean(walls),
        "stdev_wall_s": statistics.stdev(walls) if len(walls) > 1 else 0.0,
        "median_rss_mb": statistics.median(rss),
        "max_rss_mb": max(rss),
        "samples_wall_s": walls,
        "samples_rss_mb": rss,
        "rss_sources": [sample.get("rss_source") for sample in samples],
        "rss_samples": [sample.get("rss_samples") for sample in samples],
        "findings": samples[0].get("findings"),
        "exit": samples[0].get("exit"),
        "output_bytes": samples[0].get("output_bytes"),
        "valid": all(sample.get("valid", True) for sample in samples),
        "errors": [sample["error_text"] for sample in samples if sample.get("error_text")],
    }


def run_tool(tool, fixture, work, warm, tools, raw_dir, tag, keep_raw):
    result = run_once(tool, fixture, work, warm, tools, raw_dir, tag)
    result["findings"] = count_findings(tool, result["output"])
    expected = EXPECTED_EXITS[tool]
    if result["exit"] not in expected:
        try:
            result["error_text"] = result["error"].read_text(errors="replace")[-1200:]
        except OSError:
            result["error_text"] = "unreadable stderr"
        print(
            f"  ERROR {fixture['name']} {tool} {'warm' if warm else 'cold'}: "
            f"exit {result['exit']} (expected {sorted(expected)}): "
            f"{result['error_text'].strip().splitlines()[-1] if result['error_text'].strip() else ''}",
            file=sys.stderr,
        )
    result["valid"] = result["exit"] in expected and result["findings"] is not None
    if not keep_raw:
        result["output"].unlink(missing_ok=True)
        result["error"].unlink(missing_ok=True)
    result.pop("output", None)
    result.pop("error", None)
    return result


def ordered(tools, index):
    if not tools:
        return []
    offset = index % len(tools)
    return tools[offset:] + tools[:offset]


def benchmark_fixture(fixture, work, tools, tool_names, rounds, warmups, keep_raw):
    raw_dir = work / "raw" / fixture["name"]
    raw_dir.mkdir(parents=True, exist_ok=True)
    results = {tool: {} for tool in tool_names}

    # Cold: clear every tool's cache before every run, including warmups.
    for index in range(warmups):
        for tool in ordered(tool_names, index):
            clear_cache(tool, fixture, work)
            run_tool(tool, fixture, work, False, tools, raw_dir, f"cold-warmup-{index}", keep_raw)
    for index in range(rounds):
        for tool in ordered(tool_names, index):
            clear_cache(tool, fixture, work)
            sample = run_tool(tool, fixture, work, False, tools, raw_dir, f"cold-{index}", keep_raw)
            results[tool].setdefault("cold", []).append(sample)

    # Warm: populate each cache once, then run without clearing it.
    warm_tools = [tool for tool in tool_names if tool in WARM_TOOLS]
    for tool in warm_tools:
        clear_cache(tool, fixture, work)
        run_tool(tool, fixture, work, True, tools, raw_dir, "warm-populate", keep_raw)
    for index in range(warmups):
        for tool in ordered(warm_tools, index):
            run_tool(tool, fixture, work, True, tools, raw_dir, f"warm-warmup-{index}", keep_raw)
    for index in range(rounds):
        for tool in ordered(warm_tools, index):
            sample = run_tool(tool, fixture, work, True, tools, raw_dir, f"warm-{index}", keep_raw)
            results[tool].setdefault("warm", []).append(sample)

    for tool in tool_names:
        for mode in ("cold", "warm"):
            if mode in results[tool]:
                results[tool][mode] = summarize(results[tool][mode])
    return results


def usable(summary):
    """A summary that finished inside the tool's finding convention."""
    return summary if summary and summary.get("valid", True) else None


def median(summary, key="median_wall_s"):
    summary = usable(summary)
    return summary.get(key) if summary else None


def ratio(competitor, shrike):
    competitor = usable(competitor)
    shrike = usable(shrike)
    if not competitor or not shrike:
        return None
    return competitor["median_wall_s"] / shrike["median_wall_s"]


def fmt(value, digits=3):
    if value is None:
        return "—"
    if isinstance(value, float):
        return f"{value:.{digits}f}"
    return str(value)


def fmt_ratio(value):
    return "—" if value is None else f"{value:.2f}×"


def write_markdown(path, meta, rows):
    lines = [
        "# Competitor benchmark",
        "",
        f"- Date: {meta['date']}",
        f"- Host: {meta['host']} ({meta['platform']}, {meta['logical_cpus']} logical CPUs)",
        f"- CPU: {meta['cpu']}",
        f"- Shrike: {meta['shrike_commit']}",
        f"- Node: {meta['node']}",
        f"- fallow: {meta['fallow']}",
        f"- knip: {meta['knip']}",
        f"- depcheck: {meta['depcheck']}",
        f"- ts-prune: {meta['ts-prune']}",
        f"- ts-unused-exports: {meta['ts-unused-exports']}",
        f"- Rounds: {meta['rounds']} measured, {meta['warmups']} warmups",
        f"- Load average at start: {meta['load_average_start']}",
        "",
        "## Full scans",
        "",
        "Median wall seconds. Ratios are competitor ÷ Shrike; above 1 means Shrike is faster.",
        "",
        "| Fixture | Files | Shrike cold | Fallow cold | Knip cold | S/F cold | S/K cold | Shrike warm | Fallow warm | Knip cache | S/F warm | S/K warm | Shrike RSS |",
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for row in rows:
        tools = row["tools"]
        cli = tools.get("shrike-cli") or {}
        fallow = tools.get("fallow") or {}
        knip = tools.get("knip") or {}
        cold = cli.get("cold")
        warm = cli.get("warm")
        f_cold = fallow.get("cold")
        f_warm = fallow.get("warm")
        k_cold = knip.get("cold")
        k_warm = knip.get("warm")
        lines.append(
            "| {name} | {files} | {sc} | {fc} | {kc} | {sfc} | {skc} | {sw} | {fw} | {kw} | {sfw} | {skw} | {rss} |".format(
                name=row["name"],
                files=row["files"],
                sc=fmt(median(cold)),
                fc=fmt(median(f_cold)),
                kc=fmt(median(k_cold)),
                sfc=fmt_ratio(ratio(f_cold, cold)),
                skc=fmt_ratio(ratio(k_cold, cold)),
                sw=fmt(median(warm)),
                fw=fmt(median(f_warm)),
                kw=fmt(median(k_warm)),
                sfw=fmt_ratio(ratio(f_warm, warm)),
                skw=fmt_ratio(ratio(k_warm, warm)),
                rss=fmt(median(cold, "median_rss_mb"), 1),
            )
        )
    lines += [
        "",
        "A dash means the tool exited outside its finding convention or produced no",
        "parseable report; that configuration is not compared.",
        "",
        "## Engine",
        "",
        "| Fixture | Files | Engine cold | Fallow cold | Ratio | Engine warm | Fallow warm | Ratio | Engine RSS |",
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for row in rows:
        tools = row["tools"]
        engine = tools.get("shrike-engine") or {}
        fallow = tools.get("fallow") or {}
        cold = engine.get("cold")
        warm = engine.get("warm")
        f_cold = fallow.get("cold")
        f_warm = fallow.get("warm")
        lines.append(
            "| {name} | {files} | {ec} | {fc} | {rc} | {ew} | {fw} | {rw} | {rss} |".format(
                name=row["name"],
                files=row["files"],
                ec=fmt(median(cold)),
                fc=fmt(median(f_cold)),
                rc=fmt_ratio(ratio(f_cold, cold)),
                ew=fmt(median(warm)),
                fw=fmt(median(f_warm)),
                rw=fmt_ratio(ratio(f_warm, warm)),
                rss=fmt(median(cold, "median_rss_mb"), 1),
            )
        )
    lines += [
        "",
        "## Narrow-scope tools",
        "",
        "These tools do less work than a full scan: depcheck checks dependencies only; ts-prune and ts-unused-exports check exports only. Their times are context, not head-to-head full-scan results.",
        "",
        "| Fixture | Files | Depcheck | ts-prune | ts-unused-exports |",
        "|---|---:|---:|---:|---:|",
    ]
    for row in rows:
        tools = row["tools"]
        depcheck = tools.get("depcheck") or {}
        prune = tools.get("ts-prune") or {}
        unused = tools.get("ts-unused-exports") or {}
        d = depcheck.get("cold")
        p = prune.get("cold")
        u = unused.get("cold")
        lines.append(
            "| {name} | {files} | {d} | {p} | {u} |".format(
                name=row["name"],
                files=row["files"],
                d=fmt(median(d)),
                p=fmt(median(p)),
                u=fmt(median(u)),
            )
        )
    lines += [
        "",
        "## Validation",
        "",
        "| Fixture | Tool | Config | Valid | Findings | Exit | Output bytes |",
        "|---|---|---|---:|---:|---:|---:|",
    ]
    for row in rows:
        for tool, configs in row["tools"].items():
            for mode, summary in configs.items():
                if not isinstance(summary, dict) or "median_wall_s" not in summary:
                    continue
                lines.append(
                    "| {name} | {tool} | {mode} | {valid} | {findings} | {exit} | {bytes} |".format(
                        name=row["name"],
                        tool=tool,
                        mode=mode,
                        valid="yes" if summary.get("valid", True) else "no",
                        findings=fmt(summary.get("findings")),
                        exit=fmt(summary.get("exit")),
                        bytes=fmt(summary.get("output_bytes")),
                    )
                )
    path.write_text("\n".join(lines) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", type=Path, default=DEFAULT_WORK)
    parser.add_argument("--fixtures", type=Path, default=DEFAULT_FIXTURES)
    parser.add_argument("--fixture", default=None, help="comma-separated fixture names")
    parser.add_argument("--tools", default=",".join(TOOL_ORDER))
    parser.add_argument("--rounds", type=int, default=5)
    parser.add_argument("--warmups", type=int, default=2)
    parser.add_argument("--quick", action="store_true", help="3 rounds, 1 warmup")
    parser.add_argument("--keep-raw", action="store_true")
    parser.add_argument("--out", type=Path, default=None)
    args = parser.parse_args()
    if args.quick:
        args.rounds = 3
        args.warmups = 1

    fixtures = json.loads(args.fixtures.read_text())
    if args.fixture:
        selected = {name.strip() for name in args.fixture.split(",")}
        fixtures = [fixture for fixture in fixtures if fixture["name"] in selected]
    tool_names = [name.strip() for name in args.tools.split(",") if name.strip()]
    unknown = [name for name in tool_names if name not in TOOL_ORDER]
    if unknown:
        parser.error(f"unknown tools: {', '.join(unknown)}")

    tools = tool_paths(args.work)
    path_keys = {
        "shrike-cli": "cli",
        "shrike-engine": "engine",
        "fallow": "fallow",
        "knip": "knip",
        "depcheck": "depcheck",
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

    args.work.mkdir(parents=True, exist_ok=True)
    out_dir = args.out or (args.work / "results")
    out_dir.mkdir(parents=True, exist_ok=True)
    meta = metadata(args.work, tools)
    meta["rounds"] = args.rounds
    meta["warmups"] = args.warmups
    rows = []
    for fixture in fixtures:
        fixture["root"] = args.work / fixture["path"]
        if not fixture["root"].exists():
            print(f"skipping {fixture['name']}: missing {fixture['root']}", file=sys.stderr)
            continue
        fixture["files"] = source_files(fixture["root"])
        expected = fixture.get("expected_source_files")
        if expected and fixture["files"] != expected:
            print(
                f"warning: {fixture['name']} has {fixture['files']} source files, expected {expected}",
                file=sys.stderr,
            )
        print(f"benchmarking {fixture['name']} ({fixture['files']} files)", file=sys.stderr)
        results = benchmark_fixture(
            fixture,
            args.work,
            tools,
            tool_names,
            args.rounds,
            args.warmups,
            args.keep_raw,
        )
        rows.append(
            {
                "name": fixture["name"],
                "kind": fixture["kind"],
                "source": fixture["source"],
                "tag": fixture["tag"],
                "files": fixture["files"],
                "tools": results,
            }
        )
        # Persist after every fixture so an invalid competitor run cannot lose
        # the completed measurements.
        (out_dir / "partial.json").write_text(
            json.dumps({"metadata": meta, "fixtures": rows}, indent=2) + "\n"
        )

    meta["load_average_end"] = os.getloadavg()
    report = {"metadata": meta, "fixtures": rows}
    json_path = out_dir / "summary.json"
    md_path = out_dir / "summary.md"
    json_path.write_text(json.dumps(report, indent=2) + "\n")
    write_markdown(md_path, meta, rows)
    print(f"wrote {json_path}")
    print(f"wrote {md_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
