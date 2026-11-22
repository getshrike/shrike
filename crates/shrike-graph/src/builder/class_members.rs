//! Unused class member diagnostics.
//!
//! This is a warning, never a deletion candidate: removing a member changes a
//! class contract, and Shrike's batch deletion model only removes whole
//! symbols. The parser has already abstained on ambient, abstract, decorated,
//! computed-name, index-signature, and escaping classes. The graph reports a
//! member only when the class itself is live and exported, is not public API,
//! and no project reference reads the member.
//!
//! Crediting is deliberately liberal. `this.x`, `super.x`, `ClassName.x`,
//! instance bindings, `new C().x`, `C.prototype.x`, destructuring, namespace
//! aliases, dynamic imports, and whole-object uses credit members. React
//! lifecycle names and runtime fields are credited in React projects, and
//! Next.js additionally credits `getInitialProps`. A subclass member that
//! shares a name with an ancestor member is credited because a base-class
//! reference can dispatch to it. Over-crediting can hide a finding; it can
//! never invent one.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;
use shrike_core::Diagnostic;
use shrike_core::ids::symbol_key;
use shrike_syntax::{
    ClassRecord, DynamicSpecifier, FileSummary, MemberAccess, ModuleRefs, ParseStatus,
};

use super::Builder;

/// Methods React calls by convention on class components. Mirrors fallow
/// 3.45.0's lifecycle allowlist.
const REACT_LIFECYCLE_MEMBERS: &[&str] = &[
    "render",
    "componentDidMount",
    "componentDidUpdate",
    "componentWillUnmount",
    "shouldComponentUpdate",
    "getSnapshotBeforeUpdate",
    "getDerivedStateFromProps",
    "getDerivedStateFromError",
    "componentDidCatch",
    "componentWillMount",
    "componentWillReceiveProps",
    "componentWillUpdate",
    "UNSAFE_componentWillMount",
    "UNSAFE_componentWillReceiveProps",
    "UNSAFE_componentWillUpdate",
    "getChildContext",
    "contextType",
];

/// Fields React reads directly from a class component instance or constructor.
const REACT_RUNTIME_MEMBERS: &[&str] = &[
    "state",
    "props",
    "context",
    "refs",
    "contextType",
    "defaultProps",
    "propTypes",
];

#[derive(Default)]
struct ClassUsage {
    all: bool,
    members: BTreeSet<String>,
}

impl ClassUsage {
    fn credit_all(&mut self) {
        self.all = true;
    }

    fn credit(&mut self, member: &str) {
        self.members.insert(member.to_string());
    }
}

/// Resolution context shared by every reference walk in one detection pass.
struct ClassContext<'a> {
    aliases: &'a BTreeMap<String, String>,
    namespace_aliases: &'a BTreeMap<String, Vec<String>>,
    classes_by_file: &'a BTreeMap<String, Vec<String>>,
    class_keys: &'a BTreeSet<String>,
}

