#!/usr/bin/env node
/**
 * Fixture matrix runner.
 *
 * With `--engine` (or when target/debug/shrike-engine exists) this runs the
 * native engine over every delta scenario and checks the expected newly-dead
 * symbols. Verification fixtures are reported but executed by the host tests.
 */
import { spawn } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(new URL("..", import.meta.url).pathname);
const engine = process.env.SHRIKE_ENGINE ?? join(root, "target", "debug", "shrike-engine");
const runEngine = process.argv.includes("--engine") || existsSync(engine);
let failures = 0;

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function request(command, payload) {
  return new Promise((resolveRequest, reject) => {
    const child = spawn(engine, [], { stdio: ["pipe", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", reject);
    child.on("close", (code) => {
      const line = stdout.split("\n").find((value) => value.trim());
      if (!line) return reject(new Error(`engine exit ${code}: ${stderr.trim()}`));
      const response = JSON.parse(line);
      if (!response.ok) return reject(new Error(`${response.error.code}: ${response.error.message}`));
      resolveRequest(response.result);
    });
    child.stdin.end(`${JSON.stringify({ protocolVersion: "1.0", id: "fixture", command, payload })}\n`);
  });
}

const deltaDir = join(root, "fixtures", "delta");
const scenarios = readdirSync(deltaDir).filter((name) => existsSync(join(deltaDir, name, "manifest.json"))).sort();
console.log(`delta fixtures: ${scenarios.length}`);

if (runEngine) {
  for (const scenario of scenarios) {
    const directory = join(deltaDir, scenario);
    const manifest = readJson(join(directory, "manifest.json"));
    try {
      const config = { ...manifest.config };
      const base = await request("analyze", { root: join(directory, "base"), config });
      const head = await request("analyze", { root: join(directory, "head"), config });
      const delta = await request("delta", {
        baseSnapshot: base.snapshot,
        headSnapshot: head.snapshot,
        baseRef: "fixture",
        baseCommit: "base",
        headCommit: "head",
        dirty: Boolean(manifest.dirty),
      });
      const actual = new Set(delta.newlyDead.map((item) => item.symbol));
      const missing = (manifest.expected?.newlyDead ?? []).filter((symbol) => !actual.has(symbol));
      const unexpected = [...actual].filter((symbol) => !(manifest.expected?.newlyDead ?? []).includes(symbol));
      if (missing.length > 0 || unexpected.length > 0) {
        failures += 1;
        console.log(`FAIL ${scenario}: missing=[${missing.join(", ")}] unexpected=[${unexpected.join(", ")}]`);
      } else {
        console.log(`PASS ${scenario}: ${actual.size} newly dead`);
      }
    } catch (error) {
      failures += 1;
      console.log(`ERROR ${scenario}: ${error.message}`);
    }
  }
} else {
  console.log("SKIP engine delta checks: shrike-engine not built (pass --engine after cargo build)");
}

if (runEngine) {
  const buildDir = join(root, "fixtures", "build");
  const manifest = readJson(join(buildDir, "manifest.json"));
  try {
    const evidence = readJson(join(buildDir, manifest.buildFile));
    const analyzed = await request("analyze", { root: join(buildDir, "source"), config: {} });
    const joinResult = await request("join-build", { snapshot: analyzed.snapshot, build: evidence });
    const expected = manifest.expected ?? {};
    const actualRoutes = (joinResult.routes ?? []).map((route) => ({
      route: route.route,
      files: route.files,
      symbols: route.symbols,
      bytes: route.bytes,
    }));
    const checks = [
      ["status", joinResult.status, expected.status],
      ["joined", joinResult.joined, expected.joined],
      ["total", joinResult.total, expected.total],
      ["unmatched", joinResult.unmatched, expected.unmatched],
      ["bytes", joinResult.bytes, expected.bytes],
      ["routes", actualRoutes, expected.routes],
    ];
    if (Math.abs(joinResult.coverage - expected.coverage) > 1e-9) {
      checks.push(["coverage", joinResult.coverage, expected.coverage]);
    }
    const mismatches = checks.filter(([, actual, wanted]) => JSON.stringify(actual) !== JSON.stringify(wanted));
    if (mismatches.length > 0) {
      failures += 1;
      for (const [name, actual, wanted] of mismatches) {
        console.log(`FAIL build ${name}: expected ${JSON.stringify(wanted)}, got ${JSON.stringify(actual)}`);
      }
    } else {
      console.log(`PASS build join: ${joinResult.status}, coverage ${joinResult.coverage.toFixed(3)}, ${joinResult.bytes.matched} matched bytes`);
    }
  } catch (error) {
    failures += 1;
    console.log(`ERROR build join: ${error.message}`);
  }
}

for (const [name, path] of [
  ["verification", join(root, "fixtures", "verification", "manifest.json")],
  ["build", join(root, "fixtures", "build", "manifest.json")],
]) {
  if (existsSync(path)) {
    const manifest = readJson(path);
    const count = manifest.scenarios?.length ?? 1;
    console.log(`${name} fixtures: ${count}`);
  } else {
    failures += 1;
    console.log(`FAIL missing ${name} manifest`);
  }
}

if (failures > 0) {
  console.error(`FAILED: ${failures} fixture(s)`);
  process.exit(1);
}
console.log("OK: fixture matrix complete");
