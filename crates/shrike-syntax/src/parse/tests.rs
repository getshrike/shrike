use super::*;
use crate::summary::{BindingKind, DynamicKind, DynamicSpecifier, UnknownScope};

fn summary(source: &str, path: &str) -> FileSummary {
    parse_source(path, source)
}

#[test]
fn extracts_imports_and_exports() {
    let source = r#"
import React, { useState as state } from "react";
import type { Foo } from "./foo";
import * as ns from "./ns";
export function formatInvoice(value: string) { return value; }
export default function Page() { return <div />; }
export const a = 1, b = 2;
export { state as localState };
export { thing } from "./barrel";
export * from "./all";
"#;
    let parsed = summary(source, "src/app/page.tsx");
    assert_eq!(parsed.parse_status, ParseStatus::Ok);
    assert_eq!(parsed.imports.len(), 3);
    assert_eq!(parsed.imports[0].bindings.len(), 2);
    assert!(parsed.imports[1].type_only);
    assert!(parsed.imports[2].bindings[0].kind == BindingKind::Namespace);
    assert!(
        parsed
            .declarations
            .iter()
            .any(|d| d.name == "formatInvoice" && d.exported)
    );
    assert!(
        parsed
            .declarations
            .iter()
            .any(|d| d.default && d.name == "Page")
    );
    assert_eq!(parsed.re_exports.len(), 1);
    assert_eq!(parsed.export_alls.len(), 1);
    assert_eq!(parsed.local_exports.len(), 1);
    assert!(parsed.exported_names().contains(&"a".to_string()));
    assert!(parsed.exported_names().contains(&"localState".to_string()));
}

#[test]
fn captures_directives_and_server_actions() {
    let source = r#"
"use client";
export function Component() { return null; }
"#;
    let parsed = summary(source, "src/components/x.tsx");
    assert!(parsed.has_directive("use client"));
    assert!(parsed.misplaced_directives.is_empty());
}

#[test]
fn classifies_all_action_use_server_modules() {
    let action = summary(
        r#"
"use server";
export async function declared() { return 1; }
export const arrow = async () => 2;
export const expression = async function () { return 3; };
export type Payload = string;
"#,
        "src/actions.ts",
    );
    assert!(action.server_action_module);
    assert!(action.inline_server_action_exports.is_empty());

    for source in [
        "\"use server\";\nexport function sync() { return 1; }\n",
        "\"use server\";\nexport const value = 1;\n",
        "\"use server\";\nexport const mixed = async () => 1, value = 2;\n",
        "\"use server\";\nexport { helper } from \"./helper\";\n",
    ] {
        assert!(
            !summary(source, "src/actions.ts").server_action_module,
            "{source}"
        );
    }

    let aliased = summary(
        "\"use server\";\nconst action = async () => 1;\nexport { action };\nexport default action;\n",
        "src/actions.ts",
    );
    assert!(aliased.server_action_module);

    let type_reexport = summary(
        "\"use server\";\nexport type { Payload } from \"./types\";\n",
        "src/actions.ts",
    );
    assert!(type_reexport.server_action_module);
}

#[test]
fn records_inline_server_action_exports() {
    let parsed = summary(
        r#"
export async function declared() { "use server"; return 1; }
export const arrow = async () => { "use server"; return 2; };
export const expression = function () { "use server"; return 3; };
export default async function () { "use server"; return 4; }
function local() { "use server"; return 5; }
const notExported = async () => { "use server"; return 6; };
"#,
        "src/actions.ts",
    );
    assert_eq!(
        parsed.inline_server_action_exports,
        vec!["arrow", "declared", "default", "expression"]
    );
    assert!(!parsed.server_action_module);

    let default_arrow = summary(
        "export default async () => { \"use server\"; return 7; };\n",
        "src/actions.ts",
    );
    assert_eq!(default_arrow.inline_server_action_exports, vec!["default"]);
}

#[test]
fn records_directive_strings_after_the_prologue() {
    let source = r#"
import { thing } from "./thing";
"use client";
const value = 1;
"use server";
"use strict";
const nested = { directive: "use client" };
"#;
    let parsed = summary(source, "src/components/x.tsx");
    assert!(!parsed.has_directive("use client"));
    assert!(!parsed.has_directive("use server"));
    let directives: Vec<(&str, u32)> = parsed
        .misplaced_directives
        .iter()
        .map(|record| (record.directive.as_str(), record.span.line))
        .collect();
    assert_eq!(directives, vec![("use client", 3), ("use server", 5)]);
}

