use std::collections::BTreeSet;

use rayon::prelude::*;
use shrike_core::Span;
use shrike_core::ids::{module_symbol_key, symbol_key};
use shrike_syntax::{FileSummary, MemberAccess, ParseStatus};

use super::Builder;
use crate::types::{EdgeRecord, UnknownRecord};
use crate::util::edge;

impl<'a> Builder<'a> {
    pub(super) fn build_edges(&mut self) {
        let summaries = self.summaries.clone();
        // Each file builds its edges and unknowns independently. Collecting in
        // summary order and appending afterwards keeps the pre-sort stream
        // identical to the sequential version.
        let results: Vec<(Vec<EdgeRecord>, Vec<UnknownRecord>)> = summaries
            .par_iter()
            .filter(|summary| summary.parse_status == ParseStatus::Ok)
            .map(|summary| {
                let mut edges = Vec::new();
                let mut unknowns = Vec::new();
                self.build_summary_edges(summary, &mut edges, &mut unknowns);
                (edges, unknowns)
            })
            .collect();
        for (mut part, unknown_part) in results {
            self.edges.append(&mut part);
            self.unknowns.extend(unknown_part);
        }
    }

    fn build_summary_edges(
        &self,
        summary: &FileSummary,
        edges: &mut Vec<EdgeRecord>,
        unknowns: &mut Vec<UnknownRecord>,
    ) {
        let module = module_symbol_key(&summary.path);
        for declaration in &summary.declarations {
            let source = if declaration.default {
                symbol_key(&summary.path, "default")
            } else {
                self.locals
                    .get(&summary.path)
                    .and_then(|map| map.get(&declaration.name))
                    .cloned()
                    .unwrap_or_else(|| symbol_key(&summary.path, &declaration.name))
            };
            let refs = &declaration.references;
            for identifier in &refs.identifiers {
                if identifier == &declaration.name {
                    continue;
                }
                if let Some(target) = self.local_or_imported(&summary.path, identifier) {
                    if target != source {
                        edges.push(edge(
                            &source,
                            &target,
                            "reference",
                            "certain",
                            &summary.path,
                            declaration.span,
                        ));
                    }
                }
            }
            for jsx in &refs.jsx {
                if let Some(target) = self.local_or_imported(&summary.path, jsx) {
                    if target != source {
                        edges.push(edge(
                            &source,
                            &target,
                            "jsx",
                            "certain",
                            &summary.path,
                            declaration.span,
                        ));
                    }
                }
            }
            self.add_member_edges(
                edges,
                unknowns,
                &source,
                &summary.path,
                declaration.span,
                &refs.members,
            );
            self.mark_namespace_value_uses(unknowns, &summary.path, declaration.span, &refs.free);
        }
        // Module-level references keep top-level side effects and roots live.
        for identifier in &summary.module_refs.identifiers {
            if let Some(target) = self.local_or_imported(&summary.path, identifier) {
                edges.push(edge(
                    &module,
                    &target,
                    "reference",
                    "certain",
                    &summary.path,
                    Span::default(),
                ));
            }
        }
        for jsx in &summary.module_refs.jsx {
            if let Some(target) = self.local_or_imported(&summary.path, jsx) {
                edges.push(edge(
                    &module,
                    &target,
                    "jsx",
                    "certain",
                    &summary.path,
                    Span::default(),
                ));
            }
        }
        self.add_member_edges(
            edges,
            unknowns,
            &module,
            &summary.path,
            Span::default(),
            &summary.module_refs.members,
        );
        self.mark_namespace_value_uses(
            unknowns,
            &summary.path,
            Span::default(),
            &summary.module_refs.free,
        );
        for re_export in &summary.re_exports {
            let source = symbol_key(&summary.path, &re_export.exported);
            let resolution = self.resolver.resolve(&summary.path, &re_export.source);
            for target_file in resolution.targets() {
                if let Some(target) =
                    self.resolve_export(target_file, &re_export.imported, &mut BTreeSet::new())
                {
                    edges.push(edge(
                        &source,
                        &target,
                        "re-export",
                        "certain",
                        &summary.path,
                        re_export.span,
                    ));
                }
            }
        }
        // `export { X }` can re-export an imported binding instead of a
        // local declaration. The alias symbol is created with the other
        // exports; connect it to the resolved import so the chain is
        // actionable exactly like `export { X } from "./module"`.
        for local_export in &summary.local_exports {
            if self
                .locals
                .get(&summary.path)
                .is_some_and(|map| map.contains_key(&local_export.local))
            {
                continue;
            }
            let source = symbol_key(&summary.path, &local_export.exported);
            let target = self
                .imports
                .get(&summary.path)
                .and_then(|map| map.get(&local_export.local))
                .cloned()
                .and_then(|(target_file, imported)| {
                    self.resolve_export(&target_file, &imported, &mut BTreeSet::new())
                });
            if let Some(target) = target {
                edges.push(edge(
                    &source,
                    &target,
                    "re-export",
                    "certain",
                    &summary.path,
                    local_export.span,
                ));
            }
        }
        if let Some(alls) = self.export_alls.get(&summary.path) {
            for source in alls {
                let resolution = self.resolver.resolve(&summary.path, source);
                for target_file in resolution.targets() {
                    if let Some(target_exports) = self.exports.get(target_file) {
                        for target in target_exports.values() {
                            edges.push(edge(
                                &module,
                                target,
                                "re-export",
                                "certain",
                                &summary.path,
                                Span::default(),
                            ));
                        }
                    }
                }
            }
        }
    }

