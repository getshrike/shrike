//! Immutable data shared by the entry detectors, computed once per build.

use std::collections::{BTreeMap, BTreeSet};

use shrike_syntax::FileSummary;

pub(crate) struct EntryContext {
    pub(crate) summaries: std::sync::Arc<Vec<FileSummary>>,
    pub(crate) rsc_directives: bool,
    pub(crate) page_extensions: BTreeMap<String, BTreeSet<String>>,
    pub(crate) vite_packages: BTreeSet<String>,
    pub(crate) storybook_packages: BTreeSet<String>,
    pub(crate) gatsby_packages: BTreeSet<String>,
    pub(crate) expo_layout_packages: BTreeSet<String>,
    pub(crate) expo_config_modules: BTreeSet<String>,
    pub(crate) nuxt_config_packages: BTreeSet<String>,
    pub(crate) next_config_modules: BTreeSet<String>,
    pub(crate) webpack_entry_modules: BTreeSet<String>,
    pub(crate) storybook_story_files: BTreeSet<String>,
    pub(crate) ladle_story_files: BTreeSet<String>,
    pub(crate) ladle_entry_files: BTreeSet<String>,
    pub(crate) cosmos_fixture_files: BTreeSet<String>,
    pub(crate) cosmos_decorator_files: BTreeSet<String>,
    pub(crate) docusaurus_page_files: BTreeSet<String>,
    pub(crate) gatsby_template_files: BTreeSet<String>,
    pub(crate) router_route_files: BTreeSet<String>,
    pub(crate) test_config_modules: BTreeSet<String>,
    pub(crate) cypress_specs: BTreeSet<String>,
    pub(crate) cypress_support: BTreeSet<String>,
    pub(crate) expo_roots: BTreeMap<String, String>,
    pub(crate) router_packages: BTreeMap<String, String>,
}
