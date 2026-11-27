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
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { parseArgs } from "../src/args.ts";
import { installCommand } from "../src/commands/install.ts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const cli = join(root, "packages", "cli", "src", "main.ts");

const SKILL_FILES = [
  ".agents/skills/shrike/SKILL.md",
  ".agents/skills/shrike/references/commands.md",
  ".agents/skills/shrike/references/safety.md",
  ".agents/skills/shrike/references/setup.md",
  ".agents/skills/shrike/references/workflows.md",
];

function scratch() {
  return mkdtempSync(join(tmpdir(), "shrike-install-"));
}

function write(directory, path, content) {
  const full = join(directory, path);
  mkdirSync(dirname(full), { recursive: true });
  writeFileSync(full, content, "utf8");
}

function read(directory, path) {
  return readFileSync(join(directory, path), "utf8");
}

function run(directory, args, env = {}) {
  return spawnSync(process.execPath, [cli, ...args], {
    cwd: directory,
    encoding: "utf8",
    env: { ...process.env, NO_COLOR: "1", ...env },
  });
}

function json(result) {
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return JSON.parse(result.stdout);
}

function cleanup(directory) {
  rmSync(directory, { recursive: true, force: true });
}

function skillActions(parsed) {
  return parsed.actions.filter((entry) => entry.path.startsWith(".agents/skills/shrike/"));
}

function assertInstalledSkill(directory) {
  const skill = read(directory, SKILL_FILES[0]);
  assert.match(skill, /^---\nname: shrike\n/);
  assert.match(skill, /^license: MIT$/m);
  for (const path of SKILL_FILES.slice(1)) {
    const relative = path.replace(".agents/skills/shrike/", "");
    assert.ok(skill.includes(`(${relative})`), `SKILL.md links ${relative}`);
    assert.ok(read(directory, path).endsWith("\n"), `${path} ends with a newline`);
  }
  for (const path of SKILL_FILES) {
    assert.doesNotMatch(read(directory, path), /npx shrike/, `${path} has no forbidden command form`);
  }
}

test("a fresh repository gets the whole universal skill directory", () => {
  const directory = scratch();
  try {
    const parsed = json(run(directory, ["install", "--json"]));
    assert.deepEqual(parsed.targets, ["agents"]);
    assert.deepEqual(parsed.actions, SKILL_FILES.map((path) => ({ path, action: "created" })));
    assert.equal(parsed.hook, null);
    assert.deepEqual(parsed.hooks, []);
    assert.deepEqual(parsed.scripts, []);
    assert.equal(parsed.next, "npx getshrike report");
    assertInstalledSkill(directory);
    assert.ok(!existsSync(join(directory, ".claude")));
    assert.ok(!existsSync(join(directory, ".shrike")));
    assert.ok(!existsSync(join(directory, "package.json")));
  } finally {
    cleanup(directory);
  }
});

test("reinstalling an identical skill is unchanged", () => {
  const directory = scratch();
  try {
    json(run(directory, ["install", "--json"]));
    const before = SKILL_FILES.map((path) => read(directory, path));
    const second = json(run(directory, ["install", "--json"]));
    assert.deepEqual(second.actions, SKILL_FILES.map((path) => ({ path, action: "unchanged" })));
    assert.deepEqual(SKILL_FILES.map((path) => read(directory, path)), before);
  } finally {
    cleanup(directory);
  }
});

test("a local edit to SKILL.md blocks every skill file until --force", () => {
  const directory = scratch();
  try {
    write(directory, SKILL_FILES[0], "local edits\n");
    const skipped = json(run(directory, ["install", "--json"]));
    const actions = skillActions(skipped);
    assert.equal(actions.length, SKILL_FILES.length);
    assert.equal(actions[0].action, "skipped");
    assert.match(actions[0].detail, /--force/);
    for (const action of actions.slice(1)) {
      assert.equal(action.action, "skipped");
      assert.match(action.detail, /mixed version|--force/);
    }
    assert.equal(read(directory, SKILL_FILES[0]), "local edits\n");
    for (const path of SKILL_FILES.slice(1)) {
      assert.ok(!existsSync(join(directory, path)), `${path} was not written`);
    }

    const forced = json(run(directory, ["install", "--json", "--force"]));
    assert.deepEqual(skillActions(forced), [
      { path: SKILL_FILES[0], action: "updated" },
      ...SKILL_FILES.slice(1).map((path) => ({ path, action: "created" })),
    ]);
    assertInstalledSkill(directory);
  } finally {
    cleanup(directory);
  }
});