    fn add_member_edges(
        &self,
        edges: &mut Vec<EdgeRecord>,
        unknowns: &mut Vec<UnknownRecord>,
        source: &str,
        path: &str,
        span: Span,
        members: &[MemberAccess],
    ) {
        for member in members {
            let Some(target_files) = self
                .namespaces
                .get(path)
                .and_then(|map| map.get(&member.object))
            else {
                continue;
            };
            for target_file in target_files {
                match &member.property {
                    Some(property) => {
                        if let Some(target) =
                            self.resolve_export(target_file, property, &mut BTreeSet::new())
                        {
                            edges.push(edge(source, &target, "reference", "certain", path, span));
                        } else {
                            unknowns.push(UnknownRecord {
                                reason: format!("unknown member `{}`", member.object),
                                scope: "module".to_string(),
                                package: self.package_of.get(path).cloned(),
                                file: target_file.clone(),
                                span,
                            });
                        }
                    }
                    None => {
                        unknowns.push(UnknownRecord {
                            reason: format!("computed member access on `{}`", member.object),
                            scope: "module".to_string(),
                            package: self.package_of.get(path).cloned(),
                            file: target_file.clone(),
                            span,
                        });
                    }
                }
            }
        }
    }

    /// A namespace binding used as a value (`const all = { ...UI }`, a
    /// destructured `const { Button } = UI`, `Object.keys(UI)`) can reach any
    /// export. Widen only that target module rather than guessing one member.
    fn mark_namespace_value_uses(
        &self,
        unknowns: &mut Vec<UnknownRecord>,
        path: &str,
        span: Span,
        free: &[String],
    ) {
        let mut targets = Vec::new();
        if let Some(namespaces) = self.namespaces.get(path) {
            for identifier in free {
                if let Some(files) = namespaces.get(identifier) {
                    targets.extend(files.iter().cloned());
                }
            }
        }
        targets.sort();
        targets.dedup();
        for target in targets {
            unknowns.push(UnknownRecord {
                reason: format!("namespace import used as a value in `{path}`"),
                scope: "module".to_string(),
                package: self.package_of.get(path).cloned(),
                file: target,
                span,
            });
        }
    }

    pub(super) fn finish_edges(&mut self) {
        // Stable parallel sort: the pre-sort stream is deterministic, so equal
        // keys keep their original order and the snapshot bytes are unchanged.
        self.edges.par_sort_by(|a, b| {
            a.source
                .cmp(&b.source)
                .then_with(|| a.target.cmp(&b.target))
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.file.cmp(&b.file))
                .then_with(|| a.span.start.cmp(&b.span.start))
        });
        self.edges.dedup();
        self.entries.sort_by(|a, b| {
            a.symbol
                .cmp(&b.symbol)
                .then_with(|| a.framework.cmp(&b.framework))
        });
        self.entries
            .dedup_by(|a, b| a.symbol == b.symbol && a.framework == b.framework);
    }
}
