import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  closeSync,
  existsSync,
  openSync,
  readFileSync,
  readdirSync,
  readSync,
  statSync,
} from "node:fs";
import { extname, isAbsolute, join, relative, resolve } from "node:path";
import { StringDecoder } from "node:string_decoder";
import { resolveInside, safeRelativePath } from "./recovery.ts";

/**
 * Build-graph evidence loading.
 *
 * Next.js 16.4 writes `next analyze export` as JSON Lines. The JSONL path is
 * streamed in bounded chunks; older or hand-made evidence may be one JSON
 * object and is read whole. Both are normalized into the small shape understood
 * by the Rust `join-build` command. A malformed record is an explicit error,
 * never a silent skip.
 */

export type BuildStatus =
  | "full"
  | "partial"
  | "low-coverage"
  | "stale"
  | "malformed"
  | "unsupported"
  | "absent"
  | "build-failed";

export interface BuildRouteEvidence {
  route: string;
  files: string[];
  symbols: string[];
  runtime?: string;
}

export interface BuildClientReference {
  from: string;
  to: string;
  kind: string;
}

export interface BuildEvidence {
  source: string;
  version: string;
  files: string[];
  symbols: string[];
  routes: BuildRouteEvidence[];
  sizes: Record<string, number>;
  snapshotHash: string;
  layers: Record<string, string>;
  moduleIds: Record<string, string>;
  clientReferences: BuildClientReference[];
  fileHashes: Record<string, string>;
  unknownRecords: number;
  status?: string;
  clientBoundaryLeaks?: boolean;
}

export interface BuildEvidenceOptions {
  /** Run the project's build command before looking for evidence. */
  runBuild?: boolean;
  /** Override the build command. Default: `npm run build`. */
  buildCommand?: string;
  /** Build timeout in milliseconds. Default: 10 minutes. */
  timeoutMs?: number;
}

export interface LoadedBuildEvidence {
  evidence: BuildEvidence;
  path: string | null;
  /** Set when the file exists but could not be normalized. */
  error: string | null;
  /** Set when a requested build failed. Static-only, never a crash. */
  failure: string | null;
}

export class BuildEvidenceError extends Error {
  readonly status: BuildStatus;

  constructor(status: BuildStatus, message: string) {
    super(message);
    this.name = "BuildEvidenceError";
    this.status = status;
  }
}

export const EMPTY_BUILD_HASH = `sha256:${createHash("sha256").update("").digest("hex")}`;

