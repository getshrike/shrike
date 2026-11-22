//! React component prop diagnostics.
//!
//! This is a warning, never a deletion candidate: removing a prop changes a
//! component's contract, and Shrike's batch deletion model only removes whole
//! symbols. The parser has already abstained on exported, module-referenced,
//! and unharvestable components, so a record here is a local component whose
//! destructured prop is read nowhere in its own declaration.

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;
use shrike_core::Diagnostic;
use shrike_core::ids::{module_symbol_key, symbol_key};
use shrike_syntax::ParseStatus;

use super::Builder;

/// Packages that make React (or a React-compatible runtime) the renderer.
/// Matches fallow 3.45.0's `declares_react_runtime` gate.
const REACT_RUNTIME_PACKAGES: &[&str] = &["react", "react-dom", "next", "preact"];

impl<'a> Builder<'a> {
    /// Whether the project declares a React runtime. The gate is project-wide:
    /// a shared component package may be rendered by a sibling React app even
    /// when its own manifest does not depend on React. The root manifest is
    /// checked directly because the resolver only records named packages.
    pub(super) fn project_uses_react(&self) -> bool {
        self.package_declares_any("", REACT_RUNTIME_PACKAGES) == Some(true)
            || self.resolver.packages().iter().any(|package| {
                self.package_declares_any(&package.directory, REACT_RUNTIME_PACKAGES) == Some(true)
            })
    }

    /// Warn about props that a live local React component destructures but
    /// never reads. `live` is the symbol-level reachable set, so a prop on a
    /// dead component is not reported twice: the component itself is already
    /// a deletion candidate.
    pub(super) fn detect_unused_component_props(&mut self, live: &BTreeSet<String>) {
        if !self.project_uses_react() {
            return;
        }
        let summaries = self.summaries.clone();
        for summary in summaries.iter() {
            if summary.parse_status != ParseStatus::Ok || summary.component_props.is_empty() {
                continue;
            }
            if self
                .file_generated
                .get(&summary.path)
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            for prop in &summary.component_props {
                if prop.used {
                    continue;
                }
                if !live.contains(&symbol_key(&summary.path, &prop.component)) {
                    continue;
                }
                let alias = if prop.destructured && prop.local != prop.prop {
                    format!(" (bound as `{}`)", prop.local)
                } else {
                    String::new()
                };
                let verb = if prop.destructured {
                    "destructures"
                } else {
                    "declares"
                };
                self.diagnostics.push(
                    Diagnostic::warning(
                        "unused_component_prop",
                        format!(
                            "`{}` {verb} prop `{}`{} but never reads it in the component",
                            prop.component, prop.prop, alias
                        ),
                    )
                    .in_file(&summary.path)
                    .at(prop.span),
                );
            }
        }
    }