test("one reference conflict blocks a missing reference and --force applies both", () => {
  const directory = scratch();
  const edited = ".agents/skills/shrike/references/commands.md";
  const missing = ".agents/skills/shrike/references/workflows.md";
  try {
    json(run(directory, ["install", "--json"]));
    const originalSkill = read(directory, SKILL_FILES[0]);
    write(directory, edited, "local reference edits\n");
    rmSync(join(directory, missing));
    const skipped = json(run(directory, ["install", "--json"]));
    const actions = skillActions(skipped);
    assert.equal(actions.find((entry) => entry.path === edited).action, "skipped");
    assert.match(actions.find((entry) => entry.path === edited).detail, /--force/);
    assert.equal(actions.find((entry) => entry.path === missing).action, "skipped");
    assert.match(actions.find((entry) => entry.path === missing).detail, /mixed version|--force/);
    for (const path of SKILL_FILES) {
      if (path === edited || path === missing) continue;
      assert.equal(actions.find((entry) => entry.path === path).action, "unchanged", path);
    }
    assert.equal(read(directory, edited), "local reference edits\n");
    assert.ok(!existsSync(join(directory, missing)), "the missing reference was not created");
    assert.equal(read(directory, SKILL_FILES[0]), originalSkill);

    const forced = json(run(directory, ["install", "--json", "--force"]));
    assert.equal(skillActions(forced).find((entry) => entry.path === edited).action, "updated");
    assert.equal(skillActions(forced).find((entry) => entry.path === missing).action, "created");
    assertInstalledSkill(directory);
  } finally {
    cleanup(directory);
  }
});

test("--force replaces local edits in every skill file", () => {
  const directory = scratch();
  try {
    json(run(directory, ["install", "--json"]));
    for (const path of SKILL_FILES) write(directory, path, `local edits: ${path}\n`);
    const forced = json(run(directory, ["install", "--json", "--force"]));
    assert.deepEqual(skillActions(forced), SKILL_FILES.map((path) => ({ path, action: "updated" })));
    assertInstalledSkill(directory);
  } finally {
    cleanup(directory);
  }
});

test("--dry-run writes nothing and lists every skill file", () => {
  const directory = scratch();
  try {
    const result = run(directory, ["install", "--dry-run"]);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /dry run/);
    for (const path of SKILL_FILES) {
      assert.match(result.stdout, new RegExp(`created {3}${path.replaceAll(".", "\\.")}`));
    }
    assert.ok(!existsSync(join(directory, ".agents")));
    assert.deepEqual(readdirSync(directory), []);
  } finally {
    cleanup(directory);
  }
});

test("--dry-run --json reports sorted actions for the whole directory", () => {
  const directory = scratch();
  try {
    const parsed = json(run(directory, ["install", "--dry-run", "--json"]));
    assert.equal(parsed.dryRun, true);
    assert.deepEqual(parsed.actions, SKILL_FILES.map((path) => ({ path, action: "created" })));
    const codepointOrder = [...parsed.actions].sort((left, right) =>
      left.path < right.path ? -1 : left.path > right.path ? 1 : 0
    );
    assert.deepEqual(codepointOrder, parsed.actions);
    assert.ok(!existsSync(join(directory, ".agents")));
  } finally {
    cleanup(directory);
  }
});

test("AGENTS.md markers are updated in place and surrounding text is preserved", () => {
  const directory = scratch();
  try {
    write(directory, "AGENTS.md", "before\n<!-- shrike:begin -->\nold\n<!-- shrike:end -->\nafter\n");
    const parsed = json(run(directory, ["install", "--target", "codex", "--json", "--no-hook"]));
    const action = parsed.actions.find((entry) => entry.path === "AGENTS.md");
    assert.equal(action.action, "updated");
    const text = read(directory, "AGENTS.md");
    assert.match(text, /^before\n/);
    assert.match(text, /after\n$/);
    assert.match(text, /This repository uses Shrike/);
    assert.match(text, /SKILL\.md/);
    assert.doesNotMatch(text, /\nold\n/);
  } finally {
    cleanup(directory);
  }
});

test("AGENTS.md without markers gains an appended block", () => {
  const directory = scratch();
  try {
    write(directory, "AGENTS.md", "project notes\n");
    const parsed = json(run(directory, ["install", "--target", "codex", "--json", "--no-hook"]));
    assert.equal(parsed.actions.find((entry) => entry.path === "AGENTS.md").action, "updated");
    const text = read(directory, "AGENTS.md");
    assert.match(text, /^project notes\n/);
    assert.match(text, /<!-- shrike:begin -->/);
    assert.match(text, /<!-- shrike:end -->/);
  } finally {
    cleanup(directory);
  }
});

