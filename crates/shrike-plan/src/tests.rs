use shrike_core::EngineConfig;

use super::*;

#[test]
fn empty_delta_yields_empty_plan() {
    let snapshot = shrike_graph::build_snapshot(
        std::path::Path::new("."),
        Vec::new(),
        &EngineConfig::default(),
    );
    let delta =
        shrike_delta::compute_delta(&snapshot, &snapshot, None, "a".into(), "b".into(), false);
    let plan = build_plan(&snapshot, &delta, &EngineConfig::default());
    assert_eq!(plan.totals.symbols, 0);
    assert!(plan.id.starts_with("plan_"));
}
