#![forbid(unsafe_code)]
//! Whole-graph orphan delta: newly dead symbols, lost witnesses, trigger
//! attribution, cascade closure, and batch partitioning.

mod attribute;
mod cascade;
mod compute;
mod types;

#[cfg(test)]
mod tests;

pub use crate::compute::compute_delta;
pub use crate::types::{
    BatchSummary, Cascade, CommitRef, DeltaItem, DeltaReport, HeadRef, Incomparable, Trigger,
};
