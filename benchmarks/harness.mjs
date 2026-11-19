#!/usr/bin/env node
/** Run the engine against a generated repo and report wall time and RSS. */
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
const root = resolve(new URL("..", import.meta.url).pathname);
const engine = process.env.SHRIKE_ENGINE ?? [join(root, "target", "release", "shrike-engine"), join(root, "target", "debug", "shrike-engine")].find((path) => existsSync(path)) ?? join(root, "target", "release", "shrike-engine");
const files = process.argv.includes("--files") ? process.argv[process.argv.indexOf("--files") + 1] : "10000";
const directory = mkdtempSync(join(tmpdir(), "shrike-bench-"));
spawnSync(process.execPath, ["benchmarks/generate.mjs", "--out", directory, "--files", files], { cwd: root, stdio: "inherit" });
if (!existsSync(engine)) { console.error(`SKIP: engine not built at ${engine}`); process.exit(4); }
const start = process.hrtime.bigint();
const child = spawn(engine, [], { stdio: ["pipe", "pipe", "inherit"] });
let out = "";
child.stdout.setEncoding("utf8");
child.stdout.on("data", (chunk) => { out += chunk; });
child.stdin.end(`${JSON.stringify({ protocolVersion: "1.0", id: "bench", command: "analyze", payload: { root: directory, config: {} } })}\n`);
await new Promise((resolveRun) => child.on("close", resolveRun));
const ms = Number(process.hrtime.bigint() - start) / 1e6;
const response = JSON.parse(out.split("\n").find((line) => line.trim()));
const snapshot = response.result.snapshot;
console.log(JSON.stringify({ files: snapshot.files.length, symbols: snapshot.symbols.length, candidates: snapshot.candidates.length, ms: Math.round(ms), target: directory }, null, 2));
