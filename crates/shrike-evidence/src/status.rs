use crate::types::BuildEvidence;

pub(crate) fn version_label(version: &str) -> &str {
    if version.is_empty() {
        "unknown"
    } else {
        version
    }
}

/// Capability level from the Next.js version. An unknown version is treated as
/// partial rather than current: absence of a version is not evidence of 16.4.
pub(crate) fn capability(version: &str) -> &'static str {
    let trimmed = version.trim().trim_start_matches('v');
    if trimmed.is_empty() {
        return "partial";
    }
    let mut parts = trimmed.split('.');
    let major = parts.next().and_then(|part| part.parse::<u32>().ok());
    let minor = parts.next().and_then(|part| part.parse::<u32>().ok());
    match (major, minor) {
        (Some(16), Some(minor)) if minor >= 4 => "full",
        (Some(16), Some(_)) => "partial",
        (Some(0..=15), _) => "unsupported",
        _ => "unsupported",
    }
}

pub(crate) fn status_for(total: usize, coverage: f64, version: &str) -> &'static str {
    if total == 0 {
        return "none";
    }
    let capability = capability(version);
    if capability == "unsupported" {
        return "unsupported";
    }
    let coverage_status = if coverage < 0.80 {
        "low-coverage"
    } else if coverage < 0.95 {
        "partial"
    } else {
        "full"
    };
    if capability == "partial" && coverage_status == "full" {
        "partial"
    } else {
        coverage_status
    }
}

pub(crate) fn status_for_evidence(
    total: usize,
    coverage: f64,
    evidence: &BuildEvidence,
) -> &'static str {
    if let Some(status) = evidence.status.as_deref() {
        return match status {
            "stale" => "stale",
            "build-failed" => "build-failed",
            "malformed" => "malformed",
            "unsupported" => "unsupported",
            "low-coverage" => "low-coverage",
            "partial" => "partial",
            "full" => "full",
            _ => "none",
        };
    }
    status_for(total, coverage, &evidence.version)
}
