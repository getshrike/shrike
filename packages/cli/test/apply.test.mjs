import test from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { applySymbols } from "../src/apply.ts";

function scratch() {
  const directory = mkdtempSync(join(tmpdir(), "shrike-apply-"));
  mkdirSync(join(directory, "src"), { recursive: true });
  return directory;
}

function snapshot(content, file, name) {
  const start = content.indexOf(`export const ${name}`);
  const end = content.indexOf("};", start) + 2;
  return {
    symbols: [{ key: `${file}#${name}`, file, span: { line: content.slice(0, start).split("\n").length, end } }],
    live: [`${file}#__module__`],
  };
}

function reExportSnapshot(content, file, specifiers) {
  return {
    symbols: specifiers.map(({ name, text }) => {
      const start = content.indexOf(text);
      assert.notEqual(start, -1, `missing specifier text: ${text}`);
      return {
        key: `${file}#${name}`,
        file,
        kind: "re-export",
        span: {
          start,
          end: start + text.length,
          line: content.slice(0, start).split("\n").length,
          endLine: content.slice(0, start + text.length).split("\n").length,
        },
      };
    }),
    live: [`${file}#__module__`],
  };
}

test("deleting one inline re-export specifier keeps its live sibling", () => {
  const directory = scratch();
  try {
    const file = "src/barrel.ts";
    const content = 'export { live, dead } from "./impl";\n';
    writeFileSync(join(directory, file), content);
    const snapshot = reExportSnapshot(content, file, [{ name: "dead", text: "dead" }]);
    applySymbols(directory, snapshot, [`${file}#dead`]);
    assert.equal(readFileSync(join(directory, file), "utf8"), 'export { live } from "./impl";\n');
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("deleting the first inline re-export specifier keeps the trailing sibling", () => {
  const directory = scratch();
  try {
    const file = "src/barrel.ts";
    const content = 'export { dead, live } from "./impl";\n';
    writeFileSync(join(directory, file), content);
    const snapshot = reExportSnapshot(content, file, [{ name: "dead", text: "dead" }]);
    applySymbols(directory, snapshot, [`${file}#dead`]);
    assert.equal(readFileSync(join(directory, file), "utf8"), 'export { live } from "./impl";\n');
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("deleting an aliased specifier removes the whole specifier, not its target name", () => {
  const directory = scratch();
  try {
    const file = "src/barrel.ts";
    const content = 'export { live, dead as deadAlias } from "./impl";\n';
    writeFileSync(join(directory, file), content);
    const snapshot = reExportSnapshot(content, file, [
      { name: "deadAlias", text: "dead as deadAlias" },
    ]);
    applySymbols(directory, snapshot, [`${file}#deadAlias`]);
    assert.equal(readFileSync(join(directory, file), "utf8"), 'export { live } from "./impl";\n');
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("deleting a multi-line re-export specifier removes only its line", () => {
  const directory = scratch();
  try {
    const file = "src/barrel.ts";
    const content = [
      'export {',
      '  live as liveAlias,',
      '  dead as deadAlias,',
      '} from "./impl";',
      "",
    ].join("\n");
    writeFileSync(join(directory, file), content);
    const snapshot = reExportSnapshot(content, file, [
      { name: "deadAlias", text: "dead as deadAlias" },
    ]);
    applySymbols(directory, snapshot, [`${file}#deadAlias`]);
    assert.equal(
      readFileSync(join(directory, file), "utf8"),
      ['export {', '  live as liveAlias,', '} from "./impl";', ""].join("\n"),
    );
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("deleting a declaration collapses the blank separator", () => {
  const directory = scratch();
  try {
    const file = "src/icons.ts";
    const content = [
      "export const a = 1;",
      "",
      "export const dead = () => {",
      "  return 2;",
      "};",
      "",
      "export const b = 3;",
      "",
    ].join("\n");
    writeFileSync(join(directory, file), content);
    const removed = applySymbols(directory, snapshot(content, file, "dead"), [`${file}#dead`]);
    // Three declaration lines plus the collapsed blank separator.
    assert.equal(removed, 4);
    assert.equal(
      readFileSync(join(directory, file), "utf8"),
      "export const a = 1;\n\nexport const b = 3;\n",
    );
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("deleting the last declaration leaves one trailing newline", () => {
  const directory = scratch();
  try {
    const file = "src/icons.ts";
    const content = [
      "export const a = 1;",
      "",
      "export const dead = () => {",
      "  return 2;",
      "};",
      "",
    ].join("\n");
    writeFileSync(join(directory, file), content);
    applySymbols(directory, snapshot(content, file, "dead"), [`${file}#dead`]);
    assert.equal(readFileSync(join(directory, file), "utf8"), "export const a = 1;\n");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
