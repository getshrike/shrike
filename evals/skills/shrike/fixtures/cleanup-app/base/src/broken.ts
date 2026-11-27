// Unresolved import: the fixture intentionally references a missing module so
// the report carries a diagnostic. It is not a deletion candidate.
import { missingHelper } from "./missing-module";

export function brokenHelper(): string {
  return String(missingHelper);
}
