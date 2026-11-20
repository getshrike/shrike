/**
 * Structural validation for `schemas/receipt.schema.json`.
 *
 * This is intentionally a focused validator for the one schema Shrike writes.
 * It fails closed on unknown keys, missing required fields, bad enums, and
 * hash/id mismatches. The JSON Schema remains the published contract; the
 * validator exists so a receipt write cannot silently drift from it.
 *
 * Implementation lives in `validate/`: constants, primitive checks, section
 * validators, and the top-level receipt validation.
 */
export type { ReceiptValidation } from "./validate/constants.ts";
export { validateReceipt, asValidReceipt } from "./validate/receipt.ts";
