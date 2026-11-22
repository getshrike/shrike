use std::collections::{BTreeSet, HashMap};

use serde_json::{Value, json};
use shrike_core::{Diagnostic, PROTOCOL_VERSION, ShrikeError, ShrikeResult, TOOL_VERSION};
use shrike_protocol::Request;

use crate::analyze::{Analyzed, analyze_payload};
use crate::payloads::{AnalyzePayload, BuildPayload, DeltaPayload, PlanPayload, ProofPayload};

/// A command result that can be serialized straight into the response.
///
/// `analyze` and `scan` keep the snapshot as pre-serialized JSON instead of
/// materializing a `serde_json::Value` tree: a repository-sized snapshot is
/// tens of megabytes, and the extra tree allocation and second serialization
/// dominated the protocol path.
pub enum DispatchResult {
    Analyze(AnalyzeResult),
    Scan(ScanResult),
    Json(Value),
}

/// Write a success response directly to the output.
///
/// The snapshot is already JSON, so the envelope and the trailing fields are
/// written around it instead of copying the whole payload into one string.
/// The bytes are identical to `encode_success_line` on the same result.
///
/// # Errors
/// Propagates output errors.
pub fn write_response<W: std::io::Write>(
    writer: &mut W,
    id: &str,
    result: &DispatchResult,
) -> std::io::Result<()> {
    match result {
        DispatchResult::Analyze(result) => {
            write_snapshot_result(writer, id, &result.analyzed, None, None)
        }
        DispatchResult::Scan(result) => write_snapshot_result(
            writer,
            id,
            &result.analyzed,
            Some(&result.candidates),
            Some(&result.analyzed.snapshot),
        ),
        DispatchResult::Json(value) => {
            let line = shrike_protocol::encode_success_line(id, value);
            writer.write_all(line.as_bytes())
        }
    }
}

fn write_snapshot_result<W: std::io::Write>(
    writer: &mut W,
    id: &str,
    analyzed: &Analyzed,
    candidates: Option<&[CandidateView]>,
    dependency_input: Option<&shrike_graph::GraphSnapshot>,
) -> std::io::Result<()> {
    writer.write_all(b"{\"protocolVersion\":")?;
    serde_json::to_writer(&mut *writer, PROTOCOL_VERSION)?;
    writer.write_all(b",\"id\":")?;
    serde_json::to_writer(&mut *writer, id)?;
    writer.write_all(b",\"ok\":true,\"result\":{\"snapshot\":")?;
    writer.write_all(analyzed.snapshot_json.as_bytes())?;
    writer.write_all(b",\"diagnostics\":")?;
    serde_json::to_writer(&mut *writer, &analyzed.snapshot.diagnostics)?;
    if let Some(candidates) = candidates {
        writer.write_all(b",\"candidates\":")?;
        serde_json::to_writer(&mut *writer, candidates)?;
    }
    if let Some(snapshot) = dependency_input {
        writer.write_all(b",\"dependencyInput\":")?;
        serde_json::to_writer(&mut *writer, &DependencyInput::from(snapshot))?;
    }
    writer.write_all(b"},\"error\":null}\n")
}

/// The subset of the snapshot dependency analysis needs.
///
/// The CLI decodes only this projection and splices the already-serialized
/// snapshot into its output. Keeping it small is the point: the full snapshot
/// carries edges, witnesses, proofs, and every symbol record, none of which
/// affect dependency hygiene.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DependencyInput<'a> {
    files: Vec<&'a str>,
    modules: Vec<DependencyModule<'a>>,
    entries: Vec<DependencyEntry<'a>>,
    module_keys: Vec<&'a str>,
    live_modules: Vec<&'a str>,
    diagnostics: &'a [Diagnostic],
    candidate_count: usize,
    unused_file_count: usize,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DependencyModule<'a> {
    path: &'a str,
    specifiers: &'a [String],
    #[serde(skip_serializing_if = "slice_is_empty")]
    external: &'a [String],
    #[serde(skip_serializing_if = "slice_is_empty")]
    type_refs: &'a [String],
}

#[derive(serde::Serialize)]
struct DependencyEntry<'a> {
    symbol: &'a str,
    framework: &'a str,
}

fn slice_is_empty<T>(values: &[T]) -> bool {
    values.is_empty()
}

