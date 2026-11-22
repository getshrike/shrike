//! Unused TypeScript namespace member diagnostics.
//!
//! This is a warning, never a deletion candidate: removing a member changes a
//! namespace's public contract, and Shrike's batch deletion model only removes
//! whole symbols. The parser has already abstained on local namespaces,
//! declaration files, `declare global`, and `declare module "..."`.
//!
//! Crediting is deliberately liberal. A dotted member read, destructuring, an
//! `import x = Ns.member`, a bare nested-namespace read, `typeof`, a computed
//! access, a whole-object use, a dynamic import, a re-export, or an
//! unqualified reference inside the namespace body credits the member. A
//! member that is itself a nested namespace is reported alone: when it is
//! unused, its descendants are not separate findings. Over-crediting can hide
//! a finding; it can never invent one.

use std::collections::{BTreeMap, BTreeSet};

use shrike_core::Diagnostic;
use shrike_core::Span;
use shrike_core::ids::symbol_key;
use shrike_syntax::{DynamicSpecifier, FileSummary, NamespaceMemberUseRecord, ParseStatus};

use super::Builder;

#[derive(Default)]
struct NamespaceUsage {
    all: bool,
    members: BTreeSet<String>,
}

struct NamespaceData {
    file: String,
    name: String,
    /// Any declaration is `declare namespace`.
    ambient: bool,
    /// Merged declarations keep one span per member name.
    members: BTreeMap<String, Span>,
}

