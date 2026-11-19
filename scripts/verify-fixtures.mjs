#!/usr/bin/env node
/** Execute verification fixtures and check their observable status. */
import { spawn } from "node:child_process";
import { readFileSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
const root = resolve(new URL("..", import.meta.url).pathname);
const directory = join(root, "fixtures", "verification");
const manifest = JSON.parse(readFileSync(join(directory, "manifest.json"), "utf8"));
function run(script, timeoutMs) {
  return new Promise((resolveRun) => {
    const child = spawn(process.execPath, [script], { cwd: directory, stdio: ["ignore", "pipe", "pipe"] });
    const timer = setTimeout(() => { child.kill("SIGKILL"); resolveRun({ status: "timeout", code: null }); }, timeoutMs);
    child.on("close", (code) => { clearTimeout(timer); resolveRun({ status: code === 0 ? "pass" : "fail", code }); });
    child.on("error", () => { clearTimeout(timer); resolveRun({ status: "error", code: null }); });
  });
}
let failures = 0;
for (const scenario of manifest.scenarios) {
  const script = join(directory, scenario.id, "check.mjs");
  for (const transient of ["state.txt", "revert-blocked.txt", "recovery-needed.txt"]) {
    rmSync(join(directory, scenario.id, transient), { force: true });
  }
  const first = await run(script, scenario.timeoutMs ?? 2000);
  let observed = first.status;
  if (scenario.expected === "flaky") {
    const second = await run(script, scenario.timeoutMs ?? 2000);
    observed = first.status === "fail" && second.status === "pass" ? "flaky" : `${first.status}/${second.status}`;
  }
  if (scenario.expected === "known-failure") {
    const second = await run(script, scenario.timeoutMs ?? 2000);
    observed = first.status === "fail" && second.status === "fail" ? "known-failure" : `${first.status}/${second.status}`;
  }
  if (scenario.expected === "revert-failure") {
    const revertScript = join(directory, scenario.id, "revert.mjs");
    const revert = await run(revertScript, 2000);
    observed = first.status === "fail" && revert.status === "fail" ? "revert-failure" : `${first.status}/${revert.status}`;
  }
  const ok = observed === scenario.expected;
  if (!ok) failures += 1;
  console.log(`${ok ? "PASS" : "FAIL"} ${scenario.id}: expected ${scenario.expected}, observed ${observed}`);
}
process.exit(failures ? 1 : 0);
