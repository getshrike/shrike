#![forbid(unsafe_code)]
//! Core primitives shared by every Shrike crate.
//!
//! The crate owns the stable identifiers, spans, hashing, glob matching,
//! diagnostics, errors, and the normalized engine configuration. It has no
//! dependency on any other Shrike crate and performs no I/O.

pub mod config;
pub mod diagnostic;
pub mod error;
pub mod glob;
pub mod hash;
pub mod ids;
pub mod span;

pub use config::{
    BatchPolicy, ClientBoundaryPolicy, EngineConfig, EntryPolicy, normalize_relative,
};
pub use diagnostic::{Diagnostic, Severity};
pub use error::{ShrikeError, ShrikeResult};
pub use hash::{content_hash, hash_bytes, hash_json, short_hash};
pub use ids::{EdgeId, EntryId, FileId, ModuleId, SccId, SymbolId};
pub use span::{LineIndex, Span};

/// Product version reported by the engine and written into receipts.
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Engine/host wire protocol. A breaking change bumps this value.
pub const PROTOCOL_VERSION: &str = "1.0";
/// Version of the framework entry adapters. A change invalidates deltas.
pub const ENTRY_ADAPTER_VERSION: &str = "1.0";
/// Version of the serialized `GraphSnapshot` model.
pub const SNAPSHOT_SCHEMA_VERSION: &str = "1.0";
/// Version of the receipt schema written by the host.
pub const RECEIPT_SCHEMA_VERSION: &str = "1.0";
/// Cache layout version. Bumping it discards old cache entries.
pub const CACHE_VERSION: &str = "1";
