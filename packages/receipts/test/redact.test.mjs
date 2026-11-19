import test from "node:test";
import assert from "node:assert/strict";
import { redactCommand, redactText } from "../src/redact.ts";

test("redaction removes home, repository, ANSI, and synthetic secrets", () => {
  const root = "/home/partner/work/storefront";
  const input = [
    "\u001b[31m/home/partner/work/storefront/app/page.tsx\u001b[0m",
    "GH_TOKEN=ghp_1234567890abcdefghijklmnopqrstuvwxyz",
    "Authorization: Bearer abcdefghijklmnop",
    "AWS AKIAIOSFODNN7EXAMPLE",
    "run --token sk-1234567890abcdefghijklmnop",
  ].join("\n");

  const output = redactText(input, { root, homeDir: "/home/partner" });
  assert.equal(output.includes("/home/partner"), false);
  assert.equal(output.includes("storefront/app"), false);
  assert.equal(output.includes("ghp_"), false);
  assert.equal(output.includes("Bearer abc"), false);
  assert.equal(output.includes("AKIAIOSFODNN7EXAMPLE"), false);
  assert.equal(output.includes("sk-1234567890"), false);
  assert.equal(output.includes("\u001b"), false);
  assert.match(output, /<repo>\/app\/page\.tsx/);
});

test("redactCommand preserves the command shape for receipts", () => {
  const command = "pnpm --dir /home/partner/app test --token ghp_1234567890abcdefghijklmnop";
  const redacted = redactCommand(command, { root: "/home/partner/app", homeDir: "/home/partner" });
  assert.equal(redacted, "pnpm --dir <repo> test --token [redacted]");
});
