import test from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { analyzeDependencies } from "../src/dependencies.ts";

function makeRepo(t) {
  const root = mkdtempSync(join(tmpdir(), "shrike-dependencies-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  return root;
}

function write(root, path, content) {
  const full = join(root, path);
  mkdirSync(dirname(full), { recursive: true });
  writeFileSync(full, content);
}

function manifest(root, value, path = "package.json") {
  write(root, path, JSON.stringify(value, null, 2));
}

function snapshot(modules) {
  return {
    files: modules.map((module) => ({ path: module.path })),
    modules,
  };
}

function unusedNames(report) {
  return report.packages.flatMap((item) => item.unused.map((entry) => entry.name)).sort();
}

function unlistedNames(report) {
  return report.packages.flatMap((item) => item.unlisted.map((entry) => entry.name)).sort();
}

test("reports a dependency with no evidence and keeps an imported one", (t) => {
  const root = makeRepo(t);
  manifest(root, {
    name: "fixture",
    dependencies: { react: "^19.0.0", "left-pad": "^1.3.0" },
  });

  const report = analyzeDependencies(
    root,
    snapshot([{ path: "src/index.tsx", specifiers: ["react"], external: ["react"] }]),
  );

  assert.deepEqual(unusedNames(report), ["left-pad"]);
  assert.equal(report.packages[0].unused[0].kind, "dependencies");
  assert.equal(report.summary.unusedByKind.dependencies, 1);
});

test("package scripts and tool configs count as usage", (t) => {
  const root = makeRepo(t);
  manifest(root, {
    name: "fixture",
    scripts: { test: "vitest run" },
    devDependencies: { vitest: "^3.0.0", "typescript-eslint": "^8.0.0" },
  });
  write(root, "eslint.config.mjs", "import tseslint from 'typescript-eslint';\nexport default tseslint.config();\n");

  const report = analyzeDependencies(root, snapshot([]));

  assert.deepEqual(unusedNames(report), []);
});

test("file extensions and config filenames do not count as dependency usage", (t) => {
  const root = makeRepo(t);
  manifest(root, {
    name: "fixture",
    devDependencies: { tsx: "^4.0.0", json: "^11.0.0" },
  });
  write(
    root,
    "jest.config.js",
    "module.exports = { testMatch: ['**/*.test.tsx', '**/*.{js,ts,tsx}'] };\n",
  );
  write(root, "tsconfig.json", JSON.stringify({ compilerOptions: { jsx: "react-jsx" } }));

  const report = analyzeDependencies(root, snapshot([]));

  assert.deepEqual(unusedNames(report), ["json", "tsx"]);
});

test("a requirement of a used dependency is not reported unused", (t) => {
  const root = makeRepo(t);
  manifest(root, {
    name: "fixture",
    dependencies: { "@mui/material": "^5.0.0", "@emotion/react": "^11.0.0" },
  });

  const report = analyzeDependencies(
    root,
    snapshot([
      {
        path: "src/theme.ts",
        specifiers: ["@mui/material"],
        external: ["@mui/material"],
      },
    ]),
  );

  assert.deepEqual(unusedNames(report), []);
});

test("reports unlisted packages but ignores tool packages", (t) => {
  const root = makeRepo(t);
  manifest(root, { name: "fixture", dependencies: { react: "^19.0.0" } });

  const report = analyzeDependencies(
    root,
    snapshot([
      {
        path: "src/app.tsx",
        specifiers: ["react", "lodash", "@shrike/cli/config"],
        external: ["react", "lodash", "@shrike/cli/config"],
      },
    ]),
  );

  assert.deepEqual(unlistedNames(report), ["lodash"]);
  assert.deepEqual(report.packages[0].unlisted[0].importedBy, ["src/app.tsx"]);
});

test("standalone examples do not report unlisted root dependencies", (t) => {
  const root = makeRepo(t);
  manifest(root, { name: "fixture", dependencies: { react: "^19.0.0" } });

  const report = analyzeDependencies(
    root,
    snapshot([
      {
        path: "examples/demo/src/main.tsx",
        specifiers: ["react", "three"],
        external: ["react", "three"],
      },
    ]),
  );

  assert.deepEqual(unlistedNames(report), []);
});

test("peer, optional, type, workspace, and ignored dependencies are never unused", (t) => {
  const root = makeRepo(t);
  manifest(root, {
    name: "fixture",
    dependencies: { react: "^19.0.0", sharp: "^0.33.0" },
    devDependencies: { "@types/node": "^24.0.0", "internal-ui": "workspace:*" },
    peerDependencies: { "react-dom": "^19.0.0" },
    optionalDependencies: { fsevents: "^2.0.0" },
  });

  const report = analyzeDependencies(
    root,
    snapshot([{ path: "src/index.tsx", specifiers: ["react"], external: ["react"] }]),
    ["sharp"],
  );

  assert.deepEqual(unusedNames(report), []);
});

test("a nested package may import a dependency hoisted to its ancestor", (t) => {
  const root = makeRepo(t);
  manifest(root, { name: "fixture", dependencies: { react: "^19.0.0" } });
  manifest(root, { name: "web" }, "apps/web/package.json");
  write(root, "src/root.ts", "export const root = 1;\n");

  const report = analyzeDependencies(root, {
    files: [{ path: "src/root.ts" }, { path: "apps/web/src/page.tsx" }],
    modules: [
      { path: "src/root.ts", specifiers: ["react"], external: ["react"] },
      { path: "apps/web/src/page.tsx", specifiers: ["react"], external: ["react"] },
    ],
  });

  assert.deepEqual(unlistedNames(report), []);
  assert.deepEqual(unusedNames(report), []);
});

test("type-position imports are dependency evidence", (t) => {
  const root = makeRepo(t);
  manifest(root, {
    name: "fixture",
    devDependencies: { "postcss-load-config": "^6.0.0" },
  });

  const report = analyzeDependencies(
    root,
    snapshot([
      {
        path: "postcss.config.mjs",
        specifiers: [],
        external: [],
        typeRefs: ["postcss-load-config"],
      },
    ]),
  );

  assert.deepEqual(unusedNames(report), []);
});

test("type-position imports do not create unlisted findings", (t) => {
  const root = makeRepo(t);
  manifest(root, { name: "fixture" });

  const report = analyzeDependencies(
    root,
    snapshot([
      {
        path: "postcss.config.mjs",
        specifiers: [],
        external: [],
        typeRefs: ["postcss-load-config"],
      },
    ]),
  );

  assert.deepEqual(unlistedNames(report), []);
});

test("a type package follows its used runtime package", (t) => {
  const root = makeRepo(t);
  manifest(root, {
    name: "fixture",
    dependencies: { "@babel/core": "^7.0.0" },
    devDependencies: { "@types/babel__core": "^7.0.0" },
  });

  const report = analyzeDependencies(
    root,
    snapshot([
      { path: "src/transform.ts", specifiers: ["@babel/core"], external: ["@babel/core"] },
    ]),
  );

  assert.deepEqual(unusedNames(report), []);
});

test("a type package is stale when its runtime package is unused", (t) => {
  const root = makeRepo(t);
  manifest(root, {
    name: "fixture",
    dependencies: { "@babel/core": "^7.0.0" },
    devDependencies: { "@types/babel__core": "^7.0.0" },
  });

  const report = analyzeDependencies(root, snapshot([]));

  assert.deepEqual(unusedNames(report), ["@babel/core", "@types/babel__core"]);
});

test("ambient and tsconfig type packages are kept", (t) => {
  const root = makeRepo(t);
  manifest(root, {
    name: "fixture",
    devDependencies: { "@types/node": "^24.0.0", "@types/custom": "^1.0.0" },
  });
  write(root, "tsconfig.json", JSON.stringify({ compilerOptions: { types: ["custom"] } }));

  const report = analyzeDependencies(root, snapshot([]));

  assert.deepEqual(unusedNames(report), []);
});

test("a package provided by a used dependency is transitive, not unlisted", (t) => {
  const root = makeRepo(t);
  manifest(root, {
    name: "fixture",
    dependencies: { "@mui/material": "^5.0.0" },
  });

  const report = analyzeDependencies(
    root,
    snapshot([
      {
        path: "src/theme.ts",
        specifiers: ["@mui/material"],
        external: ["@mui/material"],
      },
      {
        path: "src/box.tsx",
        specifiers: ["@emotion/react"],
        external: ["@emotion/react"],
      },
    ]),
  );

  assert.deepEqual(unlistedNames(report), []);
  assert.deepEqual(
    report.packages[0].transitive.map((entry) => entry.name),
    ["@emotion/react"],
  );
  assert.equal(report.summary.transitive, 1);
});

test("framework entries keep the packages the framework runs", (t) => {
  const root = makeRepo(t);
  manifest(root, {
    name: "fixture",
    dependencies: { next: "^16.0.0", react: "^19.0.0", "react-dom": "^19.0.0" },
  });

  const report = analyzeDependencies(root, {
    files: [{ path: "app/page.tsx" }],
    modules: [{ path: "app/page.tsx", specifiers: [], external: [] }],
    entries: [{ symbol: "app/page.tsx#default", framework: "next-app" }],
  });

  assert.deepEqual(unusedNames(report), []);
});

test("a framework entry does not hide an unrelated unused package", (t) => {
  const root = makeRepo(t);
  manifest(root, {
    name: "fixture",
    dependencies: {
      next: "^16.0.0",
      react: "^19.0.0",
      "left-pad": "^1.3.0",
    },
  });

  const report = analyzeDependencies(root, {
    files: [{ path: "app/page.tsx" }],
    modules: [{ path: "app/page.tsx", specifiers: [], external: [] }],
    entries: [{ symbol: "app/page.tsx#default", framework: "next-app" }],
  });

  assert.deepEqual(unusedNames(report), ["left-pad"]);
});
