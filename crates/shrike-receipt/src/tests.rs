use super::*;

fn receipt() -> Receipt {
    Receipt {
        schema_version: "1.0".into(),
        id: String::new(),
        created_at: "2026-11-20T00:00:00.000Z".into(),
        tool: Tool {
            name: "shrike".into(),
            version: "1.0.0".into(),
        },
        base: GraphRef {
            r#ref: Some("main".into()),
            commit: "a".into(),
            graph_hash: "sha256:a".into(),
        },
        head: GraphRef {
            r#ref: None,
            commit: "b".into(),
            graph_hash: "sha256:b".into(),
        },
        plan: PlanRef {
            id: "plan_x".into(),
            hash: "sha256:p".into(),
        },
        build: None,
        batch: BatchRef {
            id: "batch_1".into(),
            symbols: vec!["a.ts#x".into()],
            files: vec!["a.ts".into()],
            lines_removed: 1,
            patches: vec![],
        },
        checks: vec![CheckRecord {
            name: "typecheck".into(),
            command: "pnpm typecheck".into(),
            status: "pass".into(),
            baseline_status: "pass".into(),
            duration_ms: Some(1),
            reruns: 0,
            failure_signature: None,
        }],
        outcome: "verified".into(),
        rejected: vec![],
        policy: None,
        environment: Environment {
            node: "v24".into(),
            package_manager: "pnpm".into(),
            platform: "linux".into(),
        },
    }
}

#[test]
fn id_is_stable_and_verifies() {
    let mut first = receipt();
    first.id = first.compute_id();
    assert!(first.verify());
    let mut second = receipt();
    second.created_at = "2030-01-01T00:00:00.000Z".into();
    second.id = second.compute_id();
    assert_eq!(first.id, second.id);
}
