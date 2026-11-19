#![forbid(unsafe_code)]
//! Command dispatch shared by the binary and tests.
use serde::Deserialize;
use serde_json::{Value, json};
use shrike_core::{EngineConfig, PROTOCOL_VERSION, ShrikeError, ShrikeResult, TOOL_VERSION};
use shrike_protocol::Request;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnalyzePayload {
    root: String,
    #[serde(default)]
    config: EngineConfig,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeltaPayload {
    base_snapshot: shrike_graph::GraphSnapshot,
    head_snapshot: shrike_graph::GraphSnapshot,
    #[serde(default)]
    base_ref: Option<String>,
    base_commit: String,
    head_commit: String,
    #[serde(default)]
    dirty: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanPayload {
    snapshot: shrike_graph::GraphSnapshot,
    delta: shrike_delta::DeltaReport,
    #[serde(default)]
    config: EngineConfig,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProofPayload {
    snapshot: shrike_graph::GraphSnapshot,
    symbol: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildPayload {
    snapshot: shrike_graph::GraphSnapshot,
    build: shrike_evidence::BuildEvidence,
}

pub fn dispatch(request: &Request) -> ShrikeResult<Value> {
    match request.command.as_str() {
        "ping" | "version" => {
            Ok(json!({"version": TOOL_VERSION, "protocolVersion": PROTOCOL_VERSION}))
        }
        "analyze" => {
            let payload: AnalyzePayload = parse(&request.payload)?;
            payload.config.validate().map_err(ShrikeError::Config)?;
            let root = std::path::Path::new(&payload.root);
            let files = shrike_graph::discover_files(root, &payload.config);
            let summaries: Vec<_> = files
                .iter()
                .filter_map(|path| {
                    let source = std::fs::read_to_string(root.join(path)).ok()?;
                    Some(shrike_syntax::parse_source(path, &source))
                })
                .collect();
            let snapshot = shrike_graph::build_snapshot(root, summaries, &payload.config);
            Ok(json!({"snapshot": snapshot, "diagnostics": snapshot.diagnostics}))
        }
        "scan" => {
            let payload: AnalyzePayload = parse(&request.payload)?;
            payload.config.validate().map_err(ShrikeError::Config)?;
            let root = std::path::Path::new(&payload.root);
            let files = shrike_graph::discover_files(root, &payload.config);
            let summaries: Vec<_> = files
                .iter()
                .filter_map(|path| {
                    let source = std::fs::read_to_string(root.join(path)).ok()?;
                    Some(shrike_syntax::parse_source(path, &source))
                })
                .collect();
            let snapshot = shrike_graph::build_snapshot(root, summaries, &payload.config);
            let candidates: Vec<Value> = snapshot.candidates.iter().filter_map(|key| {
                let symbol = snapshot.symbol(key)?;
                Some(json!({"symbol": key, "file": symbol.file, "kind": symbol.kind, "lines": symbol.lines, "reason": "no witness from any entry"}))
            }).collect();
            Ok(
                json!({"snapshot": snapshot, "diagnostics": snapshot.diagnostics, "candidates": candidates}),
            )
        }
        "delta" => {
            let payload: DeltaPayload = parse(&request.payload)?;
            let report = shrike_delta::compute_delta(
                &payload.base_snapshot,
                &payload.head_snapshot,
                payload.base_ref,
                payload.base_commit,
                payload.head_commit,
                payload.dirty,
            );
            Ok(serde_json::to_value(report)?)
        }
        "plan" => {
            let payload: PlanPayload = parse(&request.payload)?;
            let plan = shrike_plan::build_plan(&payload.snapshot, &payload.delta, &payload.config);
            Ok(serde_json::to_value(plan)?)
        }
        "proof" => {
            let payload: ProofPayload = parse(&request.payload)?;
            let witness = payload
                .snapshot
                .witnesses
                .iter()
                .find(|w| w.symbol == payload.symbol);
            let proof = payload
                .snapshot
                .proofs
                .iter()
                .find(|p| p.symbol == payload.symbol);
            Ok(json!({
                "symbol": payload.symbol,
                "live": payload.snapshot.is_live(&payload.symbol),
                "witness": witness.map_or_else(Vec::new, |w| w.path.clone()),
                "trace": proof.map_or_else(Vec::new, |p| vec![json!({"reason": p.reason, "incoming": p.incoming, "outgoing": p.outgoing})]),
            }))
        }
        "join-build" => {
            let payload: BuildPayload = parse(&request.payload)?;
            Ok(serde_json::to_value(shrike_evidence::join_build(
                &payload.snapshot,
                &payload.build,
            ))?)
        }
        other => Err(ShrikeError::UnknownCommand(other.to_string())),
    }
}

fn parse<T: serde::de::DeserializeOwned>(value: &Value) -> ShrikeResult<T> {
    serde_json::from_value(value.clone()).map_err(|error| ShrikeError::Protocol(error.to_string()))
}
