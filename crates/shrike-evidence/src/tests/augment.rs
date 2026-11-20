use super::*;

#[test]
fn build_liveness_follows_known_edges() {
    let mut snapshot = snapshot();
    snapshot.live = vec!["app/page.tsx#default".into()];
    snapshot.candidates = vec!["lib/format.ts#formatInvoice".into()];
    snapshot.symbols.push(SymbolRecord {
        key: "app/page.tsx#default".into(),
        name: "default".into(),
        kind: "function".into(),
        file: "app/page.tsx".into(),
        span: Span {
            start: 0,
            end: 10,
            line: 1,
            column: 0,
            end_line: 1,
        },
        exported: true,
        entry: true,
        public_api: false,
        generated: false,
        type_only: false,
        lines: 1,
    });
    snapshot.edges.push(shrike_graph::EdgeRecord {
        source: "app/page.tsx#default".into(),
        target: "lib/format.ts#formatInvoice".into(),
        kind: "reference".into(),
        certainty: "certain".into(),
        file: "app/page.tsx".into(),
        span: Span {
            start: 0,
            end: 10,
            line: 1,
            column: 0,
            end_line: 1,
        },
    });
    let evidence = BuildEvidence {
        version: "16.4.0".into(),
        files: vec!["app/page.tsx".into()],
        ..BuildEvidence::default()
    };
    let join = join_build(&snapshot, &evidence);
    assert_eq!(join.status, "full");
    let augmented = augment_snapshot(&snapshot, &evidence, &join);
    assert!(augmented.is_live("lib/format.ts#formatInvoice"));
    assert!(!augmented.is_candidate("lib/format.ts#formatInvoice"));
    assert!(augmented.proofs.is_empty());
    assert!(
        augmented
            .witnesses
            .iter()
            .any(|item| item.symbol == "lib/format.ts#formatInvoice")
    );
    assert_ne!(augmented.graph_hash, snapshot.graph_hash);
}

#[test]
fn low_coverage_evidence_does_not_augment() {
    let mut snapshot = snapshot();
    snapshot.live = vec!["app/page.tsx#default".into()];
    snapshot.candidates = vec!["lib/format.ts#formatInvoice".into()];
    let evidence = BuildEvidence {
        version: "16.4.0".into(),
        files: vec!["missing.tsx".into()],
        ..BuildEvidence::default()
    };
    let join = join_build(&snapshot, &evidence);
    assert_eq!(join.status, "low-coverage");
    let augmented = augment_snapshot(&snapshot, &evidence, &join);
    assert_eq!(augmented.live, snapshot.live);
    assert_eq!(augmented.candidates, snapshot.candidates);
}

#[test]
fn client_reference_creates_a_build_edge_and_liveness() {
    let snapshot = candidate_snapshot();
    let evidence = BuildEvidence {
        version: "16.4.0".into(),
        files: vec!["app/page.tsx".into(), "lib/format.ts".into()],
        client_references: vec![ClientReference {
            from: "app/page.tsx".into(),
            to: "lib/format.ts#formatInvoice".into(),
            kind: "server-action".into(),
        }],
        ..BuildEvidence::default()
    };
    let join = join_build(&snapshot, &evidence);
    assert_eq!(join.status, "full");
    assert!(
        join.edges
            .iter()
            .any(|edge| edge.to == "lib/format.ts#formatInvoice")
    );
    let augmented = augment_snapshot(&snapshot, &evidence, &join);
    assert!(augmented.is_live("lib/format.ts#formatInvoice"));
    assert!(!augmented.is_candidate("lib/format.ts#formatInvoice"));
    assert!(
        augmented
            .edges
            .iter()
            .any(|edge| { edge.kind == "build" && edge.target == "lib/format.ts#formatInvoice" })
    );
}
