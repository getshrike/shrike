import test from "node:test";
import assert from "node:assert/strict";

import { pullRequestBase } from "../src/git.ts";

const remotes = ["origin", "upstream"];

test("pullRequestBase strips a configured remote from a tracking ref", () => {
  assert.equal(pullRequestBase("origin/main", remotes), "main");
  assert.equal(pullRequestBase("upstream/release/1.0", remotes), "release/1.0");
});

test("pullRequestBase keeps a local branch name unchanged", () => {
  assert.equal(pullRequestBase("main", remotes), "main");
  assert.equal(pullRequestBase("release/1.0", remotes), "release/1.0");
});

test("pullRequestBase keeps a branch whose prefix is not a remote", () => {
  assert.equal(pullRequestBase("release/1.0", ["origin"]), "release/1.0");
});