impl Builder<'_> {
    /// Warn about exported members of live exported namespaces that no project
    /// reference reads. A dead namespace is already one deletion candidate, so
    /// its members are not reported separately.
    pub(super) fn detect_unused_namespace_members(&mut self, live: &BTreeSet<String>) {
        let summaries = self.summaries.clone();
        let namespaces = self.namespace_records_by_key();
        if namespaces.is_empty() {
            return;
        }
        let aliases = self.namespace_symbol_aliases();
        let export_all_aliases = self.namespace_export_all_aliases();
        let public_keys = self.public_namespace_keys(&aliases, &namespaces);
        let mut usage: BTreeMap<String, NamespaceUsage> = BTreeMap::new();

        for summary in summaries.iter() {
            if summary.parse_status != ParseStatus::Ok {
                continue;
            }
            for use_ in &summary.namespace_member_uses {
                self.credit_namespace_use(
                    summary,
                    use_,
                    &namespaces,
                    &aliases,
                    &export_all_aliases,
                    &mut usage,
                );
            }
            self.credit_dynamic_namespace_uses(summary, &namespaces, &aliases, &mut usage);
        }

        for (key, namespace) in &namespaces {
            if !live.contains(key) || public_keys.contains(key) {
                continue;
            }
            let Some(index) = self.symbol_index.get(key) else {
                continue;
            };
            let symbol = &self.symbols[*index];
            if !symbol.exported || symbol.public_api || symbol.generated {
                continue;
            }
            // An exported ambient namespace in a `.ts` file is an entry
            // because its declarations are program-visible, but fallow still
            // checks its exported type members. Only non-ambient entries are
            // skipped as public surfaces.
            if symbol.entry && !namespace.ambient {
                continue;
            }
            let Some(summary) = summaries
                .iter()
                .find(|summary| summary.path == namespace.file)
            else {
                continue;
            };
            if summary.module_kind == "dts"
                || self
                    .file_generated
                    .get(&namespace.file)
                    .copied()
                    .unwrap_or(false)
            {
                continue;
            }
            if usage.get(key).is_some_and(|usage| usage.all) {
                continue;
            }
            let used = usage.get(key).map(|usage| &usage.members);
            let mut suppressed: Vec<String> = Vec::new();
            let mut members: Vec<(&String, &Span)> = namespace.members.iter().collect();
            members.sort_by(|(left, _), (right, _)| {
                depth(left).cmp(&depth(right)).then_with(|| left.cmp(right))
            });
            for (name, span) in members {
                if used.is_some_and(|used| used.contains(name)) {
                    continue;
                }
                if suppressed
                    .iter()
                    .any(|prefix| name.starts_with(prefix.as_str()))
                {
                    continue;
                }
                let nested = namespace
                    .members
                    .keys()
                    .any(|other| other.starts_with(&format!("{name}.")));
                self.push_unused_namespace_member(&namespace.file, &namespace.name, name, *span);
                if nested {
                    suppressed.push(format!("{name}."));
                }
            }
        }
    }

    fn push_unused_namespace_member(
        &mut self,
        path: &str,
        namespace: &str,
        member: &str,
        span: Span,
    ) {
        let (owner, leaf) = match member.rsplit_once('.') {
            Some((prefix, leaf)) => (format!("{namespace}.{prefix}"), leaf),
            None => (namespace.to_string(), member),
        };
        self.diagnostics.push(
            Diagnostic::warning(
                "unused_namespace_member",
                format!(
                    "`{owner}` declares namespace member `{leaf}` but no project reference reads it"
                ),
            )
            .in_file(path)
            .at(span),
        );
    }

    fn namespace_records_by_key(&self) -> BTreeMap<String, NamespaceData> {
        let mut records = BTreeMap::new();
        for summary in self.summaries.iter() {
            if summary.parse_status != ParseStatus::Ok {
                continue;
            }
            for namespace in &summary.namespaces {
                let key = symbol_key(&summary.path, &namespace.name);
                let record = records.entry(key).or_insert_with(|| NamespaceData {
                    file: summary.path.clone(),
                    name: namespace.name.clone(),
                    ambient: namespace.ambient,
                    members: BTreeMap::new(),
                });
                record.ambient |= namespace.ambient;
                for member in &namespace.members {
                    record
                        .members
                        .entry(member.name.clone())
                        .or_insert(member.span);
                }
            }
        }
        records
    }

    fn credit_namespace_use(
        &self,
        summary: &FileSummary,
        use_: &NamespaceMemberUseRecord,
        namespaces: &BTreeMap<String, NamespaceData>,
        aliases: &BTreeMap<String, String>,
        export_all_aliases: &BTreeMap<String, Vec<String>>,
        usage: &mut BTreeMap<String, NamespaceUsage>,
    ) {
        if let Some(source) = inline_import_source(&use_.object) {
            if let Some(target) = self.resolver.resolve(&summary.path, source).target {
                self.credit_namespace_from_file(&target, &use_.path, namespaces, aliases, usage);
            }
            return;
        }
        // A namespace import has no local symbol key. Resolve it before the
        // local/imported lookup, which would otherwise stop the walk.
        if let Some(targets) = self
            .namespaces
            .get(&summary.path)
            .and_then(|map| map.get(&use_.object))
        {
            for target in targets {
                self.credit_namespace_from_file(target, &use_.path, namespaces, aliases, usage);
            }
            return;
        }
        let Some(key) = self.resolve_namespace_object_key(&summary.path, &use_.object) else {
            return;
        };
        let key = resolve_alias(aliases, &key);
        if namespaces.contains_key(&key) {
            credit_path(&key, &use_.path, namespaces, usage);
            return;
        }
        if let Some(targets) = export_all_aliases.get(&key) {
            for target in targets {
                self.credit_namespace_from_file(target, &use_.path, namespaces, aliases, usage);
            }
        }
    }

    fn credit_namespace_from_file(
        &self,
        file: &str,
        path: &str,
        namespaces: &BTreeMap<String, NamespaceData>,
        aliases: &BTreeMap<String, String>,
        usage: &mut BTreeMap<String, NamespaceUsage>,
    ) {
        if path == "*" {
            credit_all_in_file(file, namespaces, usage);
            return;
        }
        let mut segments = path.split('.');
        let Some(first) = segments.next() else {
            return;
        };
        let Some(key) = self.resolve_export(file, first, &mut BTreeSet::new()) else {
            return;
        };
        let key = resolve_alias(aliases, &key);
        if !namespaces.contains_key(&key) {
            return;
        }
        let rest: Vec<&str> = segments.collect();
        if rest.is_empty() {
            usage.entry(key).or_default().all = true;
        } else {
            credit_path(&key, &rest.join("."), namespaces, usage);
        }
    }

    fn credit_dynamic_namespace_uses(
        &self,
        summary: &FileSummary,
        namespaces: &BTreeMap<String, NamespaceData>,
        aliases: &BTreeMap<String, String>,
        usage: &mut BTreeMap<String, NamespaceUsage>,
    ) {
        for dynamic in &summary.dynamic {
            let DynamicSpecifier::Literal(specifier) = &dynamic.specifier else {
                continue;
            };
            let Some(target) = self.resolver.resolve(&summary.path, specifier).target else {
                continue;
            };
            match &dynamic.imported {
                Some(names) => {
                    for name in names {
                        if let Some(key) = self.resolve_export(&target, name, &mut BTreeSet::new())
                        {
                            let key = resolve_alias(aliases, &key);
                            if namespaces.contains_key(&key) {
                                usage.entry(key).or_default().all = true;
                            }
                        }
                    }
                }
                None => credit_all_in_file(&target, namespaces, usage),
            }
        }
    }

    fn resolve_namespace_object_key(&self, path: &str, name: &str) -> Option<String> {
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

    /// Local export aliases and re-exports are separate symbols from the
    /// namespace declaration. Map each alias back to the namespace so a member
    /// read through the alias credits the declaration.
    fn namespace_symbol_aliases(&self) -> BTreeMap<String, String> {
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
            if target != *alias {
                resolved.insert(alias.clone(), target);
            }
        }
        resolved
    }

    /// `export * as ns from "./module"` creates a namespace-like alias symbol.
    fn namespace_export_all_aliases(&self) -> BTreeMap<String, Vec<String>> {
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

    /// Namespace keys exposed from a public entry. Re-exporting a namespace
    /// from a package entry makes every member part of the public surface, so
    /// member removal is never a review finding there.
    fn public_namespace_keys(
        &self,
        aliases: &BTreeMap<String, String>,
        namespaces: &BTreeMap<String, NamespaceData>,
    ) -> BTreeSet<String> {
        let mut keys = BTreeSet::new();
        for summary in self.summaries.iter() {
            if !self.public_files.contains(&summary.path) {
                continue;
            }
            if let Some(exports) = self.exports.get(&summary.path) {
                for name in exports.keys() {
                    if let Some(key) =
                        self.resolve_export(&summary.path, name, &mut BTreeSet::new())
                    {
                        let key = resolve_alias(aliases, &key);
                        if namespaces.contains_key(&key) {
                            keys.insert(key);
                        }
                    }
                }
            }
            for export_all in &summary.export_alls {
                for target in self
                    .resolver
                    .resolve(&summary.path, &export_all.source)
                    .targets()
                {
                    for (key, namespace) in namespaces {
                        if namespace.file == *target {
                            keys.insert(key.clone());
                        }
                    }
                }
            }
        }
        keys
    }
}

