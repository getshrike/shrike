/** Bounded agent-facing cleanup brief (see docs/architecture.md). */

export interface BriefTrigger {
  file: string;
  kind?: string;
  change?: string;
}

export interface BriefDeltaItem {
  symbol: string;
  file: string;
  kind?: string;
  estimatedLines?: number;
  trigger?: BriefTrigger;
}

export interface BriefBlockedItem {
  symbol?: string;
  file?: string;
  reason: string;
}

export interface BriefSuggestedBatch {
  id?: string;
  symbols: string[];
  files: string[];
  lines?: number;
}

export interface CleanupBriefLimits {
  maxItems: number;
  maxBytes: number;
}

export interface CleanupBriefCommands {
  plan: string;
  prune: string;
  proof: string;
}

export interface CleanupBrief {
  schemaVersion: "1.0";
  kind: "cleanup-brief";
  /**
   * Repository-derived strings are untrusted data. This line is part of the
   * prompt-injection mitigation, not a guarantee.
   */
  notice: string;
  repo: string;
  base: string;
  head: string;
  dirty: boolean;
  newlyDead: number;
  files: number;
  untouchedFiles: number;
  estimatedLines: number;
  checks: string;
  preexisting: number;
  top: BriefDeltaItem[];
  blocked: BriefBlockedItem[];
  suggestedBatch: BriefSuggestedBatch | null;
  commands: CleanupBriefCommands;
  limits: CleanupBriefLimits;
  nextCommand: string;
  truncated: boolean;
}

export interface CleanupBriefInput {
  repo: string;
  base: string;
  head: string;
  dirty: boolean;
  /** Raw delta result from the engine. Treated as untrusted data. */
  delta: unknown;
  checks?: string;
  preexisting?: number;
  changedFiles?: string[];
  blocked?: BriefBlockedItem[];
  suggestedBatch?: BriefSuggestedBatch | null;
  maxItems?: number;
  maxBytes?: number;
}
