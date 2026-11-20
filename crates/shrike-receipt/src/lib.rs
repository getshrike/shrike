#![forbid(unsafe_code)]
//! Content-addressed verification receipts.
//!
//! The id is SHA-256 over canonical JSON excluding `createdAt`. Signing is a
//! later feature; content addressing is the v1 integrity mechanism.

mod canonical;
mod types;

#[cfg(test)]
mod tests;

pub use crate::types::{
    BatchRef, BuildRef, CheckRecord, Environment, GraphRef, PlanRef, PolicyRecord, Receipt,
    Rejection, Tool,
};
