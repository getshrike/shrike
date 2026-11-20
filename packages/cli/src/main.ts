#!/usr/bin/env node
import { booleanFlag, parseArgs } from "./args.ts";
import { repoRoot } from "./config.ts";
import { CliError, ExitCode, isCliError, UsageError } from "./errors.ts";
import { writeJson, writeJsonError, writeStderr } from "./output.ts";
import { printHelp } from "./render.ts";
import {
  cacheCommand, deltaCommand, depsCommand, evidenceCommand, initCommand, planCommand, proofCommand,
  receiptCommand, recoverCommand, scanCommand, versionCommand,
} from "./commands/basic.ts";
import { pruneCommand } from "./commands/prune.ts";
import { verifyCommand } from "./commands/verify.ts";
import { hookCommand } from "./commands/hook.ts";
import { mcpCommand } from "./commands/mcp.ts";
const BOOLEAN_FLAGS = new Set([
  "json", "help", "force", "pr", "in-place", "allow-dirty", "verify", "receipt",
  "quiet", "dry-run", "claude", "cursor", "opencode", "all", "no-checks",
  "require-build-evidence",
]);

async function main(argv: string[]): Promise<number> {
  const command = argv[0];
  if (!command || command === "--help" || command === "-h" || command === "help") {
    printHelp();
    return ExitCode.OK;
  }
  const args = parseArgs(argv.slice(1), BOOLEAN_FLAGS);
  if (booleanFlag(args, "help")) { printHelp(); return ExitCode.OK; }
  const root = repoRoot();
  switch (command) {
    case "version": return versionCommand(args);
    case "init": return initCommand(root, args);
    case "scan": return scanCommand(root, args);
    case "deps": return depsCommand(root, args);
    case "delta": return deltaCommand(root, args);
    case "plan": return planCommand(root, args);
    case "prune": return pruneCommand(root, args);
    case "verify": return verifyCommand(root, args);
    case "receipt": return receiptCommand(root, args);
    case "recover": return recoverCommand(root, args);
    case "proof": return proofCommand(root, args);
    case "evidence": return evidenceCommand(root, args);
    case "hook": return hookCommand(root, args);
    case "mcp": return mcpCommand(root, args);
    case "cache": return cacheCommand(root, args);
    default: throw new UsageError(`unknown command: ${command}`);
  }
}
main(process.argv.slice(2))
  // Set the code instead of calling process.exit(): exiting immediately can
  // truncate large JSON documents still draining to a pipe.
  .then((code) => {
    process.exitCode = code;
  })
  .catch((error: unknown) => {
    const cliError = isCliError(error) ? error : new CliError(ExitCode.COULD_NOT_RUN, "internal", error instanceof Error ? error.message : String(error));
    writeStderr(`shrike: ${cliError.message}`);
    if (process.argv.includes("--json")) writeJsonError({ code: cliError.code, message: cliError.message, exitCode: cliError.exitCode, details: cliError.details });
    process.exitCode = cliError.exitCode;
  });
