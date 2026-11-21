import test from "node:test";
import assert from "node:assert/strict";
import { summarizeDiagnostics } from "../src/diagnostics.ts";

test("separates actionable failures from non-source and generated imports", () => {
  const summary = summarizeDiagnostics([
    { code: "parse_error", severity: "error", message: "cannot parse", file: "a.ts" },
    { code: "asset_import", severity: "info", message: "`./x.css` is not analyzed as source", file: "b.tsx" },
    { code: "unresolved_import", severity: "warning", message: "cannot resolve `./App.vue`", file: "c.ts" },
    { code: "unresolved_import", severity: "warning", message: "cannot resolve `./missing`", file: "d.ts" },
    {
      code: "unresolved_import",
      severity: "warning",
      message: "cannot resolve `./.next/dev/types/routes.d.ts`",
      file: "e.d.ts",
    },
    { code: "client-boundary-leak", severity: "warning", message: "server import in client", file: "f.tsx" },
  ]);

  assert.equal(summary.total, 6);
  assert.equal(summary.actionable, 3);
  assert.equal(summary.informational, 3);
  assert.equal(summary.bySeverity.error, 1);
  assert.equal(summary.bySeverity.warning, 4);
  assert.equal(summary.bySeverity.info, 1);
  assert.equal(summary.byCode.unresolved_import, 3);

  const actionable = summary.groups.filter((group) => group.actionable).map((group) => group.code).sort();
  assert.deepEqual(actionable, ["client-boundary-leak", "parse_error", "unresolved_import"]);

  const unresolvedGroups = summary.groups.filter((group) => group.code === "unresolved_import");
  assert.equal(unresolvedGroups.reduce((total, group) => total + group.count, 0), 3);
  assert.equal(unresolvedGroups.filter((group) => group.actionable).length, 1);
  assert.equal(unresolvedGroups.filter((group) => !group.actionable).length, 1);
  assert.equal(unresolvedGroups.find((group) => group.actionable).files[0], "d.ts");
  assert.equal(unresolvedGroups.find((group) => !group.actionable).count, 2);
});

test("orders actionable groups first, then by count, and caps sample files", () => {
  const items = [];
  for (let index = 0; index < 7; index += 1) {
    items.push({
      code: "parse_error",
      severity: "error",
      message: `cannot parse ${index}`,
      file: `file${index}.ts`,
    });
  }
  items.push({
    code: "asset_import",
    severity: "info",
    message: "`style.css` is not analyzed as source",
    file: "style.css",
  });

  const summary = summarizeDiagnostics(items);

  assert.equal(summary.groups[0].actionable, true);
  assert.equal(summary.groups[0].count, 7);
  assert.equal(summary.groups[0].files.length, 5);
  assert.deepEqual(summary.groups[0].files, [...summary.groups[0].files].sort());
  assert.equal(summary.groups[1].actionable, false);
  assert.equal(summary.actionable, 7);
  assert.equal(summary.informational, 1);
});

test("invalid client exports are actionable boundary diagnostics", () => {
  const summary = summarizeDiagnostics([
    {
      code: "invalid_client_export",
      severity: "warning",
      message: '`metadata` is a Next.js server-only export and cannot be exported from a "use client" module',
      file: "app/client.tsx",
    },
  ]);
  assert.equal(summary.actionable, 1);
  assert.equal(summary.groups[0].category, "boundary");
  assert.equal(summary.groups[0].actionable, true);
});

test("invalid server exports are actionable boundary diagnostics", () => {
  const summary = summarizeDiagnostics([
    {
      code: "invalid_server_export",
      severity: "warning",
      message: '`value` cannot be exported from a "use server" module; Next.js requires every action export to be an async function',
      file: "src/actions.ts",
    },
  ]);
  assert.equal(summary.actionable, 1);
  assert.equal(summary.groups[0].category, "boundary");
  assert.equal(summary.groups[0].actionable, true);
  assert.equal(summary.bySeverity.warning, 1);
});

test("misplaced directives are actionable boundary diagnostics", () => {
  const summary = summarizeDiagnostics([
    {
      code: "misplaced_directive",
      severity: "warning",
      message: '"use client" is ignored because it is not in the leading prologue',
      file: "app/page.tsx",
    },
  ]);
  assert.equal(summary.actionable, 1);
  assert.equal(summary.groups[0].category, "boundary");
  assert.equal(summary.groups[0].actionable, true);
  assert.equal(summary.bySeverity.warning, 1);
});

test("mixed client/server barrels are actionable boundary diagnostics", () => {
  const summary = summarizeDiagnostics([
    {
      code: "mixed_client_server_barrel",
      severity: "warning",
      message:
        'barrel re-exports both a "use client" module (`../components/client`) and a server-only module (`../lib/server`)',
      file: "src/barrel.ts",
    },
  ]);
  assert.equal(summary.actionable, 1);
  assert.equal(summary.groups[0].category, "boundary");
  assert.equal(summary.groups[0].actionable, true);
  assert.equal(summary.bySeverity.warning, 1);
});

test("client/server leaks are actionable boundary diagnostics", () => {
  const summary = summarizeDiagnostics([
    {
      code: "client_server_leak",
      severity: "warning",
      message:
        '"use client" module transitively imports server-only code (imports `node:fs`); import path: app/page.tsx -> lib/read.ts',
      file: "app/page.tsx",
    },
  ]);
  assert.equal(summary.actionable, 1);
  assert.equal(summary.groups[0].category, "boundary");
  assert.equal(summary.groups[0].actionable, true);
  assert.equal(summary.bySeverity.warning, 1);
});

test("Next.js route conflicts are actionable routing diagnostics", () => {
  const summary = summarizeDiagnostics([
    {
      code: "route_collision",
      severity: "error",
      message: "two or more Next.js routes resolve to `/about`",
      file: "app/(a)/about/page.tsx",
    },
    {
      code: "dynamic_segment_name_conflict",
      severity: "error",
      message: "sibling Next.js dynamic segments at `/shop` use different names",
      file: "app/shop/[slug]/page.tsx",
    },
  ]);
  assert.equal(summary.actionable, 2);
  assert.deepEqual(
    summary.groups.map((group) => group.category),
    ["routing", "routing"],
  );
  assert.equal(summary.bySeverity.error, 2);
});

test("unknown codes are actionable only when they are not informational", () => {
  const summary = summarizeDiagnostics([
    { code: "mystery", severity: "warning", message: "warning" },
    { code: "mystery-info", severity: "info", message: "info" },
  ]);

  assert.equal(summary.groups.find((group) => group.code === "mystery").actionable, true);
  assert.equal(summary.groups.find((group) => group.code === "mystery-info").actionable, false);
});

test("an empty diagnostic list produces zero totals", () => {
  assert.deepEqual(summarizeDiagnostics(), {
    total: 0,
    actionable: 0,
    informational: 0,
    bySeverity: { error: 0, warning: 0, info: 0 },
    byCode: {},
    groups: [],
  });
});
