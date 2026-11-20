import type { BuildEvidence } from "./build.ts";
import type { VerificationTier } from "./config.ts";

export interface Snapshot {
  files: { path: string; contentHash: string; lines: number }[];
  symbols: { key: string; file: string; kind: string; span: { start: number; end: number; line: number; endLine?: number }; lines: number; publicApi: boolean; entry?: boolean }[];
  modules?: { path: string; file?: string; specifiers: string[]; resolved?: string[]; unresolved?: string[]; external?: string[]; typeRefs?: string[] }[];
  entries?: { symbol: string; framework: string; kind: string; route?: string | null }[];
  edges?: { source: string; target: string; kind: string; certainty?: string }[];
  proofs?: { symbol: string; reason: string; incoming: string[]; outgoing: string[] }[];
  diagnostics?: { code: string; severity: string; message: string; file?: string }[];
  candidates: string[];
  live?: string[];
  internalOnly?: string[];
  graphHash: string;
  [key: string]: unknown;
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
  snapshot: Snapshot;
  candidates: ScanCandidate[];
  diagnostics?: Snapshot["diagnostics"];
  [key: string]: unknown;
}
export interface DeltaReport {
  base: { ref: string | null; commit: string };
  head: { commit: string; dirty: boolean };
  newlyDead: { symbol: string; file: string; estimatedLines: number }[];
  newlyDeadFiles: string[];
  cascade: { symbols: number; files: number; lines: number };
  batches: { id: string; symbols: number; files: number; lines: number }[];
  incomparable: unknown;
  [key: string]: unknown;
}
export interface Plan {
  id: string;
  deltaHash: string;
  base: { ref: string | null; commit: string };
  head: { commit: string; dirty: boolean };
  totals: { symbols: number; files: number; lines: number };
  batches: { id: string; symbols: string[]; files: string[]; lines: number; patch: string }[];
  blocked: { symbol: string; reason: string }[];
  policy?: { entryPolicy: string; maxSymbols: number; maxFiles: number; allowOversized: boolean };
  [key: string]: unknown;
}

export interface BuildJoinResult {
  status: string;
  joined: number;
  total: number;
  coverage: number;
  unmatched: string[];
  routes: { route: string; files: number; symbols: number; bytes: number }[];
  bytes: { matched: number; shared: number; totalRoute: number };
  diagnostics: { code: string; severity: string; message: string }[];
}

export interface BuildEvidenceInfo {
  evidence: BuildEvidence;
  join: BuildJoinResult;
  path: string | null;
}
