use super::*;

#[test]
fn joins_files_and_symbols() {
    let evidence = BuildEvidence {
        version: "16.4.0".into(),
        files: vec!["app/page.tsx".into()],
        symbols: vec!["lib/format.ts#formatInvoice".into()],
        ..BuildEvidence::default()
    };
    let join = join_build(&snapshot(), &evidence);
    assert_eq!(join.status, "full");
    assert_eq!(join.joined, 2);
    assert_eq!(join.total, 2);
    assert!((join.coverage - 1.0).abs() < f64::EPSILON);
}

#[test]
fn turbopack_prefixes_join_and_dependencies_are_excluded() {
    let evidence = BuildEvidence {
        version: "16.4.2".into(),
        files: vec![
            "[project]/app/page.tsx".into(),
            "[project]/node_modules/next/dist/client/app.js".into(),
            "[externals]/crypto".into(),
            "[client-fs]/_next/static/chunks/app.js".into(),
            "[project]/.next-internal/server/app/page/actions.js".into(),
        ],
        routes: vec![BuildRoute {
            route: "/".into(),
            files: vec![
                "[project]/app/page.tsx".into(),
                "[project]/node_modules/next/dist/client/app.js".into(),
            ],
            symbols: Vec::new(),
            runtime: Some("client".into()),
        }],
        sizes: BTreeMap::from([
            ("[project]/app/page.tsx".to_string(), 100),
            (
                "[project]/node_modules/next/dist/client/app.js".to_string(),
                9_999,
            ),
        ]),
        ..BuildEvidence::default()
    };
    let join = join_build(&snapshot(), &evidence);
    assert_eq!(join.status, "full");
    assert_eq!(join.joined, 1);
    assert_eq!(join.total, 1);
    assert_eq!(join.unmatched, Vec::<String>::new());
    assert_eq!(join.bytes.matched, 100);
    assert_eq!(join.routes[0].files, 1);
}

#[test]
fn low_coverage_is_not_confirmation() {
    let evidence = BuildEvidence {
        version: "16.4.0".into(),
        files: vec!["app/page.tsx".into(), "missing.tsx".into()],
        ..BuildEvidence::default()
    };
    let join = join_build(&snapshot(), &evidence);
    assert_eq!(join.status, "low-coverage");
    assert_eq!(join.unmatched, vec!["missing.tsx"]);
}

#[test]
fn old_next_versions_are_unsupported() {
    let evidence = BuildEvidence {
        version: "15.2.0".into(),
        files: vec!["app/page.tsx".into()],
        ..BuildEvidence::default()
    };
    let join = join_build(&snapshot(), &evidence);
    assert_eq!(join.status, "unsupported");
}

#[test]
fn shared_bytes_are_not_double_counted() {
    let evidence = BuildEvidence {
        version: "16.4.0".into(),
        routes: vec![
            BuildRoute {
                route: "/".into(),
                files: vec!["app/page.tsx".into()],
                symbols: Vec::new(),
                runtime: Some("client".into()),
            },
            BuildRoute {
                route: "/about".into(),
                files: vec!["app/page.tsx".into()],
                symbols: Vec::new(),
                runtime: Some("client".into()),
            },
        ],
        sizes: BTreeMap::from([("app/page.tsx".to_string(), 100)]),
        ..BuildEvidence::default()
    };
    let join = join_build(&snapshot(), &evidence);
    assert_eq!(join.bytes.matched, 100);
    assert_eq!(join.bytes.shared, 100);
    assert_eq!(join.bytes.total_route, 0);
    assert_eq!(join.routes.len(), 2);
    assert_eq!(join.routes[0].bytes, 0);
}

#[test]
fn missing_evidence_is_explicit() {
    let join = join_build(&snapshot(), &BuildEvidence::default());
    assert_eq!(join.status, "none");
    assert_eq!(join.total, 0);
    assert!(
        join.diagnostics
            .iter()
            .any(|item| item.code == "build_absent")
    );
}

#[test]
fn normalizes_loader_query_suffix_and_module_ids() {
    let snapshot = candidate_snapshot();
    let evidence = BuildEvidence {
        version: "16.4.0".into(),
        files: vec![
            "webpack://next/dist/loader.js!./lib/format.ts?ts=1".into(),
            "/repo/app/page.tsx".into(),
        ],
        module_ids: BTreeMap::from([("module-42".to_string(), "lib/format.ts".to_string())]),
        symbols: vec!["module-42#formatInvoice".into()],
        ..BuildEvidence::default()
    };
    let join = join_build(&snapshot, &evidence);
    assert_eq!(join.status, "full");
    assert!(join.unmatched.is_empty());
    assert_eq!(verdict_for(&join, "lib/format.ts#formatInvoice"), "present");
}

#[test]
fn unknown_sizes_are_not_zero() {
    let evidence = BuildEvidence {
        version: "16.4.0".into(),
        files: vec!["app/page.tsx".into()],
        ..BuildEvidence::default()
    };
    let join = join_build(&snapshot(), &evidence);
    assert_eq!(join.bytes.matched, 0);
    assert_eq!(join.bytes.unknown, 1);
    assert!(join.bytes.partial);
    assert_eq!(join.unknown_sizes, vec!["app/page.tsx"]);
}
