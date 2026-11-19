# Benchmarks

`generate.mjs` writes a deterministic synthetic TypeScript repository (default
10,000 files). `harness.mjs` runs `analyze` once and reports wall time, file
count, symbol count, and candidate count.

```bash
cargo build --release -p shrike-engine
node benchmarks/generate.mjs --out /tmp/shrike-bench --files 10000 --seed 42
node benchmarks/harness.mjs --files 10000
```

Budgets from MASTER_PROMPT.md: warm delta ≤ 20 s at 10k files; two snapshots
≤ 600 MB; deterministic output byte-for-byte. Results are environment-specific;
record the machine and commit in TRACKER.md before claiming a metric.
