/**
 * Deterministic bisection over independent deletion units.
 *
 * The host may use this only when it can produce a patch for an arbitrary
 * subset. Plan batches currently arrive as one patch, so the CLI rejects a
 * whole failed batch and records the reason; this helper exists so the
 * verifier contract is implemented and testable independently.
 *
 * Implementation lives in `bisect/`: shared types and the search algorithm.
 */
export * from "./bisect/types.ts";
export * from "./bisect/search.ts";
