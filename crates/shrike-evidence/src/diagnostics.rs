use shrike_core::Diagnostic;

use crate::resolve::normalize_build_path;
use crate::status::version_label;
use crate::types::BuildEvidence;

pub(crate) fn diagnostics_for(
    status: &str,
    total: usize,
    coverage: f64,
    unmatched: &[String],
    evidence: &BuildEvidence,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if total == 0 && status == "none" {
        diagnostics.push(Diagnostic::info(
            "build_absent",
            "no build evidence supplied; static evidence only",
        ));
    } else if status == "unsupported" {
        diagnostics.push(Diagnostic::warning(
            "build_unsupported",
            format!(
                "Next.js version {} is below the evidence contract; static evidence only",
                version_label(&evidence.version)
            ),
        ));
    } else if status == "stale" {
        diagnostics.push(Diagnostic::warning(
            "build_stale",
            "build evidence does not match the current tree; static evidence only",
        ));
    } else if status == "build-failed" {
        diagnostics.push(Diagnostic::warning(
            "build_failed",
            "the requested build failed; static evidence only",
        ));
    } else if status == "malformed" {
        diagnostics.push(Diagnostic::warning(
            "build_malformed",
            "build evidence could not be parsed; static evidence only",
        ));
    } else if status == "low-coverage" {
        diagnostics.push(Diagnostic::warning(
            "build_low_coverage",
            format!(
                "build join coverage {:.2} is below 0.80; evidence is not used for confirmation",
                coverage
            ),
        ));
    } else if status == "partial" {
        diagnostics.push(Diagnostic::warning(
            "build_partial",
            format!(
                "build join coverage {:.2} is partial; {} entries did not match the snapshot",
                coverage,
                unmatched.len()
            ),
        ));
    }
    if evidence.client_boundary_leaks != Some(false) {
        for (item, layer) in &evidence.layers {
            if layer != "server" {
                continue;
            }
            let file = item.split('#').next().unwrap_or(item);
            let client_routes: Vec<&String> = evidence
                .routes
                .iter()
                .filter(|route| {
                    matches!(route.runtime.as_deref(), Some("client") | Some("both"))
                        && route.files.iter().any(|candidate| {
                            candidate == item
                                || candidate == file
                                || normalize_build_path(candidate) == normalize_build_path(file)
                        })
                })
                .map(|route| &route.route)
                .collect();
            if !client_routes.is_empty() {
                diagnostics.push(
                    Diagnostic::warning(
                        "client-boundary-leak",
                        format!(
                            "server-only module appears in a client chunk on {}",
                            client_routes
                                .iter()
                                .map(|route| route.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    )
                    .in_file(file),
                );
            }
        }
    }
    diagnostics
}