    /// Warn when every statically known JSX render site of a live local React
    /// component omits an optional prop that the component reads. This is
    /// deliberately advisory: a default may be intentional, and a prop can be
    /// supplied by a consumer outside the analyzed project. The rule abstains
    /// whenever the consumer set is open (a value escape, a namespace import, a
    /// spread attribute, an entry or public API surface) or when there is no
    /// known render site at all.
    pub(super) fn detect_absent_component_props(&self, live: &BTreeSet<String>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        if !self.project_uses_react() {
            return diagnostics;
        }
        let timing = std::env::var_os("SHRIKE_TIMING").is_some();
        let summaries = self.summaries.clone();
        let aliases = timed!(timing, "prop_aliases", self.reexport_alias_targets());
        let public_surface = timed!(
            timing,
            "prop_public_surface",
            self.public_component_surface(&aliases)
        );
        // The contract checks below run once per component; precompute the
        // entry-file set and reference edges so each check is a lookup instead
        // of a scan over every entry or edge.
        let entry_files: BTreeSet<&str> = self
            .entries
            .iter()
            .filter_map(|entry| entry.symbol.split('#').next())
            .collect();
        let reference_edges: BTreeSet<(&str, &str)> = self
            .edges
            .iter()
            .filter(|edge| edge.kind == "reference")
            .map(|edge| (edge.source.as_str(), edge.target.as_str()))
            .collect();
        let blocked: BTreeSet<String> = timed!(timing, "prop_blocked", {
            // Collect namespace targets across the whole project and expand each
            // module's surface once; the same barrel is often imported many times.
            let (mut blocked, namespace_roots) = summaries
                .par_iter()
                .filter(|summary| summary.parse_status == ParseStatus::Ok)
                .fold(
                    || (BTreeSet::<String>::new(), BTreeSet::<String>::new()),
                    |(mut blocked, mut roots), summary| {
                        for use_ in &summary.jsx_namespace_uses {
                            let Some(targets) = self
                                .namespaces
                                .get(&summary.path)
                                .and_then(|map| map.get(&use_.root))
                            else {
                                continue;
                            };
                            roots.extend(targets.iter().cloned());
                        }
                        // A component used as a value can be rendered by an unknown
                        // consumer: `memo(Card)`, `{ Card }`, or `const Alias = Card`.
                        // Direct exports and JSX tags are not value uses.
                        for name in &summary.component_value_uses {
                            if let Some(targets) = self
                                .namespaces
                                .get(&summary.path)
                                .and_then(|map| map.get(name))
                            {
                                // A namespace import used as a value (`{ ...UI }`,
                                // `const { Card } = UI`, or `UI.Card` outside JSX)
                                // can reach any export of the target module.
                                roots.extend(targets.iter().cloned());
                                continue;
                            }
                            if let Some(key) = self.local_or_imported(&summary.path, name) {
                                blocked.extend(origin_keys(&aliases, &key));
                            }
                        }
                        (blocked, roots)
                    },
                )
                .reduce(
                    || (BTreeSet::<String>::new(), BTreeSet::<String>::new()),
                    |(mut blocked, mut roots), (other_blocked, other_roots)| {
                        blocked.extend(other_blocked);
                        roots.extend(other_roots);
                        (blocked, roots)
                    },
                );
            if !namespace_roots.is_empty() {
                let roots: Vec<String> = namespace_roots.into_iter().collect();
                blocked.extend(self.export_surface(&roots, &aliases));
            }

            // A dynamic import hands a module namespace to a runtime consumer, so
            // every export of the target is a possible component render. Block the
            // whole target surface, matching fallow 3.45.0. Only imports from a
            // runtime-reachable module can actually execute; a dynamic import in
            // dead code must not open the consumer set.
            let dynamic_targets: BTreeSet<String> = self
                .edges
                .par_iter()
                .filter_map(|edge| {
                    if edge.kind != "dynamic" {
                        return None;
                    }
                    let source = edge.source.split('#').next()?;
                    if !live.contains(&module_symbol_key(source)) {
                        return None;
                    }
                    edge.target.split('#').next().map(str::to_string)
                })
                .collect();
            if !dynamic_targets.is_empty() {
                let targets: Vec<String> = dynamic_targets.into_iter().collect();
                blocked.extend(self.export_surface(&targets, &aliases));
            }
            blocked
        });

        let uses: BTreeMap<String, Vec<&shrike_syntax::JsxUseRecord>> =
            timed!(timing, "prop_uses", {
                summaries
                    .par_iter()
                    .filter(|summary| summary.parse_status == ParseStatus::Ok)
                    .fold(
                        BTreeMap::<String, Vec<&shrike_syntax::JsxUseRecord>>::new,
                        |mut uses, summary| {
                            for use_ in &summary.jsx_uses {
                                let Some(key) =
                                    self.local_or_imported(&summary.path, &use_.component)
                                else {
                                    continue;
                                };
                                match aliases.get(&key) {
                                    Some(origins) => {
                                        for origin in origins {
                                            uses.entry(origin.clone()).or_default().push(use_);
                                        }
                                    }
                                    None => uses.entry(key).or_default().push(use_),
                                }
                            }
                            uses
                        },
                    )
                    .reduce(BTreeMap::new, |mut left, right| {
                        for (key, mut sites) in right {
                            left.entry(key).or_default().append(&mut sites);
                        }
                        left
                    })
            });

        timed!(timing, "prop_report", {
            for summary in summaries.iter() {
                if summary.parse_status != ParseStatus::Ok || summary.component_contracts.is_empty()
                {
                    continue;
                }
                if self
                    .file_generated
                    .get(&summary.path)
                    .copied()
                    .unwrap_or(false)
                {
                    continue;
                }
                // A component exported from an entry or a public package surface
                // can be rendered by a consumer outside the analyzed project. A
                // direct export from an ordinary module is not an escape: its
                // imports are resolved and its render sites are known.
                if self.public_files.contains(&summary.path)
                    || entry_files.contains(summary.path.as_str())
                {
                    continue;
                }
                for contract in &summary.component_contracts {
                    let keys = self.component_contract_keys(
                        &summary.path,
                        &contract.component,
                        &reference_edges,
                    );
                    if contract.escaped
                        || keys
                            .iter()
                            .any(|key| public_surface.contains(key) || blocked.contains(key))
                        || !keys.iter().any(|key| live.contains(key))
                    {
                        continue;
                    }
                    let mut sites: Vec<&shrike_syntax::JsxUseRecord> = Vec::new();
                    for key in &keys {
                        if let Some(found) = uses.get(key) {
                            sites.extend(found.iter().copied());
                        }
                    }
                    if sites.is_empty() || sites.iter().any(|site| site.opaque) {
                        continue;
                    }
                    let supplied: BTreeSet<&str> = sites
                        .iter()
                        .flat_map(|site| site.supplied.iter().map(String::as_str))
                        .collect();
                    for prop in &contract.props {
                        if !prop.optional || !prop.used || supplied.contains(prop.name.as_str()) {
                            continue;
                        }
                        let sites = match sites.len() {
                            1 => "its only known render site does not supply it".to_string(),
                            count => format!("none of its {count} known render sites supply it"),
                        };
                        let default = if prop.has_default {
                            " (the component defines a default)"
                        } else {
                            ""
                        };
                        diagnostics.push(
                            Diagnostic::warning(
                                "absent_component_prop",
                                format!(
                                    "`{}` reads optional prop `{}` but {sites}{default}",
                                    contract.component, prop.name
                                ),
                            )
                            .in_file(&summary.path)
                            .at(prop.span),
                        );
                    }
                }
            }
        });
        diagnostics
    }

