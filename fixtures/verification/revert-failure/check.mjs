import { writeFileSync } from "node:fs";
writeFileSync(new URL("./revert-blocked.txt", import.meta.url), "block revert\n");
console.error("Error: deletion broke src/broken.ts:2");
process.exit(1);
