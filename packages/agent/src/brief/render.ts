import type { BriefDeltaItem, CleanupBrief } from "../types.ts";

/** One-screen human summary for the hook. */
export function renderBriefSummary(brief: CleanupBrief): string {
  const lines: string[] = [];
  lines.push(
    `Shrike: ${brief.newlyDead} newly dead symbol${brief.newlyDead === 1 ? "" : "s"} ` +
      `in ${brief.files} file${brief.files === 1 ? "" : "s"} ` +
      `(${brief.untouchedFiles} untouched by this change).`,
  );
  const byFile = new Map<string, BriefDeltaItem[]>();
  for (const item of brief.top) {
    const group = byFile.get(item.file);
    if (group) group.push(item);
    else byFile.set(item.file, [item]);
  }
  for (const [file, items] of byFile) {
    const symbols = items
      .map((item) => item.symbol.includes("#") ? item.symbol.slice(item.symbol.indexOf("#") + 1) : item.symbol)
      .join(", ");
    const lineEstimate = items.reduce((sum, item) => sum + (item.estimatedLines ?? 0), 0);
    const suffix = lineEstimate > 0 ? `  -${lineEstimate} lines` : "";
    lines.push(`  ${file.padEnd(28)} ${symbols}${suffix}`);
  }
  if (brief.truncated) lines.push("  (brief truncated)");
  lines.push(`Next: ${brief.nextCommand}`);
  return `${lines.join("\n")}\n`;
}
