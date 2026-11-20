import type { BuildClientReference, BuildEvidence } from "./types.ts";
import { BuildEvidenceError } from "./types.ts";
import {
  addUnique, isRecord, layerFromIdent, modulePathInfo, normalizeEvidenceItem, normalizeLayer,
  normalizePath, normalizeRuntime, numberValue, runtimeFromOutput, sha256, stringArray,
  stringValue,
} from "./paths.ts";
import {
  lowerVersion, packageNextVersion, recordFile, recordModuleId, recordPaths,
  recordRoute, recordSize, recordSymbol,
} from "./records.ts";

interface EvidenceBuilder {
  consume: (record: Record<string, unknown>, line?: number) => void;
  consumePlain: (record: Record<string, unknown>) => void;
  finish: (hash?: string) => BuildEvidence;
}

export function createBuilder(root: string, sourcePath: string): EvidenceBuilder {
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

export function dedupe(values: string[]): string[] {
  return [...new Set(values)];
}