impl Builder<'_> {
    /// Warn about members of live exported classes that no project reference
    /// reads. `live` is the symbol-level reachable set; a dead class is already
    /// one deletion candidate, so its members are not reported separately.
    pub(super) fn detect_unused_class_members(&self, live: &BTreeSet<String>) -> Vec<Diagnostic> {
        let timing = std::env::var_os("SHRIKE_TIMING").is_some();
        let mut diagnostics = Vec::new();
        let summaries = self.summaries.clone();
        let class_keys = timed!(timing, "class_keys", self.class_keys());
        if class_keys.is_empty() {
            return diagnostics;
        }
        let aliases = timed!(timing, "class_aliases", self.class_aliases());
        let bases = timed!(
            timing,
            "class_bases",
            self.class_base_keys(&class_keys, &aliases)
        );
        let member_names = timed!(timing, "class_member_names", self.class_member_names());
        let namespace_aliases = timed!(
            timing,
            "class_namespace_aliases",
            self.class_namespace_aliases()
        );
        let classes_by_file = timed!(timing, "class_index", self.classes_by_file());
        let react = self.project_uses_react();
        let next = self.project_uses_next();
        let context = ClassContext {
            aliases: &aliases,
            namespace_aliases: &namespace_aliases,
            classes_by_file: &classes_by_file,
            class_keys: &class_keys,
        };
        let mut usage: BTreeMap<String, ClassUsage> = timed!(timing, "class_collect", {
            summaries
                .par_iter()
                .filter(|summary| summary.parse_status == ParseStatus::Ok)
                .map(|summary| {
                    let mut usage = BTreeMap::new();
                    let bindings = self.class_binding_keys(summary, &class_keys, &aliases);
                    self.collect_class_refs(
                        &summary.path,
                        &summary.module_refs,
                        &bindings,
                        &context,
                        &mut usage,
                    );
                    for declaration in &summary.declarations {
                        self.collect_class_refs(
                            &summary.path,
                            &declaration.references,
                            &bindings,
                            &context,
                            &mut usage,
                        );
                    }
                    self.collect_dynamic_class_uses(
                        summary,
                        &aliases,
                        &classes_by_file,
                        &class_keys,
                        &mut usage,
                    );
                    // A class value passed, returned, exported, or stored as
                    // data can be inspected by the receiver. Cross-file uses
                    // are not visible to the parser's same-file escape
                    // analysis, so the summary records every unsafe value use
                    // and the graph resolves it here.
                    for name in &summary.class_value_escapes {
                        let Some(key) = self.resolve_class_object_key(&summary.path, name) else {
                            continue;
                        };
                        let key = resolve_alias(&aliases, &key);
                        if class_keys.contains(key.as_ref()) {
                            usage.entry(key.into_owned()).or_default().credit_all();
                        }
                    }
                    for class in &summary.classes {
                        let key = class_key(&summary.path, class);
                        let entry = usage.entry(key).or_default();
                        for name in class
                            .self_instance_reads
                            .iter()
                            .chain(&class.self_static_reads)
                            .chain(&class.class_static_reads)
                        {
                            entry.credit(name);
                        }
                        // `super.x` reads a base member, but crediting the same
                        // name on the subclass is conservative and covers
                        // overrides.
                        for name in &class.super_reads {
                            entry.credit(name);
                        }
                        if react {
                            for name in REACT_LIFECYCLE_MEMBERS {
                                entry.credit(name);
                            }
                            for name in REACT_RUNTIME_MEMBERS {
                                entry.credit(name);
                            }
                        }
                        if next {
                            entry.credit("getInitialProps");
                        }
                    }
                    for binding in &summary.class_bindings {
                        if !binding.escaped {
                            continue;
                        }
                        let Some(key) =
                            self.resolve_class_binding(summary, binding, &class_keys, &aliases)
                        else {
                            continue;
                        };
                        usage.entry(key).or_default().credit_all();
                    }
                    usage
                })
                .reduce(BTreeMap::new, merge_class_usage)
        });

        // A `super.x` call in a subclass reads a member declared on the base.
        timed!(timing, "class_base_credits", {
            for summary in summaries.iter() {
                for class in &summary.classes {
                    let key = class_key(&summary.path, class);
                    let Some(base) = bases.get(&key) else {
                        continue;
                    };
                    for name in &class.super_reads {
                        usage.entry(base.clone()).or_default().credit(name);
                    }
                }
            }
        });

        timed!(timing, "class_report", {
            for summary in summaries.iter() {
                if summary.parse_status != ParseStatus::Ok
                    || summary.module_kind == "dts"
                    || summary.classes.is_empty()
                    || self
                        .file_generated
                        .get(&summary.path)
                        .copied()
                        .unwrap_or(false)
                {
                    continue;
                }
                for class in &summary.classes {
                    if class.ambient || class.abstract_ || class.decorated || class.opaque {
                        continue;
                    }
                    if class.extends_member && !class.extends.as_deref().is_some_and(is_react_base)
                    {
                        // A non-React member-expression base can call any member
                        // through an inherited method. Stay out of it.
                        continue;
                    }
                    if summary
                        .classes
                        .iter()
                        .filter(|other| other.name == class.name)
                        .count()
                        > 1
                    {
                        continue;
                    }
                    let key = class_key(&summary.path, class);
                    if !live.contains(&key) {
                        continue;
                    }
                    let Some(index) = self.symbol_index.get(&key) else {
                        continue;
                    };
                    let symbol = &self.symbols[*index];
                    if !symbol.exported || symbol.entry || symbol.public_api || symbol.generated {
                        continue;
                    }
                    let inherited = inherited_member_names(&key, &bases, &member_names);
                    let entry = usage.get(&key);
                    if entry.is_some_and(|entry| entry.all) {
                        continue;
                    }
                    for member in &class.members {
                        if !reportable_member(member) {
                            continue;
                        }
                        if entry.is_some_and(|entry| entry.members.contains(&member.name)) {
                            continue;
                        }
                        if inherited.contains(&member.name) {
                            continue;
                        }
                        diagnostics.push(
                        Diagnostic::warning(
                            "unused_class_member",
                            format!(
                                "`{}` declares class member `{}` but no project reference reads it",
                                class.name, member.name
                            ),
                        )
                        .in_file(&summary.path)
                        .at(member.span),
                    );
                    }
                }
            }
        });
        diagnostics
    }

    fn collect_class_refs(
        &self,
        path: &str,
        refs: &ModuleRefs,
        bindings: &BTreeMap<String, String>,
        context: &ClassContext<'_>,
        usage: &mut BTreeMap<String, ClassUsage>,
    ) {
        for member in &refs.members {
            if let Some(key) = bindings.get(&member.object) {
                credit_class_member(usage.entry(key.clone()).or_default(), member);
                continue;
            }
            // A namespace import has no local symbol key, so resolve it before
            // the object-key lookup that would otherwise stop the loop.
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
                                let target_key = resolve_alias(context.aliases, &target_key);
                                if context.class_keys.contains(target_key.as_ref()) {
                                    usage
                                        .entry(target_key.into_owned())
                                        .or_default()
                                        .credit_all();
                                }
                            }
                        }
                        None => credit_all_in_file(target, context.classes_by_file, usage),
                    }
                }
                continue;
            }
            let Some(key) = self.resolve_class_object_key(path, &member.object) else {
                continue;
            };
            let key = resolve_alias(context.aliases, &key);
            if context.class_keys.contains(key.as_ref()) {
                credit_class_member(usage.entry(key.into_owned()).or_default(), member);
                continue;
            }
            if let Some(targets) = context.namespace_aliases.get(key.as_ref()) {
                for target in targets {
                    credit_all_in_file(target, context.classes_by_file, usage);
                }
            }
        }
    }

    /// A literal dynamic import can reach a class without leaving a static
    /// import binding. Its `imported` list is precise when the parser proved a
    /// destructure or member use; otherwise the whole target module is widened.
    /// Either way every reached class is credited whole: the binding that
    /// receives the constructor is not tracked through dynamic imports.
    fn collect_dynamic_class_uses(
        &self,
        summary: &FileSummary,
        aliases: &BTreeMap<String, String>,
        classes_by_file: &BTreeMap<String, Vec<String>>,
        class_keys: &BTreeSet<String>,
        usage: &mut BTreeMap<String, ClassUsage>,
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
                            if class_keys.contains(key.as_ref()) {
                                usage.entry(key.into_owned()).or_default().credit_all();
                            }
                        }
                    }
                }
                None => credit_all_in_file(&target, classes_by_file, usage),
            }
        }
    }

    fn resolve_class_binding(
        &self,
        summary: &FileSummary,
        binding: &shrike_syntax::ClassBindingRecord,
        class_keys: &BTreeSet<String>,
        aliases: &BTreeMap<String, String>,
    ) -> Option<String> {
        let key = self.local_or_imported(&summary.path, &binding.class)?;
        let key = resolve_alias(aliases, &key);
        class_keys.contains(key.as_ref()).then(|| key.into_owned())
    }

    fn class_binding_keys(
        &self,
        summary: &FileSummary,
        class_keys: &BTreeSet<String>,
        aliases: &BTreeMap<String, String>,
    ) -> BTreeMap<String, String> {
        let mut bindings = BTreeMap::new();
        for binding in &summary.class_bindings {
            if let Some(key) = self.resolve_class_binding(summary, binding, class_keys, aliases) {
                bindings.insert(binding.binding.clone(), key);
            }
        }
        bindings
    }

    fn resolve_class_object_key(&self, path: &str, name: &str) -> Option<String> {
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

    fn class_keys(&self) -> BTreeSet<String> {
        let mut keys = BTreeSet::new();
        for summary in self.summaries.iter() {
            for class in &summary.classes {
                keys.insert(class_key(&summary.path, class));
            }
        }
        keys
    }

    fn class_member_names(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut names = BTreeMap::new();
        for summary in self.summaries.iter() {
            for class in &summary.classes {
                names.insert(
                    class_key(&summary.path, class),
                    class
                        .members
                        .iter()
                        .map(|member| member.name.clone())
                        .collect(),
                );
            }
        }
        names
    }

    /// Local export aliases and re-exports are separate symbols from the class
    /// declaration. Map each alias back to the class so an access through the
    /// alias credits the declaration.
    fn class_aliases(&self) -> BTreeMap<String, String> {
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
    /// Accessing `ns.Class.Member` loses the final member in the syntactic
    /// member record, so any class reached through the alias is credited whole.
    fn class_namespace_aliases(&self) -> BTreeMap<String, Vec<String>> {
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

    fn classes_by_file(&self) -> BTreeMap<String, Vec<String>> {
        let mut classes = BTreeMap::new();
        for summary in self.summaries.iter() {
            let keys: Vec<String> = summary
                .classes
                .iter()
                .map(|class| class_key(&summary.path, class))
                .collect();
            if !keys.is_empty() {
                classes.insert(summary.path.clone(), keys);
            }
        }
        classes
    }

    fn class_base_keys(
        &self,
        class_keys: &BTreeSet<String>,
        aliases: &BTreeMap<String, String>,
    ) -> BTreeMap<String, String> {
        let mut bases = BTreeMap::new();
        for summary in self.summaries.iter() {
            for class in &summary.classes {
                let Some(extends) = &class.extends else {
                    continue;
                };
                if class.extends_member && !is_react_base(extends) {
                    continue;
                }
                let Some(key) = self.local_or_imported(&summary.path, extends) else {
                    continue;
                };
                let key = resolve_alias(aliases, &key);
                if class_keys.contains(key.as_ref()) {
                    bases.insert(class_key(&summary.path, class), key.into_owned());
                }
            }
        }
        bases
    }

    fn project_uses_next(&self) -> bool {
        self.package_declares_any("", &["next"]) == Some(true)
            || self.resolver.packages().iter().any(|package| {
                self.package_declares_any(&package.directory, &["next"]) == Some(true)
            })
    }
}

fn class_key(path: &str, class: &ClassRecord) -> String {
    if class.default {
        symbol_key(path, "default")
    } else {
        symbol_key(path, &class.name)
    }
}

fn reportable_member(member: &shrike_syntax::ClassMemberRecord) -> bool {
    !member.is_private
        && !member.is_protected
        && !member.is_abstract
        && !member.is_declare
        && !member.is_override
        && !member.decorated
}

fn credit_class_member(entry: &mut ClassUsage, member: &MemberAccess) {
    match &member.property {
        Some(property) if property != "prototype" => entry.credit(property),
        Some(_) => {}
        None => entry.credit_all(),
    }
}

fn inherited_member_names(
    key: &str,
    bases: &BTreeMap<String, String>,
    member_names: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut inherited = BTreeSet::new();
    let mut current = bases.get(key);
    let mut seen = BTreeSet::new();
    while let Some(base) = current {
        if !seen.insert(base.clone()) {
            break;
        }
        if let Some(names) = member_names.get(base) {
            inherited.extend(names.iter().cloned());
        }
        current = bases.get(base);
    }
    inherited
}

/// Merge per-file usage maps from the parallel scan. Credits are monotonic, so
/// the merged result is independent of the order the files were processed in.
fn merge_class_usage(
    mut left: BTreeMap<String, ClassUsage>,
    right: BTreeMap<String, ClassUsage>,
) -> BTreeMap<String, ClassUsage> {
    for (key, usage) in right {
        let entry = left.entry(key).or_default();
        entry.all |= usage.all;
        entry.members.extend(usage.members);
    }
    left
}

fn credit_all_in_file(
    file: &str,
    classes_by_file: &BTreeMap<String, Vec<String>>,
    usage: &mut BTreeMap<String, ClassUsage>,
) {
    if let Some(keys) = classes_by_file.get(file) {
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

/// `React.Component`, `React.PureComponent`, and equivalent aliases. A plain
/// `Component` heritage is handled as an ordinary identifier base.
fn is_react_base(extends: &str) -> bool {
    extends
        .rsplit('.')
        .next()
        .is_some_and(|name| matches!(name, "Component" | "PureComponent"))
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

    fn class_diagnostics(snapshot: &GraphSnapshot) -> Vec<&Diagnostic> {
        snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "unused_class_member")
            .collect()
    }

    fn package() -> &'static str {
        r#"{"name":"probe","private":true,"main":"src/index.ts"}"#
    }

    fn react_package() -> &'static str {
        r#"{"name":"probe","private":true,"main":"src/index.ts","dependencies":{"react":"^19.0.0"}}"#
    }

    #[test]
    fn reports_unused_members_on_a_live_exported_class() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/counter.ts",
                r#"
export class Counter {
  value = 0;
  increment() { this.value += 1; }
  reset() { this.value = 0; }
  dead() {}
}
"#,
            ),
            (
                "src/index.ts",
                r#"
import { Counter } from "./counter";
const counter = new Counter();
counter.increment();
export const value = counter.value;
"#,
            ),
        ]);
        let findings = class_diagnostics(&snapshot);
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(
            findings
                .iter()
                .any(|finding| finding.message.contains("`reset`"))
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.message.contains("`dead`"))
        );
        assert_eq!(findings[0].span.map(|span| span.line), Some(5));
    }

    #[test]
    fn credits_new_and_prototype_accesses() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/direct.ts",
                r#"
