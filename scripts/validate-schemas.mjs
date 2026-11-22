#!/usr/bin/env node
import { spawnSync } from "node:child_process";
const result = spawnSync(process.execPath, ["scripts/check-schemas.mjs"], { stdio: "inherit" });
process.exit(result.status ?? 1);
