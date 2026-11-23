#!/usr/bin/env node
/**
 * Generate additional must-not-report corpus cases. See docs/testing.md.
 *
 * The corpus is fixture data, not generated test output: this script writes the
 * case trees and appends the manifest entries once. It is idempotent, so the
 * cases can be reviewed and edited like any other fixture.
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const root = resolve(new URL("..", import.meta.url).pathname);
const corpus = join(root, "fixtures", "must-not-report");
const casesRoot = join(corpus, "cases");
const manifestPath = join(corpus, "manifest.json");
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));

// Generated fixtures use an `app/` page as their entry, so declare the
// framework that gives that directory meaning.
const packageJson = (name) => `${JSON.stringify({ name, private: true, type: "module", dependencies: { next: "^16.0.0" } }, null, 2)}\n`;
const tsconfig = `${JSON.stringify({ compilerOptions: { baseUrl: ".", paths: { "@/*": ["src/*"] } } })}\n`;

const cases = [];

function add(id, category, files, protectedSymbols, mustReport) {
  cases.push({ id, category, files, protected: protectedSymbols, ...(mustReport ? { mustReport } : {}) });
}

for (let index = 1; index <= 20; index += 1) {
  add(
    `parallel-routes-exp-${index}`,
    "parallel-routes",
    ["app", "package.json", "tsconfig.json"],
    [{
      symbol: `app/@modal/(.)photo/[id]/page.tsx#default`,
      reason: "parallel-route slot page is an entry",
    }],
  );
}

for (let index = 1; index <= 20; index += 1) {
  add(
    `intercepting-routes-exp-${index}`,
    "intercepting-routes",
    ["app", "package.json", "tsconfig.json"],
    [{
      symbol: `app/feed/(..)photo/[id]/page.tsx#default`,
      reason: "intercepting-route page is an entry",
    }],
  );
}

for (let index = 1; index <= 20; index += 1) {
  add(
    `route-groups-exp-${index}`,
    "route-groups",
    ["app", "package.json", "tsconfig.json"],
    [{
      symbol: `app/(marketing)/about/page.tsx#default`,
      reason: "route-group page is an entry",
    }],
  );
}

for (let index = 1; index <= 20; index += 1) {
  add(
    `use-cache-exp-${index}`,
    "use-cache",
    ["app", "src", "package.json", "tsconfig.json"],
    [{
      symbol: `src/data.ts#getData`,
      reason: "use-cache function is imported by a page entry",
    }],
  );
}

for (let index = 1; index <= 20; index += 1) {
  add(
    `monorepo-client-exp-${index}`,
    "monorepo-client-boundary",
    ["apps", "packages", "package.json", "tsconfig.json"],
    [{
      symbol: `packages/ui/src/Button.tsx#Button`,
      reason: "client-boundary component is imported across a workspace",
    }],
    ["apps/web/app/page.tsx#default"],
  );
}

for (let index = 1; index <= 10; index += 1) {
  add(
    `generated-client-exp-${index}`,
    "generated-clients",
    ["app", "src", "package.json", "tsconfig.json"],
    [{
      symbol: `src/generated/client.ts#generatedClient`,
      reason: "generated client is imported by an entry",
    }],
  );
}

for (let index = 1; index <= 5; index += 1) {
  add(
    `barrel-exp-${index}`,
    "barrel",
    ["app", "src", "package.json", "tsconfig.json"],
    [{ symbol: "src/impl.ts#thing", reason: "barrel export is followed to its implementation" }],
  );
}

for (let index = 1; index <= 5; index += 1) {
  add(
    `default-reexport-exp-${index}`,
    "default-reexport",
    ["app", "src", "package.json", "tsconfig.json"],
    [{ symbol: "src/impl.tsx#default", reason: "default re-export is followed" }],
  );
}

for (let index = 1; index <= 5; index += 1) {
  add(
    `dynamic-import-exp-${index}`,
    "dynamic-import",
    ["app", "src", "package.json", "tsconfig.json"],
    [{ symbol: "src/feature.ts#feature", reason: "literal dynamic import is a certain edge" }],
  );
}

for (let index = 1; index <= 5; index += 1) {
  add(
    `namespace-exp-${index}`,
    "namespace",
    ["app", "src", "package.json", "tsconfig.json"],
    [{ symbol: "src/lib.ts#value", reason: "namespace import is followed" }],
  );
}

for (const entry of cases) {
  const directory = join(casesRoot, entry.id);
  const files = {};

  if (entry.category === "parallel-routes") {
    files["app/@modal/(.)photo/[id]/page.tsx"] = `export default function ModalPage() { return null; }\n`;
    files["app/@modal/default.tsx"] = `export default function ModalDefault() { return null; }\n`;
    files["app/layout.tsx"] = `export default function Layout({ children }: { children: unknown }) { return children; }\n`;
  } else if (entry.category === "intercepting-routes") {
    files["app/photo/[id]/page.tsx"] = `export default function PhotoPage() { return null; }\n`;
    files["app/feed/(..)photo/[id]/page.tsx"] = `export default function InterceptedPhoto() { return null; }\n`;
  } else if (entry.category === "route-groups") {
    files["app/(marketing)/about/page.tsx"] = `export default function AboutPage() { return null; }\n`;
    files["app/(shop)/cart/page.tsx"] = `export default function CartPage() { return null; }\n`;
    files["app/(marketing)/layout.tsx"] = `export default function MarketingLayout({ children }: { children: unknown }) { return children; }\n`;
  } else if (entry.category === "use-cache") {
    files["app/page.tsx"] = `import { getData } from "../src/data";\nexport default function Page() { return getData(); }\n`;
    files["src/data.ts"] = `"use cache";\nexport async function getData() { return "data"; }\n`;
  } else if (entry.category === "monorepo-client-boundary") {
    files["apps/web/app/page.tsx"] = `import { Button } from "../../../packages/ui/src/Button";\nexport default function Page() { return <Button />; }\n`;
    files["packages/ui/src/Button.tsx"] = `"use client";\nexport function Button() { return null; }\n`;
    files["packages/ui/package.json"] = `${JSON.stringify({ name: "@acme/ui", private: true, type: "module", exports: { ".": "./src/Button.tsx" } }, null, 2)}\n`;
    files["package.json"] = `${JSON.stringify({ name: "corpus-monorepo", private: true, type: "module", workspaces: ["apps/*", "packages/*"] }, null, 2)}\n`;
  } else if (entry.category === "generated-clients") {
    files["app/page.tsx"] = `import { generatedClient } from "../src/generated/client";\nexport default function Page() { return generatedClient(); }\n`;
    files["src/generated/client.ts"] = `// @generated\n// This file is auto-generated. Do not edit.\nexport function generatedClient() { return 1; }\n`;
  } else if (entry.category === "barrel") {
    files["app/page.tsx"] = `import { thing } from "../src";\nexport default function Page() { return thing(); }\n`;
    files["src/index.ts"] = `export * from "./impl";\n`;
    files["src/impl.ts"] = `export function thing() { return 1; }\n`;
  } else if (entry.category === "default-reexport") {
    files["app/page.tsx"] = `import Widget from "../src";\nexport default function Page() { return <Widget />; }\n`;
    files["src/index.ts"] = `export { default } from "./impl";\n`;
    files["src/impl.tsx"] = `export default function Widget() { return null; }\n`;
  } else if (entry.category === "dynamic-import") {
    files["app/page.tsx"] = `export default async function Page() { const mod = await import("../src/feature"); return mod.feature(); }\n`;
    files["src/feature.ts"] = `export function feature() { return 1; }\n`;
  } else if (entry.category === "namespace") {
    files["app/page.tsx"] = `import * as ns from "../src/lib";\nexport default function Page() { return ns.value; }\n`;
    files["src/lib.ts"] = `export const value = 1;\n`;
  }

  files["package.json"] = entry.category === "monorepo-client-boundary"
    ? files["package.json"]
    : packageJson(`corpus-${entry.id}`);
  files["tsconfig.json"] = tsconfig;

  for (const [path, content] of Object.entries(files)) {
    const full = join(directory, path);
    mkdirSync(dirname(full), { recursive: true });
    writeFileSync(full, content, "utf8");
  }
}

const existing = new Set(manifest.entries.map((entry) => entry.id));
for (const entry of cases) {
  if (existing.has(entry.id)) continue;
  manifest.entries.push({ id: entry.id, category: entry.category, files: entry.files, protected: entry.protected });
  existing.add(entry.id);
}
manifest.count = manifest.entries.length;
writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");

const categories = [...new Set(manifest.entries.map((entry) => entry.category))].sort();
console.log(`OK: corpus now has ${manifest.entries.length} entries across ${categories.length} categories (${cases.length} generated cases)`);
