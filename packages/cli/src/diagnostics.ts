/**
 * Diagnostic classification.
 *
 * Diagnostics are facts the analysis could not use: an unparsed file, a
 * source import that did not resolve, a stale build, or a boundary leak. They
 * are not dead-code claims. Grouping them by cause keeps the signal visible
 * without turning a large repository's asset imports into alarm noise.
 */

export interface DiagnosticLike {
  code: string;
  severity: string;
  message: string;
  file?: string;
}

export interface DiagnosticGroup {
  category: string;
  severity: "error" | "warning" | "info";
  code: string;
  count: number;
  actionable: boolean;
  sample: string;
  files: string[];
}

export interface DiagnosticSummary {
  total: number;
  actionable: number;
  informational: number;
  bySeverity: { error: number; warning: number; info: number };
  byCode: Record<string, number>;
  groups: DiagnosticGroup[];
}

const ASSET_EXTENSION = /\.(?:css|scss|sass|less|styl|svg|png|jpe?g|gif|webp|avif|ico|bmp|woff2?|ttf|otf|eot|mp4|webm|mp3|wav|pdf|wasm|json|ya?ml|toml|graphql|gql|vue|svelte|astro)(?:[?#].*)?$/i;

const GENERATED_PATH = /(?:^|\/)(?:\.next|\.nuxt|\.svelte-kit|\.astro|\.output|\.vercel)\//;

function categoryFor(item: DiagnosticLike): { category: string; actionable: boolean } {
  if (item.code === "parse_error" || item.code === "unparsed_file") {
    return { category: "parse", actionable: true };
  }
  if (item.code === "asset_import") {
    return { category: "asset-import", actionable: false };
  }
  if (item.code === "unresolved_import") {
    const match = /`([^`]+)`/.exec(item.message);
    if (
      match &&
      (ASSET_EXTENSION.test(match[1] ?? "") || GENERATED_PATH.test(match[1] ?? ""))
    ) {
      return { category: "asset-import", actionable: false };
    }
    return { category: "unresolved-import", actionable: true };
  }
  if (item.code === "client-boundary-leak") {
    return { category: "boundary", actionable: true };
  }
  if (item.code === "stale-build" || item.code === "build-failed" || item.code === "low-coverage") {
    return { category: "build-evidence", actionable: true };
  }
  return { category: "other", actionable: item.severity !== "info" };
}

/** Group diagnostics by cause, severity, and code. Deterministic order. */
export function summarizeDiagnostics(items: DiagnosticLike[] = []): DiagnosticSummary {
  const bySeverity = { error: 0, warning: 0, info: 0 };
  const byCode: Record<string, number> = {};
  const groups = new Map<string, DiagnosticGroup>();
  for (const item of items) {
    byCode[item.code] = (byCode[item.code] ?? 0) + 1;
    if (item.severity === "error") bySeverity.error += 1;
    else if (item.severity === "warning") bySeverity.warning += 1;
    else bySeverity.info += 1;
    const { category, actionable } = categoryFor(item);
    const key = `${category}:${item.severity}:${item.code}`;
    const existing = groups.get(key);
    if (existing) {
      existing.count += 1;
      if (existing.files.length < 5 && item.file && !existing.files.includes(item.file)) {
        existing.files.push(item.file);
      }
    } else {
      groups.set(key, {
        category,
        severity: item.severity === "error" ? "error" : item.severity === "info" ? "info" : "warning",
        code: item.code,
        count: 1,
        actionable,
        sample: item.message,
        files: item.file ? [item.file] : [],
      });
    }
  }
  const ordered = [...groups.values()].sort(
    (a, b) =>
      Number(b.actionable) - Number(a.actionable) ||
      b.count - a.count ||
      a.category.localeCompare(b.category) ||
      a.code.localeCompare(b.code),
  );
  for (const group of ordered) group.files.sort();
  return {
    total: items.length,
    actionable: ordered.filter((group) => group.actionable).reduce((total, group) => total + group.count, 0),
    informational: ordered.filter((group) => !group.actionable).reduce((total, group) => total + group.count, 0),
    bySeverity,
    byCode,
    groups: ordered,
  };
}
