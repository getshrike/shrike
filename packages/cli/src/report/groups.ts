/** Display grouping. This is a stable heuristic, not an architecture claim. */
import { compareText } from "./format.ts";

function directoryOf(path: string): string {
  const index = path.lastIndexOf("/");
  return index <= 0 ? (index === 0 ? "/" : ".") : path.slice(0, index);
}

/** Validate a repository-relative POSIX path used by the report. */
export function normalizeReportPath(path: string): string {
  if (path.startsWith("/") || path.includes("\\")) {
    throw new Error(`report paths must be repository-relative POSIX paths: ${path}`);
  }
  const segments = path.split("/");
  if (segments.some((segment) => segment === "..")) {
    throw new Error(`report paths must not contain '..': ${path}`);
  }
  return segments.filter((segment) => segment.length > 0).join("/");
}

export function groupDirectory(path: string): string {
  const normalized = normalizeReportPath(path);
  const segments = normalized.split("/");
  for (let index = 0; index < segments.length; index += 1) {
    const segment = segments[index];
    if ((segment === "features" || segment === "modules") && index + 1 < segments.length - 1) {
      return segments.slice(0, index + 2).join("/");
    }
  }
  if (segments[0] === "app" || segments[0] === "pages") return directoryOf(normalized);
  if (segments[0] === "packages" && segments.length >= 3) return segments.slice(0, 2).join("/");
  if (segments[0] === "apps" && segments.length >= 3) return segments.slice(0, 2).join("/");
  return directoryOf(normalized);
}

export function compareDirectory(left: string, right: string): number {
  return compareText(left, right);
}
