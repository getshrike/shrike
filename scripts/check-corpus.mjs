#!/usr/bin/env node
/**
 * Static validation for the must-not-report corpus.
 *
 * The corpus is a set of small packages whose listed symbols must never appear
 * in `candidates`. This script checks the manifest's shape and completeness;
 * `run-fixtures.mjs` runs the engine against it when a binary is available.
 */
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(new URL("..", import.meta.url).pathname);
const corpus = join(root, "fixtures", "must-not-report");
const manifestPath = join(corpus, "manifest.json");
const failures = [];

if (!existsSync(manifestPath)) {
  console.error("missing fixtures/must-not-report/manifest.json");
  process.exit(1);
}
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const entries = manifest.entries ?? [];
if (entries.length < 100) failures.push(`corpus has ${entries.length} entries; minimum is 100`);
if (new Set(entries.map((entry) => entry.id)).size !== entries.length) failures.push("duplicate corpus ids");
if (new Set(entries.map((entry) => entry.category)).size < 8) failures.push("corpus needs at least 8 categories");

for (const entry of entries) {
  const directory = join(corpus, "cases", entry.id);
  if (!existsSync(directory)) {
    failures.push(`missing case directory: ${entry.id}`);
    continue;
  }
  if (!Array.isArray(entry.protected) || entry.protected.length === 0) {
    failures.push(`case ${entry.id} has no protected symbols`);
  }
  for (const item of entry.protected ?? []) {
    if (!item.symbol || !item.reason) failures.push(`case ${entry.id} has an incomplete protected entry`);
    const [file] = String(item.symbol).split("#");
    if (file && !existsSync(join(directory, file))) failures.push(`case ${entry.id}: protected file missing: ${file}`);
  }
  const protectedSymbols = new Set((entry.protected ?? []).map((item) => item.symbol));
  for (const field of ["mustReport", "internalOnly"]) {
    if (entry[field] !== undefined && !Array.isArray(entry[field])) {
      failures.push(`case ${entry.id}: ${field} must be an array`);
      continue;
    }
    for (const symbol of entry[field] ?? []) {
      const [file] = String(symbol).split("#");
      if (!file || !String(symbol).includes("#")) {
        failures.push(`case ${entry.id}: ${field} symbol is not a symbol key: ${symbol}`);
      } else if (!existsSync(join(directory, file))) {
        failures.push(`case ${entry.id}: ${field} file missing: ${file}`);
      }
      if (protectedSymbols.has(symbol)) {
        failures.push(`case ${entry.id}: ${symbol} is both protected and ${field}`);
      }
    }
  }
  for (const symbol of entry.mustReport ?? []) {
    if ((entry.internalOnly ?? []).includes(symbol)) {
      failures.push(`case ${entry.id}: ${symbol} is both must-report and internal-only`);
    }
  }
  if (readdirSync(directory).length === 0) failures.push(`case ${entry.id} is empty`);
}

const mustReportCases = entries.filter((entry) => (entry.mustReport ?? []).length > 0).length;
const internalOnlyCases = entries.filter((entry) => (entry.internalOnly ?? []).length > 0).length;
if (mustReportCases < 8) failures.push(`corpus has ${mustReportCases} must-report cases; minimum is 8`);
if (internalOnlyCases < 1) failures.push("corpus needs at least one internal-only case");

if (failures.length > 0) {
  for (const failure of failures) console.error(`FAIL ${failure}`);
  process.exit(1);
}
const categories = [...new Set(entries.map((entry) => entry.category))].sort();
console.log(`OK: ${entries.length} must-not-report entries across ${categories.length} categories`);
console.log(`categories: ${categories.join(", ")}`);
