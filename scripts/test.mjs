#!/usr/bin/env node
/** Run Rust and Node test suites with clear exit propagation. */
import { spawnSync } from "node:child_process";
const root = new URL("..", import.meta.url).pathname;
const cargo = spawnSync("cargo", ["test", "--workspace"], { cwd: root, stdio: "inherit", env: { ...process.env, PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH}` } });
if (cargo.status !== 0) process.exit(cargo.status ?? 1);
const node = spawnSync(process.execPath, ["--test", "packages/**/*.test.mjs"], { cwd: root, stdio: "inherit" });
process.exit(node.status ?? 0);
