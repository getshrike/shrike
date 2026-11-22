//! Unused TypeScript enum member diagnostics.
//!
//! This is a warning, never a deletion candidate: removing a member changes a
//! serialized value or a public contract, and Shrike's batch deletion model
//! only removes whole symbols. The parser has already abstained on ambient
//! enums, duplicate members, and computed member names. The graph reports a
//! member only when the enum itself is live and exported, is not public API,
//! and no project reference reads the member.
//!
//! Crediting is deliberately liberal. A whole-object use (`Object.values`,
//! spread, a bare `Status`), a computed access (`Status[key]`), a numeric
//! reverse lookup (`Status[0]`), a namespace member (`types.Status`), or a
//! dynamic import that can reach the enum credits every member. Over-crediting
//! can hide a finding; it can never invent one.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;
use shrike_core::Diagnostic;
use shrike_core::ids::symbol_key;
use shrike_syntax::{DynamicSpecifier, MemberAccess, ModuleRefs, ParseStatus};

use super::Builder;

#[derive(Default)]
struct EnumUsage {
    all: bool,
    members: BTreeSet<String>,
}

impl EnumUsage {
    fn credit_all(&mut self) {
        self.all = true;
    }

    fn credit(&mut self, member: &str) {
        self.members.insert(member.to_string());
    }
}

impl<'a> Builder<'a> {
    /// Warn about members of live exported enums that no project reference
    /// reads. `live` is the symbol-level reachable set; a dead enum is already
    /// one deletion candidate, so its members are not reported separately.
    pub(super) fn detect_unused_enum_members(&self, live: &BTreeSet<String>) -> Vec<Diagnostic> {
        let timing = std::env::var_os("SHRIKE_TIMING").is_some();
        let mut diagnostics = Vec::new();
        let summaries = self.summaries.clone();
        let enums_by_file = timed!(timing, "enum_index", self.enums_by_file());
        if enums_by_file.is_empty() {
            return diagnostics;
        }
        let aliases = timed!(timing, "enum_aliases", self.enum_aliases());
        let namespace_aliases = timed!(timing, "enum_namespace_aliases", self.namespace_aliases());
        let usage: BTreeMap<String, EnumUsage> = timed!(timing, "enum_collect", {
            summaries
                .par_iter()
                .filter(|summary| summary.parse_status == ParseStatus::Ok)
                .map(|summary| {
                    let mut usage = BTreeMap::new();
                    self.collect_enum_refs(
                        &summary.path,
                        &summary.module_refs,
                        &aliases,
                        &namespace_aliases,
                        &enums_by_file,
                        &mut usage,
                    );
                    for declaration in &summary.declarations {
                        self.collect_enum_refs(
                            &summary.path,
                            &declaration.references,
                            &aliases,
                            &namespace_aliases,
                            &enums_by_file,
                            &mut usage,
                        );
                    }
                    self.collect_dynamic_enum_uses(summary, &aliases, &enums_by_file, &mut usage);
                    for enumeration in &summary.enums {
                        let key = symbol_key(&summary.path, &enumeration.name);
                        let entry = usage.entry(key).or_default();
                        for member in &enumeration.members {
                            for referenced in &member.initializer_members {
                                entry.credit(referenced);
                            }
                        }
                    }
                    usage
                })
                .reduce(BTreeMap::new, merge_enum_usage)
        });

        timed!(timing, "enum_report", {
            for summary in summaries.iter() {
                if summary.parse_status != ParseStatus::Ok
                    || summary.module_kind == "dts"
                    || summary.enums.is_empty()
                    || self
                        .file_generated
                        .get(&summary.path)
                        .copied()
                        .unwrap_or(false)
                {
                    continue;
                }
                for enumeration in &summary.enums {
                    if enumeration.ambient || enumeration.opaque {
                        continue;
                    }
                    if summary
                        .enums
                        .iter()
                        .filter(|other| other.name == enumeration.name)
                        .count()
                        > 1
                    {
                        // Declaration merging: accesses cannot be attributed to one
                        // declaration's member set with confidence.
                        continue;
                    }
                    let key = symbol_key(&summary.path, &enumeration.name);
                    if !live.contains(&key) {
                        continue;
                    }
                    let Some(index) = self.symbol_index.get(&key) else {
                        continue;
                    };
                    let symbol = &self.symbols[*index];
                    if symbol.kind != "enum"
                        || !symbol.exported
                        || symbol.entry
                        || symbol.public_api
                        || symbol.generated
                    {
                        continue;
                    }
                    let Some(entry) = usage.get(&key) else {
                        // No access evidence at all: every member is unused.
                        for member in &enumeration.members {
                            push_unused_enum_member(
                                &mut diagnostics,
                                &summary.path,
                                &enumeration.name,
                                member,
                            );
                        }
                        continue;
                    };
                    if entry.all {
                        continue;
                    }
                    for member in &enumeration.members {
                        if entry.members.contains(&member.name) {
                            continue;
                        }
                        push_unused_enum_member(
                            &mut diagnostics,
                            &summary.path,
                            &enumeration.name,
                            member,
                        );
                    }
                }
            }
        });
        diagnostics
    }

