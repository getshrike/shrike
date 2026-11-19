import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const cli = join(root, "packages", "cli", "src", "main.ts");
const engine = process.env.SHRIKE_ENGINE ?? join(root, "target", "debug", "shrike-engine");
const hasEngine = existsSync(engine);

function run(cwd, args, env = {}) {
  return spawnSync(process.execPath, [cli, ...args], {
    cwd,
    encoding: "utf8",
    env: { ...process.env, SHRIKE_ENGINE: engine, ...env },
    maxBuffer: 64 * 1024 * 1024,
  });
}

function scratch(name) {
  return mkdtempSync(join(tmpdir(), `shrike-build-${name}-`));
}

function write(cwd, path, content) {
  const full = join(cwd, path);
  mkdirSync(dirname(full), { recursive: true });
  writeFileSync(full, content);
}

function git(cwd, args) {
  const result = spawnSync("git", args, { cwd, encoding: "utf8" });
  if (result.status !== 0) throw new Error(`git ${args.join(" ")}: ${result.stderr}`);
  return result.stdout.trim();
}

function makeRepo(name) {
  const directory = scratch(name);
  git(directory, ["init", "-q", "-b", "main"]);
  git(directory, ["config", "user.email", "build@shrike.local"]);
  git(directory, ["config", "user.name", "Shrike Build"]);
  return directory;
}

function evidence(overrides = {}) {
  return JSON.stringify({
    source: "next analyze export",
    version: "16.4.0",
    files: ["app/page.tsx", "lib/orphan.ts"],
    symbols: ["app/page.tsx#default", "lib/orphan.ts#orphan"],
    routes: [
      {
        route: "/",
        files: ["app/page.tsx", "lib/orphan.ts"],
        symbols: ["app/page.tsx#default", "lib/orphan.ts#orphan"],
      },
    ],
    sizes: { "app/page.tsx": 100, "lib/orphan.ts": 40 },
    ...overrides,
  });
}

function baseApp(directory, name = "build-app") {
  write(directory, "app/page.tsx", "export default function Page() { return null; }\n");
  write(directory, "lib/orphan.ts", "export function orphan() { return 1; }\n");
  write(directory, "package.json", `{"name":"${name}","private":true,"type":"module","dependencies":{"next":"16.4.0"}}\n`);
}

function receipts(directory) {
  const path = join(directory, ".shrike", "receipts");
  if (!existsSync(path)) return [];
  return readdirSync(path)
    .filter((entry) => entry.startsWith("rcpt_"))
    .map((entry) => JSON.parse(readFileSync(join(path, entry), "utf8")));
}

