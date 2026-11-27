import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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

test("deleting the only inline re-export specifier removes the statement", () => {
  const directory = scratch();
  try {
    const file = "src/barrel.ts";
    const content = 'export { dead } from "./impl";\nexport const live = 1;\n';
    writeFileSync(join(directory, file), content);
    const snapshot = reExportSnapshot(content, file, [{ name: "dead", text: "dead" }]);
    applySymbols(directory, snapshot, [`${file}#dead`]);
    assert.equal(readFileSync(join(directory, file), "utf8"), "export const live = 1;\n");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("deleting the only type re-export specifier removes the statement", () => {
  const directory = scratch();
  try {
    const file = "src/barrel.ts";
    const content = 'export type { Dead } from "./impl";\nexport const live = 1;\n';
    writeFileSync(join(directory, file), content);
    const snapshot = reExportSnapshot(content, file, [{ name: "Dead", text: "Dead" }]);
    applySymbols(directory, snapshot, [`${file}#Dead`]);
    assert.equal(readFileSync(join(directory, file), "utf8"), "export const live = 1;\n");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("deleting the only local export specifier removes the statement", () => {
  const directory = scratch();
  try {
    const file = "src/impl.ts";
    const content = "export const live = 1;\nconst dead = 2;\nexport { dead };\n";
    writeFileSync(join(directory, file), content);
    const declarationStart = content.indexOf("const dead");
    const specifierStart = content.lastIndexOf("dead");
    const snapshot = {
      symbols: [{
        key: `${file}#dead`,
        name: "dead",
        kind: "variable",
        file,
        span: {
          start: declarationStart,
          end: declarationStart + "const dead = 2;".length,
          line: 2,
          endLine: 2,
        },
      }],
      edges: [{
        source: `${file}#dead`,
        target: `${file}#dead`,
        kind: "re-export",
        certainty: "certain",
        file,
        span: { start: specifierStart, end: specifierStart + "dead".length, line: 3, endLine: 3 },
      }],
      live: [`${file}#__module__`],
    };
    applySymbols(directory, snapshot, [`${file}#dead`]);
    assert.equal(readFileSync(join(directory, file), "utf8"), "export const live = 1;\n");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("deleting every specifier of an inline re-export removes the statement", () => {
  const directory = scratch();
  try {
    const file = "src/barrel.ts";
    const content = 'export { deadOne, deadTwo } from "./impl";\nexport const live = 1;\n';
    writeFileSync(join(directory, file), content);
    const snapshot = reExportSnapshot(content, file, [
      { name: "deadOne", text: "deadOne" },
      { name: "deadTwo", text: "deadTwo" },
    ]);
    applySymbols(directory, snapshot, [`${file}#deadOne`, `${file}#deadTwo`]);
    assert.equal(readFileSync(join(directory, file), "utf8"), "export const live = 1;\n");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("deleting every specifier of a multi-line re-export removes the statement", () => {
  const directory = scratch();
  try {
    const file = "src/barrel.ts";
    const content = [
      "export {",
      "  deadOne,",
      "  deadTwo,",
      '} from "./impl";',
      "export const live = 1;",
      "",
    ].join("\n");
    writeFileSync(join(directory, file), content);
    const snapshot = reExportSnapshot(content, file, [
      { name: "deadOne", text: "deadOne" },
      { name: "deadTwo", text: "deadTwo" },
    ]);
    applySymbols(directory, snapshot, [`${file}#deadOne`, `${file}#deadTwo`]);
    assert.equal(readFileSync(join(directory, file), "utf8"), "export const live = 1;\n");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("deleting every specifier including a type removes the statement", () => {
  const directory = scratch();
  try {
    const file = "src/barrel.ts";
    const content = 'export { deadOne, type DeadTwo } from "./impl";\nexport const live = 1;\n';
    writeFileSync(join(directory, file), content);
    const snapshot = reExportSnapshot(content, file, [
      { name: "deadOne", text: "deadOne" },
      { name: "DeadTwo", text: "DeadTwo" },
    ]);
    applySymbols(directory, snapshot, [`${file}#deadOne`, `${file}#DeadTwo`]);
    assert.equal(readFileSync(join(directory, file), "utf8"), "export const live = 1;\n");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

function importSnapshot(content, file, specifiers) {
  return {
    symbols: [],
    edges: specifiers.map(({ target, text }) => {
      const start = content.indexOf(text);
      assert.notEqual(start, -1, `missing import text: ${text}`);
      const line = content.slice(0, start).split("\n").length;
      return {
        source: `${file}#__module__`,
        target,
        kind: "import",
        certainty: "certain",
        file,
        span: { start, end: start + text.length, line, endLine: line },
      };
    }),
    live: [`${file}#__module__`],
  };
}

test("deleting a named import specifier keeps its live sibling", () => {
  const directory = scratch();
  try {
    const file = "src/consumer.ts";
    const content = 'import { live, dead } from "./impl";\nexport const value = live;\n';
    writeFileSync(join(directory, file), content);
    const snapshot = importSnapshot(content, file, [
      { target: "src/impl.ts#dead", text: "dead" },
    ]);
    applySymbols(directory, snapshot, ["src/impl.ts#dead"]);
    assert.equal(
      readFileSync(join(directory, file), "utf8"),
      'import { live } from "./impl";\nexport const value = live;\n',
    );
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("deleting every named import removes the statement", () => {
  const directory = scratch();
  try {
    const file = "src/consumer.ts";
    const content = 'import { deadOne, deadTwo } from "./impl";\nexport const live = 1;\n';
    writeFileSync(join(directory, file), content);
    const snapshot = importSnapshot(content, file, [
      { target: "src/impl.ts#deadOne", text: "deadOne" },
      { target: "src/impl.ts#deadTwo", text: "deadTwo" },
    ]);
    applySymbols(directory, snapshot, ["src/impl.ts#deadOne", "src/impl.ts#deadTwo"]);
    assert.equal(readFileSync(join(directory, file), "utf8"), "export const live = 1;\n");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("deleting every named type import removes the statement", () => {
  const directory = scratch();
  try {
    const file = "src/consumer.ts";
    const content = 'import type { DeadOne, DeadTwo } from "./impl";\nexport const live = 1;\n';
    writeFileSync(join(directory, file), content);
    const snapshot = importSnapshot(content, file, [
      { target: "src/impl.ts#DeadOne", text: "DeadOne" },
      { target: "src/impl.ts#DeadTwo", text: "DeadTwo" },
    ]);
    applySymbols(directory, snapshot, ["src/impl.ts#DeadOne", "src/impl.ts#DeadTwo"]);
    assert.equal(readFileSync(join(directory, file), "utf8"), "export const live = 1;\n");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("deleting an aliased import removes the alias and its name", () => {
  const directory = scratch();
  try {
    const file = "src/consumer.ts";
    const content = 'import { live, dead as deadAlias } from "./impl";\nexport const value = live;\n';
    writeFileSync(join(directory, file), content);
    const snapshot = importSnapshot(content, file, [
      { target: "src/impl.ts#dead", text: "dead as deadAlias" },
    ]);
    applySymbols(directory, snapshot, ["src/impl.ts#dead"]);
    assert.equal(
      readFileSync(join(directory, file), "utf8"),
      'import { live } from "./impl";\nexport const value = live;\n',
    );
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("deleting every named import keeps a live default import", () => {
  const directory = scratch();
  try {
    const file = "src/consumer.ts";
    const content = 'import impl, { dead } from "./impl";\nexport const value = impl;\n';
    writeFileSync(join(directory, file), content);
    const snapshot = importSnapshot(content, file, [
      { target: "src/impl.ts#dead", text: "dead" },
    ]);
    applySymbols(directory, snapshot, ["src/impl.ts#dead"]);
    assert.equal(
      readFileSync(join(directory, file), "utf8"),
      'import impl from "./impl";\nexport const value = impl;\n',
    );
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

test("deleting a locally exported declaration removes its export specifier", () => {
  const directory = scratch();
  try {
    const file = "src/lazy.tsx";
    const content = [
      "// — loading helpers", // Multi-byte text proves engine byte spans are translated.
      "function CropperSkeleton() {",
      '  return "cropper";',
      "}",
      "",
      "function PdfExportSkeleton() {",
      '  return "pdf";',
      "}",
      "",
      "export { CropperSkeleton, PdfExportSkeleton };",
      "",
    ].join("\n");
    writeFileSync(join(directory, file), content);
    const byteOffset = (index) => Buffer.byteLength(content.slice(0, index), "utf8");
    const start = content.indexOf("function PdfExportSkeleton");
    const specifierStart = content.lastIndexOf("PdfExportSkeleton");
    const snapshot = {
      symbols: [{
        key: `${file}#PdfExportSkeleton`,
        name: "PdfExportSkeleton",
        kind: "function",
        file,
        // The exclusive byte end lands past the blank line, as the engine
        // reports for a declaration followed by a local export clause.
        span: {
          start: byteOffset(start),
          end: byteOffset(content.indexOf("export {")),
          line: 6,
          endLine: 8,
        },
      }],
      edges: [{
        source: `${file}#PdfExportSkeleton`,
        target: `${file}#PdfExportSkeleton`,
        kind: "re-export",
        certainty: "certain",
        file,
        span: {
          start: byteOffset(specifierStart),
          end: byteOffset(specifierStart) + Buffer.byteLength("PdfExportSkeleton"),
          line: 10,
          endLine: 10,
        },
      }],
      live: [`${file}#__module__`],
    };
    applySymbols(directory, snapshot, [`${file}#PdfExportSkeleton`]);
    assert.equal(
      readFileSync(join(directory, file), "utf8"),
      [
        "// — loading helpers",
        "function CropperSkeleton() {",
        '  return "cropper";',
        "}",
        "",
        "export { CropperSkeleton };",
        "",
      ].join("\n"),
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

test("a declaration span that ends past the trailing blank line keeps the next declaration", () => {
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
    const start = content.indexOf("export const dead");
    const snapshot = {
      symbols: [{
        key: `${file}#dead`,
        name: "dead",
        kind: "function",
        file,
        // The engine byte span is exclusive and can land on the next
        // declaration; the inclusive endLine is the authority.
        span: { start, end: content.indexOf("export const b"), line: 3, endLine: 5 },
      }],
      live: [`${file}#__module__`],
    };
    applySymbols(directory, snapshot, [`${file}#dead`]);
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

function wholeFileSnapshot(files, overrides = {}) {
  return {
    files: files.map((file) => ({
      path: file,
      contentHash: "blake3:0",
      moduleKind: "ts",
      parseStatus: "ok",
      generated: false,
      publicApi: false,
      package: "fixture",
      lines: 3,
      ...overrides,
    })),
    symbols: files.map((file) => ({
      key: `${file}#__module__`,
      name: "__module__",
      kind: "module",
      file,
      span: { line: 1, column: 1 },
      exported: false,
      entry: false,
      publicApi: false,
      generated: false,
      typeOnly: false,
      lines: 3,
    })),
    live: [],
  };
}

test("wholeFiles deletes an eligible file and counts its declared lines", () => {
  const directory = scratch();
  try {
    writeFileSync(join(directory, "src/old.ts"), "export const dead = 1;\nexport const more = 2;\n");
    const snapshot = wholeFileSnapshot(["src/old.ts"]);
    const removed = applySymbols(directory, snapshot, [], { wholeFiles: ["src/old.ts"] });
    assert.equal(removed, 3);
    assert.ok(!existsSync(join(directory, "src/old.ts")));
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("#__module__ deletes the whole file without counting it twice", () => {
  const directory = scratch();
  try {
    writeFileSync(join(directory, "src/old.ts"), "export const dead = 1;\n");
    const snapshot = {
      files: [{
        path: "src/old.ts", contentHash: "blake3:0", moduleKind: "ts", parseStatus: "ok",
        generated: false, publicApi: false, package: "fixture", lines: 2,
      }],
      symbols: [
        { key: "src/old.ts#__module__", name: "__module__", kind: "module", file: "src/old.ts", span: { line: 1, column: 1 }, lines: 1 },
        { key: "src/old.ts#dead", name: "dead", kind: "variable", file: "src/old.ts", span: { line: 1, column: 14 }, lines: 1 },
      ],
      live: [],
    };
    const removed = applySymbols(directory, snapshot, ["src/old.ts#__module__", "src/old.ts#dead"]);
    assert.equal(removed, 2);
    assert.ok(!existsSync(join(directory, "src/old.ts")));
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("whole-file deletion skips protected, generated, unparsed, missing, and live files", () => {
  const directory = scratch();
  try {
    for (const file of ["src/public.ts", "src/generated.ts", "src/broken.ts", "src/live.ts"]) {
      writeFileSync(join(directory, file), "export const dead = 1;\n");
    }
    const snapshot = wholeFileSnapshot([
      "src/public.ts", "src/generated.ts", "src/broken.ts", "src/live.ts", "src/missing.ts",
    ]);
    snapshot.files[0].publicApi = true;
    snapshot.files[1].generated = true;
    snapshot.files[2].parseStatus = "error";
    snapshot.live = ["src/live.ts#__module__"];
    const removed = applySymbols(directory, snapshot, [], {
      wholeFiles: ["src/public.ts", "src/generated.ts", "src/broken.ts", "src/live.ts", "src/missing.ts"],
    });
    assert.equal(removed, 0);
    for (const file of ["src/public.ts", "src/generated.ts", "src/broken.ts", "src/live.ts"]) {
      assert.ok(existsSync(join(directory, file)), `${file} should be protected`);
    }
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("whole-file deletion refuses a path outside the repository", () => {
  const directory = scratch();
  try {
    const snapshot = wholeFileSnapshot(["../outside.ts"]);
    assert.throws(
      () => applySymbols(directory, snapshot, [], { wholeFiles: ["../outside.ts"] }),
      /outside the repository/,
    );
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