    /// The symbol keys that identify one component contract. A component can
    /// be exposed as `default` without being renamed in `locals`:
    /// `const Card = ...; export default Card;` creates a `#default` symbol
    /// whose reference edge points at `#Card`. Imports resolve to `#default`,
    /// so the contract must be checked under both keys.
    fn component_contract_keys(
        &self,
        path: &str,
        component: &str,
        reference_edges: &BTreeSet<(&str, &str)>,
    ) -> BTreeSet<String> {
        let mut keys = BTreeSet::new();
        let key = self
            .locals
            .get(path)
            .and_then(|map| map.get(component))
            .cloned()
            .unwrap_or_else(|| symbol_key(path, component));
        keys.insert(key.clone());
        let Some(default_key) = self.locals.get(path).and_then(|map| map.get("default")) else {
            return keys;
        };
        if default_key == &key {
            return keys;
        }
        let direct_default = reference_edges.contains(&(default_key.as_str(), key.as_str()));
        if direct_default {
            keys.insert(default_key.clone());
        }
        keys
    }

    /// Resolve every re-export alias symbol to the declaration keys it exposes.
    /// `resolve_export` returns the alias symbol itself for a barrel, so the
    /// absent-prop rule follows the same re-export edges the graph uses for
    /// reachability. A cyclic alias set resolves to nothing rather than looping.
    fn reexport_alias_targets(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut direct: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for edge in &self.edges {
            if edge.kind == "re-export" {
                direct
                    .entry(edge.source.clone())
                    .or_default()
                    .insert(edge.target.clone());
            }
        }
        let mut resolved = BTreeMap::new();
        for alias in direct.keys() {
            let mut origins = BTreeSet::new();
            let mut seen = BTreeSet::new();
            let mut stack = vec![alias.clone()];
            while let Some(key) = stack.pop() {
                if !seen.insert(key.clone()) {
                    continue;
                }
                match direct.get(&key) {
                    Some(targets) => stack.extend(targets.iter().cloned()),
                    None => {
                        origins.insert(key);
                    }
                }
            }
            resolved.insert(alias.clone(), origins);
        }
        resolved
    }

    /// Every declaration key exposed from a public package file or an entry.
    /// Named re-exports are resolved to their origin, and `export *` chains are
    /// followed through the resolver so a barrel cannot hide a public surface.
    fn public_component_surface(
        &self,
        aliases: &BTreeMap<String, BTreeSet<String>>,
    ) -> BTreeSet<String> {
        let mut roots: Vec<String> = self.public_files.iter().cloned().collect();
        for entry in &self.entries {
            if let Some(file) = entry.symbol.split('#').next() {
                roots.push(file.to_string());
            }
        }
        self.export_surface(&roots, aliases)
    }

