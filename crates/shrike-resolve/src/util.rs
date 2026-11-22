use std::path::{Component, Path};

pub(crate) fn split_package(specifier: &str) -> Option<(String, Option<String>)> {
    if specifier.starts_with('@') {
        let mut parts = specifier.splitn(3, '/');
        let scope = parts.next()?;
        let name = parts.next()?;
        let subpath = parts.next().map(ToString::to_string);
        Some((format!("{scope}/{name}"), subpath))
    } else {
        let mut parts = specifier.splitn(2, '/');
        let name = parts.next()?;
        if name.is_empty() {
            return None;
        }
        Some((name.to_string(), parts.next().map(ToString::to_string)))
    }
}

pub(crate) fn match_alias(pattern: &str, specifier: &str) -> Option<String> {
    if let Some(prefix) = pattern.strip_suffix('*') {
        specifier.strip_prefix(prefix).map(ToString::to_string)
    } else if pattern == specifier {
        Some(String::new())
    } else {
        None
    }
}

pub(crate) fn parent_dir(path: &str) -> String {
    path.rsplit_once('/')
        .map_or_else(String::new, |(dir, _)| dir.to_string())
}
/// Normalize a path without touching the filesystem, resolving `..` lexically.
#[must_use]
pub fn normalize_path(path: &Path) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().to_string()),
            Component::ParentDir => {
                parts.pop();
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }
    parts.join("/")
}
