/**
 * The deterministic cleanup-report model.
 *
 * These types mirror `schemas/cleanup-report.schema.json`. The report is a
 * projection of engine facts (snapshot, delta, plan, dependencies, diagnostics)
 * and never introduces a deletion candidate of its own.
 */

export type ReportMode = "current" | "branch";
export type ReportClassification = "branch-orphan" | "never-used" | "advisory";
export type ReportItemKind = "symbol" | "file" | "dependency";
export type ReportReason =
  | "no-witness"
  | "whole-file"
  | "unused-dependency"
  | "unlisted-dependency"
  | "transitive-dependency";
export type ReportConfidence = "certain" | "probable" | "unknown";
export type TriggerKind =
  | "removed-import"
  | "removed-call"
  | "removed-jsx"
  | "removed-route"
  | "renamed-export"
  | "removed-re-export"
  | "changed-dynamic"
  | "indirect";
export type ProtectedReason =
  | "entry-role"
  | "public-api"
  | "unknown-edge"
  | "probable-edge"
  | "unparsed-file"
  | "generated-file"
  | "test-entry"
  | "external-referrer"
  | "blast-radius"
  | "workspace-boundary"
  | "policy"
  | "oversized"
  | "other";
export type DependencyReportKind = "unused" | "unlisted" | "transitive";

export interface ReportTrigger {
  file: string;
  kind: TriggerKind;
  change: string;
  certainty: "certain" | "probable" | "unknown";
  edge?: string;
}

export interface CleanupItem {
  id: string;
  kind: ReportItemKind;
  classification: ReportClassification;
  path: string;
  symbol?: string;
  name?: string;
  lines: number;
  reason: ReportReason;
  confidence: ReportConfidence;
  trigger?: ReportTrigger;
}

export interface ReportGroup {
  directory: string;
  removableLines: number;
  removableSymbols: number;
  removableFiles: number;
  items: CleanupItem[];
}

export interface ProtectedItem {
  path: string;
  symbol?: string;
  reason: ProtectedReason;
  detail?: string;
}

export interface CleanupDependency {
  package: string;
  directory: string;
  name: string;
  kind: DependencyReportKind;
  advisory: true;
}

export interface ReportDiagnosticGroup {
  category: string;
  code: string;
  count: number;
  actionable: boolean;
  files: string[];
}

export interface ReportDiagnostics {
  total: number;
  actionable: number;
  informational: number;
  groups: ReportDiagnosticGroup[];
}

export interface ReportSummary {
  analyzedFiles: number;
  analyzedLines: number;
  removableFiles: number;
  removableSymbols: number;
  removableExports: number;
  removableLines: number;
  deadWeightBasisPoints: number;
  unusedDependencies: number;
  protectedItems: number;
  uncountedRemovableSymbols: number;
  diagnostics: {
    total: number;
    actionable: number;
    informational: number;
  };
}

export interface PreexistingCounts {
  symbols: number;
  files: number;
  lines: number;
}

export interface ReportBase {
  ref: string | null;
  commit: string;
  graphHash?: string;
}

export interface ReportHead {
  commit: string;
  dirty: boolean;
  graphHash?: string;
}

export interface CleanupReport {
  schemaVersion: "1.0";
  mode: ReportMode;
  base: ReportBase | null;
  head: ReportHead;
  summary: ReportSummary;
  groups: ReportGroup[];
  preexisting?: PreexistingCounts;
  protected: ProtectedItem[];
  dependencies: CleanupDependency[];
  diagnostics: ReportDiagnostics;
}
