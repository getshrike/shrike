/** Deterministic formatting helpers shared by the plain report and the TUI. */

export function compareText(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

export function formatInteger(value: number): string {
  const whole = Math.trunc(Math.abs(value)).toString();
  const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return value < 0 ? `-${grouped}` : grouped;
}

export function formatCount(count: number, singular: string, plural = `${singular}s`): string {
  return `${formatInteger(count)} ${count === 1 ? singular : plural}`;
}

export function codePoints(text: string): string[] {
  return [...text];
}

export function codePointLength(text: string): number {
  return codePoints(text).length;
}

export function clampWidth(width: number): number {
  return Math.max(40, Math.min(120, Math.trunc(width)));
}

/** Truncate prose with a trailing ellipsis. Never exceeds `width` code points. */
export function truncateEnd(text: string, width: number): string {
  const points = codePoints(text);
  if (points.length <= width) return text;
  if (width <= 0) return "";
  if (width === 1) return "…";
  return `${points.slice(0, width - 1).join("")}…`;
}

/** Keep the tail of a path, prefixed with `…/`, for narrow terminals. */
export function truncatePath(path: string, width: number): string {
  if (codePointLength(path) <= width) return path;
  const segments = path.split("/");
  const tail = segments.slice(-2).join("/");
  const candidate = `…/${tail}`;
  if (codePointLength(candidate) <= width) return candidate;
  const points = codePoints(path);
  if (width <= 1) return "…";
  return `…${points.slice(points.length - (width - 1)).join("")}`;
}

export function shortCommit(commit: string): string {
  return codePoints(commit).slice(0, 7).join("");
}

export function renderBar(basisPoints: number, cells: number, hasRemovable = false): string {
  const safeCells = Math.max(1, Math.trunc(cells));
  let filled = Math.round((basisPoints / 10000) * safeCells);
  filled = Math.max(0, Math.min(safeCells, filled));
  if (hasRemovable && filled === 0) filled = 1;
  return `${"█".repeat(filled)}${"░".repeat(safeCells - filled)}`;
}

export function percentage(basisPoints: number, removableLines: number): string {
  if (removableLines > 0 && basisPoints === 0) return "<0.1%";
  return `${(basisPoints / 100).toFixed(1)}%`;
}

export function colorize(text: string, code: number, enabled: boolean): string {
  return enabled ? `\x1b[${code}m${text}\x1b[0m` : text;
}