fn credit_path(
    key: &str,
    path: &str,
    namespaces: &BTreeMap<String, NamespaceData>,
    usage: &mut BTreeMap<String, NamespaceUsage>,
) {
    if path == "*" {
        usage.entry(key.to_string()).or_default().all = true;
        return;
    }
    let Some(namespace) = namespaces.get(key) else {
        return;
    };
    let segments: Vec<&str> = path.split('.').collect();
    let entry = usage.entry(key.to_string()).or_default();
    for end in 1..=segments.len() {
        entry.members.insert(segments[..end].join("."));
    }
    // A bare read of a nested namespace object can access any of its members:
    // `Outer.Whole` credits every member of `Outer.Whole`.
    let full = segments.join(".");
    let prefix = format!("{full}.");
    if namespace
        .members
        .keys()
        .any(|member| member.starts_with(&prefix))
    {
        for member in namespace.members.keys() {
            if member.starts_with(&prefix) {
                entry.members.insert(member.clone());
            }
        }
    }
}

fn credit_all_in_file(
    file: &str,
    namespaces: &BTreeMap<String, NamespaceData>,
    usage: &mut BTreeMap<String, NamespaceUsage>,
) {
    for (key, namespace) in namespaces {
        if namespace.file == file {
            usage.entry(key.clone()).or_default().all = true;
        }
    }
}

fn resolve_alias(aliases: &BTreeMap<String, String>, key: &str) -> String {
    let mut current = key.to_string();
    let mut seen = BTreeSet::new();
    while let Some(next) = aliases.get(&current) {
        if !seen.insert(current.clone()) {
            break;
        }
        current = next.clone();
    }
    current
}

