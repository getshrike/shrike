#!/usr/bin/env node
/** Validate the six schemas and every fixture manifest as JSON. */
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(new URL("..", import.meta.url).pathname);
const schemaDir = join(root, "schemas");
const expected = [
  "config.schema.json",
  "deletion-plan.schema.json",
  "delta.schema.json",
  "graph-snapshot.schema.json",
  "protocol.schema.json",
  "receipt.schema.json",
];
const failures = [];
const ids = new Set();

for (const name of expected) {
  const path = join(schemaDir, name);
  if (!existsSync(path)) {
    failures.push(`missing schema: ${name}`);
    continue;
  }
  try {
    const schema = JSON.parse(readFileSync(path, "utf8"));
    if (!schema.$schema?.includes("json-schema.org")) failures.push(`${name}: missing draft $schema`);
    if (schema.type !== "object") failures.push(`${name}: root type must be object`);
    if (schema.$id) {
      if (ids.has(schema.$id)) failures.push(`${name}: duplicate $id`);
      ids.add(schema.$id);
    }
  } catch (error) {
    failures.push(`${name}: invalid JSON (${error.message})`);
  }
}

function walk(directory) {
  if (!existsSync(directory)) return [];
  const output = [];
  for (const name of readdirSync(directory)) {
    const path = join(directory, name);
    if (statSync(path).isDirectory()) output.push(...walk(path));
    else if (name.endsWith(".json")) output.push(path);
  }
  return output;
}

let manifests = 0;
for (const path of walk(join(root, "fixtures"))) {
  try {
    JSON.parse(readFileSync(path, "utf8"));
    manifests += 1;
  } catch (error) {
    failures.push(`${path}: invalid JSON (${error.message})`);
  }
}

if (failures.length > 0) {
  for (const failure of failures) console.error(`FAIL ${failure}`);
  process.exit(1);
}
console.log(`OK: ${expected.length} schemas with ${ids.size} unique $id values; ${manifests} fixture JSON files parse`);
