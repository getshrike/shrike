/**
 * Single source of truth for the TypeScript view of the Shrike wire protocol
 * and its JSON artifacts. These types mirror the JSON Schemas in `schemas/`;
 * keep both in sync in the same change. No index signatures: unknown fields
 * are surfaced through an explicit `extensions` record where genuinely needed.
 */

export interface Span {
  start: number;
  end: number;
  line: number;
  column: number;
  endLine?: number;
}

export interface Diagnostic {
  severity: "error" | "warning" | "info";
  code: string;
  message: string;
  file?: string;
  span?: Span;
  details?: string[];
}

export interface FileRecord {
  path: string;
  contentHash: string;
  moduleKind: string;
  parseStatus: "ok" | "error" | "skipped";
  generated: boolean;
  package?: string | null;
  publicApi: boolean;
  lines: number;
}

export interface SymbolRecord {
  key: string;
  name: string;
  kind: string;
  file: string;
  span: Span;
  exported: boolean;
  entry: boolean;
  publicApi: boolean;
  generated: boolean;
  typeOnly: boolean;
  lines: number;
}

export interface EntryRecord {
  symbol: string;
  framework: string;
  kind: string;
  route?: string | null;
  runtime: "both" | "client" | "server" | string;
  certainty: "certain" | "probable" | "unknown" | string;
}

export interface EdgeRecord {
  source: string;
  target: string;
  kind: string;
  certainty: "certain" | "probable" | "unknown" | string;
  file: string;
  span: Span;
}

export interface ModuleRecord {
  path: string;
  file: string;
  specifiers: string[];
  resolved: string[];
  unresolved: string[];
  external: string[];
  typeRefs: string[];
}

export interface UnknownRecord {
  reason: string;
  scope: string;
  package?: string | null;
  file: string;
  span: Span;
}

export interface WitnessRecord {
  symbol: string;
  path: string[];
}

export interface ProofRecord {
  symbol: string;
  reason: string;
  incoming: string[];
  outgoing: string[];
}

export interface GraphSnapshot {
  schemaVersion: "1.0";
  protocolVersion: "1.0";
  toolVersion: string;
  configHash: string;
  resolverHash: string;
  files: FileRecord[];
  symbols: SymbolRecord[];
  modules: ModuleRecord[];
  entries: EntryRecord[];
  edges: EdgeRecord[];
  sccs: string[][];
  live: string[];
  candidates: string[];
  unusedFiles: string[];
  internalOnly: string[];
  witnesses: WitnessRecord[];
  proofs: ProofRecord[];
  unknowns: UnknownRecord[];
  diagnostics: Diagnostic[];
  graphHash: string;
}

export interface DeltaReport {
  schemaVersion: "1.0";
  base: { ref: string | null; commit: string };
  head: { commit: string; dirty: boolean };
  newlyDead: { symbol: string; file: string; estimatedLines: number }[];
  newlyDeadFiles: string[];
  cascade: { symbols: number; files: number; lines: number };
  batches: { id: string; symbols: number; files: number; lines: number }[];
  incomparable: unknown;
  diagnostics: Diagnostic[];
}

export interface DeletionPlan {
  schemaVersion: "1.0";
  id: string;
  deltaHash: string;
  base: { ref: string | null; commit: string };
  head: { commit: string; dirty: boolean };
  totals: { symbols: number; files: number; lines: number };
  batches: { id: string; symbols: string[]; files: string[]; lines: number; patch: string }[];
  blocked: { symbol: string; reason: string }[];
  policy?: { entryPolicy: string; maxSymbols: number; maxFiles: number; allowOversized: boolean };
  diagnostics?: Diagnostic[];
}

export interface BuildEvidence {
  source: string;
  version: string;
  files: string[];
  symbols: string[];
  routes: { route: string; files: string[]; symbols: string[]; runtime?: string }[];
  sizes: Record<string, number>;
  snapshotHash: string;
  layers: Record<string, string>;
  moduleIds: Record<string, string>;
  clientReferences: { from: string; to: string; kind: string }[];
  fileHashes: Record<string, string>;
  unknownRecords: number;
  status?: string;
  clientBoundaryLeaks?: boolean;
}

export interface BuildJoin {
  status: string;
  joined: number;
  total: number;
  coverage: number;
  unmatched: string[];
  routes: { route: string; files: number; symbols: number; bytes: number }[];
  bytes: { matched: number; shared: number; totalRoute: number };
  diagnostics: { code: string; severity: string; message: string }[];
}

export interface ScanCandidate {
  symbol: string;
  file: string;
  kind: string;
  lines: number;
  reason: string;
  incoming?: number;
  outgoing?: number;
}

export interface ScanResult {
  snapshot: GraphSnapshot;
  candidates: ScanCandidate[];
  diagnostics?: Diagnostic[];
}

export type EngineCommand =
  | "ping"
  | "version"
  | "analyze"
  | "scan"
  | "delta"
  | "plan"
  | "proof"
  | "join-build"
  | "augment-build";

/** Mirror of `shrike_protocol::COMMANDS`. Keep in sync with the Rust constant
 * and the `command` enum in `schemas/protocol.schema.json`; the command
 * contract test fails when one side drifts. */
export const ENGINE_COMMANDS = [
  "ping",
  "version",
  "analyze",
  "scan",
  "delta",
  "plan",
  "proof",
  "join-build",
  "augment-build",
] as const satisfies readonly EngineCommand[];

export interface EngineRequest<T = unknown> {
  protocolVersion: "1.0";
  id: string;
  command: EngineCommand | string;
  payload: T;
}

export interface EngineError {
  code: string;
  message: string;
  details?: unknown;
}

export type EngineResponse<T = unknown> =
  | { protocolVersion: "1.0"; id: string; ok: true; result: T; error?: null }
  | { protocolVersion: "1.0"; id: string; ok: false; result?: null; error: EngineError };

/** Named extension slot for forward-compatible fields, not an index signature. */
export interface Extensible {
  extensions?: Record<string, unknown>;
}