export class Direct {
  byNew() {}
  byPrototype() {}
  unused() {}
}
"#,
            ),
            (
                "src/index.ts",
                r#"
import { Direct } from "./direct";
new Direct().byNew();
Direct.prototype.byPrototype();
"#,
            ),
        ]);
        let findings = class_diagnostics(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("`unused`"));
    }

    #[test]
    fn abstains_when_a_class_value_escapes_in_another_file() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/widget.ts",
                r#"
export class Widget {
  used() {}
  dead() {}
}
"#,
            ),
            (
                "src/index.ts",
                r#"
import { Widget } from "./widget";
console.log(Widget);
new Widget().used();
"#,
            ),
        ]);
        let findings = class_diagnostics(&snapshot);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn credits_destructured_instance_reads() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/widget.ts",
                r#"
export class Widget {
  used = 1;
  dead = 2;
}
"#,
            ),
            (
                "src/index.ts",
                r#"
import { Widget } from "./widget";
const { used } = new Widget();
export const value = used;
export function read(widget: Widget) {
  const { used: fromParameter } = widget;
  return fromParameter;
}
"#,
            ),
        ]);
        let findings = class_diagnostics(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("`dead`"));
    }

    #[test]
    fn credits_namespace_accesses() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/widget.ts",
                r#"
