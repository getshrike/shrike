use std::collections::BTreeSet;

use super::*;

#[test]
fn batch_impact_uses_orphan_spans_not_file_lines() {
    let directory = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(directory.path().join("src")).expect("mkdir");
    std::fs::write(
        directory.path().join("src/a.ts"),
        "export const one = 1;\n\nexport const two = () => {\n  return 2;\n};\n",
    )
    .expect("write");
    let snapshot = shrike_graph::analyze_paths(
        directory.path(),
        &shrike_core::EngineConfig::default(),
        &["src/a.ts".to_string()],
    );
    let one = snapshot.symbol("src/a.ts#one").expect("symbol");
    let file = snapshot
        .files
        .iter()
        .find(|file| file.path == "src/a.ts")
        .expect("file");
    assert!(file.lines > one.lines);
    let batches =
        crate::cascade::partition(&snapshot, &BTreeSet::from(["src/a.ts#one".to_string()]));
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].lines, one.lines);
    assert_eq!(batches[0].symbols, 1);
    assert_eq!(batches[0].files, 1);
}

#[test]
fn partition_keeps_a_declaration_with_its_re_export() {
    let directory = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(directory.path().join("src")).expect("mkdir");
    std::fs::write(
        directory.path().join("src/a.ts"),
        "export function dead() { return 1; }\n",
    )
    .expect("write");
    std::fs::write(
        directory.path().join("src/index.ts"),
        "export { dead } from \"./a.ts\";\nexport const live = 1;\n",
    )
    .expect("write");
    let snapshot = shrike_graph::analyze_paths(
        directory.path(),
        &shrike_core::EngineConfig::default(),
        &["src/a.ts".to_string(), "src/index.ts".to_string()],
    );
    assert!(snapshot.symbol("src/index.ts#dead").is_some());
    let batches = crate::cascade::partition(
        &snapshot,
        &BTreeSet::from(["src/a.ts#dead".to_string(), "src/index.ts#dead".to_string()]),
    );
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].symbols, 2);
    assert_eq!(batches[0].files, 2);
}

#[test]
fn empty_snapshots_are_comparable_and_empty() {
    let snapshot = shrike_graph::build_snapshot(
        std::path::Path::new("."),
        Vec::new(),
        &shrike_core::EngineConfig::default(),
    );
    let delta = compute_delta(&snapshot, &snapshot, None, "a".into(), "b".into(), false);
    assert!(delta.is_empty());
    assert!(delta.incomparable.is_none());
}
