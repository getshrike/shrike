import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const bundled = join(root, "packages", "action", "dist", "main.cjs");
const action = existsSync(bundled) ? bundled : join(root, "packages", "action", "src", "main.ts");

function scratch(name) {
  return mkdtempSync(join(tmpdir(), `shrike-action-${name}-`));
}

function fakeShrike(directory, exitCode = 0) {
  const bin = join(directory, "bin");
  mkdirSync(bin, { recursive: true });
  const path = join(bin, "shrike");
  writeFileSync(
    path,
    `#!/usr/bin/env node
const fs = require("node:fs");
const args = process.argv.slice(2);
fs.appendFileSync(process.env.FAKE_SHRIKE_LOG, JSON.stringify(args) + "\\n");
if (args[0] === "delta") console.log(JSON.stringify({ newlyDead: [{ symbol: "lib/x.ts#x", file: "lib/x.ts", estimatedLines: 2 }], newlyDeadFiles: [] }));
if (args[0] === "prune") console.log(JSON.stringify({ outcome: "verified", receipts: ["rcpt_test"] }));
process.exit(${exitCode});
`,
  );
  chmodSync(path, 0o755);
  return bin;
}

function run(directory, env) {
  const output = join(directory, "github-output");
  writeFileSync(output, "");
  const log = join(directory, "shrike.log");
  writeFileSync(log, "");
  const result = spawnSync(process.execPath, [action], {
    cwd: directory,
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${join(directory, "bin")}:${process.env.PATH}`,
      GITHUB_WORKSPACE: directory,
      GITHUB_OUTPUT: output,
      FAKE_SHRIKE_LOG: log,
      INPUT_BASE: "origin/main",
      ...env,
    },
  });
  return { result, output: readFileSync(output, "utf8"), log: readFileSync(log, "utf8") };
}

test("delta mode reports a count and fails only above max-items", () => {
  const directory = scratch("delta");
  try {
    fakeShrike(directory);
    const below = run(directory, { INPUT_MODE: "delta", INPUT_MAX_ITEMS: "2" });
    assert.equal(below.result.status, 0, below.result.stderr);
    assert.match(below.output, /count=1/);
    assert.match(below.output, /delta-path=/);

    const above = run(directory, { INPUT_MODE: "delta", INPUT_MAX_ITEMS: "0" });
    assert.equal(above.result.status, 0, above.result.stderr);

    writeFileSync(join(directory, "bin", "shrike"), readFileSync(join(directory, "bin", "shrike"), "utf8").replace("process.exit(0)", "process.exit(1)"));
    const failing = run(directory, { INPUT_MODE: "delta", INPUT_MAX_ITEMS: "0" });
    assert.equal(failing.result.status, 0, "exit 1 from delta is a finding, not an Action failure");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("fail-on newly-dead fails on a finding even when max-items is disabled", () => {
  const directory = scratch("fail-on");
  try {
    fakeShrike(directory);
    const { result } = run(directory, { INPUT_MODE: "delta", INPUT_MAX_ITEMS: "0", INPUT_FAIL_ON: "newly-dead" });
    assert.equal(result.status, 1, result.stderr);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("invalid fail-on fails closed without invoking the CLI", () => {
  const directory = scratch("fail-on-invalid");
  try {
    fakeShrike(directory);
    const { result, log } = run(directory, { INPUT_MODE: "delta", INPUT_FAIL_ON: "sometimes" });
    assert.equal(result.status, 2, result.stderr);
    assert.match(result.stderr, /expected newly-dead or never/);
    assert.equal(log, "");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("delta artifacts are replaced on rerun and removed when the delta fails", () => {
  const directory = scratch("artifact");
  try {
    fakeShrike(directory);
    const first = run(directory, { INPUT_MODE: "delta" });
    assert.equal(first.result.status, 0, first.result.stderr);
    const artifact = join(directory, ".shrike", "action-delta.json");
    const firstBytes = readFileSync(artifact, "utf8");
    assert.equal(JSON.parse(firstBytes).newlyDead.length, 1);

    const second = run(directory, { INPUT_MODE: "delta" });
    assert.equal(second.result.status, 0, second.result.stderr);
    assert.equal(readFileSync(artifact, "utf8"), firstBytes, "the artifact was appended to instead of replaced");

    writeFileSync(join(directory, "bin", "shrike"), "#!/usr/bin/env node\nprocess.exit(4);\n");
    const failed = run(directory, { INPUT_MODE: "delta" });
    assert.equal(failed.result.status, 4, failed.result.stderr);
    assert.equal(existsSync(artifact), false, "a failed run left stale evidence behind");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("prune refuses a fork pull request before invoking the CLI", () => {
  const directory = scratch("fork");
  try {
    fakeShrike(directory);
    const event = join(directory, "event.json");
    writeFileSync(event, JSON.stringify({
      pull_request: { head: { repo: { full_name: "fork/app" } } },
      repository: { full_name: "owner/app" },
    }));
    const { result, log } = run(directory, {
      INPUT_MODE: "prune",
      GITHUB_EVENT_NAME: "pull_request",
      GITHUB_EVENT_PATH: event,
    });
    assert.equal(result.status, 2, result.stderr);
    assert.match(result.stderr, /refusing prune on a fork/);
    assert.equal(log, "", "the CLI was invoked on a fork");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("prune on the base repository records the outcome and receipts", () => {
  const directory = scratch("prune");
  try {
    fakeShrike(directory);
    const event = join(directory, "event.json");
    writeFileSync(event, JSON.stringify({
      pull_request: { head: { repo: { full_name: "owner/app" } } },
      repository: { full_name: "owner/app" },
    }));
    const { result, output, log } = run(directory, {
      INPUT_MODE: "prune",
      GITHUB_EVENT_NAME: "pull_request",
      GITHUB_EVENT_PATH: event,
    });
    assert.equal(result.status, 0, result.stderr);
    assert.match(output, /outcome=verified/);
    assert.match(output, /receipts=1/);
    assert.match(log, /"prune"/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("unknown modes fail closed without invoking the CLI", () => {
  const directory = scratch("unknown");
  try {
    fakeShrike(directory);
    const { result, log } = run(directory, { INPUT_MODE: "scan" });
    assert.equal(result.status, 2, result.stderr);
    assert.match(result.stderr, /expected delta or prune/);
    assert.equal(log, "");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
