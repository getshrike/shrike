use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfo {
    pub name: String,
    pub directory: String,
    pub entry: Option<String>,
    pub exports: BTreeMap<String, String>,
    pub public_api: bool,
    /// Source files that back public package entrypoints. Published packages
    /// export built files that are absent from the analysis set, so the
    /// resolver maps each export target back to its conventional source file.
    #[serde(default)]
    pub public_sources: BTreeSet<String>,
    /// Files invoked by `package.json` scripts or exposed as package `bin`
    /// commands. They execute without being imported, so they are entries.
    #[serde(default)]
    pub script_files: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSpecifier {
    pub importer: String,
    pub specifier: String,
    pub target: Option<String>,
    pub external: bool,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub struct Resolver {
    pub(crate) root: PathBuf,
    pub(crate) files: BTreeSet<String>,
    pub(crate) files_by_dir: BTreeMap<String, Vec<String>>,
    pub(crate) base_url: Option<String>,
    pub(crate) paths: Vec<(String, Vec<String>)>,
    pub(crate) packages: BTreeMap<String, PackageInfo>,
    pub resolver_hash: String,
    pub packages_list: Vec<PackageInfo>,
}
