import test from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import {
  contentHash,
  restoreRecoverySnapshot,
  safeRelativePath,
  writeRecoverySnapshot,
} from "../src/recovery.ts";

function scratch() {
  return mkdtempSync(join(tmpdir(), "shrike-recovery-"));
}

function write(cwd, path, content) {
  const full = join(cwd, path);
  mkdirSync(dirname(full), { recursive: true });
  writeFileSync(full, content);
}

test("a recovery snapshot restores exact bytes after edits and deletes", () => {
  const directory = scratch();
  try {
    write(directory, "src/a.ts", "export const a = 1;\n");
    write(directory, "src/b.ts", "export const b = 2;\n");
    const reference = writeRecoverySnapshot(directory, "abc123", ["src/a.ts", "src/b.ts"]);
    assert.match(reference.snapshotPath, /^\.shrike\/recovery\/rec_[0-9a-f]{16}\.json$/);
    assert.equal(reference.restoreCommand, `shrike recover ${reference.snapshotPath}`);

    writeFileSync(join(directory, "src/a.ts"), "broken");
    rmSync(join(directory, "src/b.ts"));

    const restored = restoreRecoverySnapshot(directory, reference.snapshotPath);
    assert.deepEqual(restored.restored.sort(), ["src/a.ts", "src/b.ts"]);
    assert.equal(readFileSync(join(directory, "src/a.ts"), "utf8"), "export const a = 1;\n");
    assert.equal(readFileSync(join(directory, "src/b.ts"), "utf8"), "export const b = 2;\n");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("snapshot ids are content addressed", () => {
  const directory = scratch();
  try {
    write(directory, "src/a.ts", "export const a = 1;\n");
    const first = writeRecoverySnapshot(directory, "abc123", ["src/a.ts"]);
    const second = writeRecoverySnapshot(directory, "abc123", ["src/a.ts"]);
    assert.equal(first.snapshotPath, second.snapshotPath);
    const other = writeRecoverySnapshot(directory, "def456", ["src/a.ts"]);
    assert.notEqual(first.snapshotPath, other.snapshotPath);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("a tampered snapshot is rejected before restoring", () => {
  const directory = scratch();
  try {
    write(directory, "src/a.ts", "export const a = 1;\n");
    const reference = writeRecoverySnapshot(directory, "abc123", ["src/a.ts"]);
    const path = join(directory, reference.snapshotPath);
    const snapshot = JSON.parse(readFileSync(path, "utf8"));
    snapshot.files[0].content = "tampered";
    writeFileSync(path, JSON.stringify(snapshot));

    assert.throws(
      () => restoreRecoverySnapshot(directory, reference.snapshotPath),
      /hash mismatch/,
    );
    assert.equal(readFileSync(join(directory, "src/a.ts"), "utf8"), "export const a = 1;\n");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("snapshots cannot write outside the repository", () => {
  const directory = scratch();
  try {
    assert.throws(() => writeRecoverySnapshot(directory, "abc123", ["../escape.ts"]), /path traversal/);
    assert.throws(() => safeRelativePath("/etc/passwd"), /absolute file path/);
    assert.throws(() => safeRelativePath("C:/Windows/system.ini"), /absolute file path/);
    assert.throws(() => safeRelativePath(""), /empty file path/);
    assert.throws(() => safeRelativePath("./"), /empty file path|invalid file path/);
    assert.throws(() => safeRelativePath("./src/../src/a.ts"), /path traversal/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("restoring a snapshot with an escaping path is refused", () => {
  const directory = scratch();
  try {
    const path = join(directory, ".shrike", "recovery", "rec_0000000000000000.json");
    mkdirSync(dirname(path), { recursive: true });
    const content = "escaped";
    writeFileSync(path, JSON.stringify({
      schemaVersion: "1.0",
      id: "rec_0000000000000000",
      createdAt: new Date().toISOString(),
      headCommit: "abc123",
      files: [{ path: "../outside.ts", content, contentHash: contentHash(content) }],
    }));
    assert.throws(
      () => restoreRecoverySnapshot(directory, ".shrike/recovery/rec_0000000000000000.json"),
      /path traversal/,
    );
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
