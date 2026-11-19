import { existsSync, readFileSync, writeFileSync } from "node:fs";
const f = new URL("./state.txt", import.meta.url);
const n = existsSync(f) ? Number(readFileSync(f, "utf8")) : 0;
writeFileSync(f, String(n + 1));
if (n === 0) { console.error("Error: flaky failure at src/flaky.ts:1"); process.exit(1); }
process.exit(0);
