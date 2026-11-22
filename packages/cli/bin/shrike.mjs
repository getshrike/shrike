#!/usr/bin/env node
/**
 * npm launcher.
 *
 * The published package ships compiled JavaScript in `dist/`. This tiny
 * launcher keeps the command name stable and forwards stdio, signals, and the
 * exit code without re-implementing the CLI.
 */
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const entry = join(here, "..", "dist", "main.js");
const child = spawn(process.execPath, [entry, ...process.argv.slice(2)], {
  stdio: "inherit",
  env: process.env,
});

child.on("error", (error) => {
  process.stderr.write(`shrike: could not start the CLI: ${error.message}\n`);
  process.exitCode = 4;
});
child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exitCode = code ?? 4;
});
