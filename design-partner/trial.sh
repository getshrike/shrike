#!/usr/bin/env bash
# Read-only design-partner trial: delta + plan, no edits, no commits, no PRs.
set -euo pipefail

REPO="${1:-$(pwd)}"
BASE="${2:-${SHRIKE_BASE:-main}}"
SHRIKE_ROOT="${SHRIKE_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
CLI="$SHRIKE_ROOT/packages/cli/src/main.ts"
ENGINE="${SHRIKE_ENGINE:-}"

if [[ -z "$ENGINE" ]]; then
  for candidate in "$SHRIKE_ROOT/target/release/shrike-engine" "$SHRIKE_ROOT/target/debug/shrike-engine"; do
    if [[ -x "$candidate" ]]; then
      ENGINE="$candidate"
      break
    fi
  done
fi

if [[ ! -d "$REPO/.git" ]]; then
  echo "shrike trial: $REPO is not a Git repository" >&2
  exit 2
fi
if [[ -z "$ENGINE" || ! -x "$ENGINE" ]]; then
  echo "shrike trial: build the engine with 'cargo build --release -p shrike-engine'" >&2
  echo "or set SHRIKE_ENGINE=/path/to/shrike-engine" >&2
  exit 2
fi

cd "$REPO"
mkdir -p .shrike/trial

status=0
SHRIKE_ENGINE="$ENGINE" node "$CLI" delta --base "$BASE" --json \
  > .shrike/trial/delta.json 2> .shrike/trial/delta.err || status=$?
if [[ "$status" -ne 0 && "$status" -ne 1 ]]; then
  cat .shrike/trial/delta.err >&2
  echo "shrike trial: delta failed with exit $status" >&2
  exit "$status"
fi

plan_status=0
SHRIKE_ENGINE="$ENGINE" node "$CLI" plan --base "$BASE" --json \
  > .shrike/trial/plan.json 2> .shrike/trial/plan.err || plan_status=$?
if [[ "$plan_status" -ne 0 && "$plan_status" -ne 1 ]]; then
  cat .shrike/trial/plan.err >&2
  echo "shrike trial: plan failed with exit $plan_status" >&2
  exit "$plan_status"
fi

node - .shrike/trial/delta.json .shrike/trial/plan.json <<'NODE'
const fs = require("node:fs");
const [deltaPath, planPath] = process.argv.slice(2);
const delta = JSON.parse(fs.readFileSync(deltaPath, "utf8"));
const plan = JSON.parse(fs.readFileSync(planPath, "utf8"));
const dead = delta.newlyDead ?? [];
const batches = plan.batches ?? [];
const blocked = plan.blocked ?? [];
console.log(`Shrike trial: ${dead.length} newly dead symbol(s), ${batches.length} batch(es), ${blocked.length} blocked item(s).`);
for (const item of dead.slice(0, 10)) console.log(`  ${item.symbol ?? item}`);
if (dead.length > 10) console.log(`  ... ${dead.length - 10} more in .shrike/trial/delta.json`);
console.log("Reports: .shrike/trial/delta.json, .shrike/trial/plan.json");
console.log("Nothing was edited. Review the reports before running prune.");
NODE

exit "$status"