    /// The declaration keys exposed by the given files, following `export *`
    /// and resolving re-export aliases to their origins.
    fn export_surface(
        &self,
        roots: &[String],
        aliases: &BTreeMap<String, BTreeSet<String>>,
    ) -> BTreeSet<String> {
        let mut surface = BTreeSet::new();
        let mut queue = roots.to_vec();
        let mut visited = BTreeSet::new();
        while let Some(file) = queue.pop() {
            if !visited.insert(file.clone()) {
                continue;
            }
            if let Some(exports) = self.exports.get(&file) {
                // Every name in this map is a direct export, and
                // `resolve_export` returns the map value without recursion, so
                // iterate the values instead of re-looking-up each name.
                for key in exports.values() {
                    match aliases.get(key) {
                        Some(origins) => surface.extend(origins.iter().cloned()),
                        None => {
                            surface.insert(key.clone());
                        }
                    }
                }
            }
            if let Some(alls) = self.export_alls.get(&file) {
                for source in alls {
                    for target in self.resolver.resolve(&file, source).targets() {
                        queue.push(target.clone());
                    }
                }
            }
        }
        surface
    }
}

fn origin_keys(aliases: &BTreeMap<String, BTreeSet<String>>, key: &str) -> BTreeSet<String> {
    aliases
        .get(key)
        .cloned()
        .unwrap_or_else(|| BTreeSet::from([key.to_string()]))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use shrike_core::{Diagnostic, EngineConfig};
    use tempfile::tempdir;

    use crate::discovery::analyze_paths;
    use crate::types::GraphSnapshot;

    fn snapshot(files: &[(&str, &str)]) -> GraphSnapshot {
        let directory = tempdir().expect("tempdir");
        for (path, content) in files {
            let full = directory.path().join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).expect("create parents");
            }
            fs::write(full, content).expect("write fixture");
        }
        let paths: Vec<String> = files.iter().map(|(path, _)| (*path).to_string()).collect();
        analyze_paths(directory.path(), &EngineConfig::default(), &paths)
    }

    fn prop_diagnostics(snapshot: &GraphSnapshot) -> Vec<&Diagnostic> {
        snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "unused_component_prop")
            .collect()
    }

    fn react_package() -> &'static str {
        r#"{"name":"probe","private":true,"main":"src/index.ts","dependencies":{"react":"^19.0.0"}}"#
    }

    #[test]
    fn reports_unused_prop_on_live_local_component() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            (
                "src/index.ts",
                "import { Widget } from \"./Widget\";\nWidget();\n",
            ),
            (
                "src/Widget.tsx",
                r#"
function Badge({ label, tone }: { label: string; tone: string }) {
  return <span>{label}</span>;
}
export function Widget() {
  return <Badge label="hello" tone="danger" />;
}
"#,
            ),
        ]);
        let findings = prop_diagnostics(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].file.as_deref(), Some("src/Widget.tsx"));
        assert!(findings[0].message.contains("`Badge`"));
        assert!(findings[0].message.contains("`tone`"));
        assert_eq!(findings[0].span.map(|span| span.line), Some(2));
    }

    #[test]
    fn does_not_report_without_a_react_runtime() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"probe","private":true,"main":"src/index.ts"}"#,
            ),
            (
                "src/index.ts",
                "import { Widget } from \"./Widget\";\nWidget();\n",
            ),
            (
                "src/Widget.tsx",
                r#"
function Badge({ label, tone }: { label: string; tone: string }) {
  return <span>{label}</span>;
}
export function Widget() {
  return <Badge label="hello" tone="danger" />;
}
"#,
            ),
        ]);
        assert!(prop_diagnostics(&snapshot).is_empty());
    }

    #[test]
    fn abstains_on_exported_hoc_exported_and_aliased_components() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            (
                "src/index.ts",
                "import { Widget } from \"./Widget\";\nWidget();\n",
            ),
            (
                "src/Widget.tsx",
                r#"
import { memo } from "react";

function Hoc({ title, subtitle }: { title: string; subtitle: string }) {
  return <h2>{title}</h2>;
}
export default memo(Hoc);

export function Exported({ title, subtitle }: { title: string; subtitle: string }) {
  return <h2>{title}</h2>;
}

function Alias({ title, subtitle }: { title: string; subtitle: string }) {
  return <h2>{title}</h2>;
}
export { Alias as PublicAlias };

function Local({ title, subtitle }: { title: string; subtitle: string }) {
  return <h2>{title}</h2>;
}
export function Widget() {
  return <Local title="a" subtitle="b" />;
}
"#,
            ),
        ]);
        let findings = prop_diagnostics(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("`Local`"));
        assert!(findings[0].message.contains("`subtitle`"));
    }

    #[test]
    fn reports_alias_binding_in_message() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            (
                "src/index.ts",
                "import { Widget } from \"./Widget\";\nWidget();\n",
            ),
            (
                "src/Widget.tsx",
                r#"
function Badge({ tone: shade, label }: { tone: string; label: string }) {
  return <span>{label}</span>;
}
export function Widget() {
  return <Badge tone="danger" label="hello" />;
}
"#,
            ),
        ]);
        let findings = prop_diagnostics(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("(bound as `shade`)"));
    }

    #[test]
    fn reports_unused_prop_on_typed_interface_component() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            (
                "src/index.ts",
                "import { Widget } from \"./Widget\";\nWidget();\n",
            ),
            (
                "src/Widget.tsx",
                r#"
interface BadgeProps {
  label: string;
  tone: string;
}

function Badge(props: BadgeProps) {
  return <span>{props.label}</span>;
}
export function Widget() {
  return <Badge label="hello" tone="danger" />;
}
"#,
            ),
        ]);
        let findings = prop_diagnostics(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].file.as_deref(), Some("src/Widget.tsx"));
        assert!(findings[0].message.contains("`Badge` declares prop `tone`"));
        assert_eq!(findings[0].span.map(|span| span.line), Some(4));
        assert!(
            !snapshot
                .candidates
                .contains(&"src/Widget.tsx#Badge".to_string()),
            "props never create deletion candidates: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn reports_unused_prop_on_local_wrapped_component() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            (
                "src/index.ts",
                "import { Widget } from \"./Widget\";\nWidget();\n",
            ),
            (
                "src/Widget.tsx",
                r#"
import { forwardRef, memo } from "react";

interface CardProps {
  title: string;
  subtitle: string;
}

const MemoCard = memo((props: CardProps) => <h2>{props.title}</h2>);
const ForwardCard = forwardRef<HTMLHeadingElement, CardProps>((props, ref) => (
  <h2 ref={ref}>{props.title}</h2>
));

export function Widget() {
  return (
    <>
      <MemoCard title="a" subtitle="b" />
      <ForwardCard title="c" subtitle="d" />
    </>
  );
}
"#,
            ),
        ]);
        let findings = prop_diagnostics(&snapshot);
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(
            findings
                .iter()
                .all(|finding| finding.message.contains("`subtitle`")),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.message.contains("`MemoCard`")),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.message.contains("`ForwardCard`")),
            "{findings:?}"
        );
    }

    #[test]
    fn does_not_report_props_on_dead_components() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            (
                "src/index.ts",
                "import { Widget } from \"./Widget\";\nWidget();\n",
            ),
            (
                "src/Widget.tsx",
                r#"
function Dead({ title, subtitle }: { title: string; subtitle: string }) {
  return <h2>{title}</h2>;
}
export function Widget() {
  return <h2>ok</h2>;
}
"#,
            ),
        ]);
        assert!(prop_diagnostics(&snapshot).is_empty());
        assert!(
            snapshot
                .candidates
                .contains(&"src/Widget.tsx#Dead".to_string()),
            "the dead component itself stays a deletion candidate: {:?}",
            snapshot.candidates
        );
    }
}

