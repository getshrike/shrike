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
fn generated_markers_are_detected() {
    let parsed = summary("// @generated\nconst x = 1;\n", "src/gen.ts");
    assert!(parsed.generated);
}
