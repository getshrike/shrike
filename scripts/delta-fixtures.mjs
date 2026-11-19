#!/usr/bin/env node
import { spawnSync } from "node:child_process";
const result = spawnSync(process.execPath, ["scripts/run-fixtures.mjs", "--engine"], { stdio: "inherit" });
process.exit(result.status ?? 1);
