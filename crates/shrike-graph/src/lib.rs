#![forbid(unsafe_code)]
//! Deterministic whole-graph construction, entry detection, reachability with
//! witnesses, SCCs, and candidate proofs.

mod builder;
mod cosmos_config;
mod discovery;
mod next_config;
mod storybook_config;
mod types;
mod util;
mod webpack_config;

pub use crate::builder::{build_snapshot, build_snapshot_with_resolver};
pub use crate::discovery::{Discovery, analyze_paths, discover, discover_files, input_hash};
pub use crate::types::{
    EdgeRecord, EntryRecord, FileRecord, GraphSnapshot, ModuleRecord, ProofRecord, SymbolRecord,
    UnknownRecord, WitnessRecord,
};
pub use crate::util::rehash_snapshot;