impl<'a> From<&'a shrike_graph::GraphSnapshot> for DependencyInput<'a> {
    fn from(snapshot: &'a shrike_graph::GraphSnapshot) -> Self {
        let module_keys: Vec<&str> = snapshot
            .symbols
            .iter()
            .filter(|symbol| symbol.kind == "module")
            .map(|symbol| symbol.key.as_str())
            .collect();
        let key_set: BTreeSet<&str> = module_keys.iter().copied().collect();
        let live_modules = snapshot
            .live
            .iter()
            .map(String::as_str)
            .filter(|key| key_set.contains(key))
            .collect();
        Self {
            files: snapshot
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect(),
            modules: snapshot
                .modules
                .iter()
                .map(|module| DependencyModule {
                    path: &module.path,
                    specifiers: &module.specifiers,
                    external: &module.external,
                    type_refs: &module.type_refs,
                })
                .collect(),
            entries: snapshot
                .entries
                .iter()
                .map(|entry| DependencyEntry {
                    symbol: &entry.symbol,
                    framework: &entry.framework,
                })
                .collect(),
            module_keys,
            live_modules,
            diagnostics: &snapshot.diagnostics,
            candidate_count: snapshot.candidates.len(),
            unused_file_count: snapshot.unused_files.len(),
        }
    }
}

pub struct AnalyzeResult {
    analyzed: Analyzed,
}

pub struct ScanResult {
    analyzed: Analyzed,
    candidates: Vec<CandidateView>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CandidateView {
    symbol: String,
    file: String,
    kind: String,
    lines: u32,
    reason: &'static str,
}

pub fn dispatch(request: &Request) -> ShrikeResult<DispatchResult> {
    match request.command.as_str() {
        "ping" | "version" => Ok(DispatchResult::Json(
            json!({"version": TOOL_VERSION, "protocolVersion": PROTOCOL_VERSION}),
        )),
        "analyze" => {
            let payload: AnalyzePayload = parse(&request.payload)?;
            let analyzed = analyze_payload(&payload)?;
            Ok(DispatchResult::Analyze(AnalyzeResult { analyzed }))
        }
        "scan" => {
            let payload: AnalyzePayload = parse(&request.payload)?;
            let analyzed = analyze_payload(&payload)?;
            // Candidate keys are a subset of the symbol keys. Index them once
            // rather than scanning every symbol per candidate.
            let symbols: HashMap<&str, _> = analyzed
                .snapshot
                .symbols
                .iter()
                .map(|symbol| (symbol.key.as_str(), symbol))
                .collect();
            let candidates: Vec<CandidateView> = analyzed
                .snapshot
                .candidates
                .iter()
                .filter_map(|key| {
                    let symbol = symbols.get(key.as_str())?;
                    Some(CandidateView {
                        symbol: key.clone(),
                        file: symbol.file.clone(),
                        kind: symbol.kind.clone(),
                        lines: symbol.lines,
                        reason: "no witness from any entry",
                    })
                })
                .collect();
            Ok(DispatchResult::Scan(ScanResult {
                analyzed,
                candidates,
            }))
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
            Ok(DispatchResult::Json(serde_json::to_value(report)?))
        }
        "plan" => {
            let payload: PlanPayload = parse(&request.payload)?;
            let plan = shrike_plan::build_plan(&payload.snapshot, &payload.delta, &payload.config);
            Ok(DispatchResult::Json(serde_json::to_value(plan)?))
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
            Ok(DispatchResult::Json(json!({
                "symbol": payload.symbol,
                "live": payload.snapshot.is_live(&payload.symbol),
                "witness": witness.map_or_else(Vec::new, |w| w.path.clone()),
                "trace": proof.map_or_else(Vec::new, |p| vec![json!({"reason": p.reason, "incoming": p.incoming, "outgoing": p.outgoing})]),
            })))
        }
        "join-build" => {
            let payload: BuildPayload = parse(&request.payload)?;
            Ok(DispatchResult::Json(serde_json::to_value(
                shrike_evidence::join_build(&payload.snapshot, &payload.build),
            )?))
        }
        "augment-build" => {
            let payload: BuildPayload = parse(&request.payload)?;
            let join = shrike_evidence::join_build(&payload.snapshot, &payload.build);
            let snapshot =
                shrike_evidence::augment_snapshot(&payload.snapshot, &payload.build, &join);
            Ok(DispatchResult::Json(
                json!({"snapshot": snapshot, "join": join}),
            ))
        }
        other => Err(ShrikeError::UnknownCommand(other.to_string())),
    }
}

fn parse<T: serde::de::DeserializeOwned>(value: &Value) -> ShrikeResult<T> {
    // Deserialize from the borrowed value. `from_value(value.clone())` copied
    // the entire payload tree before decoding it.
    T::deserialize(value).map_err(|error| ShrikeError::Protocol(error.to_string()))
}
