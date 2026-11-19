import { existsSync, writeFileSync } from "node:fs";
// Simulate a revert that cannot restore the tree: the host must exit 5 and
// preserve a recovery snapshot instead of claiming success.
writeFileSync(new URL("./recovery-needed.txt", import.meta.url), "restore from this snapshot\n");
if (existsSync(new URL("./revert-blocked.txt", import.meta.url))) process.exit(1);
process.exit(0);
