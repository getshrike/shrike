use std::collections::BTreeSet;

use shrike_core::Span;
use shrike_core::ids::{module_symbol_key, symbol_key};
use shrike_syntax::{MemberAccess, ParseStatus};

use super::Builder;
use crate::types::UnknownRecord;
use crate::util::edge;

impl<'a> Builder<'a> {
    pub(super) fn build_edges(&mut self) {
        let summaries = self.summaries.clone();
        for summary in &summaries {
            if summary.parse_status != ParseStatus::Ok {
                continue;
            }
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
                            self.edges.push(edge(
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
                            self.edges.push(edge(
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
                self.add_member_edges(&source, &summary.path, declaration.span, &refs.members);
                self.mark_namespace_value_uses(&summary.path, declaration.span, &refs.free);
            }
            // Module-level references keep top-level side effects and roots live.
            for identifier in &summary.module_refs.identifiers {
                if let Some(target) = self.local_or_imported(&summary.path, identifier) {
                    self.edges.push(edge(
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
                    self.edges.push(edge(
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
                &module,
                &summary.path,
                Span::default(),
                &summary.module_refs.members,
            );
            self.mark_namespace_value_uses(
                &summary.path,
                Span::default(),
                &summary.module_refs.free,
            );
            for re_export in &summary.re_exports {
                let source = symbol_key(&summary.path, &re_export.exported);
                if let Some(target_file) = self
                    .resolver
                    .resolve(&summary.path, &re_export.source)
                    .target
                {
                    if let Some(target) =
                        self.resolve_export(&target_file, &re_export.imported, &mut BTreeSet::new())
                    {
                        self.edges.push(edge(
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
            if let Some(alls) = self.export_alls.get(&summary.path).cloned() {
                for source in alls {
                    if let Some(target_file) = self.resolver.resolve(&summary.path, &source).target
                    {
                        if let Some(target_exports) = self.exports.get(&target_file).cloned() {
                            for (_, target) in target_exports {
                                self.edges.push(edge(
                                    &module,
                                    &target,
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
    }

    fn add_member_edges(&mut self, source: &str, path: &str, span: Span, members: &[MemberAccess]) {
        for member in members {
            let Some(target_file) = self
                .namespaces
                .get(path)
                .and_then(|map| map.get(&member.object))
            else {
                continue;
            };
            match &member.property {
                Some(property) => {
                    if let Some(target) =
                        self.resolve_export(target_file, property, &mut BTreeSet::new())
                    {
                        self.edges
                            .push(edge(source, &target, "reference", "certain", path, span));
                    } else {
                        self.unknowns.push(UnknownRecord {
                            reason: format!("unknown member `{}`", member.object),
                            scope: "module".to_string(),
                            package: self.package_of.get(path).cloned(),
                            file: target_file.clone(),
                            span,
                        });
                    }
                }
                None => {
                    self.unknowns.push(UnknownRecord {
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

    /// A namespace binding used as a value (`const all = { ...UI }`, a
    /// destructured `const { Button } = UI`, `Object.keys(UI)`) can reach any
    /// export. Widen only that target module rather than guessing one member.
    fn mark_namespace_value_uses(&mut self, path: &str, span: Span, free: &[String]) {
        let mut targets = Vec::new();
        if let Some(namespaces) = self.namespaces.get(path) {
            for identifier in free {
                if let Some(target) = namespaces.get(identifier) {
                    targets.push(target.clone());
                }
            }
        }
        targets.sort();
        targets.dedup();
        for target in targets {
            self.unknowns.push(UnknownRecord {
                reason: format!("namespace import used as a value in `{path}`"),
                scope: "module".to_string(),
                package: self.package_of.get(path).cloned(),
                file: target,
                span,
            });
        }
    }

    pub(super) fn finish_edges(&mut self) {
        self.edges.sort_by(|a, b| {
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
