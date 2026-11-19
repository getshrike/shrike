import test from "node:test";
import assert from "node:assert/strict";
import { detectChecks } from "../src/checks.ts";
import { runCheck } from "../src/run.ts";

test("type-check is recognized as the typecheck script", () => {
  const checks = detectChecks(
    { "type-check": "tsc --noEmit", lint: "eslint ." },
    { manager: "npm", tier: "fast" },
  );
  assert.deepEqual(
    checks.map((check) => [check.name, check.command]),
    [
      ["typecheck", "npm run type-check"],
      ["lint", "npm run lint"],
    ],
  );
});

test("tier controls which discovered checks run", () => {
  const scripts = {
    typecheck: "tsc --noEmit",
    lint: "eslint .",
    test: "vitest run",
    build: "next build",
  };
  assert.deepEqual(
    detectChecks(scripts, { tier: "fast" }).map((check) => check.name),
    ["typecheck", "lint"],
  );
  assert.deepEqual(
    detectChecks(scripts, { tier: "standard" }).map((check) => check.name),
    ["typecheck", "lint", "test"],
  );
  assert.deepEqual(
    detectChecks(scripts, { tier: "full" }).map((check) => check.name),
    ["typecheck", "lint", "test", "build"],
  );
});

test("watch and dev scripts are never used as checks", () => {
  const checks = detectChecks(
    {
      typecheck: "tsc --noEmit --watch",
      lint: "eslint . --watch",
      test: "vitest --watch",
      build: "next dev",
    },
    { tier: "full" },
  );
  assert.deepEqual(checks, []);
});

test("project checks inherit only the safe environment unless a secret is declared", async () => {
  const key = "SHRIKE_SECRET_LEAK_TEST";
  const previous = process.env[key];
  process.env[key] = "top-secret";
  try {
    const command = `"${process.execPath}" -p "process.env.${key} ?? 'missing'"`;
    const inherited = await runCheck({ name: "env", command });
    assert.equal(inherited.output.trim(), "missing");

    const declared = await runCheck({
      name: "env",
      command,
      env: { [key]: "declared" },
    });
    assert.equal(declared.output.trim(), "declared");
  } finally {
    if (previous === undefined) delete process.env[key];
    else process.env[key] = previous;
  }
});
