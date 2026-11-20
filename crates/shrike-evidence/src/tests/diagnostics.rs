use super::*;

#[test]
fn client_boundary_leak_is_a_diagnostic_and_can_be_disabled() {
    let evidence = BuildEvidence {
        version: "16.4.0".into(),
        files: vec!["lib/server.ts".into()],
        layers: BTreeMap::from([("lib/server.ts".to_string(), "server".to_string())]),
        routes: vec![BuildRoute {
            route: "/".into(),
            files: vec!["lib/server.ts".into()],
            symbols: Vec::new(),
            runtime: Some("client".into()),
        }],
        ..BuildEvidence::default()
    };
    let snapshot = snapshot();
    let join = join_build(&snapshot, &evidence);
    assert!(
        join.diagnostics
            .iter()
            .any(|item| item.code == "client-boundary-leak")
    );
    let disabled = BuildEvidence {
        client_boundary_leaks: Some(false),
        ..evidence
    };
    let join = join_build(&snapshot, &disabled);
    assert!(
        !join
            .diagnostics
            .iter()
            .any(|item| item.code == "client-boundary-leak")
    );
}

#[test]
fn stale_and_build_failed_statuses_are_explicit() {
    let evidence = BuildEvidence {
        version: "16.4.0".into(),
        files: vec!["app/page.tsx".into()],
        status: Some("stale".into()),
        ..BuildEvidence::default()
    };
    let join = join_build(&snapshot(), &evidence);
    assert_eq!(join.status, "stale");
    assert!(
        join.diagnostics
            .iter()
            .any(|item| item.code == "build_stale")
    );

    let failed = BuildEvidence {
        status: Some("build-failed".into()),
        ..BuildEvidence::default()
    };
    let join = join_build(&snapshot(), &failed);
    assert_eq!(join.status, "build-failed");
    assert!(
        join.diagnostics
            .iter()
            .any(|item| item.code == "build_failed")
    );
}