#[test]
fn a_leading_directive_before_imports_is_not_misplaced() {
    let source = r#"
"use client";
import { thing } from "./thing";
"use server";
export function Component() { return null; }
"#;
    let parsed = summary(source, "src/components/x.tsx");
    assert!(parsed.has_directive("use client"));
    assert_eq!(parsed.misplaced_directives.len(), 1);
    assert_eq!(parsed.misplaced_directives[0].directive, "use server");
    assert_eq!(parsed.misplaced_directives[0].span.line, 4);
}

#[test]
fn captures_dynamic_forms() {
    let source = r#"
const a = import("./literal");
const b = import(`./locales/${lang}.json`);
const c = import(someVar);
const d = require("./required");
const e = require(name);
const f = ns[key];
const g = globalThis[name];
"#;
    let parsed = summary(source, "src/dynamic.ts");
    let kinds: Vec<&DynamicSpecifier> = parsed
        .dynamic
        .iter()
        .map(|record| &record.specifier)
        .collect();
    assert!(
        kinds
            .iter()
            .any(|kind| matches!(kind, DynamicSpecifier::Literal(value) if value == "./literal"))
    );
    assert!(kinds.iter().any(
        |kind| matches!(kind, DynamicSpecifier::TemplatePrefix(value) if value == "./locales/")
    ));
    assert!(
        kinds
            .iter()
            .any(|kind| matches!(kind, DynamicSpecifier::Unknown))
    );
    assert!(
        parsed
            .unknown
            .iter()
            .any(|record| matches!(record.scope, UnknownScope::Repo))
    );
    assert!(
        parsed
            .module_refs
            .members
            .iter()
            .any(|member| member.object == "ns" && member.property.is_none())
    );
}

#[test]
fn captures_bundler_globs() {
    let source = r#"
const pages = import.meta.glob("./pages/*.tsx");
const mixed = import.meta.glob(["./pages/*.tsx", "!./pages/skip.tsx", "./layouts/**/*.tsx"]);
const dynamic = import.meta.glob(pattern);
const icons = require.context("./icons", true, /\.tsx$/);
const flat = require.context("./flat", false);
const bracketed = require.context("./[icons]", false);
const contextDynamic = require.context(directory);
"#;
    let parsed = summary(source, "src/main.tsx");
    assert_eq!(parsed.parse_status, ParseStatus::Ok);
    let globs: Vec<&DynamicSpecifier> = parsed
        .dynamic
        .iter()
        .map(|record| &record.specifier)
        .collect();
    assert_eq!(globs.len(), 7, "{globs:?}");
    assert!(matches!(
        globs[0],
        DynamicSpecifier::Glob { patterns, excludes }
            if patterns == &vec!["./pages/*.tsx".to_string()] && excludes.is_empty()
    ));
    assert!(matches!(
        globs[1],
        DynamicSpecifier::Glob { patterns, excludes }
            if patterns == &vec!["./pages/*.tsx".to_string(), "./layouts/**/*.tsx".to_string()]
                && excludes == &vec!["./pages/skip.tsx".to_string()]
    ));
    assert!(matches!(globs[2], DynamicSpecifier::Unknown));
    assert!(matches!(
        globs[3],
        DynamicSpecifier::Glob { patterns, .. } if patterns == &vec!["./icons/**/*".to_string()]
    ));
    assert!(matches!(
        globs[4],
        DynamicSpecifier::Glob { patterns, .. } if patterns == &vec!["./flat/*".to_string()]
    ));
    assert!(matches!(
        globs[5],
        DynamicSpecifier::Glob { patterns, .. }
            if patterns == &vec!["./[[]icons[]]/*".to_string()]
    ));
    assert!(matches!(globs[6], DynamicSpecifier::Unknown));
    // `require.context` must not also produce a plain `require()` record.
    assert!(!parsed.dynamic.iter().any(
        |record| matches!(&record.specifier, DynamicSpecifier::Literal(value) if value == "./icons")
    ));
}

