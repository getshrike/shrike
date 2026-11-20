/**
 * Build-graph evidence loading.
 *
 * Next.js 16.4 writes `next analyze export` as JSON Lines. The JSONL path is
 * streamed in bounded chunks; older or hand-made evidence may be one JSON
 * object and is read whole. Both are normalized into the small shape understood
 * by the Rust `join-build` command. A malformed record is an explicit error,
 * never a silent skip.
 *
 * Implementation lives in `build/`: types, path normalization, record helpers,
 * the evidence builder, stream/normalize readers, and the loader.
 */
export type {
  BuildStatus,
  BuildRouteEvidence,
  BuildClientReference,
  BuildEvidence,
  BuildEvidenceOptions,
  LoadedBuildEvidence,
} from "./build/types.ts";
export { BuildEvidenceError, EMPTY_BUILD_HASH } from "./build/types.ts";
export { packageNextVersion, discoverBuildEvidence } from "./build/records.ts";
export { normalizeBuildEvidence, streamBuildEvidence } from "./build/normalize.ts";
export { loadBuildEvidence } from "./build/load.ts";
