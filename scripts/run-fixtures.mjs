#!/usr/bin/env node
/**
 * Fixture matrix runner.
 *
 * With `--engine` (or when target/debug/shrike-engine exists) this runs the
 * native engine over every delta scenario and checks the expected newly-dead
 * symbols. Verification fixtures are reported but executed by the host tests.
 */
import { spawn, spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";

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
      const bySymbol = new Map(delta.newlyDead.map((item) => [item.symbol, item]));
      const triggerMismatches = [];
      for (const [symbol, kind] of Object.entries(manifest.expected?.triggerKinds ?? {})) {
        if (bySymbol.get(symbol)?.trigger?.kind !== kind) {
          triggerMismatches.push(`${symbol} kind=${bySymbol.get(symbol)?.trigger?.kind ?? "missing"} (wanted ${kind})`);
        }
      }
      for (const [symbol, file] of Object.entries(manifest.expected?.triggerFiles ?? {})) {
        if (bySymbol.get(symbol)?.trigger?.file !== file) {
          triggerMismatches.push(`${symbol} file=${bySymbol.get(symbol)?.trigger?.file ?? "missing"} (wanted ${file})`);
        }
      }
      if (missing.length > 0 || unexpected.length > 0 || triggerMismatches.length > 0) {
        failures += 1;
        console.log(`FAIL ${scenario}: missing=[${missing.join(", ")}] unexpected=[${unexpected.join(", ")}] triggers=[${triggerMismatches.join(", ")}]`);
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
      runtime: route.runtime ?? null,
      unknown: route.unknown,
    }));
    const checks = [
      ["status", joinResult.status, expected.status],
      ["joined", joinResult.joined, expected.joined],
      ["total", joinResult.total, expected.total],
      ["unmatched", joinResult.unmatched, expected.unmatched],
      ["bytes", joinResult.bytes, expected.bytes],
      ["unknownSizes", joinResult.unknownSizes, expected.unknownSizes],
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

function subsetMatch(actual, wanted) {
  return Object.entries(wanted).every(([key, value]) => JSON.stringify(actual?.[key]) === JSON.stringify(value));
}

function compareJoin(joinResult, expected) {
  const mismatches = [];
  for (const key of ["status", "joined", "total"]) {
    if (key in expected && joinResult[key] !== expected[key]) {
      mismatches.push([key, joinResult[key], expected[key]]);
    }
  }
  if ("coverage" in expected && Math.abs(joinResult.coverage - expected.coverage) > 1e-9) {
    mismatches.push(["coverage", joinResult.coverage, expected.coverage]);
  }
  for (const key of ["bytes", "unknownSizes", "routes", "unmatched"]) {
    if (key in expected && JSON.stringify(joinResult[key]) !== JSON.stringify(expected[key])) {
      mismatches.push([key, joinResult[key], expected[key]]);
    }
  }
  for (const wanted of expected.edges ?? []) {
    if (!(joinResult.edges ?? []).some((edge) => subsetMatch(edge, wanted))) {
      mismatches.push(["edges", joinResult.edges, wanted]);
    }
  }
  for (const [symbol, verdict] of Object.entries(expected.verdicts ?? {})) {
    const actual = (joinResult.verdicts ?? []).find((item) => item.symbol === symbol);
    if (actual?.verdict !== verdict) mismatches.push([`verdict ${symbol}`, actual?.verdict, verdict]);
  }
  for (const wanted of expected.diagnostics ?? []) {
    if (!(joinResult.diagnostics ?? []).some((item) => subsetMatch(item, wanted))) {
      mismatches.push(["diagnostics", joinResult.diagnostics, wanted]);
    }
  }
  return mismatches;
}

function copyFixtureSource(source, destination) {
  cpSync(source, destination, {
    recursive: true,
    filter: (path) => !path.split(/[\\/]/).includes(".shrike") && !path.split(/[\\/]/).includes(".next"),
  });
}

function runCli(cwd, args) {
  return spawnSync(process.execPath, [join(root, "packages", "cli", "src", "main.ts"), ...args], {
    cwd,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, SHRIKE_ENGINE: engine },
  });
}