#[test]
fn captures_worker_and_worklet_modules() {
    let source = r#"
const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
const shared = new SharedWorker(new URL("./shared.ts", import.meta.url));
const plain = new Worker("./plain.js");
navigator.serviceWorker.register(new URL("./service-worker.ts", import.meta.url));
audioContext.audioWorklet.addModule("./processor.ts");
CSS.paintWorklet.addModule(new URL("./paint.ts", import.meta.url));
importScripts("./first.js", "./second.js");
const dynamicWorker = new Worker(workerUrl);
"#;
    let parsed = summary(source, "src/worker-host.ts");
    assert_eq!(parsed.parse_status, ParseStatus::Ok);
    let workers: Vec<(DynamicKind, &DynamicSpecifier)> = parsed
        .dynamic
        .iter()
        .filter(|record| record.kind == DynamicKind::Worker)
        .map(|record| (record.kind, &record.specifier))
        .collect();
    let literals: Vec<&str> = workers
        .iter()
        .filter_map(|(_, specifier)| match specifier {
            DynamicSpecifier::Literal(value) => Some(value.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        literals,
        vec![
            "./worker.ts",
            "./shared.ts",
            "./plain.js",
            "./service-worker.ts",
            "./processor.ts",
            "./paint.ts",
            "./first.js",
            "./second.js"
        ]
    );
    assert!(
        workers
            .iter()
            .any(|(_, specifier)| matches!(specifier, DynamicSpecifier::Unknown)),
        "a computed worker URL must fail closed: {workers:?}"
    );
}

#[test]
fn captures_jsx_namespace_members_and_value_uses() {
    let source = r#"
import * as UI from "./ui";
export function Page() { return <UI.Button />; }
const all = { ...UI };
const { Card } = UI;
"#;
    let parsed = summary(source, "src/page.tsx");
    let page = parsed
        .declarations
        .iter()
        .find(|declaration| declaration.name == "Page")
        .expect("Page declaration");
    assert!(
        page.references
            .members
            .iter()
            .any(|member| member.object == "UI" && member.property.as_deref() == Some("Button")),
        "{:?}",
        page.references.members
    );
    assert!(
        !page.references.free.iter().any(|name| name == "UI"),
        "a JSX member object is not a free namespace use: {:?}",
        page.references.free
    );
    assert!(
        !page.references.jsx.iter().any(|name| name == "Button"),
        "the JSX member property is not a local component reference: {:?}",
        page.references.jsx
    );
    assert!(
        parsed.module_refs.free.iter().any(|name| name == "UI"),
        "spread and destructuring are free namespace uses: {:?}",
        parsed.module_refs.free
    );
}

#[test]
fn parse_errors_block_summary() {
    let parsed = summary("export function broken( {", "src/broken.ts");
    assert_eq!(parsed.parse_status, ParseStatus::Error);
    assert!(parsed.declarations.is_empty());
    assert!(parsed.parse_message.is_some());
}

#[test]
fn unsupported_extension_is_skipped() {
    let parsed = summary("hello", "README.txt");
    assert_eq!(parsed.parse_status, ParseStatus::Skipped);
}

#[test]
fn framework_components_are_informational() {
    let parsed = summary("<template><div /></template>", "src/App.vue");
    assert_eq!(parsed.parse_status, ParseStatus::Skipped);
    assert_eq!(parsed.module_kind, "vue");
    assert_eq!(parsed.diagnostics[0].code, "framework_component");
    assert_eq!(parsed.diagnostics[0].severity, shrike_core::Severity::Info);
}

#[test]
fn generated_markers_are_detected() {
    let parsed = summary("// @generated\nconst x = 1;\n", "src/gen.ts");
    assert!(parsed.generated);
}

#[test]
fn mdx_esm_boundary_is_parsed_and_fences_are_ignored() {
    let source = r#"---
title: Hello
---

import { Widget } from "../components/widget";

# Hello

<Widget />

```ts
import { NotReal } from "./fake";
```
"#;
    let parsed = summary(source, "app/content.mdx");
    assert_eq!(parsed.parse_status, ParseStatus::Ok);
    assert_eq!(parsed.module_kind, "mdx");
    assert_eq!(parsed.imports.len(), 1);
    assert_eq!(parsed.imports[0].source, "../components/widget");
    assert_eq!(parsed.imports[0].bindings[0].local, "Widget");
}

#[test]
fn markdown_page_uses_the_mdx_esm_boundary() {
    let source = r#"# About

import { Widget } from "../components/widget";
import { helper as localHelper } from "../utils";

<Widget value={localHelper()} />
"#;
    let parsed = summary(source, "pages/about.md");
    assert_eq!(parsed.parse_status, ParseStatus::Ok);
    assert_eq!(parsed.module_kind, "mdx");
    assert_eq!(parsed.imports.len(), 2);
    assert_eq!(parsed.imports[0].source, "../components/widget");
    assert_eq!(parsed.imports[0].bindings[0].local, "Widget");
    assert_eq!(parsed.imports[1].bindings[0].imported, "helper");
    assert_eq!(parsed.imports[1].bindings[0].local, "localHelper");
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn mdx_multiline_esm_is_preserved_without_the_body() {
    let source = r#"
import {
  Widget,
  Other,
} from "../components";

export const meta = {
  title: "Hello",
};

export function Local() {
  return <div>Hello</div>;
}

# Body

<Local />
"#;
    let parsed = summary(source, "app/page.mdx");
    assert_eq!(parsed.parse_status, ParseStatus::Ok);
    assert_eq!(parsed.imports.len(), 1);
    assert_eq!(parsed.imports[0].bindings.len(), 2);
    assert!(parsed.declarations.iter().any(|item| item.name == "meta"));
    assert!(parsed.declarations.iter().any(|item| item.name == "Local"));
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn mdx_declarations_are_treated_as_body_references() {
    let source = r#"
export function Local() { return <div />; }
export const helper = 1;

# Hello

<Local />
"#;
    let parsed = summary(source, "app/content.mdx");
    assert_eq!(parsed.parse_status, ParseStatus::Ok);
    assert!(parsed.declarations.iter().any(|item| item.name == "Local"));
    assert!(
        parsed
            .module_refs
            .identifiers
            .contains(&"Local".to_string())
    );
    assert!(
        parsed
            .module_refs
            .identifiers
            .contains(&"helper".to_string())
    );
}

#[test]
fn narrows_const_namespace_dynamic_imports_from_static_members() {
    let source = r#"
const used = await import("./used");
used.alpha;
used.beta();
async function load() {
  const lazy = await import("./lazy");
  return lazy.gamma;
}
const deep = await import("./deep");
deep.outer.inner;
const required = require("./required");
required.delta;
const ui = await import("./ui");
const element = <ui.Button />;
const escaped = await import("./escaped");
console.log(escaped);
const computed = await import("./computed");
computed[key];
let reassigned = await import("./reassigned");
reassigned.epsilon;
const exported = await import("./exported");
export { exported };
const unused = await import("./unused");
"#;
    let parsed = summary(source, "src/namespace-dynamic.tsx");
    let imported = |specifier: &str| {
        parsed
            .dynamic
            .iter()
            .find(|record| {
                matches!(&record.specifier, DynamicSpecifier::Literal(value) if value == specifier)
            })
            .and_then(|record| record.imported.clone())
    };
    assert_eq!(
        imported("./used"),
        Some(vec!["alpha".to_string(), "beta".to_string()])
    );
    assert_eq!(imported("./lazy"), Some(vec!["gamma".to_string()]));
    assert_eq!(imported("./deep"), Some(vec!["outer".to_string()]));
    assert_eq!(imported("./required"), Some(vec!["delta".to_string()]));
    assert_eq!(imported("./ui"), Some(vec!["Button".to_string()]));
    assert_eq!(imported("./escaped"), None);
    assert_eq!(imported("./computed"), None);
    assert_eq!(imported("./reassigned"), None);
    assert_eq!(imported("./exported"), None);
    assert_eq!(imported("./unused"), None);
    assert_eq!(
        parsed
            .dynamic
            .iter()
            .filter(|record| {
                matches!(&record.specifier, DynamicSpecifier::Literal(value) if value == "./used")
            })
            .count(),
        1,
        "a narrowed namespace binding must not also produce a widening record"
    );
}

#[test]
fn captures_statically_named_dynamic_imports() {
    let source = r#"
const { getDb, isDbConfigured: isConfigured, default: fallback } = await import("./db");
const editor = (await import("./editor")).default;
const requireConfig = require("./config").config;
const { ...rest } = await import("./rest");
const namespace = await import("./namespace");
const promise = import("./promise");
const thenable = await import("./thenable").then((module) => module.used);
const [{ first }, second] = await Promise.all([
  import("./one"),
  import("./two"),
]);
const [{ third }, { fourth }] = await Promise.all([import("./three"), import("./four")]);
const [fifth, ...remaining] = await Promise.all([import("./five"), import("./six")]);
const [{ ...all }] = await Promise.all([import("./seven")]);
const [eighth] = await Promise.all([import("./eight").then((module) => module.used)]);
const blockThen = import("./block-then").then((module) => {
  module.alpha();
  module.beta();
});
const destructuredThen = import("./destructured-then").then(({ gamma, delta }) => gamma + delta);
const functionThen = import("./function-then").then(function (module) {
  return module.epsilon;
});
const objectThen = import("./object-then").then((module) => ({ default: module.zeta }));
const escapedThen = import("./escaped-then").then((module) => use(module));
const computedThen = import("./computed-then").then((module) => module[key]);
const unusedThen = import("./unused-then").then(() => 1);
"#;
    let parsed = summary(source, "src/dynamic.ts");
    let imported = |specifier: &str| {
        parsed
            .dynamic
            .iter()
            .find(|record| {
                matches!(&record.specifier, DynamicSpecifier::Literal(value) if value == specifier)
            })
            .and_then(|record| record.imported.clone())
    };
    assert_eq!(
        imported("./db"),
        Some(vec![
            "default".to_string(),
            "getDb".to_string(),
            "isDbConfigured".to_string()
        ])
    );
    assert_eq!(imported("./editor"), Some(vec!["default".to_string()]));
    assert_eq!(imported("./config"), Some(vec!["config".to_string()]));
    assert_eq!(imported("./rest"), None);
    assert_eq!(imported("./namespace"), None);
    assert_eq!(imported("./promise"), None);
    assert_eq!(imported("./thenable"), Some(vec!["used".to_string()]));
    assert_eq!(imported("./one"), Some(vec!["first".to_string()]));
    assert_eq!(imported("./two"), None);
    assert_eq!(imported("./three"), Some(vec!["third".to_string()]));
    assert_eq!(imported("./four"), Some(vec!["fourth".to_string()]));
    assert_eq!(imported("./five"), None);
    assert_eq!(imported("./six"), None);
    assert_eq!(imported("./seven"), None);
    assert_eq!(imported("./eight"), Some(vec!["used".to_string()]));
    assert_eq!(
        imported("./block-then"),
        Some(vec!["alpha".to_string(), "beta".to_string()])
    );
    assert_eq!(
        imported("./destructured-then"),
        Some(vec!["delta".to_string(), "gamma".to_string()])
    );
    assert_eq!(
        imported("./function-then"),
        Some(vec!["epsilon".to_string()])
    );
    assert_eq!(imported("./object-then"), Some(vec!["zeta".to_string()]));
    assert_eq!(imported("./escaped-then"), None);
    assert_eq!(imported("./computed-then"), None);
    assert_eq!(imported("./unused-then"), None);
    assert_eq!(
        parsed
            .dynamic
            .iter()
            .filter(|record| {
                matches!(&record.specifier, DynamicSpecifier::Literal(value) if value == "./db")
            })
            .count(),
        1,
        "a handled import must not also produce a widening record"
    );
    assert_eq!(
        parsed
            .dynamic
            .iter()
            .filter(|record| {
                matches!(&record.specifier, DynamicSpecifier::Literal(value) if value == "./one")
            })
            .count(),
        1,
        "a narrowed Promise.all import must not also widen"
    );
}

#[test]
fn collects_type_position_references() {
    let source = r#"
type Config = import("postcss-load-config").Config;
/** @type {import("left-pad").Thing} */
const value = 1;
"#;
    let parsed = summary(source, "src/config.ts");
    assert!(
        parsed
            .type_refs
            .contains(&"postcss-load-config".to_string())
    );
    assert!(parsed.type_refs.contains(&"left-pad".to_string()));
    assert!(parsed.type_imports.iter().any(|item| {
        item.source == "postcss-load-config" && item.imported.as_deref() == Some("Config")
    }));
    assert!(
        parsed
            .type_imports
            .iter()
            .any(|item| item.source == "left-pad" && item.imported.as_deref() == Some("Thing"))
    );
}

#[test]
fn collects_jsdoc_import_types_with_a_qualifier() {
    let parsed = summary(
        "/** @type {import('./types').JSDocType} */\nexport const value = { name: 'x' };\n",
        "src/value.js",
    );
    assert_eq!(parsed.parse_status, ParseStatus::Ok);
    assert!(parsed.type_refs.contains(&"./types".to_string()));
    assert!(
        parsed.type_imports.iter().any(|item| {
            item.source == "./types" && item.imported.as_deref() == Some("JSDocType")
        })
    );
}
