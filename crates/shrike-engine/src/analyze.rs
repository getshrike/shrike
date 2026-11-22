use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rayon::prelude::*;
use shrike_core::{SNAPSHOT_SCHEMA_VERSION, ShrikeError, ShrikeResult, hash_bytes};

use crate::cache::{load_manifest, snapshot_cache_key, write_cache_bytes, write_manifest};
use crate::payloads::{AnalyzePayload, CacheManifest, ManifestEntry};

/// A built snapshot plus its canonical JSON. The JSON is produced once and
/// reused for the cache file and the protocol response; serializing a
/// repository-sized snapshot is expensive enough to matter on every scan.
pub(crate) struct Analyzed {
    pub(crate) snapshot: shrike_graph::GraphSnapshot,
    pub(crate) snapshot_json: String,
}

impl Analyzed {
    fn new(snapshot: shrike_graph::GraphSnapshot) -> Self {
        let snapshot_json = shrike_graph::snapshot_json(&snapshot);
        Self {
            snapshot,
            snapshot_json,
        }
    }
}

pub(crate) fn analyze_payload(payload: &AnalyzePayload) -> ShrikeResult<Analyzed> {
    let timing = std::env::var_os("SHRIKE_TIMING").is_some();
    let started = std::time::Instant::now();
    payload.config.validate().map_err(ShrikeError::Config)?;
    let root = Path::new(&payload.root);
    let shrike_graph::Discovery {
        files,
        cache_inputs,
    } = shrike_graph::discover(root, &payload.config);
    let resolver = shrike_resolve::Resolver::new(root, &files, &payload.config);
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
    // Framework config that is not analyzed still changes entry detection.
    // Hash it into the cache key so a config edit cannot reuse a stale
    // snapshot. These files are never added to `files` or parsed.
    for path in &cache_inputs {
        if entries.contains_key(path) {
            continue;
        }
        let full = root.join(path);
        let Ok(metadata) = fs::metadata(&full) else {
            continue;
        };
        let Ok(bytes) = fs::read(&full) else {
            continue;
        };
        let mtime_ns = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |value| value.as_nanos());
        entries.insert(
            path.clone(),
            ManifestEntry {
                size: metadata.len(),
                mtime_ns,
                hash: hash_bytes(&bytes),
            },
        );
    }
    let scanned = started.elapsed();
    let cache_key = snapshot_cache_key(&payload.config, &resolver.resolver_hash, &entries);
    let cache_path = cache_dir
        .join("snapshots")
        .join(format!("{}.json", cache_key.trim_start_matches("blake3:")));
    if let Ok(bytes) = fs::read(&cache_path) {
        let cache_read = started.elapsed();
        if let Ok(snapshot) = serde_json::from_slice::<shrike_graph::GraphSnapshot>(&bytes) {
            let cache_parsed = started.elapsed();
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
                        "shrike timing: discover={}ms scan={}ms cache_read={}ms cache_parse={}ms cache=hit total={}ms",
                        discovered.as_millis(),
                        (scanned - discovered).as_millis(),
                        (cache_read - discovered).as_millis(),
                        (cache_parsed - cache_read).as_millis(),
                        started.elapsed().as_millis()
                    );
                }
                // The cache file is canonical snapshot JSON. Reuse it for the
                // response instead of re-serializing the snapshot.
                let snapshot_json = String::from_utf8(bytes)
                    .map_err(|error| ShrikeError::Protocol(error.to_string()))?;
                return Ok(Analyzed {
                    snapshot,
                    snapshot_json,
                });
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
    let snapshot = shrike_graph::build_snapshot_with_resolver(summaries, &payload.config, resolver);
    let built = started.elapsed();
    let analyzed = Analyzed::new(snapshot);
    // Cache is an optimization. A read-only checkout still analyzes.
    let _ = write_cache_bytes(&cache_path, analyzed.snapshot_json.as_bytes());
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
    Ok(analyzed)
}