test("evidence --build reports coverage, routes, and shared bytes", { skip: !hasEngine }, () => {
  const directory = scratch("join");
  try {
    baseApp(directory);
    write(directory, "evidence.json", evidence());
    const result = run(directory, ["evidence", "--build", "evidence.json", "--json"]);
    assert.equal(result.status, 0, result.stderr);
    const joined = JSON.parse(result.stdout);
    assert.equal(joined.status, "full");
    assert.equal(joined.joined, 4);
    assert.equal(joined.total, 4);
    assert.equal(joined.coverage, 1);
    assert.deepEqual(joined.unmatched, []);
    assert.equal(joined.bytes.matched, 140);
    assert.equal(joined.bytes.shared, 0);
    assert.equal(joined.bytes.totalRoute, 140);
    assert.equal(joined.routes[0].route, "/");
    assert.equal(joined.routes[0].files, 2);
    assert.equal(joined.routes[0].symbols, 2);
    assert.equal(joined.evidence.version, "16.4.0");
    assert.match(joined.evidence.snapshotHash, /^sha256:[0-9a-f]{64}$/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("evidence discovers a default .next/analyze export", { skip: !hasEngine }, () => {
  const directory = scratch("discover");
  try {
    baseApp(directory);
    write(directory, ".next/analyze/export.json", evidence());
    const result = run(directory, ["evidence", "--json"]);
    assert.equal(result.status, 0, result.stderr);
    const joined = JSON.parse(result.stdout);
    assert.equal(joined.status, "full");
    assert.match(joined.evidence.path, /\.next[/\\]analyze[/\\]export\.json$/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("malformed build evidence fails closed with exit 4", { skip: !hasEngine }, () => {
  const directory = scratch("malformed");
  try {
    baseApp(directory);
    write(directory, "evidence.json", "{ not json\n");
    const result = run(directory, ["evidence", "--build", "evidence.json", "--json"]);
    assert.equal(result.status, 4, result.stderr);
    assert.match(`${result.stderr}${result.stdout}`, /invalid JSON/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("build evidence outside the repository is refused", { skip: !hasEngine }, () => {
  const directory = scratch("traversal");
  const outside = join(directory, "..", `shrike-outside-${process.pid}.json`);
  try {
    baseApp(directory);
    writeFileSync(outside, evidence());
    const result = run(directory, ["evidence", "--build", "../" + outside.split("/").pop(), "--json"]);
    assert.equal(result.status, 4, result.stderr);
    assert.match(`${result.stderr}${result.stdout}`, /path traversal|outside the repository/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
    rmSync(outside, { force: true });
  }
});

test("unsupported Next.js versions are refused when evidence is required", { skip: !hasEngine }, () => {
  const directory = scratch("unsupported");
  try {
    baseApp(directory);
    write(directory, "evidence.json", evidence({ version: "15.2.0" }));
    const optional = run(directory, ["evidence", "--build", "evidence.json", "--json"]);
    assert.equal(optional.status, 0, optional.stderr);
    assert.equal(JSON.parse(optional.stdout).status, "unsupported");

    const required = run(directory, [
      "evidence", "--build", "evidence.json", "--require-build-evidence", "--json",
    ]);
    assert.equal(required.status, 4, required.stderr);
    assert.match(`${required.stderr}${required.stdout}`, /required but status is unsupported/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("required build evidence fails closed when no evidence exists", { skip: !hasEngine }, () => {
  const directory = scratch("absent");
  try {
    baseApp(directory);
    const result = run(directory, ["evidence", "--require-build-evidence", "--json"]);
    assert.equal(result.status, 4, result.stderr);
    assert.match(`${result.stderr}${result.stdout}`, /required but status is none/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("full build evidence suppresses a candidate; low coverage does not", { skip: !hasEngine }, () => {
  const directory = makeRepo("suppress");
  try {
    write(directory, "app/page.tsx", "import { formatInvoice } from '../lib/format';\nexport default function Page() { return formatInvoice(1); }\n");
    write(directory, "lib/format.ts", "export function formatInvoice(value) { return String(value); }\n");
    write(directory, "package.json", '{"name":"suppress","private":true,"type":"module","dependencies":{"next":"16.4.0"}}\n');
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "base"]);
    git(directory, ["checkout", "-q", "-b", "feature"]);
    write(directory, "app/page.tsx", "export default function Page() { return null; }\n");
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "remove caller"]);

    const without = run(directory, ["delta", "--base", "main", "--json"]);
    assert.equal(without.status, 1, without.stderr);
    assert.ok(
      JSON.parse(without.stdout).newlyDead.some((item) => item.symbol === "lib/format.ts#formatInvoice"),
      "the candidate was not reported without build evidence",
    );

    write(directory, ".shrike/build/full.json", evidence({
      files: ["app/page.tsx", "lib/format.ts"],
      symbols: ["app/page.tsx#default", "lib/format.ts#formatInvoice"],
      routes: [{ route: "/", files: ["app/page.tsx", "lib/format.ts"], symbols: ["app/page.tsx#default", "lib/format.ts#formatInvoice"] }],
      sizes: { "app/page.tsx": 100, "lib/format.ts": 40 },
    }));
    const suppressed = run(directory, ["delta", "--base", "main", "--build", ".shrike/build/full.json", "--json"]);
    assert.equal(suppressed.status, 0, suppressed.stderr);
    assert.deepEqual(JSON.parse(suppressed.stdout).newlyDead, []);

    write(directory, ".shrike/build/low.json", evidence({
      files: ["app/page.tsx", "missing.tsx"],
      symbols: ["app/page.tsx#default"],
      routes: [],
      sizes: { "app/page.tsx": 100 },
    }));
    const low = run(directory, ["delta", "--base", "main", "--build", ".shrike/build/low.json", "--json"]);
    assert.equal(low.status, 1, low.stderr);
    assert.ok(
      JSON.parse(low.stdout).newlyDead.some((item) => item.symbol === "lib/format.ts#formatInvoice"),
      "low-coverage evidence suppressed a candidate",
    );
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("build evidence follows known edges from a build-live module", { skip: !hasEngine }, () => {
  const directory = makeRepo("closure");
  try {
    write(directory, "app/page.tsx", "import { helper } from '../lib/helper';\nexport default function Page() { return helper(); }\n");
    write(directory, "lib/dynamic.ts", "import { helper } from './helper';\nexport function Widget() { return helper(); }\n");
    write(directory, "lib/helper.ts", "export function helper() { return 1; }\n");
    write(directory, "package.json", '{"name":"closure","private":true,"type":"module","dependencies":{"next":"16.4.0"}}\n');
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "base"]);
    git(directory, ["checkout", "-q", "-b", "feature"]);
    write(directory, "app/page.tsx", "export default function Page() { return null; }\n");
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "remove caller"]);

    const without = run(directory, ["delta", "--base", "main", "--json"]);
    assert.equal(without.status, 1, without.stderr);
    assert.ok(
      JSON.parse(without.stdout).newlyDead.some((item) => item.symbol === "lib/helper.ts#helper"),
      "the helper was not reported without build evidence",
    );

    write(directory, ".shrike/build/closure.json", evidence({
      files: ["app/page.tsx", "lib/dynamic.ts"],
      symbols: ["app/page.tsx#default", "lib/dynamic.ts#Widget"],
      routes: [{ route: "/", files: ["app/page.tsx", "lib/dynamic.ts"], symbols: ["app/page.tsx#default", "lib/dynamic.ts#Widget"] }],
      sizes: { "app/page.tsx": 100, "lib/dynamic.ts": 50 },
    }));
    const withBuild = run(directory, ["delta", "--base", "main", "--build", ".shrike/build/closure.json", "--json"]);
    assert.equal(withBuild.status, 0, withBuild.stderr);
    assert.deepEqual(JSON.parse(withBuild.stdout).newlyDead, []);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("prune records build evidence in the receipt", { skip: !hasEngine }, () => {
  const directory = makeRepo("receipt");
  try {
    write(directory, "app/page.tsx", "import { formatInvoice } from '../lib/format';\nexport default function Page() { return formatInvoice(1); }\n");
    write(directory, "lib/format.ts", "export function formatInvoice(value) { return String(value); }\n");
    write(directory, "package.json", '{"name":"receipt-build","private":true,"type":"module","dependencies":{"next":"16.4.0"}}\n');
    write(directory, "check.mjs", "import { readFileSync } from 'node:fs';\nconst page = readFileSync('app/page.tsx', 'utf8');\nif (page.includes('formatInvoice')) throw new Error('stale caller');\n");
    write(directory, "shrike.config.json", JSON.stringify({
      checks: [{ name: "smoke", command: "node check.mjs", required: true, timeoutMs: 30_000 }],
    }));
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "base"]);
    git(directory, ["checkout", "-q", "-b", "feature"]);
    write(directory, "app/page.tsx", "export default function Page() { return null; }\n");
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "remove caller"]);

    write(directory, ".shrike/build/evidence.json", evidence({
      files: ["app/page.tsx"],
      symbols: ["app/page.tsx#default"],
      routes: [{ route: "/", files: ["app/page.tsx"], symbols: ["app/page.tsx#default"] }],
      sizes: { "app/page.tsx": 100 },
    }));
    const result = run(directory, [
      "prune", "--base", "main", "--build", ".shrike/build/evidence.json", "--json",
    ]);
    assert.equal(result.status, 0, result.stderr);
    const output = JSON.parse(result.stdout);
    assert.equal(output.outcome, "verified");
    const written = receipts(directory);
    assert.equal(written.length, 1);
    assert.equal(written[0].outcome, "verified");
    assert.equal(written[0].build.status, "full");
    assert.equal(written[0].build.version, "16.4.0");
    assert.equal(written[0].build.source, "next analyze export");
    assert.match(written[0].build.snapshotHash, /^sha256:[0-9a-f]{64}$/);
    assert.ok(written[0].batch.symbols.includes("lib/format.ts#formatInvoice"));
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("JSONL streaming normalizes module ids, layers, and client references", { skip: !hasEngine }, () => {
  const directory = scratch("jsonl");
  try {
    baseApp(directory);
    const lines = [
      { type: "module", id: "m1", path: "app/page.tsx", layer: "client", size: 100 },
      { type: "module", id: "m2", path: "lib/orphan.ts", layer: "server", size: 40 },
      { type: "entry", route: "/", runtime: "client", modules: ["m1"] },
      { type: "client-reference", from: "m1", to: "m2#orphan", referenceKind: "server-action" },
      { type: "asset", path: "static/chunk.js", size: 10 },
    ];
    write(directory, ".next/analyze/export.jsonl", `${lines.map((line) => JSON.stringify(line)).join("\n")}\n`);
    const result = run(directory, ["evidence", "--build", ".next/analyze/export.jsonl", "--json"]);
    assert.equal(result.status, 0, result.stderr);
    const joined = JSON.parse(result.stdout);
    assert.equal(joined.status, "full");
    assert.equal(joined.unmatched.length, 0);
    assert.equal(joined.evidence.unknownRecords, 1);
    assert.ok(joined.edges.some((edge) => edge.to === "lib/orphan.ts#orphan"));
    const verdict = joined.verdicts.find((item) => item.symbol === "lib/orphan.ts#orphan");
    assert.equal(verdict.verdict, "server-only");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("stale evidence is explicit and fails closed only when required", { skip: !hasEngine }, () => {
  const directory = scratch("stale");
  try {
    baseApp(directory);
    write(directory, "evidence.json", evidence({
      fileHashes: { "app/page.tsx": `sha256:${"0".repeat(64)}` },
    }));
    const optional = run(directory, ["evidence", "--build", "evidence.json", "--json"]);
    assert.equal(optional.status, 0, optional.stderr);
    assert.equal(JSON.parse(optional.stdout).status, "stale");
    const required = run(directory, [
      "evidence", "--build", "evidence.json", "--require-build-evidence", "--json",
    ]);
    assert.equal(required.status, 4, required.stderr);
    assert.match(`${required.stderr}${required.stdout}`, /required but status is stale/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("a failed build is a recorded status, not a crash", { skip: !hasEngine }, () => {
  const directory = scratch("build-failed");
  try {
    baseApp(directory);
    const optional = run(directory, [
      "evidence", "--run-build", "--build-command", "node -e \"process.exit(1)\"", "--json",
    ]);
    assert.equal(optional.status, 0, optional.stderr);
    const joined = JSON.parse(optional.stdout);
    assert.equal(joined.status, "build-failed");
    assert.match(joined.evidence.failure, /exit 1|Command failed/);
    const required = run(directory, [
      "evidence", "--run-build", "--build-command", "node -e \"process.exit(1)\"",
      "--require-build-evidence", "--json",
    ]);
    assert.equal(required.status, 4, required.stderr);
    assert.match(`${required.stderr}${required.stdout}`, /build failed/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("unknown sizes and client-boundary leaks are reported and opt-outable", { skip: !hasEngine }, () => {
  const directory = scratch("leak");
  try {
    baseApp(directory);
    write(directory, "evidence.json", evidence({
      files: ["app/page.tsx", "lib/orphan.ts"],
      symbols: ["app/page.tsx#default", "lib/orphan.ts#orphan"],
      layers: { "lib/orphan.ts": "server" },
      routes: [{
        route: "/",
        runtime: "client",
        files: ["app/page.tsx", "lib/orphan.ts"],
        symbols: ["app/page.tsx#default", "lib/orphan.ts#orphan"],
      }],
      // The symbol and the file carry sizes; the other file and symbol do not.
      sizes: { "app/page.tsx#default": 100, "lib/orphan.ts": 40 },
    }));
    const joined = run(directory, ["evidence", "--build", "evidence.json", "--json"]);
    assert.equal(joined.status, 0, joined.stderr);
    const payload = JSON.parse(joined.stdout);
    assert.equal(payload.bytes.unknown, 2);
    assert.equal(payload.bytes.partial, true);
    assert.deepEqual(payload.unknownSizes, ["app/page.tsx", "lib/orphan.ts#orphan"]);
    assert.ok(payload.diagnostics.some((item) => item.code === "client-boundary-leak"));

    const disabled = run(directory, [
      "evidence", "--build", "evidence.json", "--no-client-boundary-leaks", "--json",
    ]);
    assert.equal(disabled.status, 0, disabled.stderr);
    assert.ok(!JSON.parse(disabled.stdout).diagnostics.some((item) => item.code === "client-boundary-leak"));
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("loader prefixes, query strings, and module ids resolve by suffix", { skip: !hasEngine }, () => {
  const directory = scratch("suffix");
  try {
    baseApp(directory);
    write(directory, "evidence.json", evidence({
      files: ["webpack://next/dist/loader.js!./lib/orphan.ts?ts=1", "/repo/app/page.tsx"],
      symbols: ["m1#default", "lib/orphan.ts#orphan"],
      moduleIds: { m1: "app/page.tsx" },
      routes: [],
      sizes: {},
    }));
    const result = run(directory, ["evidence", "--build", "evidence.json", "--json"]);
    assert.equal(result.status, 0, result.stderr);
    const joined = JSON.parse(result.stdout);
    assert.equal(joined.status, "full");
    assert.deepEqual(joined.unmatched, []);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("a large JSONL export streams without loading the whole file", { skip: !hasEngine }, () => {
  const directory = scratch("large-jsonl");
  try {
    baseApp(directory);
    const lines = [
      JSON.stringify({ type: "module", id: "m0", path: "app/page.tsx", layer: "client", size: 100 }),
      ...Array.from({ length: 10_000 }, (_, index) =>
        JSON.stringify({ type: "module", id: `v${index}`, path: `virtual/module-${index}.ts`, size: 1 }),
      ),
    ];
    write(directory, ".next/analyze/export.jsonl", `${lines.join("\n")}\n`);
    const result = run(directory, ["evidence", "--build", ".next/analyze/export.jsonl", "--json"]);
    assert.equal(result.status, 0, result.stderr);
    const joined = JSON.parse(result.stdout);
    assert.equal(joined.status, "low-coverage");
    assert.ok(joined.unmatched.length >= 10_000);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("prune with required evidence exits 4 before deleting", { skip: !hasEngine }, () => {
  const directory = makeRepo("required");
  try {
    write(directory, "app/page.tsx", "import { formatInvoice } from '../lib/format';\nexport default function Page() { return formatInvoice(1); }\n");
    write(directory, "lib/format.ts", "export function formatInvoice(value) { return String(value); }\n");
    write(directory, "package.json", '{"name":"required-build","private":true,"type":"module"}\n');
    write(directory, "shrike.config.json", JSON.stringify({
      checks: [{ name: "smoke", command: "node -e 0", required: true }],
    }));
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "base"]);
    git(directory, ["checkout", "-q", "-b", "feature"]);
    write(directory, "app/page.tsx", "export default function Page() { return null; }\n");
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "remove caller"]);

    const result = run(directory, ["prune", "--base", "main", "--require-build-evidence", "--json"]);
    assert.equal(result.status, 4, result.stderr);
    assert.match(`${result.stderr}${result.stdout}`, /required but status is none/);
    assert.equal(receipts(directory).length, 0);
    assert.equal(git(directory, ["branch", "--list", "shrike/cleanup-*"]), "");
    const worktrees = git(directory, ["worktree", "list", "--porcelain"])
      .split("\n")
      .filter((line) => line.startsWith("worktree "));
    assert.equal(worktrees.length, 1, "a worktree leaked on the fail-closed path");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
