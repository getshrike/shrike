#!/usr/bin/env node
/** Deterministic synthetic repository generator: no dependencies. */
import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
const args = process.argv.slice(2);
function flag(name, fallback) { const i = args.indexOf(`--${name}`); return i >= 0 ? args[i + 1] : fallback; }
const target = resolve(flag("out", "benchmarks/.work/repo"));
const files = Number(flag("files", "10000"));
const seed = Number(flag("seed", "42"));
const deadRatio = Number(flag("dead-ratio", "0.2"));
let state = seed >>> 0;
function random() { state = (state * 1664525 + 1013904223) >>> 0; return state / 0x100000000; }
function write(path, text) { mkdirSync(join(target, path, ".."), { recursive: true }); writeFileSync(join(target, path), text); }
write("package.json", JSON.stringify({ name: "bench-repo", private: true, type: "module" }, null, 2));
write("tsconfig.json", JSON.stringify({ compilerOptions: { baseUrl: ".", paths: { "@/*": ["src/*"] } } }, null, 2));
write("app/page.tsx", 'import { feature0 } from "../src/feature0";\nexport default function Page() { return feature0(); }\n');
for (let i = 0; i < files; i += 1) {
  const dead = random() < deadRatio;
  const importLine = i === 0 ? "" : `import { feature${i - 1} } from "./feature${i - 1}";\n`;
  write(`src/feature${i}.ts`, `${importLine}export function feature${i}() { return ${dead ? i : `feature${i - 1}?.() ?? ${i}`}; }\n${dead ? `export function unused${i}() { return ${i}; }\n` : ""}`);
}
write("benchmarks.json", JSON.stringify({ files, seed, deadRatio, target }, null, 2));
console.log(`generated ${files} source files at ${target}`);
