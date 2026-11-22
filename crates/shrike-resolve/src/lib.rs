#![forbid(unsafe_code)]
//! Module resolution: relative paths, tsconfig `paths`/`baseUrl`, workspace
//! packages, package entry points, framework aliases, and extension probing.
//!
//! Resolution is pure: it is given the repository root and the canonical file
//! list, and never walks outside that list. Unknown bare specifiers are
//! classified as external rather than guessed.

mod config;
mod resolver;
mod types;
mod util;

#[cfg(test)]
mod tests;

pub use crate::types::{PackageInfo, ResolvedSpecifier, Resolver};
pub use crate::util::normalize_path;
