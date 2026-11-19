#!/usr/bin/env node
/**
 * Deterministic engine benchmark.
 *
 * Generates a synthetic repository, then measures cold and warm `analyze`
 * latency with three warmups and five measured runs. Peak RSS is sampled from
 * `/proc/<pid>/status`. Results are printed as JSON for the tracker.
 */
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const args = process.argv.slice(2);
function flag(name, fallback) {
  const index = args.indexOf(`--${name}`);
  return index >= 0 ? args[index + 1] : fallback;
}
const files = Number(flag("files", "10000"));
const runs = Number(flag("runs", "5"));
const warmups = Number(flag("warmups", "3"));
const engine = process.env.SHRIKE_ENGINE ?? [join(root, "target", "release", "shrike-engine"), join(root, "target", "debug", "shrike-engine")].find((path) => existsSync(path));
if (!engine || !existsSync(engine)) {
  console.error("SKIP: build shrike-engine first (cargo build --release -p shrike-engine)");
  process.exit(4);
}
const directory = mkdtempSync(join(tmpdir(), `shrike-bench-${files}-`));
function request(id) {
  return `${JSON.stringify({ protocolVersion: "1.0", id, command: "analyze", payload: { root: directory, config: {} } })}\n`;
}

function peakRss(pid) {
  try {
    const status = readFileSync(`/proc/${pid}/status`, "utf8");
    const match = status.match(/VmHWM:\s+(\d+) kB/);
    return match ? Number(match[1]) : 0;
  } catch {
    return 0;
  }
}

function run(id) {
  return new Promise((resolveRun, reject) => {
    const started = process.hrtime.bigint();
    const child = spawn(engine, [], { stdio: ["pipe", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    let rssKb = 0;
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    const poll = setInterval(() => { rssKb = Math.max(rssKb, peakRss(child.pid)); }, 10);
    child.on("close", (code) => {
      clearInterval(poll);
      const ms = Number(process.hrtime.bigint() - started) / 1e6;
      if (code !== 0) {
        reject(new Error(`engine failed (${code}): ${stderr.slice(0, 400)}`));
        return;
      }
      const line = stdout.split("\n").find((value) => value.trim().length > 0);
      if (!line) {
        reject(new Error("engine produced no response"));
        return;
      }
      const response = JSON.parse(line);
      if (!response.ok) {
        reject(new Error(`engine error: ${JSON.stringify(response.error)}`));
        return;
      }
      resolveRun({ ms, snapshot: response.result.snapshot, rssKb: rssKb || null });
    });
    child.stdin.end(request(id));
  });
}

function median(values) {
  const sorted = [...values].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

try {
  const generated = spawnSync(process.execPath, ["benchmarks/generate.mjs", "--out", directory, "--files", String(files)], { cwd: root, encoding: "utf8" });
  if (generated.status !== 0) throw new Error(`generator failed: ${generated.stderr}`);
  rmSync(join(directory, ".shrike"), { recursive: true, force: true });

  const cold = await run("cold");
  for (let index = 0; index < warmups; index += 1) await run(`warmup-${index}`);
  const warm = [];
  for (let index = 0; index < runs; index += 1) warm.push(await run(`warm-${index}`));

  const report = {
    engine,
    files: cold.snapshot.files.length,
    symbols: cold.snapshot.symbols.length,
    candidates: cold.snapshot.candidates.length,
    coldMs: Math.round(cold.ms),
    warmMs: Math.round(median(warm.map((value) => value.ms))),
    warmRunsMs: warm.map((value) => Math.round(value.ms)),
    peakRssMb: cold.rssKb === null ? null : Math.round(cold.rssKb / 1024),
    cacheEntries: existsSync(join(directory, ".shrike", "cache", "snapshots")) ? readdirSync(join(directory, ".shrike", "cache", "snapshots")).length : 0,
    generatedFiles: files,
  };
  console.log(JSON.stringify(report, null, 2));
  if (process.env.SHRIKE_BENCH_KEEP === "1") console.error(`kept ${directory}`);
} finally {
  if (process.env.SHRIKE_BENCH_KEEP !== "1") rmSync(directory, { recursive: true, force: true });
}
