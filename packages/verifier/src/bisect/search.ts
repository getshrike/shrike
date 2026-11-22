import type { BisectOptions, BisectResult, BisectState, BisectUnit } from "./types.ts";

function findRoot(parent: number[], index: number): number {
  let current = index;
  while (parent[current] !== current) {
    current = parent[current] as number;
  }
  return current;
}

function union(parent: number[], left: number, right: number): void {
  const a = findRoot(parent, left);
  const b = findRoot(parent, right);
  if (a !== b) parent[b] = a;
}

/** Group units that share a file; a shared file cannot be split safely. */
export function clusterUnits<T extends BisectUnit>(units: T[]): T[][] {
  const parent = units.map((_, index) => index);
  for (let left = 0; left < units.length; left += 1) {
    const leftFiles = new Set(units[left]?.files ?? []);
    for (let right = left + 1; right < units.length; right += 1) {
      const rightFiles = units[right]?.files ?? [];
      if (rightFiles.some((file) => leftFiles.has(file))) {
        union(parent, left, right);
      }
    }
  }

  const groups = new Map<number, T[]>();
  for (let index = 0; index < units.length; index += 1) {
    const root = findRoot(parent, index);
    const unit = units[index];
    if (!unit) continue;
    const group = groups.get(root);
    if (group) group.push(unit);
    else groups.set(root, [unit]);
  }

  return [...groups.values()]
    .map((group) => group.sort((a, b) => a.id.localeCompare(b.id)))
    .sort((a, b) => (a[0]?.id ?? "").localeCompare(b[0]?.id ?? ""));
}

function splitClusters<T extends BisectUnit>(clusters: T[][]): [T[], T[]] {
  const left: T[] = [];
  const right: T[] = [];
  for (const cluster of clusters) {
    if (left.length <= right.length) left.push(...cluster);
    else right.push(...cluster);
  }
  return [left, right];
}

async function bisectSet<T extends BisectUnit>(
  units: T[],
  verify: (subset: T[]) => Promise<boolean>,
  state: BisectState,
): Promise<BisectResult<T>> {
  if (units.length <= 1) {
    return {
      status: "failed",
      rejected: units,
      passing: [],
      attempts: state.attempts,
      reason: "single unit",
    };
  }

  if (state.attempts >= state.maxAttempts) {
    return {
      status: "inconclusive",
      rejected: units,
      passing: [],
      attempts: state.attempts,
      reason: `bisection exceeded ${state.maxAttempts} attempts`,
    };
  }

  const clusters = clusterUnits(units);
  if (clusters.length <= 1) {
    return {
      status: "failed",
      rejected: units,
      passing: [],
      attempts: state.attempts,
      reason: "units share files and cannot be split safely",
    };
  }

  const [left, right] = splitClusters(clusters);
  if (left.length === 0 || right.length === 0) {
    return {
      status: "failed",
      rejected: units,
      passing: [],
      attempts: state.attempts,
      reason: "no independent split available",
    };
  }

  state.attempts += 1;
  const leftPasses = await verify(left);
  state.attempts += 1;
  const rightPasses = await verify(right);

  if (leftPasses && rightPasses) {
    return {
      status: "interaction-failure",
      rejected: units,
      passing: [],
      attempts: state.attempts,
      reason: "each half passes but the full batch fails",
    };
  }

  if (leftPasses && !rightPasses) {
    return bisectSet(right, verify, state);
  }
  if (!leftPasses && rightPasses) {
    return bisectSet(left, verify, state);
  }

  const leftResult = await bisectSet(left, verify, state);
  const rightResult = await bisectSet(right, verify, state);
  if (leftResult.status === "inconclusive" || rightResult.status === "inconclusive") {
    return {
      status: "inconclusive",
      rejected: units,
      passing: [],
      attempts: state.attempts,
      reason: "both halves failed and bisection was inconclusive",
    };
  }
  return {
    status: "failed",
    rejected: [...leftResult.rejected, ...rightResult.rejected],
    passing: [...leftResult.passing, ...rightResult.passing],
    attempts: state.attempts,
    reason: "multiple failing units",
  };
}

/**
 * Bisect a failing set. `verify` must return true when the subset passes.
 *
 * Sorting is by `id`, so the same failure produces the same rejection set.
 */
export async function bisectFailure<T extends BisectUnit>(
  units: T[],
  verify: (subset: T[]) => Promise<boolean>,
  options: BisectOptions = {},
): Promise<BisectResult<T>> {
  const sorted = [...units].sort((a, b) => a.id.localeCompare(b.id));
  const state: BisectState = {
    attempts: 0,
    maxAttempts: options.maxAttempts ?? 32,
  };

  if (sorted.length === 0) {
    return {
      status: "pass",
      rejected: [],
      passing: [],
      attempts: 0,
      reason: "empty batch",
    };
  }

  state.attempts += 1;
  const fullPasses = await verify(sorted);
  if (fullPasses) {
    return {
      status: "pass",
      rejected: [],
      passing: sorted,
      attempts: state.attempts,
      reason: "full batch passes",
    };
  }

  return bisectSet(sorted, verify, state);
}
