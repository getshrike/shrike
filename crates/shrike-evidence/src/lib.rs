#![forbid(unsafe_code)]
//! Build-graph evidence join. Build evidence can only upgrade confidence; it
//! never invents reachability.

mod augment;
mod diagnostics;
mod join;
mod resolve;
mod status;
mod types;
mod verdicts;

#[cfg(test)]
mod tests;

pub use crate::augment::augment_snapshot;
pub use crate::join::join_build;
pub use crate::types::{
    BuildBytes, BuildEdge, BuildEvidence, BuildJoin, BuildRoute, BuildRouteJoin, BuildVerdict,
    ClientReference,
};
