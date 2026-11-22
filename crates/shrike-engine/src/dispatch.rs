use serde_json::{Value, json};
use shrike_core::{PROTOCOL_VERSION, ShrikeError, ShrikeResult, TOOL_VERSION};
use shrike_protocol::Request;

use crate::analyze::analyze_payload;
use crate::payloads::{AnalyzePayload, BuildPayload, DeltaPayload, PlanPayload, ProofPayload};

pub fn dispatch(request: &Request) -> ShrikeResult<Value> {
    match request.command.as_str() {
        "ping" | "version" => {
            Ok(json!({"version": TOOL_VERSION, "protocolVersion": PROTOCOL_VERSION}))
        }
        "analyze" => {
            let payload: AnalyzePayload = parse(&request.payload)?;
            let snapshot = analyze_payload(&payload)?;
            Ok(json!({"snapshot": snapshot, "diagnostics": snapshot.diagnostics}))
        }
        "scan" => {
            let payload: AnalyzePayload = parse(&request.payload)?;
            let snapshot = analyze_payload(&payload)?;
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
        "augment-build" => {
            let payload: BuildPayload = parse(&request.payload)?;
            let join = shrike_evidence::join_build(&payload.snapshot, &payload.build);
            let snapshot =
                shrike_evidence::augment_snapshot(&payload.snapshot, &payload.build, &join);
            Ok(json!({"snapshot": snapshot, "join": join}))
        }
        other => Err(ShrikeError::UnknownCommand(other.to_string())),
    }
}

fn parse<T: serde::de::DeserializeOwned>(value: &Value) -> ShrikeResult<T> {
    serde_json::from_value(value.clone()).map_err(|error| ShrikeError::Protocol(error.to_string()))
}
