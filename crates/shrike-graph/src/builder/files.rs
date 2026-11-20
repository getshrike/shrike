use std::collections::BTreeMap;

use shrike_core::glob::glob_match;
use shrike_core::ids::{module_symbol_key, symbol_key};
use shrike_core::{Diagnostic, Span};
use shrike_syntax::{FileSummary, ParseStatus};

use super::Builder;
use crate::types::{FileRecord, SymbolRecord};
use crate::util::{declaration_kind, edge};

impl<'a> Builder<'a> {
    pub(super) fn build_files(&mut self) {
        let mut diagnostics = Vec::new();
        let summaries = self.summaries.clone();
        for summary in &summaries {
            let package = self
                .resolver
                .package_for(&summary.path)
                .map(|p| p.name.clone());
            if let Some(package) = &package {
                self.package_of
                    .insert(summary.path.clone(), package.clone());
            }
            let generated = summary.generated
                || self
                    .config
                    .generated
                    .iter()
                    .any(|glob| glob_match(glob, &summary.path))
                || summary.path.ends_with(".min.js")
                || summary.path.contains(".generated.");
            let public_api = self
                .config
                .public_api
                .iter()
                .any(|glob| glob_match(glob, &summary.path))
                || summary.path.ends_with(".d.ts")
                || self
                    .resolver
                    .package_for(&summary.path)
                    .and_then(|package| package.entry.as_deref())
                    == Some(summary.path.as_str());
            if public_api {
                self.public_files.insert(summary.path.clone());
                self.file_public.insert(summary.path.clone());
            }
            self.file_generated.insert(summary.path.clone(), generated);
            self.file_lines
                .insert(summary.path.clone(), summary.line_count);
            if summary.parse_status == ParseStatus::Ok {
                self.file_parse_ok.insert(summary.path.clone());
            }
            self.files.push(FileRecord {
                path: summary.path.clone(),
                content_hash: summary.content_hash.clone(),
                module_kind: summary.module_kind.clone(),
                parse_status: summary.parse_status,
                generated,
                package,
                public_api,
                lines: summary.line_count,
            });
            if summary.parse_status != ParseStatus::Ok {
                diagnostics.extend(summary.diagnostics.clone());
                let component = matches!(
                    summary
                        .path
                        .rsplit_once('.')
                        .map(|(_, extension)| extension),
                    Some("vue" | "svelte" | "astro")
                );
                if !component {
                    diagnostics.push(
                        Diagnostic::warning(
                            "unparsed_file",
                            "file could not be parsed; its symbols are not deletion candidates",
                        )
                        .in_file(&summary.path),
                    );
                }
            }
        }
        self.diagnostics = diagnostics;
    }

    fn add_symbol(
        &mut self,
        summary: &FileSummary,
        name: &str,
        kind: &str,
        span: Span,
        exported: bool,
        type_only: bool,
    ) -> String {
        let key = if name == "default" {
            symbol_key(&summary.path, "default")
        } else {
            symbol_key(&summary.path, name)
        };
        if let Some(index) = self.symbol_index.get(&key) {
            let record = &mut self.symbols[*index];
            record.exported |= exported;
            record.type_only &= type_only;
            return key;
        }
        let generated = self
            .file_generated
            .get(&summary.path)
            .copied()
            .unwrap_or(false);
        let public_api = self.file_public.contains(&summary.path) && exported;
        let record = SymbolRecord {
            key: key.clone(),
            name: name.to_string(),
            kind: kind.to_string(),
            file: summary.path.clone(),
            span,
            exported,
            entry: false,
            public_api,
            generated,
            type_only,
            lines: 1,
        };
        self.symbol_index.insert(key.clone(), self.symbols.len());
        self.symbols.push(record);
        key
    }

    pub(super) fn build_symbols(&mut self) {
        let summaries = self.summaries.clone();
        for summary in &summaries {
            if summary.parse_status != ParseStatus::Ok {
                // Framework files with their own compilers (Svelte, Astro, Vue)
                // are still entries. They have no parsed symbols, so only the
                // module key exists; `detect_entries` marks it live.
                if super::entries::framework_module(&summary.path).is_some() {
                    self.add_symbol(
                        summary,
                        "__module__",
                        "module",
                        Span::default(),
                        false,
                        false,
                    );
                    self.exports.insert(summary.path.clone(), BTreeMap::new());
                    self.locals.insert(summary.path.clone(), BTreeMap::new());
                }
                continue;
            }
            let module_key = module_symbol_key(&summary.path);
            self.add_symbol(
                summary,
                "__module__",
                "module",
                Span::default(),
                false,
                false,
            );
            let _ = module_key;
            let mut local_map = BTreeMap::new();
            let mut export_map = BTreeMap::new();
            for declaration in &summary.declarations {
                let kind = declaration_kind(declaration.kind);
                if declaration.default {
                    let key = self.add_symbol(
                        summary,
                        "default",
                        "default",
                        declaration.span,
                        true,
                        declaration.type_only,
                    );
                    local_map.insert(declaration.name.clone(), key.clone());
                    for name in &declaration.names {
                        local_map.insert(name.clone(), key.clone());
                    }
                    export_map.insert("default".to_string(), key);
                } else {
                    let mut first: Option<String> = None;
                    for name in &declaration.names {
                        let key = self.add_symbol(
                            summary,
                            name,
                            kind,
                            declaration.span,
                            declaration.exported,
                            declaration.type_only,
                        );
                        local_map.insert(name.clone(), key.clone());
                        if declaration.exported {
                            export_map.insert(name.clone(), key.clone());
                        }
                        if first.is_none() {
                            first = Some(key);
                        }
                    }
                    if let Some(first) = first {
                        local_map.insert(declaration.name.clone(), first);
                    }
                }
            }
            for local_export in &summary.local_exports {
                let key = symbol_key(&summary.path, &local_export.exported);
                if let Some(local) = local_map.get(&local_export.local).cloned() {
                    self.add_alias_symbol(
                        summary,
                        &local_export.exported,
                        local_export.span,
                        local_export.type_only,
                    );
                    export_map.insert(local_export.exported.clone(), key.clone());
                    self.edges.push(edge(
                        &key,
                        &local,
                        "re-export",
                        "certain",
                        &summary.path,
                        local_export.span,
                    ));
                }
            }
            for re_export in &summary.re_exports {
                let key = self.add_alias_symbol(
                    summary,
                    &re_export.exported,
                    re_export.span,
                    re_export.type_only,
                );
                export_map.insert(re_export.exported.clone(), key);
            }
            for export_all in &summary.export_alls {
                if let Some(exported) = &export_all.exported {
                    let key = self.add_alias_symbol(
                        summary,
                        exported,
                        export_all.span,
                        export_all.type_only,
                    );
                    export_map.insert(exported.clone(), key);
                }
                self.export_alls
                    .entry(summary.path.clone())
                    .or_default()
                    .push(export_all.source.clone());
            }
            self.exports.insert(summary.path.clone(), export_map);
            self.locals.insert(summary.path.clone(), local_map);
        }
    }

    fn add_alias_symbol(
        &mut self,
        summary: &FileSummary,
        name: &str,
        span: Span,
        type_only: bool,
    ) -> String {
        self.add_symbol(summary, name, "re-export", span, true, type_only)
    }
}
