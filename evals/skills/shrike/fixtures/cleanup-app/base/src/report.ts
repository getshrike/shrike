// `zod` is imported but not declared in package.json: an unlisted dependency.
import { z } from "zod";

export function buildReport() {
  return z.object({ ok: z.boolean() });
}
