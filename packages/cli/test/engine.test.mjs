import test, { afterEach } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { findEngine } from "../src/engine.ts";

const originalCwd = process.cwd();
const originalEngine = process.env.SHRIKE_ENGINE;
const originalPath = process.env.PATH;
const directories = [];

afterEach(() => {
  process.chdir(originalCwd);
  if (originalEngine === undefined) delete process.env.SHRIKE_ENGINE;
  else process.env.SHRIKE_ENGINE = originalEngine;
  if (originalPath === undefined) delete process.env.PATH;
  else process.env.PATH = originalPath;
  for (const directory of directories.splice(0)) {
    rmSync(directory, { recursive: true, force: true });
  }
});

function scratch() {
  const directory = mkdtempSync(join(tmpdir(), "shrike-engine-"));
  directories.push(directory);
  return directory;
}

function fakeEngine(directory) {
  mkdirSync(directory, { recursive: true });
  const path = join(directory, process.platform === "win32" ? "shrike-engine.exe" : "shrike-engine");
  writeFileSync(path, "#!/usr/bin/env node\n");
  if (process.platform !== "win32") chmodSync(path, 0o755);
  return path;
}

function writeSidecar(engine, digest) {
  writeFileSync(`${engine}.sha256`, `${digest}  ${engine.split("/").pop()}\n`);
}

test("SHRIKE_ENGINE is used without a sidecar", () => {
  const engine = fakeEngine(scratch());
  process.env.SHRIKE_ENGINE = engine;
  assert.equal(findEngine(), engine);
});

test("a matching sidecar is accepted", () => {
  const engine = fakeEngine(scratch());
  process.env.SHRIKE_ENGINE = engine;
  writeSidecar(engine, createHash("sha256").update(readFileSync(engine)).digest("hex"));
  assert.equal(findEngine(), engine);
});

test("a mismatched sidecar fails closed", () => {
  const engine = fakeEngine(scratch());
  process.env.SHRIKE_ENGINE = engine;
  writeSidecar(engine, "0".repeat(64));
  assert.throws(() => findEngine(), /checksum mismatch/);
});

test("a malformed sidecar fails closed", () => {
  const engine = fakeEngine(scratch());
  process.env.SHRIKE_ENGINE = engine;
  writeFileSync(`${engine}.sha256`, "not-a-digest\n");
  assert.throws(() => findEngine(), /malformed/);
});

test("a repository target build is found from a nested directory", () => {
  const directory = scratch();
  const engine = fakeEngine(join(directory, "target", "release"));
  const nested = join(directory, "app", "src");
  mkdirSync(nested, { recursive: true });
  delete process.env.SHRIKE_ENGINE;
  process.chdir(nested);
  assert.equal(findEngine(), engine);
});

test("a broken SHRIKE_ENGINE falls through to a repository build", () => {
  const directory = scratch();
  const engine = fakeEngine(join(directory, "target", "debug"));
  process.env.SHRIKE_ENGINE = join(directory, "missing-engine");
  process.chdir(directory);
  assert.equal(findEngine(), engine);
});
