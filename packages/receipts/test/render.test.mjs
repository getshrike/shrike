import test from "node:test";
import assert from "node:assert/strict";
import { escapeCell, truncate } from "../src/render.ts";

test("escapeCell cannot break a Markdown table row", () => {
  assert.equal(escapeCell("a|b\nc"), "a\\|b c");
  assert.equal(escapeCell("  plain  "), "plain");
});

test("truncate bounds output and preserves surrogate pairs", () => {
  assert.equal(truncate("abcdef", 5), "abcd…");
  assert.equal(truncate("ab😀cd", 4), "ab😀…");
  assert.equal([...truncate("ab😀cd", 4)].length, 4);
  assert.equal(truncate("short", 10), "short");
});
