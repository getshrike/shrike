import type { Receipt } from "../types.ts";

/** Human-readable Markdown summary for a PR body or comment. */
export function renderReceiptMarkdown(receipt: Receipt): string {
  const lines: string[] = [];
  lines.push("## Verified cleanup");
  lines.push("");
  lines.push(
    `Batch **${receipt.batch.id}** removed **${receipt.batch.linesRemoved} lines** across ` +
      `**${receipt.batch.files.length} files** (${receipt.batch.symbols.length} symbols).`,
  );
  lines.push("");
  lines.push("| Check | Status | Baseline | Duration |");
  lines.push("|---|---|---|---|");
  for (const check of receipt.checks) {
    lines.push(
      `| ${check.name} | ${check.status} | ${check.baselineStatus ?? "missing"} | ${check.durationMs} ms |`,
    );
  }
  lines.push("");
  lines.push(`**Outcome:** ${receipt.outcome}`);
  if (receipt.rejected.length > 0) {
    lines.push("");
    lines.push("**Rejected:**");
    for (const rejection of receipt.rejected) {
      lines.push(
        `- ${rejection.symbols.join(", ") || "(batch)"} — ${rejection.reason}` +
          (rejection.check ? ` (${rejection.check})` : ""),
      );
    }
  }
  lines.push("");
  lines.push(`Receipt: \`.shrike/receipts/${receipt.id}.json\``);
  lines.push("");
  lines.push(`Content hash: \`${receipt.contentHash}\``);
  return `${lines.join("\n")}\n`;
}
