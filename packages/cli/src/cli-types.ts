/**
 * CLI-local projections of the shared protocol types.
 *
 * The canonical types live in `@shrike/protocol` and mirror `schemas/`. This
 * file only re-exports them under the names the CLI has always used and adds
 * the one shape that is CLI-local (the build-evidence path).
 */
import type { BuildEvidence, BuildJoin, DeletionPlan, Diagnostic, GraphSnapshot, ScanResult as ProtocolScanResult } from "@shrike/protocol";

export type {
  BuildJoin,
  DeletionPlan,
  Diagnostic,
  EdgeRecord,
  EntryRecord,
  EngineRequest,
  EngineResponse,
  FileRecord,
  GraphSnapshot,
  ModuleRecord,
  ProofRecord,
  ScanCandidate,
  SymbolRecord,
  UnknownRecord,
  WitnessRecord,
} from "@shrike/protocol";

export type Snapshot = GraphSnapshot;
export type DeltaReport = import("@shrike/protocol").DeltaReport;
export type Plan = DeletionPlan;
export type ScanResult = ProtocolScanResult;
export type BuildJoinResult = BuildJoin;

export interface BuildEvidenceInfo {
  evidence: BuildEvidence;
  join: BuildJoin;
  path: string | null;
}

export type { BuildEvidence };
