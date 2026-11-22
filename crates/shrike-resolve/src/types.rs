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
    /// Whether Metro resolves this package's modules. Metro tries platform
    /// suffixes (`Button.ios.tsx`, `Button.native.tsx`, ...) for a neutral
    /// import before falling back to the base module.
    #[serde(default)]
    pub metro: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSpecifier {
    pub importer: String,
    pub specifier: String,
    /// The preferred target. When Metro platform variants exist this is the
    /// base module when present, otherwise the first platform variant.
    pub target: Option<String>,
    /// Additional Metro platform variants for the same neutral specifier.
    #[serde(default)]
    pub alternatives: Vec<String>,
    pub external: bool,
    pub reason: String,
}

impl ResolvedSpecifier {
    /// Every local module the specifier can load, preferred target first.
    pub fn targets(&self) -> impl Iterator<Item = &String> {
        self.target.iter().chain(self.alternatives.iter())
    }
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
