import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
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
import { writeFakeEngine } from "@shrike/testkit";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const cli = join(root, "packages", "cli", "src", "main.ts");
const engine = process.env.SHRIKE_ENGINE ?? join(root, "target", "debug", "shrike-engine");
const hasEngine = existsSync(engine);

function run(cwd, args, env = {}) {
  return spawnSync(process.execPath, [cli, ...args], {
    cwd,
    encoding: "utf8",
    env: { ...process.env, SHRIKE_ENGINE: engine, ...env },
  });
}

function scratch(name) {
  return mkdtempSync(join(tmpdir(), `shrike-contract-${name}-`));
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
  git(directory, ["config", "user.email", "contract@shrike.local"]);
  git(directory, ["config", "user.name", "Shrike Contract"]);
  return directory;
}

test("config evaluation timeout fails closed with exit 2", () => {
  const directory = scratch("config-timeout");
  try {
    write(
      directory,
      "shrike.config.mjs",
      "const until = Date.now() + 60_000;\nwhile (Date.now() < until) {}\nexport default {};\n",
    );
    const started = Date.now();
    const result = run(directory, ["scan", "--json"], { SHRIKE_CONFIG_TIMEOUT_MS: "250" });
    assert.equal(result.status, 2, result.stderr);
    assert.match(`${result.stderr}${result.stdout}`, /timed out after 250ms|terminated/);
    assert.ok(Date.now() - started < 5_000, "config timeout was not enforced");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("config that does not export an object fails closed with exit 2", () => {
  const directory = scratch("config-shape");
  try {
    write(directory, "shrike.config.mjs", "export default 42;\n");
    const result = run(directory, ["scan", "--json"]);
    assert.equal(result.status, 2, result.stderr);
    assert.match(`${result.stderr}${result.stdout}`, /must export an object/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("--no-checks is refused before any analysis", () => {
  const directory = scratch("no-checks-flag");
  try {
    const result = run(directory, ["prune", "--no-checks", "--json"]);
    assert.equal(result.status, 2, result.stderr);
    assert.match(`${result.stderr}${result.stdout}`, /requires at least one project check/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("prune refuses to run when no checks are discoverable", { skip: !hasEngine }, () => {
  const directory = makeRepo("missing-checks");
  try {
    write(directory, "app/page.tsx", "import { orphan } from '../lib/orphan';\nexport default function Page() { return orphan(); }\n");
    write(directory, "lib/orphan.ts", "export function orphan() { return 1; }\n");
    write(directory, "package.json", '{"name":"missing-checks","private":true,"type":"module"}\n');
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "base"]);
    git(directory, ["checkout", "-q", "-b", "feature"]);
    write(directory, "app/page.tsx", "export default function Page() { return null; }\n");
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "remove caller"]);

    const result = run(directory, ["prune", "--base", "main", "--json"]);
    assert.equal(result.status, 4, result.stderr);
    assert.match(`${result.stderr}${result.stdout}`, /no verification checks were found/);
    assert.equal(existsSync(join(directory, ".shrike", "receipts")), false, "a receipt was written without checks");
    const dirty = git(directory, ["status", "--porcelain"])
      .split("\n")
      .filter((line) => line.trim() && !line.slice(3).startsWith(".shrike/"));
    assert.deepEqual(dirty, [], "working tree was modified without checks");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("a missing engine fails closed with exit 4", () => {
  const directory = scratch("engine-missing");
  try {
    const emptyPath = join(directory, "empty-bin");
    mkdirSync(emptyPath, { recursive: true });
    const result = spawnSync(process.execPath, [cli, "scan", "--json"], {
      cwd: directory,
      encoding: "utf8",
      env: { ...process.env, PATH: emptyPath, SHRIKE_ENGINE: join(directory, "absent-engine") },
    });
    assert.equal(result.status, 4, result.stderr);
    assert.match(`${result.stderr}${result.stdout}`, /was not found/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("a malformed engine response fails closed with exit 4", () => {
  const directory = scratch("engine-malformed");
  try {
    const fake = writeFakeEngine(join(directory, "fake"), {
      responses: { scan: { raw: "this is not json\n" } },
    });
    const result = run(directory, ["scan", "--json"], { SHRIKE_ENGINE: fake.path });
    assert.equal(result.status, 4, result.stderr);
    assert.match(`${result.stderr}${result.stdout}`, /invalid JSON/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("an engine error envelope fails closed with exit 4", () => {
  const directory = scratch("engine-error");
  try {
    const fake = writeFakeEngine(join(directory, "fake"), {
      responses: { scan: { error: { code: "parse_error", message: "broken source" } } },
    });
    const result = run(directory, ["scan", "--json"], { SHRIKE_ENGINE: fake.path });
    assert.equal(result.status, 4, result.stderr);
    assert.match(`${result.stderr}${result.stdout}`, /parse_error: broken source/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("a crashed engine fails closed with exit 4", () => {
  const directory = scratch("engine-crash");
  try {
    const fake = writeFakeEngine(join(directory, "fake"), {
      responses: { scan: { raw: "" } },
      exitCode: 7,
    });
    const result = run(directory, ["scan", "--json"], { SHRIKE_ENGINE: fake.path });
    assert.equal(result.status, 4, result.stderr);
    assert.match(`${result.stderr}${result.stdout}`, /produced no response/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("an engine timeout fails closed with exit 4", () => {
  const directory = scratch("engine-timeout");
  try {
    const fake = writeFakeEngine(join(directory, "fake"), {
      responses: { scan: { delayMs: 5_000, result: { snapshot: { files: [] }, candidates: [] } } },
    });
    const started = Date.now();
    const result = run(directory, ["scan", "--json"], {
      SHRIKE_ENGINE: fake.path,
      SHRIKE_ENGINE_TIMEOUT_MS: "200",
    });
    assert.equal(result.status, 4, result.stderr);
    assert.match(`${result.stderr}${result.stdout}`, /timed out after 200ms/);
    assert.ok(Date.now() - started < 5_000, "engine timeout was not enforced");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("MCP advertises exactly four read-only tools and rejects unknown tools", () => {
  const directory = scratch("mcp");
  try {
    const requests = [
      { jsonrpc: "2.0", id: 1, method: "initialize", params: {} },
      { jsonrpc: "2.0", id: 2, method: "tools/list" },
      { jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "shrike_delete", arguments: {} } },
      { jsonrpc: "2.0", id: 4, method: "tools/call", params: { name: "shrike_delta", arguments: { base: "../main" } } },
    ].map((request) => JSON.stringify(request)).join("\n");
    const child = spawnSync(process.execPath, [cli, "mcp", "serve"], {
      cwd: directory,
      encoding: "utf8",
      input: `${requests}\n`,
      env: { ...process.env, SHRIKE_ENGINE: engine },
    });
    assert.equal(child.status, 0, child.stderr);
    const responses = child.stdout.trim().split("\n").map((line) => JSON.parse(line));
    assert.equal(responses.length, 4);

    const initialized = responses.find((response) => response.id === 1);
    assert.equal(initialized.result.serverInfo.name, "shrike");

    const tools = responses.find((response) => response.id === 2).result.tools;
    assert.deepEqual(
      tools.map((tool) => tool.name).sort(),
      ["shrike_delta", "shrike_plan", "shrike_proof", "shrike_receipt"],
    );

    const unknown = responses.find((response) => response.id === 3);
    assert.equal(unknown.error.code, -32601);

    const traversal = responses.find((response) => response.id === 4);
    assert.equal(traversal.error.code, -32602);
    assert.match(traversal.error.message, /invalid base ref/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("hook install is idempotent, preserves foreign hooks, and uninstalls only Shrike entries", () => {
  const directory = scratch("hooks");
  try {
    write(directory, ".claude/settings.json", JSON.stringify({
      keep: true,
      hooks: { Stop: [{ hooks: [{ type: "command", command: "echo foreign" }] }] },
    }));

    const first = run(directory, ["hook", "install", "--claude", "--json"]);
    assert.equal(first.status, 0, first.stderr);
    assert.equal(JSON.parse(first.stdout).changed, true);
    const installed = JSON.parse(readFileSync(join(directory, ".claude", "settings.json"), "utf8"));
    assert.equal(installed.keep, true);
    assert.equal(installed.hooks.Stop.length, 2);
    assert.ok(installed.hooks.Stop.some((entry) => entry.hooks?.[0]?.command === "echo foreign"));
    assert.ok(installed.hooks.Stop.some((entry) => entry.hooks?.[0]?.command?.includes("shrike hook run")));

    const second = run(directory, ["hook", "install", "--claude", "--json"]);
    assert.equal(second.status, 0, second.stderr);
    assert.equal(JSON.parse(second.stdout).changed, false);

    const removed = run(directory, ["hook", "uninstall", "--claude", "--json"]);
    assert.equal(removed.status, 0, removed.stderr);
    const uninstalled = JSON.parse(readFileSync(join(directory, ".claude", "settings.json"), "utf8"));
    assert.equal(uninstalled.keep, true);
    assert.equal(uninstalled.hooks.Stop.length, 1);
    assert.equal(uninstalled.hooks.Stop[0].hooks[0].command, "echo foreign");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("cursor and opencode hook installs are reversible", () => {
  const directory = scratch("hooks-other");
  try {
    const cursorInstall = run(directory, ["hook", "install", "--cursor", "--json"]);
    assert.equal(cursorInstall.status, 0, cursorInstall.stderr);
    const cursor = JSON.parse(readFileSync(join(directory, ".cursor", "hooks.json"), "utf8"));
    assert.equal(cursor.version, 1);
    assert.ok(cursor.hooks.afterFileEdit[0].command.includes("shrike hook run"));
    assert.ok(cursor.hooks.stop[0].command.includes("shrike hook run"));

    const cursorRemove = run(directory, ["hook", "uninstall", "--cursor", "--json"]);
    assert.equal(cursorRemove.status, 0, cursorRemove.stderr);
    const cursorAfter = JSON.parse(readFileSync(join(directory, ".cursor", "hooks.json"), "utf8"));
    assert.equal(cursorAfter.version, 1);
    assert.equal("hooks" in cursorAfter, false);

    const opencodeInstall = run(directory, ["hook", "install", "--opencode", "--json"]);
    assert.equal(opencodeInstall.status, 0, opencodeInstall.stderr);
    const pluginPath = join(directory, ".opencode", "plugins", "shrike-hook.js");
    assert.match(readFileSync(pluginPath, "utf8"), /shrike-hook v1/);
    const opencode = JSON.parse(readFileSync(join(directory, "opencode.json"), "utf8"));
    assert.deepEqual(opencode.plugin, ["./.opencode/plugins/shrike-hook.js"]);

    const opencodeRemove = run(directory, ["hook", "uninstall", "--opencode", "--json"]);
    assert.equal(opencodeRemove.status, 0, opencodeRemove.stderr);
    assert.equal(existsSync(pluginPath), false);
    const opencodeAfter = JSON.parse(readFileSync(join(directory, "opencode.json"), "utf8"));
    assert.equal("plugin" in opencodeAfter, false);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("hook run exits zero and writes a brief when a base is available", { skip: !hasEngine }, () => {
  const directory = makeRepo("hook-run");
  try {
    write(directory, "app/page.tsx", "import { orphan } from '../lib/orphan';\nexport default function Page() { return orphan(); }\n");
    write(directory, "lib/orphan.ts", "export function orphan() { return 1; }\n");
    write(directory, "package.json", '{"name":"hook-run","private":true,"type":"module"}\n');
    write(directory, "shrike.config.json", JSON.stringify({ checks: [{ name: "smoke", command: "node -e 0", required: true }] }));
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "base"]);
    git(directory, ["checkout", "-q", "-b", "feature"]);
    write(directory, "app/page.tsx", "export default function Page() { return null; }\n");
    git(directory, ["add", "-A"]);
    git(directory, ["commit", "-q", "-m", "remove caller"]);

    const result = run(directory, ["hook", "run", "--json"], { SHRIKE_HOOK_BASE: "main" });
    assert.equal(result.status, 0, result.stderr);
    const brief = JSON.parse(readFileSync(join(directory, ".shrike", "brief.json"), "utf8"));
    assert.equal(brief.base, "main");
    assert.ok(brief.newlyDead >= 1);
    assert.match(brief.nextCommand, /shrike/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("hook run is non-blocking outside a repository", () => {
  const directory = scratch("hook-nobase");
  try {
    const result = run(directory, ["hook", "run"]);
    assert.equal(result.status, 0, result.stderr);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
