#!/usr/bin/env node
import { booleanFlag, parseArgs } from "./args.ts";
import { repoRoot } from "./config.ts";
import { CliError, ExitCode, isCliError, UsageError } from "./errors.ts";
import { writeJson, writeJsonError, writeStderr } from "./output.ts";
import { printHelp } from "./render.ts";
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
  // Load only the command being run. A scan used to pay for the verifier,
  // receipts, agent, and MCP modules even though it needs none of them.
  switch (command) {
    case "version": {
      const { versionCommand } = await import("./commands/basic.ts");
      return versionCommand(args);
    }
    case "init": {
      const { initCommand } = await import("./commands/basic.ts");
      return initCommand(root, args);
    }
    case "scan": {
      const { scanCommand } = await import("./commands/scan.ts");
      return scanCommand(root, args);
    }
    case "deps": {
      const { depsCommand } = await import("./commands/scan.ts");
      return depsCommand(root, args);
    }
    case "delta": {
      const { deltaCommand } = await import("./commands/basic.ts");
      return deltaCommand(root, args);
    }
    case "plan": {
      const { planCommand } = await import("./commands/basic.ts");
      return planCommand(root, args);
    }
    case "prune": {
      const { pruneCommand } = await import("./commands/prune.ts");
      return pruneCommand(root, args);
    }
    case "verify": {
      const { verifyCommand } = await import("./commands/verify.ts");
      return verifyCommand(root, args);
    }
    case "receipt": {
      const { receiptCommand } = await import("./commands/basic.ts");
      return receiptCommand(root, args);
    }
    case "recover": {
      const { recoverCommand } = await import("./commands/basic.ts");
      return recoverCommand(root, args);
    }
    case "proof": {
      const { proofCommand } = await import("./commands/basic.ts");
      return proofCommand(root, args);
    }
    case "evidence": {
      const { evidenceCommand } = await import("./commands/basic.ts");
      return evidenceCommand(root, args);
    }
    case "hook": {
      const { hookCommand } = await import("./commands/hook.ts");
      return hookCommand(root, args);
    }
    case "mcp": {
      const { mcpCommand } = await import("./commands/mcp.ts");
      return mcpCommand(root, args);
    }
    case "cache": {
      const { cacheCommand } = await import("./commands/basic.ts");
      return cacheCommand(root, args);
    }
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
