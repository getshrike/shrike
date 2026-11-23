#!/usr/bin/env node
/**
 * Validate real JSON instances against the Shrike JSON Schemas.
 *
 * `check-schemas.mjs` only proves the schema files and fixtures are parseable.
 * This gate proves the contract in the other direction: actual engine output,
 * the default config, and generated/fixture receipts conform to their schemas.
 * A deliberately invalid object must fail (negative self-test), so a schema
 * that silently accepts everything also fails this gate.
 */
import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { DEFAULT_CONFIG, engineConfig } from "../packages/cli/src/config.ts";

const root = resolve(new URL("..", import.meta.url).pathname);
const engineCandidates = [
  join(root, "target", "release", "shrike-engine"),
  join(root, "target", "debug", "shrike-engine"),
];
const engine = process.env.SHRIKE_ENGINE ?? engineCandidates.find(existsSync);
const failures = [];
let checked = 0;

const ajv = new Ajv2020({ allErrors: true, strict: false });
addFormats(ajv);
const schemas = new Map();
for (const name of [
  "config",
  "deletion-plan",
  "delta",
  "graph-snapshot",
  "protocol",
  "receipt",
]) {
  const path = join(root, "schemas", `${name}.schema.json`);
  const schema = JSON.parse(readFileSync(path, "utf8"));
  ajv.addSchema(schema);
  schemas.set(name, schema);
}
const validator = (name) => ajv.getSchema(schemas.get(name).$id);

function validate(name, instance, label) {
  checked += 1;
  const ok = validator(name)(instance);
  if (!ok) {
    const first = validator(name).errors?.slice(0, 3).map((e) => `${e.instancePath || "/"} ${e.message}`).join("; ");
    failures.push(`${label}: does not match ${name}.schema.json (${first})`);
  }
}

function request(id, command, payload) {
  return JSON.stringify({ protocolVersion: "1.0", id, command, payload });
}

function run(line) {
  if (!engine) throw new Error("build shrike-engine first (cargo build --release -p shrike-engine)");
  const result = spawnSync(engine, [], {
    input: `${line}\n`,
    encoding: "utf8",
    maxBuffer: 128 * 1024 * 1024,
  });
  if (result.status !== 0) throw new Error(`engine failed: ${result.stderr}`);
  return JSON.parse(result.stdout);
}

function fixtureRoots(directory, depth = 0) {
  if (depth > 4 || !existsSync(directory)) return [];
  const found = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (!entry.isDirectory() || entry.name.startsWith(".") || entry.name === "node_modules") continue;
    const path = join(directory, entry.name);
    if (existsSync(join(path, "package.json"))) found.push(path);
    found.push(...fixtureRoots(path, depth + 1));
  }
  return found;
}

async function main() {
  // The CLI's own default config is the canonical config instance.
  validate("config", engineConfig(DEFAULT_CONFIG), "DEFAULT_CONFIG");

  const relative = (p) => p.slice(root.length + 1);
  const roots = fixtureRoots(join(root, "fixtures"));
  const samples = new Map();
  for (const dir of roots) {
    const analyzeLine = request("req_schema", "analyze", {
      root: relative(dir),
      config: {},
    });
    validate("protocol", JSON.parse(analyzeLine), `${relative(dir)} request envelope`);
    let response;
    try {
      response = run(analyzeLine);
    } catch (error) {
      failures.push(`${relative(dir)} analyze: ${error.message}`);
      continue;
    }
    validate("protocol", response, `${relative(dir)} response envelope`);
    if (!response.ok) {
      failures.push(`${relative(dir)} analyze: engine error ${response.error?.code}`);
      continue;
    }
    const snapshot = response.result.snapshot;
    validate("graph-snapshot", snapshot, `${relative(dir)} snapshot`);
    samples.set("analyze", snapshot);
    // Keep a small spread of roots; the last one is used for the derived
    // delta/plan commands below.
    samples.set("last", snapshot);
  }

  // One small fixture drives the protocol envelope of every other command.
  const base = samples.get("analyze");
  if (base) {
    const head = run(request("req_schema", "analyze", {
      root: "fixtures/delta/removed-caller/head",
      config: {},
    })).result.snapshot;
    validate("graph-snapshot", head, "delta head snapshot");

    const commands = [
      ["ping", {}],
      ["scan", { root: "fixtures/build-graph/source", config: {} }],
      ["delta", { baseSnapshot: base, headSnapshot: head, baseCommit: "base", headCommit: "head" }],
      ["proof", { snapshot: base, symbol: base.symbols[0].key }],
    ];
    for (const [command, payload] of commands) {
      const line = request("req_schema", command, payload);
      validate("protocol", JSON.parse(line), `${command} request envelope`);
      const response = run(line);
      validate("protocol", response, `${command} response envelope`);
      if (command === "scan") validate("graph-snapshot", response.result.snapshot, "scan snapshot");
      if (command === "delta") {
        validate("delta", response.result, "delta result");
        const plan = run(request("req_schema", "plan", {
          snapshot: base,
          delta: response.result,
          config: engineConfig(DEFAULT_CONFIG),
        }));
        validate("protocol", plan, "plan response envelope");
        validate("deletion-plan", plan.result, "deletion plan result");
      }
    }
  }

  // Receipt fixtures and any locally generated receipts must be instances.
  for (const dir of [join(root, "fixtures", "receipts"), join(root, ".shrike", "receipts")]) {
    if (!existsSync(dir)) continue;
    for (const name of readdirSync(dir)) {
      if (!name.endsWith(".json")) continue;
      const receipt = JSON.parse(readFileSync(join(dir, name), "utf8"));
      validate("receipt", receipt, `${relative(dir)}/${name}`);
    }
  }

  // Negative self-test: a corrupted snapshot must be rejected.
  checked += 1;
  const corrupted = { ...base, graphHash: 42 };
  if (validator("graph-snapshot")(corrupted)) {
    failures.push("negative self-test: invalid graphHash (number) was accepted");
  }

  if (failures.length) {
    for (const failure of failures.slice(0, 30)) console.error(`FAIL ${failure}`);
    if (failures.length > 30) console.error(`... and ${failures.length - 30} more`);
    process.exit(1);
  }
  console.log(`OK: ${checked} JSON instances validate against 6 schemas (${roots.length} fixture snapshots)`);
}

main().catch((error) => {
  console.error(`FAIL ${error.stack ?? error}`);
  process.exit(1);
});