fn depth(name: &str) -> usize {
    name.matches('.').count()
}

fn inline_import_source(object: &str) -> Option<&str> {
    object
        .strip_prefix("import(\"")
        .and_then(|rest| rest.strip_suffix("\")"))
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

    fn namespace_diagnostics(snapshot: &GraphSnapshot) -> Vec<&Diagnostic> {
        snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "unused_namespace_member")
            .collect()
    }

    fn messages(snapshot: &GraphSnapshot) -> Vec<String> {
        namespace_diagnostics(snapshot)
            .into_iter()
            .map(|diagnostic| diagnostic.message.clone())
            .collect()
    }

    fn package() -> &'static str {
        r#"{"name":"probe","private":true,"main":"src/index.ts"}"#
    }

    #[test]
    fn reports_unused_members_and_suppresses_nested_descendants() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/legacy.ts",
                r#"
export namespace Outer {
  export namespace Inner {
    export const deep = 1;
    export const unusedDeep = 2;
  }
  export namespace UnusedNested {
    export const c = 1;
  }
}
export namespace Legacy {
  export const used = 1;
  export const unusedValue = 2;
  export function unusedFn(): void {}
}
"#,
            ),
            (
                "src/index.ts",
                r#"
import { Legacy, Outer } from "./legacy";
export const values = [Legacy.used, Outer.Inner.deep];
"#,
            ),
        ]);
        let findings = messages(&snapshot);
        assert_eq!(findings.len(), 4, "{findings:?}");
        assert!(
            findings
                .iter()
                .any(|message| message.contains("`Legacy`") && message.contains("`unusedValue`"))
        );
        assert!(
            findings
                .iter()
                .any(|message| message.contains("`Legacy`") && message.contains("`unusedFn`"))
        );
        assert!(
            findings.iter().any(
                |message| message.contains("`Outer.Inner`") && message.contains("`unusedDeep`")
            )
        );
        assert!(
            findings
                .iter()
                .any(|message| message.contains("`Outer`") && message.contains("`UnusedNested`"))
        );
        assert!(
            !findings
                .iter()
                .any(|message| message.contains("`UnusedNested.c`") || message.contains("`c`")),
            "a reported nested namespace suppresses its descendants: {findings:?}"
        );
    }

    #[test]
    fn credits_destructuring_import_equals_whole_uses_and_body_references() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/legacy.ts",
                r#"
export namespace Legacy {
  export const used = 1;
  export const aliased = 2;
  export const destructured = 3;
  export const unusedValue = 4;
  export function unusedFn(): void {}
}
export namespace Whole {
  export const a = 1;
  export const b = 2;
}
export namespace Spread {
  export const a = 1;
}
export namespace Body {
  export const helper = 1;
  export function entry(): number { return helper; }
}
"#,
            ),
            (
                "src/index.ts",
                r#"
import { Legacy, Whole, Spread, Body } from "./legacy";
import x = Legacy.aliased;
const { destructured } = Legacy;
export const values = [
  Legacy.used,
  x,
  destructured,
  Object.keys(Whole),
  { ...Spread },
  Body.entry(),
];
"#,
            ),
        ]);
        let findings = messages(&snapshot);
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(
            findings
                .iter()
                .any(|message| message.contains("`unusedValue`"))
        );
        assert!(
            findings
                .iter()
                .any(|message| message.contains("`unusedFn`"))
        );
        assert!(
            !findings.iter().any(|message| message.contains("`helper`")
                || message.contains("`entry`")
                || message.contains("`Whole`")
                || message.contains("`Spread`")
                || message.contains("`aliased`")
                || message.contains("`destructured`")),
            "all credited uses must stay quiet: {findings:?}"
        );
    }

    #[test]
    fn credits_namespace_imports_inline_import_types_and_typeof() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/more.ts",
                r#"
