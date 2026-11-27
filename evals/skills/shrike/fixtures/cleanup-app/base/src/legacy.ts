// Pre-existing dead code. It is unreachable in both the base and the feature
// branch, so it must not appear in the branch delta.
export function legacyFormatter(value: string): string {
  return `legacy:${value}`;
}
