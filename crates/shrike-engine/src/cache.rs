use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use shrike_core::{
    EngineConfig, PROTOCOL_VERSION, SNAPSHOT_SCHEMA_VERSION, ShrikeResult, TOOL_VERSION,
};

use crate::payloads::{CacheManifest, ManifestEntry};

// Bump whenever graph construction, entry detection, edge resolution, or
// reachability semantics change. The tool version alone is not enough: a local
// rebuild at the same version must not reuse snapshots built by older logic.
const ANALYSIS_VERSION: &str = "73";

pub(crate) fn load_manifest(path: &Path) -> CacheManifest {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<CacheManifest>(&bytes).ok())
        .filter(|manifest| manifest.version == 1)
        .unwrap_or(CacheManifest {
            version: 1,
            entries: BTreeMap::new(),
        })
}

pub(crate) fn snapshot_cache_key(
    config: &EngineConfig,
    resolver_hash: &str,
    entries: &BTreeMap<String, ManifestEntry>,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(ANALYSIS_VERSION.as_bytes());
    hasher.update(b"\0");
    hasher.update(TOOL_VERSION.as_bytes());
    hasher.update(b"\0");
    hasher.update(PROTOCOL_VERSION.as_bytes());
    hasher.update(b"\0");
    hasher.update(SNAPSHOT_SCHEMA_VERSION.as_bytes());
    hasher.update(b"\0");
    hasher.update(config.config_hash().as_bytes());
    hasher.update(b"\0");
    hasher.update(resolver_hash.as_bytes());
    hasher.update(b"\0");
    for (path, entry) in entries {
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
        hasher.update(entry.hash.as_bytes());
        hasher.update(b"\0");
    }
    format!("blake3:{}", hasher.finalize().to_hex())
}

pub(crate) fn write_cache(path: &Path, snapshot: &shrike_graph::GraphSnapshot) -> ShrikeResult<()> {
    atomic_write(path, &serde_json::to_vec(snapshot)?)
}

pub(crate) fn write_manifest(path: &Path, manifest: &CacheManifest) -> ShrikeResult<()> {
    atomic_write(path, &serde_json::to_vec(manifest)?)
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> ShrikeResult<()> {
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