    /// Local export aliases (`enum Status {}; export { Status as State }`) are
    /// separate symbols from the declaration. Map each alias back to the enum
    /// so an access through the alias credits the declaration.
    fn enum_aliases(&self) -> BTreeMap<String, String> {
        let mut direct = BTreeMap::new();
        for summary in self.summaries.iter() {
            for local_export in &summary.local_exports {
                let alias = symbol_key(&summary.path, &local_export.exported);
                if let Some(target) = self.local_or_imported(&summary.path, &local_export.local) {
                    direct.insert(alias, target);
                }
            }
            for re_export in &summary.re_exports {
                let alias = symbol_key(&summary.path, &re_export.exported);
                let resolution = self.resolver.resolve(&summary.path, &re_export.source);
                for target_file in resolution.targets() {
                    if let Some(target) =
                        self.resolve_export(target_file, &re_export.imported, &mut BTreeSet::new())
                    {
                        direct.insert(alias.clone(), target);
                    }
                }
            }
        }
        let mut resolved = BTreeMap::new();
        for alias in direct.keys() {
            let target = resolve_alias(&direct, alias);
            if target.as_ref() != alias.as_str() {
                resolved.insert(alias.clone(), target.into_owned());
            }
        }
        resolved
    }

    /// `export * as ns from "./module"` creates a namespace-like alias symbol.
    /// Accessing `ns.Enum.Member` loses the final member in the syntactic
    /// member record, so any enum reached through the alias is credited whole.
    fn namespace_aliases(&self) -> BTreeMap<String, Vec<String>> {
        let mut aliases = BTreeMap::new();
        for summary in self.summaries.iter() {
            for export_all in &summary.export_alls {
                let Some(exported) = &export_all.exported else {
                    continue;
                };
                let key = symbol_key(&summary.path, exported);
                let targets: Vec<String> = self
                    .resolver
                    .resolve(&summary.path, &export_all.source)
                    .targets()
                    .cloned()
                    .collect();
                if !targets.is_empty() {
                    aliases.insert(key, targets);
                }
            }
        }
        aliases
    }

    fn enums_by_file(&self) -> BTreeMap<String, Vec<String>> {
        let mut enums = BTreeMap::new();
        for summary in self.summaries.iter() {
            let keys: Vec<String> = summary
                .enums
                .iter()
                .map(|enumeration| symbol_key(&summary.path, &enumeration.name))
                .collect();
            if !keys.is_empty() {
                enums.insert(summary.path.clone(), keys);
            }
        }
        enums
    }

    fn collect_enum_refs(
        &self,
        path: &str,
        refs: &ModuleRefs,
        aliases: &BTreeMap<String, String>,
        namespace_aliases: &BTreeMap<String, Vec<String>>,
        enums_by_file: &BTreeMap<String, Vec<String>>,
        usage: &mut BTreeMap<String, EnumUsage>,
    ) {
        for member in &refs.members {
            self.collect_member_access(
                path,
                member,
                aliases,
                namespace_aliases,
                enums_by_file,
                usage,
            );
        }
        for name in &refs.free {
            self.collect_whole_object_use(
                path,
                name,
                aliases,
                namespace_aliases,
                enums_by_file,
                usage,
            );
        }
    }

    fn collect_member_access(
        &self,
        path: &str,
        member: &MemberAccess,
        aliases: &BTreeMap<String, String>,
        namespace_aliases: &BTreeMap<String, Vec<String>>,
        enums_by_file: &BTreeMap<String, Vec<String>>,
        usage: &mut BTreeMap<String, EnumUsage>,
    ) {
        let Some(key) = self.resolve_object_key(path, &member.object) else {
            return;
        };
        let key = resolve_alias(aliases, &key);
        if self.is_enum_symbol(key.as_ref()) {
            let entry = usage.entry(key.into_owned()).or_default();
            match &member.property {
                Some(property) if !is_numeric_property(property) => entry.credit(property),
                Some(_) | None => entry.credit_all(),
            }
            return;
        }
        if let Some(targets) = namespace_aliases.get(key.as_ref()) {
            for target in targets {
                credit_all_in_file(target, enums_by_file, usage);
            }
            return;
        }
        if let Some(targets) = self
            .namespaces
            .get(path)
            .and_then(|map| map.get(&member.object))
        {
            for target in targets {
                match &member.property {
                    Some(property) => {
                        if let Some(target_key) =
                            self.resolve_export(target, property, &mut BTreeSet::new())
                        {
                            let target_key = resolve_alias(aliases, &target_key);
                            if self.is_enum_symbol(target_key.as_ref()) {
                                usage
                                    .entry(target_key.into_owned())
                                    .or_default()
                                    .credit_all();
                            }
                        }
                    }
                    None => credit_all_in_file(target, enums_by_file, usage),
                }
            }
        }
    }

