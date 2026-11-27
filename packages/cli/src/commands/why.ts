import { booleanFlag, rejectUnknownFlags, type ParsedArgs } from "../args.ts";
import { loadConfig } from "../config.ts";
import { requestEngine } from "../engine.ts";
import { ExitCode, UsageError } from "../errors.ts";
import { writeHuman, writeJson } from "../output.ts";
import { analyze } from "../session.ts";
import type { Snapshot } from "../cli-types.ts";

interface ProofTrace {
  reason: string;
  incoming: string[];
  outgoing: string[];
}

interface ProofResult {
  symbol: string;
  live: boolean;
  witness: string[];
  trace: ProofTrace[];
}

/**
 * Human explanation for one symbol.
 *
 * `why --json` is exactly the engine's proof object. The text form adds the
 * symbol metadata from the same snapshot; it never runs a second analysis.
 */
export async function whyCommand(root: string, args: ParsedArgs): Promise<number> {
  rejectUnknownFlags(args, ["json", "help"]);
  const symbol = args.positionals[0];
  if (!symbol) throw new UsageError("why requires a symbol key (path#Name)");
  if (args.positionals.length > 1) {
    throw new UsageError(`unexpected argument: ${args.positionals[1]}`);
  }
  const config = await loadConfig(root);
  const snapshot: Snapshot = await analyze(root, config);
  const proof = await requestEngine<ProofResult>("proof", { snapshot, symbol });
  if (booleanFlag(args, "json")) {
    writeJson(proof);
    return ExitCode.OK;
  }

  const record = snapshot.symbols.find((candidate) => candidate.key === symbol);
  writeHuman(symbol);
  if (record) {
    writeHuman(`  file: ${record.file}`);
    writeHuman(`  kind: ${record.kind}`);
    writeHuman(`  lines: ${record.lines}`);
  }
  writeHuman(proof.live ? "  live at HEAD" : "  dead at HEAD");
  if (proof.live) {
    if (proof.witness.length > 0) {
      writeHuman("  witness:");
      for (const step of proof.witness) writeHuman(`    ${step}`);
    } else {
      writeHuman("  reachable from an entry.");
    }
  } else if (proof.trace.length === 0) {
    writeHuman("  no witness from any entry.");
  } else {
    for (const trace of proof.trace) {
      writeHuman(`  proof: ${trace.reason}`);
      if (trace.incoming.length > 0) writeHuman(`    incoming: ${trace.incoming.join(", ")}`);
      if (trace.outgoing.length > 0) writeHuman(`    outgoing: ${trace.outgoing.join(", ")}`);
    }
  }
  writeHuman("");
  writeHuman(proof.live ? "  Next: npx getshrike report" : "  Next: npx getshrike clean --all");
  return ExitCode.OK;
}