export namespace Api {
  export namespace Types {
    export type Request = { id: string };
    export type UnusedResponse = { ok: boolean };
  }
  export namespace Escaped {
    export const x = 1;
    export const y = 2;
  }
  export const viaNamespaceImport = 1;
  export const viaTypeof = 1;
}
"#,
            ),
            (
                "src/index.ts",
                r#"
import * as more from "./more";
import { Api } from "./more";
export type R = Api.Types.Request;
export const viaImport = more.Api.viaNamespaceImport;
export type T = typeof Api.viaTypeof;
const escaped = Api.Escaped;
export const e = escaped;
export type A = typeof import("./ambient").Ambient.a;
"#,
            ),
            (
                "src/ambient.d.ts",
                r#"
export declare namespace Ambient {
  export const a: number;
  export const unusedAmbient: number;
}
"#,
            ),
        ]);
        let findings = messages(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].contains("`Api.Types`") && findings[0].contains("`UnusedResponse`"),
            "{findings:?}"
        );
        assert!(
            !findings.iter().any(|message| message.contains("`Ambient`")
                || message.contains("`Escaped`")
                || message.contains("`viaNamespaceImport`")
                || message.contains("`viaTypeof`")
                || message.contains("`Request`")),
            "credited paths must stay quiet: {findings:?}"
        );
    }

    #[test]
    fn merges_declarations_and_skips_local_and_publicly_reexported_namespaces() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/more.ts",
                r#"
export namespace Split {
  export const first = 1;
}
export namespace Split {
  export const second = first + 1;
}
namespace LocalOnly {
  export const l = 1;
}
export namespace PublicNs {
  export const a = 1;
}
"#,
            ),
            (
                "src/index.ts",
                r#"
import { Split, PublicNs } from "./more";
export const second = Split.second;
export { PublicNs };
"#,
            ),
        ]);
        let findings = messages(&snapshot);
        assert!(findings.is_empty(), "{findings:?}");
        assert!(
            snapshot
                .candidates
                .contains(&"src/more.ts#LocalOnly".to_string()),
            "the local namespace itself stays a deletion candidate: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn skips_dead_and_public_api_namespaces() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"probe","private":true,"main":"src/public.ts"}"#,
            ),
            (
                "src/status.ts",
                r#"
export namespace Dead {
  export const a = 1;
}
export namespace Live {
  export const used = 1;
  export const unused = 2;
}
"#,
            ),
            (
                "src/public.ts",
                r#"
import { Live } from "./status";
export namespace Public {
  export const used = 1;
  export const unused = 2;
}
export const value = Live.used;
export const publicValue = Public.used;
"#,
            ),
        ]);
        let findings = messages(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].contains("`Live`") && findings[0].contains("`unused`"),
            "{findings:?}"
        );
        assert!(
            snapshot
                .candidates
                .contains(&"src/status.ts#Dead".to_string()),
            "the dead namespace itself stays a deletion candidate: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn reports_ambient_namespace_members_in_ts_files() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/types.ts",
                r#"
export declare namespace Typed {
  export type Shape = { a: number };
  export type UnusedShape = { b: number };
}
"#,
            ),
            (
                "src/index.ts",
                "import { Typed } from \"./types\";\nexport type T = Typed.Shape;\n",
            ),
        ]);
        let findings = messages(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].contains("`Typed`") && findings[0].contains("`UnusedShape`"),
            "{findings:?}"
        );
    }

    #[test]
    fn credits_body_references_in_nested_namespaces() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/more.ts",
                r#"
export namespace Body {
  export const helper = 1;
  export function entry(): number { return helper; }
}
export namespace Nested {
  export namespace Inner {
    export const a = 1;
    export function use(): number { return a; }
    export const unused = 2;
  }
}
"#,
            ),
            (
                "src/index.ts",
                r#"
import { Body, Nested } from "./more";
export const value = [Body.entry(), Nested.Inner.use()];
"#,
            ),
        ]);
        let findings = messages(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].contains("`Nested.Inner`") && findings[0].contains("`unused`"),
            "{findings:?}"
        );
    }

    #[test]
    fn credits_reexports_and_local_import_aliases() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/ns.ts",
                "export namespace Local { export const used = 1; export const unused = 2; }\n",
            ),
            (
                "src/barrel.ts",
                "export { Local as Aliased } from \"./ns\";\n",
            ),
            (
                "src/index.ts",
                "import { Aliased } from \"./barrel\";\nexport const value = Aliased.used;\n",
            ),
        ]);
        let findings = messages(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].contains("`Local`") && findings[0].contains("`unused`"),
            "{findings:?}"
        );
    }
}