    fn collect_whole_object_use(
        &self,
        path: &str,
        name: &str,
        aliases: &BTreeMap<String, String>,
        namespace_aliases: &BTreeMap<String, Vec<String>>,
        enums_by_file: &BTreeMap<String, Vec<String>>,
        usage: &mut BTreeMap<String, EnumUsage>,
    ) {
        let Some(key) = self.resolve_object_key(path, name) else {
            return;
        };
        let key = resolve_alias(aliases, &key);
        if self.is_enum_symbol(key.as_ref()) {
            usage.entry(key.into_owned()).or_default().credit_all();
            return;
        }
        if let Some(targets) = namespace_aliases.get(key.as_ref()) {
            for target in targets {
                credit_all_in_file(target, enums_by_file, usage);
            }
            return;
        }
        if let Some(targets) = self.namespaces.get(path).and_then(|map| map.get(name)) {
            for target in targets {
                credit_all_in_file(target, enums_by_file, usage);
            }
        }
    }

    /// A literal dynamic import can reach an enum without leaving a static
    /// import binding. Its `imported` list is precise when the parser proved a
    /// destructure or member use; otherwise the whole target module is widened.
    fn collect_dynamic_enum_uses(
        &self,
        summary: &shrike_syntax::FileSummary,
        aliases: &BTreeMap<String, String>,
        enums_by_file: &BTreeMap<String, Vec<String>>,
        usage: &mut BTreeMap<String, EnumUsage>,
    ) {
        for dynamic in &summary.dynamic {
            let DynamicSpecifier::Literal(specifier) = &dynamic.specifier else {
                continue;
            };
            let resolution = self.resolver.resolve(&summary.path, specifier);
            let Some(target) = resolution.target else {
                continue;
            };
            match &dynamic.imported {
                Some(names) => {
                    for name in names {
                        if let Some(key) = self.resolve_export(&target, name, &mut BTreeSet::new())
                        {
                            let key = resolve_alias(aliases, &key);
                            if self.is_enum_symbol(key.as_ref()) {
                                usage.entry(key.into_owned()).or_default().credit_all();
                            }
                        }
                    }
                }
                None => credit_all_in_file(&target, enums_by_file, usage),
            }
        }
    }

    fn resolve_object_key(&self, path: &str, name: &str) -> Option<String> {
        if let Some(key) = self.local_or_imported(path, name) {
            return Some(key);
        }
        // A same-file `export * as ns` alias is not a local binding, but its
        // exported name can still be used inside the module.
        self.exports
            .get(path)
            .and_then(|map| map.get(name))
            .cloned()
    }

    fn is_enum_symbol(&self, key: &str) -> bool {
        self.symbol_index
            .get(key)
            .is_some_and(|index| self.symbols[*index].kind == "enum")
    }
}

fn push_unused_enum_member(
    diagnostics: &mut Vec<Diagnostic>,
    path: &str,
    enumeration: &str,
    member: &shrike_syntax::EnumMemberRecord,
) {
    diagnostics.push(
        Diagnostic::warning(
            "unused_enum_member",
            format!(
                "`{enumeration}` declares enum member `{}` but no project reference reads it",
                member.name
            ),
        )
        .in_file(path)
        .at(member.span),
    );
}

/// Merge per-file usage maps from the parallel scan. Credits are monotonic, so
/// the merged result is independent of the order the files were processed in.
fn merge_enum_usage(
    mut left: BTreeMap<String, EnumUsage>,
    right: BTreeMap<String, EnumUsage>,
) -> BTreeMap<String, EnumUsage> {
    for (key, usage) in right {
        let entry = left.entry(key).or_default();
        entry.all |= usage.all;
        entry.members.extend(usage.members);
    }
    left
}

fn credit_all_in_file(
    file: &str,
    enums_by_file: &BTreeMap<String, Vec<String>>,
    usage: &mut BTreeMap<String, EnumUsage>,
) {
    if let Some(keys) = enums_by_file.get(file) {
        for key in keys {
            usage.entry(key.clone()).or_default().credit_all();
        }
    }
}

fn resolve_alias<'a>(aliases: &'a BTreeMap<String, String>, key: &'a str) -> Cow<'a, str> {
    let mut current = key;
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    while let Some(next) = aliases.get(current) {
        if !seen.insert(current) {
            break;
        }
        current = next;
    }
    Cow::Borrowed(current)
}

