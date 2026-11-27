#!/usr/bin/env node
/**
 * Report-build benchmark for P3.
 *
 * Builds a synthetic snapshot with 10,000 removable symbols, then measures the
 * pure `buildCurrentReport` projection. The engine is not involved; the number
 * is the report overhead after the snapshot is available.
 *
 * This is a manual benchmark, not a CI gate. The 100 ms budget is a target on
 * the benchmark machine, and a noisy machine can exceed it for one run.
 */
import { performance } from "node:perf_hooks";

import { buildCurrentReport } from "../packages/cli/src/report/build.ts";
import { summarizeDiagnostics } from "../packages/cli/src/diagnostics.ts";

const args = process.argv.slice(2);
function numberFlag(name, fallback) {
  const index = args.indexOf(`--${name}`);
  if (index < 0) return fallback;
  const value = Number(args[index + 1]);
  return Number.isFinite(value) && value > 0 ? Math.floor(value) : fallback;
}

const files = numberFlag("files", 1000);
const perFile = numberFlag("symbols-per-file", 10);
const runs = numberFlag("runs", 15);
const warmups = numberFlag("warmups", 10);

const fileRecords = [];
const symbolRecords = [];
const candidates = [];
for (let fileIndex = 0; fileIndex < files; fileIndex += 1) {
  const path = `src/group-${String(fileIndex).padStart(4, "0")}/file.ts`;
  fileRecords.push({
    path,
    contentHash: "blake3:0",
    moduleKind: "ts",
    parseStatus: "ok",
    generated: false,
    publicApi: false,
    package: "benchmark",
    lines: perFile * 2,
  });
  for (let symbolIndex = 0; symbolIndex < perFile; symbolIndex += 1) {
    const key = `${path}#symbol${symbolIndex}`;
    symbolRecords.push({
      key,
      name: `symbol${symbolIndex}`,
      kind: "function",
      file: path,
      span: { line: symbolIndex * 2 + 1, column: 1 },
      exported: true,
      entry: false,
      publicApi: false,
      generated: false,
      typeOnly: false,
      lines: 1,
    });
    candidates.push(key);
  }
}

const snapshot = {
  files: fileRecords,
  symbols: symbolRecords,
  candidates,
  unusedFiles: [],
  entries: [],
  proofs: [],
  unknowns: [],
  internalOnly: [],
  diagnostics: [],
  graphHash: "blake3:benchmark",
};
const dependencies = {
  packages: [],
  summary: {
    packages: 0,
    unused: 0,
    unlisted: 0,
    transitive: 0,
    unusedByKind: {
      dependencies: 0,
      devDependencies: 0,
      optionalDependencies: 0,
      peerDependencies: 0,
    },
  },
};
const input = {
  snapshot,
  headCommit: "a".repeat(40),
  dirty: false,
  dependencies,
  diagnostics: summarizeDiagnostics([]),
};

let report;
for (let index = 0; index < warmups; index += 1) report = buildCurrentReport(input);

const times = [];
for (let index = 0; index < runs; index += 1) {
  const start = performance.now();
  report = buildCurrentReport(input);
  times.push(performance.now() - start);
}

const sorted = [...times].sort((left, right) => left - right);
const median = sorted[Math.floor(sorted.length / 2)];
const summary = {
  files,
  symbols: report.summary.removableSymbols,
  groups: report.groups.length,
  runs,
  warmups,
  medianMs: Number(median.toFixed(2)),
  minMs: Number(sorted[0].toFixed(2)),
  maxMs: Number(sorted[sorted.length - 1].toFixed(2)),
  times: times.map((value) => Number(value.toFixed(2))),
};
console.log(JSON.stringify(summary, null, 2));
