use super::*;
use crate::summary::{BindingKind, DynamicSpecifier, UnknownScope};

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
    let parsed = summary("hello", "README.md");
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
fn captures_statically_named_dynamic_imports() {
    let source = r#"
const { getDb, isDbConfigured: isConfigured, default: fallback } = await import("./db");
const editor = (await import("./editor")).default;
const requireConfig = require("./config").config;
const { ...rest } = await import("./rest");
const namespace = await import("./namespace");
const promise = import("./promise");
const thenable = await import("./thenable").then((module) => module.used);
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
    assert_eq!(imported("./thenable"), None);
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
