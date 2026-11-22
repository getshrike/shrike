#!/usr/bin/env bash
# Prepare the competitor benchmark corpus.
#
# Defaults match the report in this directory. Override with:
#   SHRIKE_BENCH_WORK=/path/to/work
#   SHRIKE_BENCH_NEXT_REPOS=/path/to/next/apps
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORK="${SHRIKE_BENCH_WORK:-/tmp/opencode/bench-v2}"
NEXT_REPOS="${SHRIKE_BENCH_NEXT_REPOS:-/tmp/opencode/bench-repos}"
TOOLS="$WORK/tools"

mkdir -p "$WORK/repos" "$WORK/synthetic" "$WORK/cache" "$WORK/results" "$TOOLS"

link_next_app() {
  local name="$1"
  if [ ! -d "$NEXT_REPOS/$name" ]; then
    echo "missing Next.js benchmark copy: $NEXT_REPOS/$name" >&2
    return 1
  fi
  ln -sfn "$NEXT_REPOS/$name" "$WORK/repos/$name"
}

for name in portfolio next-portfolio markalvarez accounteer requo; do
  link_next_app "$name"
done

clone() {
  local name="$1" tag="$2" url="$3"
  if [ -d "$WORK/repos/$name/.git" ]; then
    echo "$name: already cloned"
    return
  fi
  rm -rf "$WORK/repos/$name"
  echo "$name: cloning $url @ $tag"
  git clone --depth 1 --branch "$tag" "$url" "$WORK/repos/$name"
}

clone preact 10.25.4 https://github.com/preactjs/preact.git
clone fastify v5.2.1 https://github.com/fastify/fastify.git
clone zod v3.24.2 https://github.com/colinhacks/zod.git
clone query v5.90.3 https://github.com/TanStack/query.git
clone vite v8.0.1 https://github.com/vitejs/vite.git

install_npm() {
  local name="$1"
  if [ -d "$WORK/repos/$name/node_modules" ]; then
    echo "$name: dependencies present"
    return
  fi
  echo "$name: npm install"
  (cd "$WORK/repos/$name" && npm install --prefer-offline --ignore-scripts --no-audit --no-fund)
}

install_pnpm() {
  local name="$1"
  if [ -d "$WORK/repos/$name/node_modules" ]; then
    echo "$name: dependencies present"
    return
  fi
  echo "$name: pnpm install"
  # The pinned packageManager versions are newer than some Corepack caches and
  # require a registry round-trip; the local pnpm can install the same lockfile.
  (cd "$WORK/repos/$name" && \
    COREPACK_ENABLE_PROJECT_SPEC=0 pnpm install --prefer-offline --ignore-scripts \
      --no-frozen-lockfile --config.pmOnFail=ignore)
}

install_npm preact
install_npm fastify
install_npm zod
install_pnpm query
install_pnpm vite

for size in 1000 5000; do
  dir="$WORK/synthetic/${size%000}k"
  if [ -f "$dir/benchmarks.json" ]; then
    echo "synthetic ${size}: present"
  else
    echo "synthetic ${size}: generating"
    node "$ROOT/benchmarks/generate.mjs" --out "$dir" --files "$size" --seed 42
  fi
done

if [ ! -x "$TOOLS/node_modules/.bin/knip" ]; then
  echo "installing competitor tools"
  (cd "$TOOLS" && npm init -y >/dev/null 2>&1 && \
    npm install --ignore-scripts --no-audit --no-fund \
      knip@6.47.0 depcheck@1.4.7 ts-prune@0.10.3 ts-unused-exports@11.0.1 typescript@5.9.3)
else
  echo "competitor tools: present"
fi

echo
echo "Corpus ready under $WORK"
echo "Run: python3 benchmarks/competitors/bench.py --work $WORK"