#[cfg(test)]
mod absent_tests {
    use std::fs;

    use shrike_core::{Diagnostic, EngineConfig};
    use tempfile::tempdir;

    use crate::discovery::analyze_paths;
    use crate::types::GraphSnapshot;

    fn snapshot(files: &[(&str, &str)]) -> GraphSnapshot {
        let directory = tempdir().expect("tempdir");
        for (path, content) in files {
            let full = directory.path().join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).expect("create parents");
            }
            fs::write(full, content).expect("write fixture");
        }
        let paths: Vec<String> = files.iter().map(|(path, _)| (*path).to_string()).collect();
        analyze_paths(directory.path(), &EngineConfig::default(), &paths)
    }

    fn absent(snapshot: &GraphSnapshot) -> Vec<&Diagnostic> {
        snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "absent_component_prop")
            .collect()
    }

    fn react_package() -> &'static str {
        r#"{"name":"probe","private":true,"main":"src/index.tsx","dependencies":{"react":"^19.0.0"}}"#
    }

    const CARD: &str = r#"
export function Card({ title, flag }: { title: string; flag?: boolean }) {
  return <p>{title}{flag ? "on" : ""}</p>;
}
"#;

    #[test]
    fn reports_optional_prop_missing_from_every_render_site() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            ("src/Card.tsx", CARD),
            (
                "src/index.tsx",
                "import { Card } from \"./Card\";\nexport const app = <Card title=\"x\" />;\n",
            ),
        ]);
        let findings = absent(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].file.as_deref(), Some("src/Card.tsx"));
        assert!(findings[0].message.contains("`Card`"));
        assert!(findings[0].message.contains("`flag`"));
        assert!(findings[0].message.contains("only known render site"));
        assert_eq!(findings[0].span.map(|span| span.line), Some(2));
    }

    #[test]
    fn does_not_report_a_supplied_optional_prop() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            ("src/Card.tsx", CARD),
            (
                "src/index.tsx",
                "import { Card } from \"./Card\";\nexport const app = <Card title=\"x\" flag />;\n",
            ),
        ]);
        assert!(absent(&snapshot).is_empty());
    }

    #[test]
    fn does_not_report_when_any_render_site_supplies_the_prop() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            ("src/Card.tsx", CARD),
            (
                "src/index.tsx",
                "import { Card } from \"./Card\";\nexport const app = <><Card title=\"x\" /><Card title=\"y\" flag /></>;\n",
            ),
        ]);
        assert!(absent(&snapshot).is_empty());
    }

    #[test]
    fn abstains_on_spread_and_namespace_render_sites() {
        let spread = snapshot(&[
            ("package.json", react_package()),
            ("src/Card.tsx", CARD),
            (
                "src/index.tsx",
                "import { Card } from \"./Card\";\nexport function App({ rest }: { rest: object }) { return <Card title=\"x\" {...rest} />; }\n",
            ),
        ]);
        assert!(absent(&spread).is_empty());

        let namespace = snapshot(&[
            ("package.json", react_package()),
            ("src/Card.tsx", CARD),
            (
                "src/index.tsx",
                "import * as UI from \"./Card\";\nexport const app = <UI.Card title=\"x\" />;\n",
            ),
        ]);
        assert!(absent(&namespace).is_empty());
    }

    #[test]
    fn abstains_when_there_are_no_render_sites() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            ("src/Card.tsx", CARD),
            ("src/index.tsx", "export const app = 1;\n"),
        ]);
        assert!(absent(&snapshot).is_empty());
    }

    #[test]
    fn abstains_on_escaped_components() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            ("src/Card.tsx", CARD),
            (
                "src/index.tsx",
                "import { Card } from \"./Card\";\nexport const app = <Card title=\"x\" />;\nexport const registry = { Card };\n",
            ),
        ]);
        assert!(absent(&snapshot).is_empty());
    }

    #[test]
    fn abstains_when_the_only_render_site_tag_is_shadowed() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            ("src/Card.tsx", CARD),
            (
                "src/index.tsx",
                r#"
import { Card } from "./Card";
function Shadow(Card: () => unknown) {
  return <Card />;
}
export const app = <Shadow />;
"#,
            ),
        ]);
        assert!(absent(&snapshot).is_empty(), "{:?}", absent(&snapshot));
    }

    #[test]
    fn abstains_when_a_namespace_import_escapes_as_a_value() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            ("src/Card.tsx", CARD),
            (
                "src/index.tsx",
                r#"
import { Card } from "./Card";
import * as UI from "./Card";
export const app = <Card title="x" />;
export const registry = { UI };
"#,
            ),
        ]);
        assert!(absent(&snapshot).is_empty(), "{:?}", absent(&snapshot));
    }

    #[test]
    fn abstains_when_a_reachable_module_dynamically_imports_the_component() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            ("src/Card.tsx", CARD),
            (
                "src/index.tsx",
                r#"
import { Card } from "./Card";
export const app = <Card title="x" />;
export function load() {
  return import("./Card");
}
"#,
            ),
        ]);
        assert!(absent(&snapshot).is_empty(), "{:?}", absent(&snapshot));
    }

    #[test]
    fn abstains_on_public_package_surface() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            (
                "src/index.tsx",
                "export function Card({ title, flag }: { title: string; flag?: boolean }) { return <p>{title}{flag ? \"on\" : \"\"}</p>; }\nexport const app = <Card title=\"x\" />;\n",
            ),
        ]);
        assert!(absent(&snapshot).is_empty());
    }

    #[test]
    fn does_not_report_without_a_react_runtime() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"probe","private":true,"main":"src/index.tsx"}"#,
            ),
            ("src/Card.tsx", CARD),
            (
                "src/index.tsx",
                "import { Card } from \"./Card\";\nexport const app = <Card title=\"x\" />;\n",
            ),
        ]);
        assert!(absent(&snapshot).is_empty());
    }

    #[test]
    fn resolves_barrel_aliases_to_the_origin_component() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            ("src/Card.tsx", CARD),
            (
                "src/barrel.ts",
                "export { Card as Public } from \"./Card\";\n",
            ),
            (
                "src/index.tsx",
                "import { Public as Alias } from \"./barrel\";\nexport const app = <Alias title=\"x\" />;\n",
            ),
        ]);
        let findings = absent(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("`Card`"));
        assert!(findings[0].message.contains("`flag`"));
    }

    #[test]
    fn namespace_import_of_a_barrel_blocks_the_reexported_component() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            ("src/Card.tsx", CARD),
            ("src/barrel.ts", "export * from \"./Card\";\n"),
            (
                "src/index.tsx",
                "import * as UI from \"./barrel\";\nexport const app = <UI.Card title=\"x\" />;\n",
            ),
        ]);
        assert!(absent(&snapshot).is_empty());
    }

    #[test]
    fn reports_typed_optional_prop_and_mentions_the_default() {
        let typed = snapshot(&[
            ("package.json", react_package()),
            (
                "src/Card.tsx",
                r#"
interface CardProps {
  title: string;
  flag?: boolean;
}

export function Card(props: CardProps) {
  return <p>{props.title}{props.flag ? "on" : ""}</p>;
}
"#,
            ),
            (
                "src/index.tsx",
                "import { Card } from \"./Card\";\nexport const app = <Card title=\"x\" />;\n",
            ),
        ]);
        let findings = absent(&typed);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("`flag`"));
        assert_eq!(findings[0].span.map(|span| span.line), Some(4));

        let defaulted = snapshot(&[
            ("package.json", react_package()),
            (
                "src/Card.tsx",
                "export function Card({ flag = false }: { flag?: boolean }) { return <p>{flag ? \"on\" : \"\"}</p>; }\n",
            ),
            (
                "src/index.tsx",
                "import { Card } from \"./Card\";\nexport const app = <Card />;\n",
            ),
        ]);
        let findings = absent(&defaulted);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("defines a default"));
    }

    #[test]
    fn reports_absent_prop_on_default_exported_const_component() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            (
                "src/Card.tsx",
                r#"
interface CardProps {
  title: string;
  flag?: boolean;
}

const Card = ({ title, flag }: CardProps) => {
  return <p>{title}{flag ? "on" : ""}</p>;
};

export default Card;
"#,
            ),
            (
                "src/index.tsx",
                "import Card from \"./Card\";\nexport const app = <Card title=\"x\" />;\n",
            ),
        ]);
        let findings = absent(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("`Card`"));
        assert!(findings[0].message.contains("`flag`"));
    }

    #[test]
    fn reports_absent_prop_on_local_default_export_alias() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            (
                "src/Card.tsx",
                r#"
const Card = ({ title, flag }: { title: string; flag?: boolean }) => {
  return <p>{title}{flag ? "on" : ""}</p>;
};

export { Card as default };
"#,
            ),
            (
                "src/index.tsx",
                "import Card from \"./Card\";\nexport const app = <Card title=\"x\" />;\n",
            ),
        ]);
        let findings = absent(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("`Card`"));
        assert!(findings[0].message.contains("`flag`"));
    }

    #[test]
    fn does_not_report_an_unread_optional_prop() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            (
                "src/Card.tsx",
                "export function Card({ title, flag }: { title: string; flag?: boolean }) { return <p>{title}</p>; }\n",
            ),
            (
                "src/index.tsx",
                "import { Card } from \"./Card\";\nexport const app = <Card title=\"x\" />;\n",
            ),
        ]);
        assert!(absent(&snapshot).is_empty());
    }
}
