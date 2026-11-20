#![forbid(unsafe_code)]
//! The versioned, line-delimited JSON contract between the TypeScript host and
//! the Rust engine.
//!
//! One request per line, one response per line. A response is either a complete
//! success (`ok: true`, non-null `result`, null `error`) or a complete failure
//! (`ok: false`, null `result`, non-null `error`). A partial response is a
//! protocol error and fails closed.

mod codec;
mod types;

#[cfg(test)]
mod tests;

pub use shrike_core::PROTOCOL_VERSION;

pub use crate::codec::{decode_request, encode_line};
pub use crate::types::{COMMANDS, ProtocolError, Request, Response};
