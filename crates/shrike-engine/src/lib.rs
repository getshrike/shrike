#![forbid(unsafe_code)]
//! Command dispatch shared by the binary and tests.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use shrike_core::{
    EngineConfig, PROTOCOL_VERSION, SNAPSHOT_SCHEMA_VERSION, ShrikeError, ShrikeResult,
    TOOL_VERSION, hash_bytes,
};
use shrike_protocol::Request;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheManifest {
    version: u32,
    entries: BTreeMap<String, ManifestEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestEntry {
    size: u64,
    mtime_ns: u128,
    hash: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnalyzePayload {
    root: String,
    #[serde(default)]
    config: EngineConfig,
    #[serde(default)]
    cache_dir: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeltaPayload {
    base_snapshot: shrike_graph::GraphSnapshot,
    head_snapshot: shrike_graph::GraphSnapshot,
    #[serde(default)]
    base_ref: Option<String>,
    base_commit: String,
    head_commit: String,
    #[serde(default)]
    dirty: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanPayload {
    snapshot: shrike_graph::GraphSnapshot,
    delta: shrike_delta::DeltaReport,
    #[serde(default)]
    config: EngineConfig,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProofPayload {
    snapshot: shrike_graph::GraphSnapshot,
    symbol: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildPayload {
    snapshot: shrike_graph::GraphSnapshot,
    build: shrike_evidence::BuildEvidence,
}

pub fn dispatch(request: &Request) -> ShrikeResult<Value> {
    match request.command.as_str() {
        "ping" | "version" => {
            Ok(json!({"version": TOOL_VERSION, "protocolVersion": PROTOCOL_VERSION}))
        }
        "analyze" => {
            let payload: AnalyzePayload = parse(&request.payload)?;
            let snapshot = analyze_payload(&payload)?;
            Ok(json!({"snapshot": snapshot, "diagnostics": snapshot.diagnostics}))
        }
        "scan" => {
            let payload: AnalyzePayload = parse(&request.payload)?;
            let snapshot = analyze_payload(&payload)?;
            let candidates: Vec<Value> = snapshot.candidates.iter().filter_map(|key| {
                let symbol = snapshot.symbol(key)?;
                Some(json!({"symbol": key, "file": symbol.file, "kind": symbol.kind, "lines": symbol.lines, "reason": "no witness from any entry"}))
            }).collect();
            Ok(
                json!({"snapshot": snapshot, "diagnostics": snapshot.diagnostics, "candidates": candidates}),
            )
        }
        "delta" => {
            let payload: DeltaPayload = parse(&request.payload)?;
            let report = shrike_delta::compute_delta(
                &payload.base_snapshot,
                &payload.head_snapshot,
                payload.base_ref,
                payload.base_commit,
                payload.head_commit,
                payload.dirty,
            );
            Ok(serde_json::to_value(report)?)
        }
        "plan" => {
            let payload: PlanPayload = parse(&request.payload)?;
            let plan = shrike_plan::build_plan(&payload.snapshot, &payload.delta, &payload.config);
            Ok(serde_json::to_value(plan)?)
        }
        "proof" => {
            let payload: ProofPayload = parse(&request.payload)?;
            let witness = payload
                .snapshot
                .witnesses
                .iter()
                .find(|w| w.symbol == payload.symbol);
            let proof = payload
                .snapshot
                .proofs
                .iter()
                .find(|p| p.symbol == payload.symbol);
            Ok(json!({
                "symbol": payload.symbol,
                "live": payload.snapshot.is_live(&payload.symbol),
                "witness": witness.map_or_else(Vec::new, |w| w.path.clone()),
                "trace": proof.map_or_else(Vec::new, |p| vec![json!({"reason": p.reason, "incoming": p.incoming, "outgoing": p.outgoing})]),
            }))
        }
        "join-build" => {
            let payload: BuildPayload = parse(&request.payload)?;
            Ok(serde_json::to_value(shrike_evidence::join_build(
                &payload.snapshot,
                &payload.build,
            ))?)
        }
        "augment-build" => {
            let payload: BuildPayload = parse(&request.payload)?;
            let join = shrike_evidence::join_build(&payload.snapshot, &payload.build);
            let snapshot =
                shrike_evidence::augment_snapshot(&payload.snapshot, &payload.build, &join);
            Ok(json!({"snapshot": snapshot, "join": join}))
        }
        other => Err(ShrikeError::UnknownCommand(other.to_string())),
    }
}

fn analyze_payload(payload: &AnalyzePayload) -> ShrikeResult<shrike_graph::GraphSnapshot> {
    let timing = std::env::var_os("SHRIKE_TIMING").is_some();
    let started = std::time::Instant::now();
    payload.config.validate().map_err(ShrikeError::Config)?;
    let root = Path::new(&payload.root);
    let files = shrike_graph::discover_files(root, &payload.config);
    let discovered = started.elapsed();
    let cache_dir = payload
        .cache_dir
        .as_ref()
        .map_or_else(|| root.join(".shrike").join("cache"), PathBuf::from);
    let manifest_path = cache_dir.join("manifest.json");
    let manifest = load_manifest(&manifest_path);

    let mut entries = BTreeMap::new();
    let mut sources: BTreeMap<String, String> = BTreeMap::new();
    let scanned_entries: Vec<(String, Option<ManifestEntry>, Option<String>)> = files
        .par_iter()
        .map(|path| {
            let full = root.join(path);
            let Ok(metadata) = fs::metadata(&full) else {
                return (path.clone(), None, None);
            };
            let size = metadata.len();
            let mtime_ns = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |value| value.as_nanos());
            if let Some(entry) = manifest.entries.get(path) {
                if entry.size == size && entry.mtime_ns == mtime_ns {
                    return (path.clone(), Some(entry.clone()), None);
                }
            }
            match fs::read_to_string(&full) {
                Ok(source) => {
                    let hash = hash_bytes(source.as_bytes());
                    (
                        path.clone(),
                        Some(ManifestEntry {
                            size,
                            mtime_ns,
                            hash,
                        }),
                        Some(source),
                    )
                }
                Err(_) => (path.clone(), None, None),
            }
        })
        .collect();
    for (path, entry, source) in scanned_entries {
        if let Some(entry) = entry {
            entries.insert(path.clone(), entry);
        }
        if let Some(source) = source {
            sources.insert(path, source);
        }
    }
    let scanned = started.elapsed();
    let cache_key = snapshot_cache_key(&payload.config, &entries);
    let cache_path = cache_dir
        .join("snapshots")
        .join(format!("{}.json", cache_key.trim_start_matches("blake3:")));
    if let Ok(bytes) = fs::read(&cache_path) {
        if let Ok(snapshot) = serde_json::from_slice::<shrike_graph::GraphSnapshot>(&bytes) {
            if snapshot.schema_version == SNAPSHOT_SCHEMA_VERSION {
                let _ = write_manifest(
                    &manifest_path,
                    &CacheManifest {
                        version: 1,
                        entries,
                    },
                );
                if timing {
                    eprintln!(
                        "shrike timing: discover={}ms scan={}ms cache=hit total={}ms",
                        discovered.as_millis(),
                        (scanned - discovered).as_millis(),
                        started.elapsed().as_millis()
                    );
                }
                return Ok(snapshot);
            }
        }
    }
    for path in &files {
        if !sources.contains_key(path) {
            if let Ok(source) = fs::read_to_string(root.join(path)) {
                sources.insert(path.clone(), source);
            }
        }
    }
    let source_list: Vec<(String, String)> = files
        .iter()
        .filter_map(|path| sources.remove(path).map(|source| (path.clone(), source)))
        .collect();
    let read = started.elapsed();
    let summaries: Vec<_> = source_list
        .par_iter()
        .map(|(path, source)| shrike_syntax::parse_source(path, source))
        .collect();
    let parsed = started.elapsed();
    let snapshot = shrike_graph::build_snapshot(root, summaries, &payload.config);
    let built = started.elapsed();
    // Cache is an optimization. A read-only checkout still analyzes.
    let _ = write_cache(&cache_path, &snapshot);
    let _ = write_manifest(
        &manifest_path,
        &CacheManifest {
            version: 1,
            entries,
        },
    );
    if timing {
        eprintln!(
            "shrike timing: discover={}ms scan={}ms read={}ms parse={}ms build={}ms cache_write={}ms total={}ms",
            discovered.as_millis(),
            (scanned - discovered).as_millis(),
            (read - scanned).as_millis(),
            (parsed - read).as_millis(),
            (built - parsed).as_millis(),
            started.elapsed().as_millis() - built.as_millis(),
            started.elapsed().as_millis()
        );
    }
    Ok(snapshot)
}

fn load_manifest(path: &Path) -> CacheManifest {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<CacheManifest>(&bytes).ok())
        .filter(|manifest| manifest.version == 1)
        .unwrap_or(CacheManifest {
            version: 1,
            entries: BTreeMap::new(),
        })
}

fn snapshot_cache_key(config: &EngineConfig, entries: &BTreeMap<String, ManifestEntry>) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(TOOL_VERSION.as_bytes());
    hasher.update(b"\0");
    hasher.update(PROTOCOL_VERSION.as_bytes());
    hasher.update(b"\0");
    hasher.update(SNAPSHOT_SCHEMA_VERSION.as_bytes());
    hasher.update(b"\0");
    hasher.update(config.config_hash().as_bytes());
    hasher.update(b"\0");
    for (path, entry) in entries {
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
        hasher.update(entry.hash.as_bytes());
        hasher.update(b"\0");
    }
    format!("blake3:{}", hasher.finalize().to_hex())
}

fn write_cache(path: &Path, snapshot: &shrike_graph::GraphSnapshot) -> ShrikeResult<()> {
    atomic_write(path, &serde_json::to_vec(snapshot)?)
}

fn write_manifest(path: &Path, manifest: &CacheManifest) -> ShrikeResult<()> {
    atomic_write(path, &serde_json::to_vec(manifest)?)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> ShrikeResult<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("cache"),
        std::process::id()
    ));
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

fn parse<T: serde::de::DeserializeOwned>(value: &Value) -> ShrikeResult<T> {
    serde_json::from_value(value.clone()).map_err(|error| ShrikeError::Protocol(error.to_string()))
}
