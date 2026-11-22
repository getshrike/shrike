import { createHash } from "node:crypto";

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
