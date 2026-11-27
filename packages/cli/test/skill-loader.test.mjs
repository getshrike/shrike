import test from "node:test";
import assert from "node:assert/strict";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  loadShrikeSkill,
  loadShrikeSkillFrom,
  resolveShrikeSkillDirectory,
  skillDirectoryCandidates,
  SkillLoadError,
} from "../src/install/skill.ts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const canonical = join(root, "skills", "shrike");

function scratch() {
  return mkdtempSync(join(tmpdir(), "shrike-skill-"));
}

function copySkill(destination) {
  cpSync(canonical, destination, { recursive: true });
  return destination;
}

function cleanup(directory) {
  rmSync(directory, { recursive: true, force: true });
}

test("candidate order covers bundled, source, and fallback layouts", () => {
  const bundled = skillDirectoryCandidates(join(root, "packages", "cli", "dist"));
  assert.equal(bundled[0], join(root, "packages", "cli", "dist", "skills", "shrike"));
  assert.equal(bundled[2], join(root, "packages", "skills", "shrike"));

  const source = skillDirectoryCandidates(join(root, "packages", "cli", "src", "install"));
  assert.equal(source[1], join(root, "skills", "shrike"));
  assert.equal(source[2], join(root, "packages", "cli", "skills", "shrike"));
});

test("source resolution finds the canonical skills directory", () => {
  const candidates = skillDirectoryCandidates(join(root, "packages", "cli", "src", "install"));
  assert.equal(resolveShrikeSkillDirectory(candidates), canonical);
});

test("a bundled skills directory resolves first", () => {
  const directory = scratch();
  try {
    const dist = join(directory, "packages", "cli", "dist");
    copySkill(join(dist, "skills", "shrike"));
    const candidates = skillDirectoryCandidates(dist);
    assert.equal(resolveShrikeSkillDirectory(candidates), join(dist, "skills", "shrike"));
  } finally {
    cleanup(directory);
  }
});

test("a candidate without SKILL.md is skipped", () => {
  const directory = scratch();
  try {
    const empty = join(directory, "empty");
    mkdirSync(empty, { recursive: true });
    assert.equal(resolveShrikeSkillDirectory([empty, canonical]), canonical);
  } finally {
    cleanup(directory);
  }
});

test("an empty SKILL.md is not a valid candidate", () => {
  const directory = scratch();
  try {
    const empty = join(directory, "empty");
    mkdirSync(empty, { recursive: true });
    writeFileSync(join(empty, "SKILL.md"), "  \n", "utf8");
    assert.equal(resolveShrikeSkillDirectory([empty, canonical]), canonical);
    assert.equal(resolveShrikeSkillDirectory([empty]), null);
  } finally {
    cleanup(directory);
  }
});

test("no candidate produces a typed loader failure", () => {
  const directory = scratch();
  try {
    assert.throws(
      () => loadShrikeSkillFrom(directory),
      (error) => error instanceof SkillLoadError && /SKILL\.md was not found/.test(error.message),
    );
  } finally {
    cleanup(directory);
  }
});

test("the canonical skill loads SKILL.md first and references in sorted order", () => {
  const files = loadShrikeSkill();
  assert.deepEqual(files.map((file) => file.path), [
    "SKILL.md",
    "references/commands.md",
    "references/safety.md",
    "references/setup.md",
    "references/workflows.md",
  ]);
  for (const file of files) {
    assert.ok(file.content.endsWith("\n"), `${file.path} ends with a newline`);
    assert.doesNotMatch(file.content, /npx shrike/);
  }
});

test("a missing trailing newline is normalized", () => {
  const directory = scratch();
  try {
    const skill = copySkill(join(directory, "shrike"));
    const path = join(skill, "SKILL.md");
    writeFileSync(path, readFileSync(path, "utf8").trimEnd(), "utf8");
    const files = loadShrikeSkillFrom(skill);
    assert.ok(files[0].content.endsWith("\n"));
  } finally {
    cleanup(directory);
  }
});

test("non-Markdown shipped files are rejected", () => {
  const directory = scratch();
  try {
    const skill = copySkill(join(directory, "shrike"));
    writeFileSync(join(skill, "notes.txt"), "not shipped\n", "utf8");
    assert.throws(
      () => loadShrikeSkillFrom(skill),
      (error) => error instanceof SkillLoadError && /non-Markdown/.test(error.message),
    );
  } finally {
    cleanup(directory);
  }
});

test("nested reference directories are rejected", () => {
  const directory = scratch();
  try {
    const skill = copySkill(join(directory, "shrike"));
    mkdirSync(join(skill, "references", "nested"), { recursive: true });
    assert.throws(
      () => loadShrikeSkillFrom(skill),
      (error) => error instanceof SkillLoadError && /nested reference/.test(error.message),
    );
  } finally {
    cleanup(directory);
  }
});

test("a missing linked reference is rejected", () => {
  const directory = scratch();
  try {
    const skill = copySkill(join(directory, "shrike"));
    rmSync(join(skill, "references", "commands.md"));
    assert.throws(
      () => loadShrikeSkillFrom(skill),
      (error) => error instanceof SkillLoadError && /links to a missing file/.test(error.message),
    );
  } finally {
    cleanup(directory);
  }
});

test("a link that escapes the skill directory is rejected", () => {
  const directory = scratch();
  try {
    const skill = copySkill(join(directory, "shrike"));
    const path = join(skill, "SKILL.md");
    writeFileSync(path, `${readFileSync(path, "utf8")}\n[escape](../outside.md)\n`, "utf8");
    writeFileSync(join(directory, "outside.md"), "outside\n", "utf8");
    assert.throws(
      () => loadShrikeSkillFrom(skill),
      (error) => error instanceof SkillLoadError && /escapes the skill directory/.test(error.message),
    );
  } finally {
    cleanup(directory);
  }
});

test("CRLF line endings are rejected", () => {
  const directory = scratch();
  try {
    const skill = copySkill(join(directory, "shrike"));
    const path = join(skill, "SKILL.md");
    writeFileSync(path, readFileSync(path, "utf8").replaceAll("\n", "\r\n"), "utf8");
    assert.throws(
      () => loadShrikeSkillFrom(skill),
      (error) => error instanceof SkillLoadError && /LF line endings/.test(error.message),
    );
  } finally {
    cleanup(directory);
  }
});

test("the loader rejects an unsupported link scheme", () => {
  const directory = scratch();
  try {
    const skill = copySkill(join(directory, "shrike"));
    const path = join(skill, "SKILL.md");
    writeFileSync(path, `${readFileSync(path, "utf8")}\n[bad](http://example.test)\n`, "utf8");
    assert.throws(
      () => loadShrikeSkillFrom(skill),
      (error) => error instanceof SkillLoadError && /unsupported link/.test(error.message),
    );
  } finally {
    cleanup(directory);
  }
});

test("the real canonical directory exists and is loadable", () => {
  assert.ok(existsSync(join(canonical, "SKILL.md")));
  assert.ok(loadShrikeSkillFrom(canonical).length === 5);
});