test("a plain opencode.json is skipped instead of corrupted", () => {
  const directory = scratch();
  try {
    write(directory, "opencode.json", '{"plugin":[]}\n');
    const parsed = json(run(directory, ["install", "--target", "opencode", "--json", "--no-hook"]));
    assert.equal(parsed.actions.find((entry) => entry.path === "opencode.jsonc").action, "skipped");
    assert.equal(read(directory, "opencode.json"), '{"plugin":[]}\n');
    assert.ok(!existsSync(join(directory, "opencode.jsonc")));
  } finally {
    cleanup(directory);
  }
});

test("comment-capable opencode.jsonc receives a managed block", () => {
  const directory = scratch();
  try {
    write(directory, "opencode.jsonc", '{\n  // keep me\n  "theme": "dark"\n}\n');
    const parsed = json(run(directory, ["install", "--target", "opencode", "--json", "--no-hook"]));
    assert.equal(parsed.actions.find((entry) => entry.path === "opencode.jsonc").action, "updated");
    const text = read(directory, "opencode.jsonc");
    assert.match(text, /keep me/);
    assert.match(text, /\/\/ shrike:begin/);
    assert.match(text, /\/\/ shrike:end/);
  } finally {
    cleanup(directory);
  }
});

test("--scripts adds only missing names and preserves existing ones", () => {
  const directory = scratch();
  try {
    write(directory, "package.json", JSON.stringify({
      name: "fixture",
      scripts: { "shrike:clean": "custom clean", test: "node --test" },
    }, null, 2) + "\n");
    const parsed = json(run(directory, ["install", "--scripts", "--json"]));
    assert.deepEqual(parsed.scripts, [
      { name: "shrike:ci", action: "created" },
      { name: "shrike:clean", action: "skipped" },
      { name: "shrike:report", action: "created" },
    ]);
    const manifest = JSON.parse(read(directory, "package.json"));
    assert.equal(manifest.scripts["shrike:clean"], "custom clean");
    assert.equal(manifest.scripts["shrike:report"], "shrike report");
    assert.equal(manifest.scripts["shrike:ci"], "shrike ci install");
    assert.equal(manifest.scripts.test, "node --test");
    assert.ok(read(directory, "package.json").endsWith("\n"));
  } finally {
    cleanup(directory);
  }
});

test("a marker detected target installs its pointer skill and hook", () => {
  const directory = scratch();
  try {
    mkdirSync(join(directory, ".claude"), { recursive: true });
    const parsed = json(run(directory, ["install", "--json"]));
    assert.deepEqual(parsed.targets, ["claude"]);
    assert.equal(parsed.hooks[0].target, "claude");
    assert.equal(parsed.hooks[0].action, "installed");
    const pointer = read(directory, ".claude/skills/shrike/SKILL.md");
    assert.match(pointer, /portable skill and its references/);
    assert.match(pointer, /\.agents\/skills\/shrike\/SKILL\.md/);
    assert.doesNotMatch(pointer, /npx /);
    const settings = JSON.parse(read(directory, ".claude/settings.json"));
    assert.match(JSON.stringify(settings), /npx getshrike hook run --quiet/);
    assert.doesNotMatch(JSON.stringify(settings), /npx shrike/);
    assert.ok(!existsSync(join(directory, "CLAUDE.md")));
    assertInstalledSkill(directory);
  } finally {
    cleanup(directory);
  }
});

test("the Gemini pointer skill references the portable skill", () => {
  const directory = scratch();
  try {
    const parsed = json(run(directory, ["install", "--target", "gemini", "--json", "--no-hook"]));
    assert.equal(parsed.actions.find((entry) => entry.path === ".gemini/skills/shrike/SKILL.md").action, "created");
    const pointer = read(directory, ".gemini/skills/shrike/SKILL.md");
    assert.match(pointer, /portable skill and its references/);
    assert.match(pointer, /\.agents\/skills\/shrike\/SKILL\.md/);
    assert.doesNotMatch(pointer, /npx /);
    assertInstalledSkill(directory);
  } finally {
    cleanup(directory);
  }
});

test("--no-hook and --no-skill are honored", () => {
  const directory = scratch();
  try {
    mkdirSync(join(directory, ".claude"), { recursive: true });
    const parsed = json(run(directory, ["install", "--json", "--no-hook", "--no-skill"]));
    assert.deepEqual(parsed.actions, []);
    assert.deepEqual(parsed.hooks, []);
    assert.ok(!existsSync(join(directory, ".claude/skills")));
    assert.ok(!existsSync(join(directory, ".claude/settings.json")));
    assert.ok(!existsSync(join(directory, ".agents")));
  } finally {
    cleanup(directory);
  }
});

