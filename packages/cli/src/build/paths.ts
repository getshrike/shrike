import { createHash } from "node:crypto";
import { extname, isAbsolute, relative } from "node:path";

export function sha256(value: string | Buffer): string {
  return `sha256:${createHash("sha256").update(value).digest("hex")}`;
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function stringValue(value: unknown): string {
  return typeof value === "string" && value.length > 0 ? value : "";
}

export function stringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is string => typeof item === "string" && item.length > 0);
}

export function numberValue(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}

export function normalizePath(root: string, input: string): string {
  return modulePathInfo(root, input).path;
}

type ModuleKind = "source" | "dependency" | "external" | "asset" | "generated";

interface ModulePathInfo {
  /** Repository-relative path with Turbopack annotations removed. */
  path: string;
  kind: ModuleKind;
}

export const SOURCE_EXTENSIONS = new Set([
  ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts",
]);

/**
 * Normalize one Turbopack module path and classify it.
 *
 * `next analyze export` addresses modules as `[project]/app/page.tsx`,
 * `[project]/node_modules/...`, `[externals]/crypto`, `[client-fs]/...`, and
 * `[output]/...`, and appends `[layer] (kind)` annotations to module
 * identifiers. Only project source modules can join the static graph; counting
 * dependencies and emitted assets as unmatched would make coverage meaningless
 * on every real application.
 */
export function modulePathInfo(root: string, input: string): ModulePathInfo {
  let value = input.replaceAll("\\", "/").trim();
  // Turbopack proxy modules name the real source in `{ server => "..." }`.
  const proxy = value.match(/\{\s*(?:server|client|edge)\s*=>\s*"([^"]+)"\s*\}/);
  if (proxy?.[1]) value = proxy[1];
  const loader = value.lastIndexOf("!");
  if (loader >= 0) value = value.slice(loader + 1);
  value = value.replace(/^[a-z]+:\/\/+/, "").replace(/^turbopack:\/\//, "").replace(/^file:\/\//, "");
  // `[app-rsc] (ecmascript, Next.js Server Component)` and trailing `<locals>`.
  value = value.replace(/\s+\[[^\]]+\]\s+\([^)]*\)(?:\s+<[^>]*>)?\s*$/, "");
  value = value.replace(/\s+<[^>]*>\s*$/, "");
  value = value.split("?")[0]!.split("#")[0]!;
  let kind: ModuleKind = "source";
  if (value.startsWith("[externals]/")) {
    kind = "external";
    value = value.slice("[externals]/".length);
  } else if (
    value.startsWith("[client-fs]/") ||
    value.startsWith("[output]/") ||
    value.startsWith("[next]/") ||
    value.startsWith("[turbopack]/")
  ) {
    kind = "asset";
    value = value.replace(/^\[[^\]]+\]\//, "");
  } else if (value.startsWith("[project]/")) {
    value = value.slice("[project]/".length);
  }
  if (isAbsolute(value)) value = relative(root, value).replaceAll("\\", "/");
  value = value.replace(/^\.\//, "");
  if (kind === "source") {
    if (value.startsWith("node_modules/") || value.includes("/node_modules/")) {
      kind = "dependency";
    } else if (value.startsWith("../") || value === ".." || isAbsolute(value)) {
      kind = "external";
    } else if (
      value.includes("--route-entry.") ||
      value.includes(".[json].") ||
      value.startsWith(".next-internal/") ||
      value.startsWith(".next/") ||
      value.endsWith("/__nextjs-internal-proxy.mjs")
    ) {
      kind = "generated";
    } else if (!SOURCE_EXTENSIONS.has(extname(value).toLowerCase())) {
      kind = "asset";
    }
  }
  return { path: value, kind };
}

export function layerFromIdent(ident: string): string {
  if (ident.includes("[app-client]") || ident.includes("[middleware-client]")) return "client";
  if (ident.includes("[app-rsc]") || ident.includes("[app-ssr]") || ident.includes("[middleware]")) return "server";
  if (ident.includes("[edge]")) return "edge";
  return "";
}

export function runtimeFromOutput(filename: string): string {
  const value = filename.replaceAll("\\", "/");
  if (value.includes("/static/") || value.startsWith("[client-fs]/")) return "client";
  if (value.includes("/server/") || value.startsWith("[output]/.next/server/")) return "server";
  return "";
}

/** Normalize a file or `path#Symbol` evidence key without losing the symbol. */
export function normalizeEvidenceItem(root: string, input: string): string {
  const hash = input.indexOf("#");
  if (hash < 0) return normalizePath(root, input);
  return `${normalizePath(root, input.slice(0, hash))}${input.slice(hash)}`;
}

export function normalizeLayer(value: unknown): string {
  const layer = String(value ?? "").toLowerCase();
  return ["client", "server", "edge"].includes(layer) ? layer : "";
}

export function normalizeRuntime(value: unknown): string {
  const runtime = String(value ?? "").toLowerCase();
  return ["client", "server", "edge", "both"].includes(runtime) ? runtime : "";
}

export function addUnique(list: string[], value: string): void {
  if (value.length > 0 && !list.includes(value)) list.push(value);
}
