#![forbid(unsafe_code)]
//! Deterministic whole-graph construction, entry detection, reachability with
//! witnesses, SCCs, and candidate proofs.

mod builder;
mod config;
mod discovery;
mod types;
mod util;

pub use crate::builder::{build_snapshot, build_snapshot_with_resolver};
pub use crate::discovery::{Discovery, analyze_paths, discover, discover_files, input_hash};
pub use crate::types::{
    EdgeRecord, EntryRecord, FileRecord, GraphSnapshot, ModuleRecord, ProofRecord, SymbolRecord,
    UnknownRecord, WitnessRecord, unused_files,
};
pub use crate::util::{rehash_snapshot, snapshot_json};
