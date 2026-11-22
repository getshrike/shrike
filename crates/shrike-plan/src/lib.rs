#![forbid(unsafe_code)]
//! Deletion plan construction: independent, bounded batches with blocked items
//! and a content-addressed plan id.

mod build;
mod types;

#[cfg(test)]
mod tests;

pub use crate::build::build_plan;
pub use crate::types::{BlockedItem, DeletionPlan, PlanBatch, PlanPolicy, PlanTotals};
