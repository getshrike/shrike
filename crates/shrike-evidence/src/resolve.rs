use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn resolve_build_item(
    item: &str,
    known_files: &BTreeSet<&str>,
    known_symbols: &BTreeSet<&str>,
    module_ids: &BTreeMap<String, String>,
) -> Option<String> {
    if known_files.contains(item) || known_symbols.contains(item) {
        return Some(item.to_string());
    }
    if let Some(mapped) = module_ids.get(item) {
        if let Some(resolved) = resolve_build_item(mapped, known_files, known_symbols, module_ids) {
            return Some(resolved);
        }
    }
    if let Some((file, name)) = item.rsplit_once('#') {
        let file = normalize_build_path(file);
        let file = module_ids
            .get(&file)
            .map_or(file, |mapped| normalize_build_path(mapped));
        if let Some(resolved_file) = match_file(&file, known_files) {
            let key = format!("{resolved_file}#{name}");
            if known_symbols.contains(key.as_str()) {
                return Some(key);
            }
        }
        return None;
    }
    let normalized = normalize_build_path(item);
    if let Some(mapped) = module_ids.get(&normalized) {
        if let Some(resolved) = resolve_build_item(mapped, known_files, known_symbols, module_ids) {
            return Some(resolved);
        }
    }
    match_file(&normalized, known_files).map(ToString::to_string)
}

pub(crate) fn match_file<'a>(
    normalized: &str,
    known_files: &'a BTreeSet<&'a str>,
) -> Option<&'a str> {
    if let Some(exact) = known_files.get(normalized) {
        return Some(*exact);
    }
    let mut candidates: Vec<&str> = known_files
        .iter()
        .copied()
        .filter(|known| {
            normalized.ends_with(&format!("/{known}")) || known.ends_with(&format!("/{normalized}"))
        })
        .collect();
    candidates.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    candidates.into_iter().next()
}

pub(crate) fn normalize_build_path(input: &str) -> String {
    let mut value = input.replace('\\', "/");
    if let Some(index) = value.rfind('!') {
        value = value[index + 1..].to_string();
    }
    // Turbopack appends `[layer] (kind)` to module identifiers.
    if let Some(index) = value.find(" [") {
        if value[index..].contains("] (") {
            value.truncate(index);
        }
    }
    for prefix in ["turbopack://", "file://", "[project]/"] {
        if let Some(rest) = value.strip_prefix(prefix) {
            value = rest.to_string();
        }
    }
    if let Some(index) = value.find('?') {
        value.truncate(index);
    }
    while value.starts_with("./") {
        value = value[2..].to_string();
    }
    value.trim_start_matches('/').to_string()
}

/// True when an evidence item can correspond to a project source module.
///
/// `next analyze export` includes framework, dependency, and emitted-asset
/// modules. They are not candidates and must not lower join coverage.
pub(crate) fn is_project_evidence_item(input: &str) -> bool {
    let raw = input.replace('\\', "/");
    for prefix in [
        "[externals]/",
        "[client-fs]/",
        "[output]/",
        "[next]/",
        "[turbopack]/",
    ] {
        if raw.starts_with(prefix) {
            return false;
        }
    }
    let normalized = normalize_build_path(input);
    if normalized.starts_with("node_modules/") || normalized.contains("/node_modules/") {
        return false;
    }
    if normalized.starts_with("../") || normalized == ".." {
        return false;
    }
    if normalized.contains("--route-entry.")
        || normalized.contains(".[json].")
        || normalized.starts_with(".next-internal/")
        || normalized.starts_with(".next/")
    {
        return false;
    }
    true
}