if (runEngine) {
  const buildGraphDir = join(root, "fixtures", "build-graph");
  const manifest = readJson(join(buildGraphDir, "manifest.json"));
  const source = join(buildGraphDir, manifest.source);
  const work = mkdtempSync(join(tmpdir(), "shrike-build-graph-"));
  try {
    copyFixtureSource(source, work);
    const analyzed = await request("analyze", { root: work, config: {} });
    for (const scenario of manifest.scenarios) {
      const expected = scenario.expected ?? {};
      try {
        if (scenario.kind === "host") {
          const directory = mkdtempSync(join(tmpdir(), `shrike-build-graph-${scenario.name}-`));
          try {
            copyFixtureSource(source, directory);
            const args = ["evidence", "--json"];
            if (scenario.evidence) {
              const evidencePath = join(buildGraphDir, scenario.evidence);
              const target = join(directory, basename(evidencePath));
              cpSync(evidencePath, target);
              args.push("--build", basename(evidencePath));
            }
            const result = runCli(directory, args);
            const output = `${result.stdout}${result.stderr}`;
            const actualExit = result.status;
            if ("exit" in expected && actualExit !== expected.exit) {
              throw new Error(`expected exit ${expected.exit}, got ${actualExit}: ${output}`);
            }
            if (expected.error && !output.includes(expected.error)) {
              throw new Error(`expected error containing ${JSON.stringify(expected.error)}: ${output}`);
            }
            if (expected.status) {
              const payload = JSON.parse(result.stdout);
              if (payload.status !== expected.status) {
                throw new Error(`expected status ${expected.status}, got ${payload.status}`);
              }
            }
          } finally {
            rmSync(directory, { recursive: true, force: true });
          }
        } else {
          const evidence = readJson(join(buildGraphDir, scenario.evidence));
          if (scenario.kind === "augment") {
            const result = await request("augment-build", { snapshot: analyzed.snapshot, build: evidence });
            const mismatches = compareJoin(result.join, expected);
            for (const symbol of expected.live ?? []) {
              if (!result.snapshot.live.includes(symbol)) {
                mismatches.push(["live", symbol, "not live"]);
              }
            }
            for (const symbol of expected.dead ?? []) {
              if (result.snapshot.live.includes(symbol) || !result.snapshot.candidates.includes(symbol)) {
                mismatches.push(["dead", symbol, "not a dead candidate"]);
              }
            }
            if (mismatches.length > 0) {
              failures += 1;
              for (const [name, actual, wanted] of mismatches) {
                console.log(`FAIL build-graph ${scenario.name} ${name}: expected ${JSON.stringify(wanted)}, got ${JSON.stringify(actual)}`);
              }
            } else {
              console.log(`PASS build-graph ${scenario.name}: ${result.join.status}, ${result.snapshot.candidates.length} candidates`);
            }
          } else {
            const joinResult = await request("join-build", { snapshot: analyzed.snapshot, build: evidence });
            const mismatches = compareJoin(joinResult, expected);
            if (mismatches.length > 0) {
              failures += 1;
              for (const [name, actual, wanted] of mismatches) {
                console.log(`FAIL build-graph ${scenario.name} ${name}: expected ${JSON.stringify(wanted)}, got ${JSON.stringify(actual)}`);
              }
            } else {
              console.log(`PASS build-graph ${scenario.name}: ${joinResult.status}`);
            }
          }
        }
      } catch (error) {
        failures += 1;
        console.log(`ERROR build-graph ${scenario.name}: ${error.message}`);
      }
    }
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

for (const [name, path] of [
  ["verification", join(root, "fixtures", "verification", "manifest.json")],
  ["build", join(root, "fixtures", "build", "manifest.json")],
  ["build-graph", join(root, "fixtures", "build-graph", "manifest.json")],
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
