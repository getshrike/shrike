use super::*;

#[test]
fn verdicts_distinguish_client_server_absent_and_unknown() {
    let mut snapshot = candidate_snapshot();
    let span = Span {
        start: 0,
        end: 10,
        line: 1,
        column: 0,
        end_line: 1,
    };
    snapshot.files.push(FileRecord {
        path: "lib/server.ts".into(),
        content_hash: "blake3:02".into(),
        module_kind: "module".into(),
        parse_status: shrike_syntax::ParseStatus::Ok,
        generated: false,
        package: None,
        public_api: false,
        lines: 1,
    });
    snapshot.symbols.push(SymbolRecord {
        key: "lib/server.ts#action".into(),
        name: "action".into(),
        kind: "function".into(),
        file: "lib/server.ts".into(),
        span,
        exported: true,
        entry: false,
        public_api: false,
        generated: false,
        type_only: false,
        lines: 1,
    });
    snapshot.candidates = vec![
        "lib/format.ts#formatInvoice".into(),
        "lib/server.ts#action".into(),
        "lib/absent.ts#dead".into(),
    ];
    let evidence = BuildEvidence {
        version: "16.4.0".into(),
        files: vec![
            "app/page.tsx".into(),
            "lib/format.ts".into(),
            "lib/server.ts".into(),
        ],
        symbols: vec![
            "lib/format.ts#formatInvoice".into(),
            "lib/server.ts#action".into(),
        ],
        layers: BTreeMap::from([
            ("lib/format.ts".to_string(), "client".to_string()),
            ("lib/server.ts".to_string(), "server".to_string()),
        ]),
        routes: vec![
            BuildRoute {
                route: "/".into(),
                files: vec!["app/page.tsx".into(), "lib/format.ts".into()],
                symbols: vec!["lib/format.ts#formatInvoice".into()],
                runtime: Some("client".into()),
            },
            BuildRoute {
                route: "/api".into(),
                files: vec!["lib/server.ts".into()],
                symbols: vec!["lib/server.ts#action".into()],
                runtime: Some("server".into()),
            },
        ],
        ..BuildEvidence::default()
    };
    let join = join_build(&snapshot, &evidence);
    assert_eq!(verdict_for(&join, "lib/format.ts#formatInvoice"), "present");
    assert_eq!(verdict_for(&join, "lib/server.ts#action"), "server-only");
    assert_eq!(verdict_for(&join, "lib/absent.ts#dead"), "absent");

    let low = BuildEvidence {
        files: vec!["app/page.tsx".into(), "missing.tsx".into()],
        routes: Vec::new(),
        ..BuildEvidence::default()
    };
    let join = join_build(&snapshot, &low);
    assert_eq!(join.status, "low-coverage");
    assert_eq!(verdict_for(&join, "lib/format.ts#formatInvoice"), "unknown");
}