function sha256(value: string | Buffer): string {
  return `sha256:${createHash("sha256").update(value).digest("hex")}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringValue(value: unknown): string {
  return typeof value === "string" && value.length > 0 ? value : "";
}

function stringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is string => typeof item === "string" && item.length > 0);
}

function numberValue(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}

function normalizePath(root: string, input: string): string {
  return modulePathInfo(root, input).path;
}

type ModuleKind = "source" | "dependency" | "external" | "asset" | "generated";

interface ModulePathInfo {
  /** Repository-relative path with Turbopack annotations removed. */
  path: string;
  kind: ModuleKind;
}

const SOURCE_EXTENSIONS = new Set([
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
function modulePathInfo(root: string, input: string): ModulePathInfo {
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

function layerFromIdent(ident: string): string {
  if (ident.includes("[app-client]") || ident.includes("[middleware-client]")) return "client";
  if (ident.includes("[app-rsc]") || ident.includes("[app-ssr]") || ident.includes("[middleware]")) return "server";
  if (ident.includes("[edge]")) return "edge";
  return "";
}

function runtimeFromOutput(filename: string): string {
  const value = filename.replaceAll("\\", "/");
  if (value.includes("/static/") || value.startsWith("[client-fs]/")) return "client";
  if (value.includes("/server/") || value.startsWith("[output]/.next/server/")) return "server";
  return "";
}

/** Normalize a file or `path#Symbol` evidence key without losing the symbol. */
function normalizeEvidenceItem(root: string, input: string): string {
  const hash = input.indexOf("#");
  if (hash < 0) return normalizePath(root, input);
  return `${normalizePath(root, input.slice(0, hash))}${input.slice(hash)}`;
}

function normalizeLayer(value: unknown): string {
  const layer = String(value ?? "").toLowerCase();
  return ["client", "server", "edge"].includes(layer) ? layer : "";
}

function normalizeRuntime(value: unknown): string {
  const runtime = String(value ?? "").toLowerCase();
  return ["client", "server", "edge", "both"].includes(runtime) ? runtime : "";
}

function addUnique(list: string[], value: string): void {
  if (value.length > 0 && !list.includes(value)) list.push(value);
}

/** Read the installed Next.js version from package.json, if present. */
export function packageNextVersion(root: string): string {
  const path = join(root, "package.json");
  if (!existsSync(path)) return "";
  try {
    const parsed: unknown = JSON.parse(readFileSync(path, "utf8"));
    if (!isRecord(parsed)) return "";
    const dependencies = isRecord(parsed.dependencies) ? parsed.dependencies : {};
    const devDependencies = isRecord(parsed.devDependencies) ? parsed.devDependencies : {};
    const value = dependencies.next ?? devDependencies.next;
    return typeof value === "string" ? value.replace(/^[~^]/, "") : "";
  } catch {
    return "";
  }
}

function lowerVersion(left: string, right: string): string {
  if (!left) return right;
  if (!right) return left;
  const parse = (value: string): number[] =>
    value.replace(/^[~^v]/, "").split(".").map((part) => Number.parseInt(part, 10) || 0);
  const a = parse(left);
  const b = parse(right);
  for (let index = 0; index < Math.max(a.length, b.length); index += 1) {
    const delta = (a[index] ?? 0) - (b[index] ?? 0);
    if (delta !== 0) return delta < 0 ? left : right;
  }
  return left;
}

/** Find the newest default evidence file without running a build. */
export function discoverBuildEvidence(root: string): string | null {
  const directories = [
    join(root, ".next", "analyze"),
    join(root, ".shrike", "build"),
    join(root, "next-analyze"),
  ];
  const candidates: string[] = [];
  for (const directory of directories) {
    if (!existsSync(directory)) continue;
    for (const name of readdirSync(directory)) {
      if (!name.endsWith(".json") && !name.endsWith(".jsonl")) continue;
      const path = join(directory, name);
      try {
        if (statSync(path).isFile()) candidates.push(path);
      } catch {
        // A disappearing file is not evidence.
      }
    }
  }
  candidates.sort((left, right) => {
    const leftTime = statSync(left).mtimeMs;
    const rightTime = statSync(right).mtimeMs;
    return rightTime - leftTime || right.localeCompare(left);
  });
  return candidates[0] ?? null;
}

function recordFile(record: Record<string, unknown>): string | null {
  for (const key of ["path", "file", "source", "module"]) {
    const value = record[key];
    if (typeof value === "string" && value.length > 0) return value;
  }
  return null;
}

function recordModuleId(record: Record<string, unknown>): string | null {
  for (const key of ["id", "moduleId", "chunkId", "ident"]) {
    const value = record[key];
    if (typeof value === "string" && value.length > 0) return value;
    if (typeof value === "number" && Number.isFinite(value)) return String(value);
  }
  return null;
}

function recordSymbol(record: Record<string, unknown>): string | null {
  for (const key of ["symbol", "symbolKey", "key"]) {
    const value = record[key];
    if (typeof value === "string" && value.includes("#")) return value;
  }
  return null;
}

function recordRoute(record: Record<string, unknown>): string | null {
  for (const key of ["route", "pathname", "entry"]) {
    const value = record[key];
    if (typeof value === "string" && value.length > 0) return value;
  }
  return null;
}

function recordSize(record: Record<string, unknown>): number | null {
  for (const key of ["size", "bytes", "byteSize"]) {
    const value = numberValue(record[key]);
    if (value !== null) return value;
  }
  return null;
}

function recordPaths(record: Record<string, unknown>, keys: string[]): string[] {
  const result: string[] = [];
  for (const key of keys) {
    for (const value of stringArray(record[key])) result.push(value);
  }
  return result;
}

interface EvidenceBuilder {
  consume: (record: Record<string, unknown>, line?: number) => void;
  consumePlain: (record: Record<string, unknown>) => void;
  finish: (hash?: string) => BuildEvidence;
}

function createBuilder(root: string, sourcePath: string): EvidenceBuilder {
  const files: string[] = [];
  const symbols: string[] = [];
  interface RouteAccumulator {
    route: string;
    files: string[];
    clientFiles: string[];
    serverFiles: string[];
    symbols: string[];
    runtime?: string;
  }
  const routes = new Map<string, RouteAccumulator>();
  const sizes: Record<string, number> = {};
  const sizeClient: Record<string, number> = {};
  const sizeServer: Record<string, number> = {};
  const layers: Record<string, string> = {};
  const clientModules = new Set<string>();
  const serverModules = new Set<string>();
  const moduleIds: Record<string, string> = {};
  const clientReferences: BuildClientReference[] = [];
  const fileHashes: Record<string, string> = {};
  let source = "next analyze export";
  let version = packageNextVersion(root);
  let snapshotHash = "";
  let unknownRecords = 0;
  let status: string | undefined;
  let clientBoundaryLeaks: boolean | undefined;

  const routeFor = (route: string): RouteAccumulator => {
    const existing = routes.get(route);
    if (existing) return existing;
    const created: RouteAccumulator = {
      route,
      files: [],
      clientFiles: [],
      serverFiles: [],
      symbols: [],
    };
    routes.set(route, created);
    return created;
  };

  const addRouteFile = (target: RouteAccumulator, runtime: string, file: string): void => {
    if (runtime === "client") addUnique(target.clientFiles, file);
    else if (runtime === "server") addUnique(target.serverFiles, file);
    else addUnique(target.files, file);
  };

  const addItem = (item: string): string | null => {
    // Keep a `path#Symbol` suffix: module ids address the file, not the symbol.
    const hash = item.indexOf("#");
    const filePart = hash >= 0 ? item.slice(0, hash) : item;
    const suffix = hash >= 0 ? item.slice(hash) : "";
    // Resolve module ids before classifying: an id such as `m1` has no source
    // extension, but it maps to a project source path.
    const mapped = moduleIds[item] ?? moduleIds[filePart];
    const info = modulePathInfo(root, mapped ?? filePart);
    if (info.kind !== "source") return null;
    return `${mapped ? normalizePath(root, mapped) : info.path}${suffix}`;
  };

  const addSourceFile = (file: string): string | null => {
    const info = modulePathInfo(root, file);
    if (info.kind !== "source") return null;
    addUnique(files, info.path);
    return info.path;
  };

  const consumePlain = (record: Record<string, unknown>): void => {
    if (typeof record.source === "string" && record.source.length > 0) source = record.source;
    if (typeof record.version === "string" && record.version.length > 0) {
      version = lowerVersion(version, record.version);
    }
    if (typeof record.snapshotHash === "string" && record.snapshotHash.length > 0) {
      snapshotHash = record.snapshotHash;
    }
    if (typeof record.status === "string" && record.status.length > 0) {
      status = record.status;
    }
    if (typeof record.clientBoundaryLeaks === "boolean") {
      clientBoundaryLeaks = record.clientBoundaryLeaks;
    }
    for (const file of stringArray(record.files)) addSourceFile(file);
    for (const symbol of stringArray(record.symbols)) addUnique(symbols, symbol);
    if (isRecord(record.sizes)) {
      for (const [key, value] of Object.entries(record.sizes)) {
        const size = numberValue(value);
        if (size !== null) sizes[normalizeEvidenceItem(root, key)] = size;
      }
    }
    if (isRecord(record.layers)) {
      for (const [key, value] of Object.entries(record.layers)) {
        const layer = normalizeLayer(value);
        if (layer) layers[normalizeEvidenceItem(root, key)] = layer;
      }
    }
    if (isRecord(record.moduleIds)) {
      for (const [key, value] of Object.entries(record.moduleIds)) {
        if (typeof value !== "string" || value.length === 0) continue;
        const info = modulePathInfo(root, value);
        if (info.kind === "source") moduleIds[key] = info.path;
      }
    }
    if (isRecord(record.fileHashes)) {
      for (const [key, value] of Object.entries(record.fileHashes)) {
        if (typeof value === "string" && value.length > 0) {
          fileHashes[normalizePath(root, key)] = value;
        }
      }
    }
    if (Array.isArray(record.routes)) {
      for (const item of record.routes) {
        if (!isRecord(item)) continue;
        const route = recordRoute(item);
        if (!route) continue;
        const runtime = normalizeRuntime(item.runtime ?? item.layer);
        const target = routeFor(route);
        if (runtime) target.runtime = runtime;
        for (const file of stringArray(item.files)) {
          const normalized = addSourceFile(file);
          if (normalized) addUnique(target.files, normalized);
        }
        for (const symbol of stringArray(item.symbols)) addUnique(target.symbols, symbol);
      }
    }
    if (Array.isArray(record.clientReferences)) {
      for (const item of record.clientReferences) {
        if (!isRecord(item)) continue;
        const from = stringValue(item.from ?? item.client ?? item.importer);
        const to = stringValue(item.to ?? item.server ?? item.action ?? item.target);
        if (!from || !to) continue;
        clientReferences.push({
          from: normalizeEvidenceItem(root, from),
          to: normalizeEvidenceItem(root, to),
          kind: stringValue(item.referenceKind ?? item.referenceType ?? item.kind) || "unknown",
        });
      }
    }
  };

  const consume = (record: Record<string, unknown>, line?: number): void => {
    const type = String(record.type ?? record.kind ?? record.record ?? "").toLowerCase();
    const where = line === undefined ? sourcePath : `${sourcePath}:${line}`;
    if (!type) {
      consumePlain(record);
      return;
    }
    if (type === "meta") {
      // `next analyze export` metadata: snapshot name, schema version, counts.
      // It adds no join evidence and is not an unknown family.
      if (typeof record.source === "string" && record.source.length > 0) source = record.source;
      return;
    }
    if (type === "module") {
      const id = recordModuleId(record);
      const path = recordFile(record);
      if (!path && !id) {
        throw new BuildEvidenceError("malformed", `module record has no path or id at ${where}`);
      }
      const info = modulePathInfo(root, path ?? String(id));
      if (info.kind !== "source") return;
      addUnique(files, info.path);
      if (id) moduleIds[String(id)] = info.path;
      const layer = normalizeLayer(record.layer ?? record.runtime) || layerFromIdent(String(id ?? path));
      if (layer === "client") clientModules.add(info.path);
      else if (layer === "server") serverModules.add(info.path);
      const size = recordSize(record);
      if (size !== null) sizes[info.path] = size;
      return;
    }
    if (type === "chunk") {
      const route = recordRoute(record);
      const runtime = normalizeRuntime(record.runtime ?? record.layer);
      const chunkFiles = recordPaths(record, ["files", "modules", "moduleIds"])
        .map(addItem)
        .filter((item): item is string => item !== null);
      const chunkSymbols = recordPaths(record, ["symbols", "symbolKeys"]);
      const size = recordSize(record);
      if (route) {
        const target = routeFor(route);
        if (runtime) target.runtime = runtime;
        for (const file of chunkFiles) addUnique(target.files, file);
        for (const symbol of chunkSymbols) addUnique(target.symbols, symbol);
      }
      for (const file of chunkFiles) addUnique(files, file);
      for (const symbol of chunkSymbols) addUnique(symbols, symbol);
      if (size !== null && route) {
        for (const file of chunkFiles) if (sizes[file] === undefined) sizes[file] = size;
      }
      return;
    }
    if (type === "entry" || type === "route") {
      const route = recordRoute(record);
      if (!route) return;
      const runtime = normalizeRuntime(record.runtime ?? record.layer);
      const entryFiles = recordPaths(record, ["files", "modules", "moduleIds"])
        .map(addItem)
        .filter((item): item is string => item !== null);
      const entrySymbols = recordPaths(record, ["symbols", "symbolKeys"]);
      const target = routeFor(route);
      if (runtime) target.runtime = runtime;
      for (const file of entryFiles) addUnique(target.files, file);
      for (const symbol of entrySymbols) addUnique(target.symbols, symbol);
      for (const file of entryFiles) addUnique(files, file);
      for (const symbol of entrySymbols) addUnique(symbols, symbol);
      // Real Turbopack route records list entry modules instead of a flat
      // `files` array. Keep only project sources; bootstrap and template
      // modules are framework code.
      if (Array.isArray(record.entries)) {
        for (const entry of record.entries) {
          if (!isRecord(entry)) continue;
          const modulePath = stringValue(entry.module_path ?? entry.path ?? entry.module);
          const info = modulePathInfo(root, modulePath);
          if (info.kind !== "source") continue;
          addUnique(files, info.path);
          const entryRuntime =
            stringValue(entry.entry_kind) === "client_bootstrap"
              ? "client"
              : layerFromIdent(stringValue(entry.module_ident)) || "server";
          addRouteFile(target, entryRuntime, info.path);
          if (entryRuntime === "client") clientModules.add(info.path);
          else if (entryRuntime === "server") serverModules.add(info.path);
        }
      }
      return;
    }
    if (type === "output") {
      const route = recordRoute(record);
      const runtime = runtimeFromOutput(stringValue(record.filename));
      for (const module of stringArray(record.modules)) {
        const info = modulePathInfo(root, module);
        if (info.kind !== "source") continue;
        addUnique(files, info.path);
        if (runtime === "client") clientModules.add(info.path);
        else if (runtime === "server") serverModules.add(info.path);
        if (route) addRouteFile(routeFor(route), runtime, info.path);
      }
      return;
    }
    if (type === "part") {
      const route = recordRoute(record);
      const runtime = runtimeFromOutput(stringValue(record.filename));
      const info = modulePathInfo(
        root,
        stringValue(record.source_path ?? record.source ?? record.path ?? record.module),
      );
      if (info.kind !== "source") return;
      addUnique(files, info.path);
      if (runtime === "client") clientModules.add(info.path);
      else if (runtime === "server") serverModules.add(info.path);
      const size = recordSize(record);
      if (size !== null) {
        if (runtime === "client") {
          sizeClient[info.path] = Math.max(sizeClient[info.path] ?? 0, size);
        } else if (runtime === "server") {
          sizeServer[info.path] = Math.max(sizeServer[info.path] ?? 0, size);
        } else {
          sizes[info.path] = Math.max(sizes[info.path] ?? 0, size);
        }
      }
      if (route) addRouteFile(routeFor(route), runtime, info.path);
      return;
    }
    if (type === "group") {
      // Turbopack bootstrap groups list emitted chunk filenames, not source
      // modules. They are understood and intentionally add no join evidence.
      return;
    }
    if (type === "client-reference" || type === "clientreference") {
      const from = stringValue(record.from ?? record.client ?? record.importer);
      const to = stringValue(record.to ?? record.server ?? record.action ?? record.target);
      const normalizedFrom = from ? addItem(from) : null;
      const normalizedTo = to ? addItem(to) : null;
      if (normalizedFrom && normalizedTo) {
        clientReferences.push({
          from: normalizedFrom,
          to: normalizedTo,
          kind: stringValue(record.referenceKind ?? record.referenceType ?? record.reference) || "unknown",
        });
      }
      const file = recordFile(record);
      if (file) addSourceFile(file);
      const symbol = recordSymbol(record);
      if (symbol) addUnique(symbols, symbol);
      return;
    }
    if (type === "file-hash" || type === "source-hash" || type === "filehash") {
      const file = recordFile(record);
      const hash = stringValue(record.hash ?? record.sha256 ?? record.value);
      if (file && hash) fileHashes[normalizePath(root, file)] = hash;
      return;
    }
    // Additive record families are counted, never guessed.
    unknownRecords += 1;
  };

  const finish = (hash?: string): BuildEvidence => {
    const mapItem = (item: string): string => moduleIds[item] ?? item;
    const mappedFiles = dedupe(files.map(mapItem));
    const mappedSymbols = symbols.map((symbol) => {
      const [file, name] = symbol.split("#");
      if (name && file && moduleIds[file]) return `${moduleIds[file]}#${name}`;
      return symbol;
    });
    const mappedSizes: Record<string, number> = {};
    for (const key of new Set([
      ...Object.keys(sizes),
      ...Object.keys(sizeClient),
      ...Object.keys(sizeServer),
    ])) {
      const value = sizeClient[key] ?? sizeServer[key] ?? sizes[key];
      if (value !== undefined) mappedSizes[mapItem(key)] = value;
    }
    const mappedLayers: Record<string, string> = {};
    for (const file of files) {
      if (clientModules.has(file)) mappedLayers[file] = "client";
      else if (serverModules.has(file)) mappedLayers[file] = "server";
    }
    for (const [key, value] of Object.entries(layers)) {
      mappedLayers[mapItem(key)] = value;
    }
    const mappedRoutes = [...routes.values()].map((route) => {
      const client = dedupe(route.clientFiles.map(mapItem));
      const server = dedupe(route.serverFiles.map(mapItem));
      const neutral = dedupe(route.files.map(mapItem));
      let runtime = route.runtime;
      let routeFiles: string[];
      if (runtime === "client" && client.length > 0) {
        routeFiles = client;
      } else if (runtime === "server" && server.length > 0) {
        routeFiles = server;
      } else if (client.length > 0) {
        runtime = "client";
        routeFiles = client;
      } else if (server.length > 0) {
        runtime = "server";
        routeFiles = server;
      } else {
        routeFiles = neutral;
      }
      return {
        route: route.route,
        files: routeFiles,
        symbols: dedupe(route.symbols.map((symbol) => {
          const [file, name] = symbol.split("#");
          if (name && file && moduleIds[file]) return `${moduleIds[file]}#${name}`;
          return symbol;
        })),
        ...(runtime ? { runtime } : {}),
      };
    });
    return {
      source,
      version,
      files: mappedFiles,
      symbols: dedupe(mappedSymbols),
      routes: mappedRoutes,
      sizes: mappedSizes,
      snapshotHash: snapshotHash || (hash ? `sha256:${hash}` : sha256("")),
      layers: mappedLayers,
      moduleIds,
      clientReferences,
      fileHashes,
      unknownRecords,
      ...(status ? { status } : {}),
      ...(clientBoundaryLeaks === undefined ? {} : { clientBoundaryLeaks }),
    };
  };

  return { consume, consumePlain, finish };
}

function dedupe(values: string[]): string[] {
  return [...new Set(values)];
}

/**
 * Normalize one JSON object or a JSONL record stream from an in-memory string.
 * Unknown record families are counted; malformed JSON is not.
 */
export function normalizeBuildEvidence(
  root: string,
  text: string,
  sourcePath: string,
): BuildEvidence {
  const builder = createBuilder(root, sourcePath);
  const parsed: unknown = (() => {
    try {
      return JSON.parse(text);
    } catch {
      return undefined;
    }
  })();

  if (parsed !== undefined) {
    const records = Array.isArray(parsed) ? parsed : [parsed];
    for (const [index, record] of records.entries()) {
      if (!isRecord(record)) {
        throw new BuildEvidenceError("malformed", `record ${index + 1} is not an object: ${sourcePath}`);
      }
      builder.consume(record, index + 1);
    }
  } else {
    const lines = text.split(/\r?\n/);
    for (const [index, line] of lines.entries()) {
      if (!line.trim()) continue;
      let record: unknown;
      try {
        record = JSON.parse(line);
      } catch {
        throw new BuildEvidenceError("malformed", `invalid JSON at ${sourcePath}:${index + 1}`);
      }
      if (!isRecord(record)) {
        throw new BuildEvidenceError("malformed", `record is not an object at ${sourcePath}:${index + 1}`);
      }
      builder.consume(record, index + 1);
    }
  }
  return builder.finish(sha256(text).slice("sha256:".length));
}

/**
 * Stream a JSONL export in bounded chunks. The file is never loaded whole, so a
 * 200 MB export costs one chunk plus the current line.
 */
export function streamBuildEvidence(root: string, path: string): BuildEvidence {
  const builder = createBuilder(root, path);
  const hash = createHash("sha256");
  const decoder = new StringDecoder("utf8");
  const buffer = Buffer.alloc(64 * 1024);
  const fd = openSync(path, "r");
  let remainder = "";
  let lineNumber = 0;
  try {
    for (;;) {
      const bytes = readSync(fd, buffer, 0, buffer.length, null);
      if (bytes === 0) break;
      const chunk = buffer.subarray(0, bytes);
      hash.update(chunk);
      remainder += decoder.write(chunk);
      for (;;) {
        const newline = remainder.indexOf("\n");
        if (newline < 0) break;
        const line = remainder.slice(0, newline).replace(/\r$/, "");
        remainder = remainder.slice(newline + 1);
        lineNumber += 1;
        if (!line.trim()) continue;
        let record: unknown;
        try {
          record = JSON.parse(line);
        } catch {
          throw new BuildEvidenceError("malformed", `invalid JSON at ${path}:${lineNumber}`);
        }
        if (!isRecord(record)) {
          throw new BuildEvidenceError("malformed", `record is not an object at ${path}:${lineNumber}`);
        }
        builder.consume(record, lineNumber);
      }
    }
    remainder += decoder.end();
    if (remainder.trim()) {
      lineNumber += 1;
      let record: unknown;
      try {
        record = JSON.parse(remainder);
      } catch {
        throw new BuildEvidenceError("malformed", `invalid JSON at ${path}:${lineNumber}`);
      }
      if (!isRecord(record)) {
        throw new BuildEvidenceError("malformed", `record is not an object at ${path}:${lineNumber}`);
      }
      builder.consume(record, lineNumber);
    }
  } finally {
    closeSync(fd);
  }
  return builder.finish(hash.digest("hex"));
}

function emptyEvidence(root: string): BuildEvidence {
  return {
    source: "static",
    version: packageNextVersion(root) || "unknown",
    files: [],
    symbols: [],
    routes: [],
    sizes: {},
    snapshotHash: EMPTY_BUILD_HASH,
    layers: {},
    moduleIds: {},
    clientReferences: [],
    fileHashes: {},
    unknownRecords: 0,
  };
}

function applyStaleness(root: string, evidence: BuildEvidence): void {
  const entries = Object.entries(evidence.fileHashes);
  if (entries.length === 0) return;
  for (const [file, expected] of entries) {
    const full = join(root, file);
    if (!existsSync(full)) {
      evidence.status = "stale";
      return;
    }
    const actual = sha256(readFileSync(full));
    const normalized = expected.startsWith("sha256:") ? expected : `sha256:${expected}`;
    if (actual !== normalized) {
      evidence.status = "stale";
      return;
    }
  }
}

function runBuild(root: string, options: BuildEvidenceOptions): string | null {
  const command = options.buildCommand ?? "npm run build";
  const result = spawnSync(command, {
    cwd: root,
    shell: true,
    encoding: "utf8",
    timeout: options.timeoutMs ?? 600_000,
    env: process.env,
  });
  if (result.error) return result.error.message;
  if (result.status !== 0) {
    const detail = (result.stderr || result.stdout || `exit ${result.status}`).trim();
    return detail.slice(0, 2_000);
  }
  return null;
}

/** Load explicit evidence or discover a default snapshot. */
export function loadBuildEvidence(
  root: string,
  explicitPath?: string | null,
  options: BuildEvidenceOptions = {},
): LoadedBuildEvidence {
  if (options.runBuild || options.buildCommand) {
    const failure = runBuild(root, options);
    if (failure) {
      return {
        evidence: { ...emptyEvidence(root), status: "build-failed" },
        path: null,
        error: null,
        failure,
      };
    }
  }

  let path: string | null;
  try {
    path = explicitPath ? resolveEvidencePath(root, explicitPath) : discoverBuildEvidence(root);
  } catch (error) {
    if (error instanceof BuildEvidenceError) {
      return { evidence: emptyEvidence(root), path: null, error: error.message, failure: null };
    }
    throw error;
  }
  if (!path) {
    return { evidence: emptyEvidence(root), path: null, error: null, failure: null };
  }
  try {
    const evidence = path.endsWith(".jsonl")
      ? streamBuildEvidence(root, path)
      : normalizeBuildEvidence(root, readFileSync(path, "utf8"), path);
    applyStaleness(root, evidence);
    return { evidence, path, error: null, failure: null };
  } catch (error) {
    if (error instanceof BuildEvidenceError) {
      return { evidence: emptyEvidence(root), path, error: error.message, failure: null };
    }
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return {
        evidence: emptyEvidence(root),
        path,
        error: `build evidence does not exist: ${path}`,
        failure: null,
      };
    }
    throw error;
  }
}

function resolveEvidencePath(root: string, input: string): string {
  if (isAbsolute(input)) {
    const normalized = resolve(input);
    const base = resolve(root);
    if (normalized !== base && !normalized.startsWith(`${base}/`)) {
      throw new BuildEvidenceError("malformed", `build evidence is outside the repository: ${input}`);
    }
    return normalized;
  }
  try {
    return resolveInside(root, safeRelativePath(input));
  } catch (error) {
    throw new BuildEvidenceError(
      "malformed",
      error instanceof Error ? error.message : String(error),
    );
  }
}