test("--no-skill still installs hooks for the detected target", () => {
  const directory = scratch();
  try {
    mkdirSync(join(directory, ".claude"), { recursive: true });
    const parsed = json(run(directory, ["install", "--json", "--no-skill"]));
    assert.deepEqual(parsed.targets, ["claude"]);
    assert.deepEqual(parsed.actions, []);
    assert.equal(parsed.hooks[0].target, "claude");
    assert.equal(parsed.hooks[0].action, "installed");
    assert.ok(!existsSync(join(directory, ".agents")));
    assert.ok(!existsSync(join(directory, ".claude/skills")));
    assert.ok(existsSync(join(directory, ".claude/settings.json")));
  } finally {
    cleanup(directory);
  }
});

test("a symlink that escapes the repository is refused", () => {
  const directory = scratch();
  const outside = mkdtempSync(join(tmpdir(), "shrike-outside-"));
  try {
    symlinkSync(outside, join(directory, ".claude"));
    const result = run(directory, ["install", "--target", "claude", "--json"]);
    assert.equal(result.status, 4, result.stderr);
    const parsed = JSON.parse(result.stdout);
    const action = parsed.actions.find((entry) => entry.path === ".claude/skills/shrike/SKILL.md");
    assert.equal(action.action, "failed");
    assert.doesNotMatch(result.stdout, new RegExp(directory.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
    assert.deepEqual(readdirSync(outside), []);
  } finally {
    cleanup(directory);
    cleanup(outside);
  }
});

test("a symlink escape through a skill reference is refused without partial writes", () => {
  const directory = scratch();
  const outside = mkdtempSync(join(tmpdir(), "shrike-outside-"));
  try {
    mkdirSync(join(directory, ".agents/skills/shrike"), { recursive: true });
    symlinkSync(outside, join(directory, ".agents/skills/shrike/references"));
    const result = run(directory, ["install", "--json"]);
    assert.equal(result.status, 4, result.stderr);
    const parsed = JSON.parse(result.stdout);
    const failed = parsed.actions.find((entry) => entry.action === "failed");
    assert.equal(failed.path, ".agents/skills/shrike");
    assert.match(failed.detail, /symlink outside the repository/);
    assert.doesNotMatch(result.stdout, new RegExp(directory.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
    assert.ok(!existsSync(join(directory, ".agents/skills/shrike/SKILL.md")));
    assert.deepEqual(readdirSync(outside), []);
  } finally {
    cleanup(directory);
    cleanup(outside);
  }
});

test("a canonical skill loader failure reports one failed action and writes nothing", async () => {
  const directory = scratch();
  const chunks = [];
  const originalWrite = process.stdout.write;
  process.stdout.write = (chunk) => {
    chunks.push(typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf8"));
    return true;
  };
  try {
    const code = await installCommand(
      directory,
      parseArgs(["--json", "--no-hook"]),
      {
        loadSkill: () => {
          throw new Error(`cannot read ${join(directory, "skills")}`);
        },
      },
    );
    assert.equal(code, 4);
    const output = chunks.join("");
    const parsed = JSON.parse(output);
    assert.deepEqual(parsed.actions, [{
      path: ".agents/skills/shrike",
      action: "failed",
      detail: "cannot read /skills",
    }]);
    assert.doesNotMatch(output, new RegExp(directory.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
    assert.doesNotMatch(output, /\n\s+at /);
    assert.ok(!existsSync(join(directory, ".agents")));
    assert.deepEqual(readdirSync(directory), []);
  } finally {
    process.stdout.write = originalWrite;
    cleanup(directory);
  }
});

test("output is deterministic and contains no absolute paths", () => {
  const directory = scratch();
  try {
    const first = run(directory, ["install", "--target", "claude,cursor", "--json"]);
    assert.equal(first.status, 0, first.stderr);
    const beforeSkill = SKILL_FILES.map((path) => read(directory, path));
    rmSync(join(directory, ".claude"), { recursive: true, force: true });
    rmSync(join(directory, ".cursor"), { recursive: true, force: true });
    rmSync(join(directory, ".agents"), { recursive: true, force: true });
    const second = run(directory, ["install", "--target", "claude,cursor", "--json"]);
    assert.equal(second.status, 0, second.stderr);
    assert.equal(first.stdout, second.stdout);
    assert.deepEqual(SKILL_FILES.map((path) => read(directory, path)), beforeSkill);
    assert.doesNotMatch(first.stdout, new RegExp(directory.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  } finally {
    cleanup(directory);
  }
});