/// `Status[0]` is the numeric enum reverse mapping. Its property text is not a
/// member name, so credit the whole enum rather than inventing a member.
fn is_numeric_property(property: &str) -> bool {
    property.parse::<f64>().is_ok()
        || property.starts_with("0x")
        || property.starts_with("0b")
        || property.starts_with("0o")
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

    fn enum_diagnostics(snapshot: &GraphSnapshot) -> Vec<&Diagnostic> {
        snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "unused_enum_member")
            .collect()
    }

    fn package() -> &'static str {
        r#"{"name":"probe","private":true,"main":"src/index.ts"}"#
    }

    #[test]
    fn reports_unused_members_on_a_live_exported_enum() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/status.ts",
                "export enum Status { Active = \"active\", Inactive = \"inactive\" }\n",
            ),
            (
                "src/index.ts",
                "import { Status } from \"./status\";\nexport const current = Status.Active;\n",
            ),
        ]);
        let findings = enum_diagnostics(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("`Status`"));
        assert!(findings[0].message.contains("`Inactive`"));
        assert_eq!(findings[0].span.map(|span| span.line), Some(1));
    }

    #[test]
    fn credits_member_initializer_reads() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/status.ts",
                "export enum Initializer { A = 1, B = A, C = 4 }\n",
            ),
            (
                "src/index.ts",
                "import { Initializer } from \"./status\";\nexport const value = Initializer.B;\n",
            ),
        ]);
        let findings = enum_diagnostics(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("`C`"));
    }

    #[test]
    fn credits_whole_object_computed_and_destructured_uses() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/status.ts",
                r#"
export enum Whole { A, B }
export enum Computed { A, B }
export enum Numeric { A, B }
export enum Destructured { A, B }
"#,
            ),
            (
                "src/use.ts",
                r#"
import { Whole, Computed, Numeric, Destructured } from "./status";
const key: keyof typeof Computed = "A";
export const all = Object.values(Whole);
export const computed = Computed[key];
export const reverse = Numeric[0];
const { A } = Destructured;
export const destructured = A;
"#,
            ),
            ("src/index.ts", "export * from \"./use\";\n"),
        ]);
        let findings = enum_diagnostics(&snapshot);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn resolves_reexports_and_local_export_aliases() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/status.ts",
                r#"
export enum Reexported { Red, Green }
enum Local { One, Two }
export { Local as Aliased };
"#,
            ),
            (
                "src/barrel.ts",
                "export { Reexported } from \"./status\";\n",
            ),
            (
                "src/consumer.ts",
                "import { Reexported } from \"./barrel\";\nexport const red = Reexported.Red;\n",
            ),
            (
                "src/alias.ts",
                "import { Aliased } from \"./status\";\nexport const one = Aliased.One;\n",
            ),
            (
                "src/index.ts",
                "export * from \"./consumer\";\nexport * from \"./alias\";\n",
            ),
        ]);
        let findings = enum_diagnostics(&snapshot);
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(
            findings
                .iter()
                .any(|finding| finding.message.contains("`Reexported`")
                    && finding.message.contains("`Green`"))
        );
        assert!(findings.iter().any(
            |finding| finding.message.contains("`Local`") && finding.message.contains("`Two`")
        ));
    }

    #[test]
    fn credits_qualified_type_uses_precisely() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            ("src/status.ts", "export enum Status { Active, Inactive }\n"),
            (
                "src/use.ts",
                "import { Status } from \"./status\";\nexport type Active = Status.Active;\n",
            ),
            ("src/index.ts", "export * from \"./use\";\n"),
        ]);
        let findings = enum_diagnostics(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("`Inactive`"));
    }

    #[test]
    fn skips_dead_ambient_and_opaque_enums() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/status.ts",
                r#"
export enum Dead { A, B }
declare enum Ambient { X, Y }
export enum Opaque { [`dynamic`] = 1, Other = 2 }
"#,
            ),
            (
                "src/index.ts",
                "import { Opaque } from \"./status\";\nexport const value = Opaque.Other;\n",
            ),
        ]);
        let findings = enum_diagnostics(&snapshot);
        assert!(findings.is_empty(), "{findings:?}");
        assert!(
            snapshot
                .candidates
                .contains(&"src/status.ts#Dead".to_string()),
            "the dead enum itself stays a deletion candidate: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn skips_public_api_enums() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"probe","private":true,"main":"src/public.ts"}"#,
            ),
            ("src/public.ts", "export enum Public { Used, Unused }\n"),
            (
                "src/index.ts",
                "import { Public } from \"./public\";\nexport const used = Public.Used;\n",
            ),
        ]);
        let findings = enum_diagnostics(&snapshot);
        assert!(findings.is_empty(), "{findings:?}");
    }
}