export class Widget {
  used() {}
  dead() {}
}
"#,
            ),
            (
                "src/index.ts",
                r#"
import * as ns from "./widget";
new ns.Widget().used();
"#,
            ),
        ]);
        let findings = class_diagnostics(&snapshot);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn credits_super_reads_and_inherited_names() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/base.ts",
                r#"
export class Base {
  fromSuper() { return 1; }
  shared() { return 2; }
}
"#,
            ),
            (
                "src/sub.ts",
                r#"
import { Base } from "./base";
export class Sub extends Base {
  call() { return super.fromSuper(); }
  shared() { return 3; }
  dead() { return 4; }
}
"#,
            ),
            (
                "src/index.ts",
                "import { Sub } from \"./sub\";\nnew Sub().call();\n",
            ),
        ]);
        let findings = class_diagnostics(&snapshot);
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(
            findings
                .iter()
                .any(|finding| finding.message.contains("`Base`")
                    && finding.message.contains("`shared`"))
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.message.contains("`Sub`")
                    && finding.message.contains("`dead`"))
        );
        assert!(
            !findings
                .iter()
                .any(|finding| finding.message.contains("`fromSuper`")),
            "a super read credits the base member: {findings:?}"
        );
        assert!(
            !findings
                .iter()
                .any(|finding| finding.message.contains("`Sub`")
                    && finding.message.contains("`shared`")),
            "a name shared with an ancestor is credited: {findings:?}"
        );
    }

    #[test]
    fn skips_ambient_decorated_abstract_opaque_and_non_reportable_members() {
        let snapshot = snapshot(&[
            ("package.json", package()),
            (
                "src/mixed.ts",
                r#"
function sealed(value: unknown) { return value; }

export class Live {
  used() {}
  dead() {}
  private secret() {}
  protected guarded() {}
  declare typed: string;
  #hidden() {}
}

@sealed
export class Decorated {
  dead() {}
}

export abstract class Abstract {
  dead() {}
}

export class Concrete extends Abstract {
  run() {}
}

export class Opaque {
  [key: string]: unknown;
  dead() {}
}

export declare class Ambient {
  dead: string;
}

export class Dead {
  dead() {}
}
"#,
            ),
            (
                "src/index.ts",
                r#"
import { Ambient, Concrete, Decorated, Live, Opaque } from "./mixed";
const live = new Live();
live.used();
new Decorated();
new Concrete().run();
new Opaque();
new Ambient();
"#,
            ),
        ]);
        let findings = class_diagnostics(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].message.contains("`Live`") && findings[0].message.contains("`dead`"),
            "{findings:?}"
        );
        assert!(
            snapshot
                .candidates
                .contains(&"src/mixed.ts#Dead".to_string()),
            "the dead class itself stays a deletion candidate: {:?}",
            snapshot.candidates
        );
    }

    #[test]
    fn skips_public_api_classes() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"probe","private":true,"main":"src/public.ts"}"#,
            ),
            (
                "src/public.ts",
                "export class Public { used() {} unused() {} }\n",
            ),
            (
                "src/index.ts",
                "import { Public } from \"./public\";\nnew Public().used();\n",
            ),
        ]);
        let findings = class_diagnostics(&snapshot);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn credits_next_initial_props() {
        let snapshot = snapshot(&[
            (
                "package.json",
                r#"{"name":"probe","private":true,"main":"src/index.ts","dependencies":{"next":"^16.0.0"}}"#,
            ),
            (
                "src/widget.ts",
                r#"
export class Widget {
  getInitialProps() { return {}; }
  render() { return null; }
  dead() {}
}
"#,
            ),
            (
                "src/index.ts",
                "import { Widget } from \"./widget\";\nnew Widget();\n",
            ),
        ]);
        let findings = class_diagnostics(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("`dead`"));
    }

    #[test]
    fn credits_react_lifecycle_and_runtime_members() {
        let snapshot = snapshot(&[
            ("package.json", react_package()),
            (
                "src/Widget.tsx",
                r#"
import React from "react";
export class Widget extends React.Component {
  state = { count: 0 };
  componentDidMount() {}
  render() { return <div>{this.state.count}</div>; }
  deadHandler() {}
}
"#,
            ),
            (
                "src/index.ts",
                "import { Widget } from \"./Widget\";\nnew Widget();\n",
            ),
        ]);
        let findings = class_diagnostics(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("`deadHandler`"));
    }
}
