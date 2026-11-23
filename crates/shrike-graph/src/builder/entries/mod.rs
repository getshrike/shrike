//! Framework entry detection: orchestration over precomputed facts.

mod context;
mod detect;
mod helpers;
mod precompute;
#[cfg(test)]
mod tests;

use super::Builder;
use crate::types::EntryRecord;

pub(crate) use helpers::{NEXT_APP_ROOT_ONLY_STEMS, framework_module, next_convention_stem};
#[cfg(test)]
pub(crate) use helpers::{expo_config_plugin_paths, expo_router_root};

impl Builder<'_> {
    pub(super) fn detect_entries(&mut self) {
        let context = precompute::build(self);
        detect::run(&context, self);
    }
}

/// Mark a symbol as an entry and record why. Kept as a free function so
/// detectors never need mutable access to the whole `Builder`.
pub(super) fn add(
    builder: &mut Builder<'_>,
    symbol: String,
    framework: &str,
    kind: &str,
    route: Option<String>,
    runtime: &str,
) {
    if let Some(record) = builder
        .symbols
        .iter_mut()
        .find(|record| record.key == symbol)
    {
        record.entry = true;
        builder.entries.push(EntryRecord {
            symbol: symbol.clone(),
            framework: framework.to_string(),
            kind: kind.to_string(),
            route,
            runtime: runtime.to_string(),
            certainty: "certain".to_string(),
        });
    }
}
