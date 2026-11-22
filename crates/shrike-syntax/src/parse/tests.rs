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
fn records_definitely_invalid_server_exports() {
    let parsed = summary(
        r#"
"use server";
export const count = 1;
export const payload = { id: 1 };
export const list = [1, 2];
export let uninitialized;
export enum Kind { A }
const local = "text";
export { local as alias };
export const wrapped = (1 as number);
export const action = async () => 1;
export const later = makeAction();
export const maybe = someIdentifier;
export function declared() { return 1; }
export class Direct {}
export { remote } from "./remote";
export type Payload = string;
export declare const ambient: string;
declare const localAmbient: string;
export { localAmbient };
"#,
        "src/actions.ts",
    );
    let invalid: Vec<(&str, u32)> = parsed
        .invalid_server_exports
        .iter()
        .map(|record| (record.name.as_str(), record.span.line))
        .collect();
    assert_eq!(
        invalid,
        vec![
            ("Kind", 7),
            ("alias", 9),
            ("count", 3),
            ("list", 5),
            ("payload", 4),
            ("uninitialized", 6),
            ("wrapped", 10),
        ],
        "only definitely non-callable values are recorded"
    );

    let default_value = summary("\"use server\";\nexport default 42;\n", "src/actions.ts");
    assert_eq!(default_value.invalid_server_exports.len(), 1);
    assert_eq!(default_value.invalid_server_exports[0].name, "default");
    assert_eq!(default_value.invalid_server_exports[0].span.line, 2);

    // Without the leading directive the file is an ordinary module.
    let no_directive = summary(
        "export const count = 1;\nexport enum Kind { A }\n",
        "src/actions.ts",
    );
    assert!(no_directive.invalid_server_exports.is_empty());

    // A body-level `"use server"` makes only that function an action; the
    // module-level export is still ordinary.
    let inline = summary(
        "export const value = 1;\nexport async function action() { \"use server\"; return 1; }\n",
        "src/actions.ts",
    );
    assert!(inline.invalid_server_exports.is_empty());
}

#[test]
fn records_next_segment_config_export_shapes() {
    let parsed = summary(
        r#"
export const runtime = "edge";
export const instant = true;
export const prefetch = "auto";
export const ensureStatic = false;
export const unstable_dynamicStaleTime = 60;
export const dynamic = "force-dynamic";
export const revalidate = 0;
export const maxDuration = 10;
export const preferredRegion = ["iad1"];
export const fetchCache = "auto";
export const dynamicParams = false;
export const ignored = "value";
export const later = makeRuntime();
export let mutableRuntime = "edge";
export const parenthesized = ("edge");
export const asserted = "edge" as const;
export const template = `edge`;
export const config = { runtime: "edge" };
"#,
        "src/app/page.tsx",
    );
    let values: Vec<(&str, String)> = parsed
        .exported_const_values
        .iter()
        .map(|record| (record.name.as_str(), format!("{:?}", record.value)))
        .collect();
    assert_eq!(
        values,
        vec![
            ("runtime", "String(\"edge\")".to_string()),
            ("instant", "Boolean(true)".to_string()),
            ("prefetch", "String(\"auto\")".to_string()),
            ("ensureStatic", "Boolean(false)".to_string()),
            ("unstable_dynamicStaleTime", "Number(\"60\")".to_string()),
            ("dynamic", "String(\"force-dynamic\")".to_string()),
            ("revalidate", "Number(\"0\")".to_string()),
            ("maxDuration", "Number(\"10\")".to_string()),
            ("preferredRegion", "Array".to_string()),
            ("fetchCache", "String(\"auto\")".to_string()),
            ("dynamicParams", "Boolean(false)".to_string()),
        ],
        "only directly exported consts with Next.js-extractable values are recorded"
    );

    let wrapped = summary(
        "export const runtime = 'edge' as const;\nexport const prefetch = `auto`;\n",
        "src/app/page.tsx",
    );
    let wrapped_values: Vec<(&str, String)> = wrapped
        .exported_const_values
        .iter()
        .map(|record| (record.name.as_str(), format!("{:?}", record.value)))
        .collect();
    assert_eq!(
        wrapped_values,
        vec![
            ("runtime", "String(\"edge\")".to_string()),
            ("prefetch", "String(\"auto\")".to_string()),
        ],
        "TypeScript assertions and no-substitution templates are extracted"
    );

    let parenthesized = summary("export const runtime = ('edge');\n", "src/app/page.tsx");
    assert!(
        parenthesized.exported_const_values.is_empty(),
        "Next.js does not extract parenthesized initializers"
    );

    let special = summary(
        r#"
export function generateStaticParams() { return []; }
export const unstable_paramMatching = {};
export { unstable_generateParamMatching } from "./params";
export type { generateStaticParams as typedParams } from "./types";
"#,
        "src/app/page.tsx",
    );
    let names: Vec<&str> = special
        .next_segment_exports
        .iter()
        .map(|record| record.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "generateStaticParams",
            "unstable_paramMatching",
            "unstable_generateParamMatching",
        ],
        "function declarations, first variable declarators, and export specifiers count"
    );

    // `export * as` and local aliases are not recognized by Next.js.
    let ignored = summary(
        r#"
export * as generateStaticParams from "./params";
const { generateStaticParams: destructured } = {};
export { destructured as generateStaticParams };
"#,
        "src/app/page.tsx",
    );
    assert!(ignored.next_segment_exports.is_empty());

    // Next.js checks only the first declarator for the special-export flags,
    // but reads config values from every declarator.
    let mixed = summary(
        "export const runtime = 'edge', generateStaticParams = () => [];\n",
        "src/app/page.tsx",
    );
    assert_eq!(mixed.exported_const_values.len(), 1);
    assert_eq!(mixed.exported_const_values[0].name, "runtime");
    assert!(mixed.next_segment_exports.is_empty());

    let declared = summary(
        "export let generateStaticParams = () => [];\n",
        "src/app/page.tsx",
    );
    assert_eq!(declared.next_segment_exports.len(), 1);
    assert_eq!(
        declared.next_segment_exports[0].name,
        "generateStaticParams"
    );
}

#[test]
fn extracts_unused_pages_router_data_keys() {
    for (source, expected) in [
        (
            "export async function getStaticProps() { return { props: { used: 1, unused: 2 } }; }\nexport default function Page({ used }: { used: number }) { return <div>{used}</div>; }\n",
            vec![("getStaticProps", "unused")],
        ),
        (
            "export const getServerSideProps = async () => ({ props: { unused: 1 } });\nexport default function Page() { return <div />; }\n",
            vec![("getServerSideProps", "unused")],
        ),
        (
            "interface Props { used: number; unused: string; }\nfunction Page(props: Props) { return <div>{props.used}</div>; }\nPage.getInitialProps = async () => ({ used: 1, unused: 'x' });\nexport default Page;\n",
            vec![("getInitialProps", "unused")],
        ),
        (
            "export async function getStaticProps() { return { props: { used: 1 } }; }\nexport default function Page({ used }: { used: number }) { return <div>{used}</div>; }\n",
            vec![],
        ),
        (
            "export async function getStaticProps() { return { notFound: true }; }\nexport default function Page() { return <div />; }\n",
            vec![],
        ),
    ] {
        let parsed = summary(source, "pages/index.tsx");
        let actual: Vec<(&str, &str)> = parsed
            .unused_page_data_keys
            .iter()
            .map(|record| (record.function.as_str(), record.key.as_str()))
            .collect();
        assert_eq!(actual, expected, "data keys for {source:?}");
    }

    for source in [
        // Opaque `props` value.
        "const data = { used: 1, unused: 2 };\nexport async function getStaticProps() { return { props: data }; }\nexport default function Page({ used }: { used: number }) { return <div>{used}</div>; }\n",
        // Spread and computed keys.
        "export async function getStaticProps() { return { props: { ...base, unused: 2 } }; }\nexport default function Page({ used }: { used: number }) { return <div>{used}</div>; }\n",
        "export async function getStaticProps() { return { props: { [key]: 2 } }; }\nexport default function Page({ used }: { used: number }) { return <div>{used}</div>; }\n",
        // Multiple returns.
        "export async function getStaticProps() { if (x) { return { props: { unused: 1 } }; } return { props: { used: 1 } }; }\nexport default function Page({ used }: { used: number }) { return <div>{used}</div>; }\n",
        // Whole-object props use.
        "export async function getStaticProps() { return { props: { used: 1, unused: 2 } }; }\nexport default function Page(props: { used: number; unused: number }) { return <pre>{JSON.stringify(props)}</pre>; }\n",
        // Rest destructure.
        "export async function getStaticProps() { return { props: { used: 1, unused: 2 } }; }\nexport default function Page({ used, ...rest }: { used: number; unused: number }) { return <pre>{JSON.stringify(rest)}</pre>; }\n",
        // Imported/unknown props type.
        "import type { Props } from './types';\nexport async function getStaticProps() { return { props: { unused: 2 } }; }\nexport default function Page(props: Props) { return <div>{props.used}</div>; }\n",
        // Named getInitialProps export is not read by Next.js.
        "export const getInitialProps = async () => ({ unused: 2 });\nexport default function Page() { return <div />; }\n",
        // Not exported, so Next.js does not call it.
        "async function getStaticProps() { return { props: { unused: 2 } }; }\nexport default function Page() { return <div />; }\n",
        // Class component.
        "export async function getStaticProps() { return { props: { unused: 2 } }; }\nexport default class Page extends React.Component { render() { return <div />; } }\n",
        // HOC wrapper.
        "export async function getStaticProps() { return { props: { unused: 2 } }; }\nfunction Page() { return <div />; }\nexport default withAuth(Page);\n",
        // Re-exported data function.
        "export { getStaticProps } from './data';\nexport default function Page() { return <div />; }\n",
    ] {
        let parsed = summary(source, "pages/index.tsx");
        assert!(
            parsed.unused_page_data_keys.is_empty(),
            "expected abstention for {source:?}, got {:?}",
            parsed.unused_page_data_keys
        );
    }
}

#[test]
fn extracts_pages_router_component_metadata() {
    for (source, expected) in [
        ("export default function Page() {}\n", Some("Page")),
        ("export default class Page {}\n", Some("Page")),
        (
            "const Page = () => null;\nexport default Page;\n",
            Some("Page"),
        ),
        (
            "function Page() {}\nexport { Page as default };\n",
            Some("Page"),
        ),
        (
            "const Page = () => null;\nexport default (Page);\n",
            Some("Page"),
        ),
        ("export default function () {}\n", None),
        ("export default memo(Page);\n", None),
    ] {
        let parsed = summary(source, "pages/index.tsx");
        assert_eq!(
            parsed.default_export_local.as_deref(),
            expected,
            "default export local for {source:?}"
        );
    }

    for (source, expected) in [
        (
            "function Page() {}\nPage.getInitialProps = async () => ({});\nexport default Page;\n",
            true,
        ),
        (
            "export default function Page() {}\nPage.getInitialProps = async () => ({});\n",
            true,
        ),
        (
            "export default class Page {\n  static async getInitialProps() { return {}; }\n}\n",
            true,
        ),
        (
            "export default class {\n  static getInitialProps = async () => ({});\n}\n",
            true,
        ),
        (
            "const Page = class { static getInitialProps() {} };\nexport default Page;\n",
            true,
        ),
        (
            "export default class Page {}\nPage.getInitialProps = async () => ({});\n",
            true,
        ),
        (
            "export default function Page() {}\nPage.getInitialProps = undefined;\n",
            false,
        ),
        (
            "export default function Page() {}\nPage.getInitialProps = null;\n",
            false,
        ),
        (
            "export default class Page {\n  static getInitialProps = undefined;\n}\n",
            false,
        ),
        (
            "export default class Page {\n  static getInitialProps;\n}\n",
            false,
        ),
        (
            "export default class Page {\n  static getInitialProps = () => ({});\n}\n",
            true,
        ),
        (
            "const Helper = () => null;\nHelper.getInitialProps = async () => ({});\nexport default function Page() {}\n",
            false,
        ),
        (
            "export const getInitialProps = async () => ({});\nexport default function Page() {}\n",
            false,
        ),
        (
            "export default withAuth(Page);\nPage.getInitialProps = async () => ({});\n",
            false,
        ),
        ("Page.getInitialProps = async () => ({});\n", false),
    ] {
        let parsed = summary(source, "pages/index.tsx");
        assert_eq!(
            parsed.default_export_has_get_initial_props, expected,
            "getInitialProps for {source:?}"
        );
    }

    for (source, expected) in [
        ("export const config = { runtime: 'edge' };\n", Some("edge")),
        (
            "export const config = { runtime: 'bogus' as const };\n",
            Some("bogus"),
        ),
        (
            "export const config = { 'runtime': 'experimental-edge' };\n",
            Some("experimental-edge"),
        ),
        (
            "export const config = { runtime: 'edge' } as const;\n",
            Some("edge"),
        ),
        ("export const config = ({ runtime: 'edge' });\n", None),
        ("export const config = { runtime: someVar };\n", None),
        ("export const config = { runtime: ('edge') };\n", None),
        ("export const config = { revalidate: 60 };\n", None),
        ("export let config = { runtime: 'edge' };\n", None),
        ("export const config = getConfig();\n", None),
        ("export const config = { [runtime]: 'edge' };\n", None),
    ] {
        let parsed = summary(source, "pages/index.tsx");
        assert_eq!(
            parsed
                .pages_config_runtime
                .as_ref()
                .map(|record| match &record.value {
                    crate::summary::ExportedConstValue::String(value) => value.as_str(),
                    _ => "non-string",
                }),
            expected,
            "config runtime for {source:?}"
        );
    }
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
    assert_eq!(imported("./unused"), Some(Vec::new()));
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
fn narrows_dynamic_imports_returned_from_local_loaders() {
    let source = r#"
async function loadDeployment(mode: string) {
  reset(mode);
  return import("./deployment");
}
async function loadRealtime(overrides: Record<string, string>) {
  return import("./env");
}
const { resolveDeploymentPlan } = await loadDeployment("cloud");
const realtime = await loadRealtime({});
realtime.isRealtimeConfigured;
const { named } = await loadDeployment("self-hosted");

function opaque() {
  return import("./opaque");
}
consume(opaque);

async function discarded() {
  return import("./discarded");
}
await discarded();

async function unknown() {
  return import("./unknown");
}
const anything = await unknown();
consume(anything);

async function mixed() {
  if (flag) return import("./mixed");
  return null;
}
const { used } = await mixed();
"#;
    let parsed = summary(source, "src/loaders.ts");
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
        imported("./deployment"),
        Some(vec![
            "named".to_string(),
            "resolveDeploymentPlan".to_string()
        ])
    );
    assert_eq!(
        imported("./env"),
        Some(vec!["isRealtimeConfigured".to_string()])
    );
    assert_eq!(imported("./opaque"), None);
    assert_eq!(imported("./discarded"), Some(Vec::new()));
    assert_eq!(imported("./unknown"), None);
    assert_eq!(imported("./mixed"), None);
}

#[test]
fn abstains_when_a_loader_name_is_shadowed() {
    let source = r#"
async function load() {
  return import("./mod");
}
function consume(load: () => Promise<unknown>) {
  return load();
}
const { used } = await load();

async function other() {
  return import("./other");
}
function local() {
  const other = () => null;
  return other();
}
const { alpha } = await other();

async function duplicate() {
  return import("./duplicate");
}
function scope() {
  function duplicate() {
    return null;
  }
  return duplicate();
}
const { beta } = await duplicate();
"#;
    let parsed = summary(source, "src/shadowed-loaders.ts");
    let imported = |specifier: &str| {
        parsed
            .dynamic
            .iter()
            .find(|record| {
                matches!(&record.specifier, DynamicSpecifier::Literal(value) if value == specifier)
            })
            .and_then(|record| record.imported.clone())
    };
    assert_eq!(imported("./mod"), None);
    assert_eq!(imported("./other"), None);
    assert_eq!(imported("./duplicate"), None);
}

#[test]
fn narrows_assertion_only_namespace_bindings() {
    let source = r#"
const mod = await import("./actions");
expect(mod).not.toHaveProperty("removedAction");
expect(mod).not.toHaveProperty("otherAction");

const positive = await import("./positive");
expect(positive).toHaveProperty("kept");
positive.extra;

const opaque = await import("./opaque");
expect(opaque).toBeDefined();

const read = await import("./read");
expect(read.used).toBe(true);
"#;
    let parsed = summary(source, "src/assertions.ts");
    let imported = |specifier: &str| {
        parsed
            .dynamic
            .iter()
            .find(|record| {
                matches!(&record.specifier, DynamicSpecifier::Literal(value) if value == specifier)
            })
            .and_then(|record| record.imported.clone())
    };
    assert_eq!(imported("./actions"), Some(Vec::new()));
    assert_eq!(
        imported("./positive"),
        Some(vec!["extra".to_string(), "kept".to_string()])
    );
    assert_eq!(imported("./opaque"), None);
    assert_eq!(imported("./read"), Some(vec!["used".to_string()]));
}

#[test]
fn narrows_side_effect_only_dynamic_imports() {
    let source = r#"
await import("./side-effect");
import("./promise");
void import("./void");
const used = await import("./used");
used.alpha;
"#;
    let parsed = summary(source, "src/side-effects.ts");
    let imported = |specifier: &str| {
        parsed
            .dynamic
            .iter()
            .find(|record| {
                matches!(&record.specifier, DynamicSpecifier::Literal(value) if value == specifier)
            })
            .and_then(|record| record.imported.clone())
    };
    assert_eq!(imported("./side-effect"), Some(Vec::new()));
    assert_eq!(imported("./promise"), Some(Vec::new()));
    assert_eq!(imported("./void"), None);
    assert_eq!(imported("./used"), Some(vec!["alpha".to_string()]));
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
    assert_eq!(imported("./namespace"), Some(Vec::new()));
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
fn narrows_react_lazy_and_next_dynamic_loaders_to_default() {
    let source = r#"
import React from "react";
import * as ReactNamespace from "react";
import { lazy as component } from "react";
import dynamic from "next/dynamic";
import { default as nextComponent } from "next/dynamic";
const A = React.lazy(() => import("./a"));
const B = ReactNamespace.lazy(() => { return import("./b"); });
const C = component(async () => await import("./c"));
const D = dynamic(() => import("./d"), { ssr: false });
const E = nextComponent(() => {
  if (flag) { return import("./e1"); }
  return import("./e2");
});
const F = component(() => flag ? import("./f1") : import("./f2"));
const G = component(() => import("./g").then((module) => module.Named));
"#;
    let parsed = summary(source, "src/components.tsx");
    assert_eq!(parsed.parse_status, ParseStatus::Ok);
    let imported = |specifier: &str| {
        parsed
            .dynamic
            .iter()
            .find(|record| {
                matches!(&record.specifier, DynamicSpecifier::Literal(value) if value == specifier)
            })
            .and_then(|record| record.imported.clone())
    };
    for specifier in ["./a", "./b", "./c", "./d", "./e1", "./e2", "./f1", "./f2"] {
        assert_eq!(
            imported(specifier),
            Some(vec!["default".to_string()]),
            "{specifier}"
        );
    }
    assert_eq!(imported("./g"), Some(vec!["Named".to_string()]));
}

#[test]
fn component_loader_narrowing_requires_unshadowed_import_provenance() {
    for source in [
        "const lazy = (load) => load(); lazy(() => import('./target'));",
        "import { lazy } from './local-wrapper'; lazy(() => import('./target'));",
        "import type { lazy } from 'react'; lazy(() => import('./target'));",
        "import { type lazy } from 'react'; lazy(() => import('./target'));",
        "import type * as React from 'react'; React.lazy(() => import('./target'));",
        "import { lazy } from 'react'; function run(lazy) { return lazy(() => import('./target')); }",
        "import * as React from 'react'; function run(React) { return React.lazy(() => import('./target')); }",
        "import { lazy } from 'react'; lazy(null, () => import('./target'));",
        "import { lazy } from 'react'; { const lazy = (load) => load(); lazy(() => import('./target')); }",
        "import * as React from 'react'; try {} catch (React) { React.lazy(() => import('./target')); }",
        "import dynamic from 'next/dynamic'; { const dynamic = (load) => load(); dynamic(() => import('./target')); }",
        "import { lazy } from 'react'; namespace N { const lazy = (load) => load(); export const result = lazy(() => import('./target')); }",
        "import { lazy } from 'react'; const wrapper = { lazy: (load) => load() }; wrapper.lazy(() => import('./target'));",
        "import { dynamic } from 'next/dynamic'; dynamic(() => import('./target'));",
        "import * as dynamic from 'next/dynamic'; dynamic(() => import('./target'));",
    ] {
        let parsed = summary(source, "src/components.tsx");
        let record = parsed.dynamic.iter().find(|record| {
            matches!(&record.specifier, DynamicSpecifier::Literal(value) if value == "./target")
        });
        assert_eq!(
            record.and_then(|record| record.imported.clone()),
            None,
            "{source}"
        );
    }
}

#[test]
fn component_loader_narrowing_ignores_non_direct_namespace_returns() {
    for source in [
        "import { lazy } from 'react'; lazy(() => use(import('./target')));",
        "import { lazy } from 'react'; lazy(() => import('./target').then((module) => module.Named));",
        "import { lazy } from 'react'; lazy(() => (await import('./target')).Named);",
        "import { lazy } from 'react'; lazy(() => { const module = import('./target'); return module; });",
    ] {
        let parsed = summary(source, "src/components.tsx");
        let record = parsed.dynamic.iter().find(|record| {
            matches!(&record.specifier, DynamicSpecifier::Literal(value) if value == "./target")
        });
        let imported = record.and_then(|record| record.imported.clone());
        if source.contains("import('./target').then(") {
            assert_eq!(imported, Some(vec!["Named".to_string()]), "{source}");
        } else {
            assert_eq!(imported, None, "{source}");
        }
    }
}

#[test]
fn narrows_local_component_loaders_to_default() {
    let source = r#"
import { lazy } from "react";
import dynamic from "next/dynamic";
const loadA = () => import("./a");
const A = lazy(loadA);
function loadB() { return import("./b"); }
const B = dynamic(loadB);
const loadC = async () => { return await import("./c"); };
lazy(loadC);
const loadD = () => flag ? import("./d1") : import("./d2");
lazy(loadD);
const loadE = function () { return import("./e"); };
lazy(loadE);
const loadF = () => import("./f");
function shadow(loadF) { return loadF(); }
lazy(loadF);
"#;
    let parsed = summary(source, "src/components.tsx");
    assert_eq!(parsed.parse_status, ParseStatus::Ok);
    let imported = |specifier: &str| {
        parsed
            .dynamic
            .iter()
            .find(|record| {
                matches!(&record.specifier, DynamicSpecifier::Literal(value) if value == specifier)
            })
            .and_then(|record| record.imported.clone())
    };
    for specifier in ["./a", "./b", "./c", "./d1", "./d2", "./e", "./f"] {
        assert_eq!(
            imported(specifier),
            Some(vec!["default".to_string()]),
            "{specifier}"
        );
    }
}

#[test]
fn local_component_loader_narrowing_requires_exclusive_loader_references() {
    for source in [
        "import { lazy } from 'react'; const load = () => import('./target'); lazy(load); load().then((m) => m.Named);",
        "import { lazy } from 'react'; const load = () => import('./target'); export { load }; lazy(load);",
        "import { lazy } from 'react'; export const load = () => import('./target'); lazy(load);",
        "import { lazy } from 'react'; const load = () => import('./target'); export default load; lazy(load);",
        "import { lazy } from 'react'; let load = () => import('./target'); lazy(load);",
        "import { lazy } from 'react'; var load = () => import('./target'); lazy(load);",
        "import { lazy } from 'react'; const load = () => import(`./${name}`); lazy(load);",
        "import { lazy } from 'react'; const load = () => import('./target'); function run(load) { return lazy(load); }",
        "import { lazy } from 'react'; const load = () => import('./target'); function run() { return load(); } lazy(load);",
        "import { lazy } from 'react'; const load = () => import('./target'); wrap(load);",
        "import { lazy } from 'react'; const load = () => import('./target'); lazy(null, load);",
        "import { lazy } from 'react'; const load = () => { const module = import('./target'); return module; }; lazy(load);",
        "import { lazy } from 'react'; const load = () => import('./target').then((m) => ({ default: m.Named })); lazy(load);",
    ] {
        let parsed = summary(source, "src/components.tsx");
        let record = parsed.dynamic.iter().find(|record| {
            matches!(&record.specifier, DynamicSpecifier::Literal(value) if value == "./target")
        });
        let imported = record.and_then(|record| record.imported.clone());
        if source.contains("import('./target').then(") {
            assert_eq!(imported, Some(vec!["Named".to_string()]), "{source}");
        } else {
            assert_eq!(imported, None, "{source}");
        }
    }
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

#[test]
fn harvests_enum_members_and_initializer_reads() {
    let parsed = summary(
        r#"
export enum Status { Active = "active", Inactive = "inactive" }
export enum Initializer { A = 1, B = A, C = A | 4 }
declare enum Ambient { X, Y }
enum Opaque { [`dynamic`] = 1, Other = 2 }
"#,
        "src/status.ts",
    );
    assert_eq!(parsed.parse_status, ParseStatus::Ok);
    let status = parsed
        .enums
        .iter()
        .find(|enumeration| enumeration.name == "Status")
        .expect("Status");
    assert_eq!(
        status
            .members
            .iter()
            .map(|member| member.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Active", "Inactive"]
    );
    let initializer = parsed
        .enums
        .iter()
        .find(|enumeration| enumeration.name == "Initializer")
        .expect("Initializer");
    let b = initializer
        .members
        .iter()
        .find(|member| member.name == "B")
        .expect("B");
    assert_eq!(b.initializer_members, vec!["A"]);
    let c = initializer
        .members
        .iter()
        .find(|member| member.name == "C")
        .expect("C");
    assert_eq!(c.initializer_members, vec!["A"]);

    let ambient = parsed
        .enums
        .iter()
        .find(|enumeration| enumeration.name == "Ambient")
        .expect("Ambient");
    assert!(ambient.ambient);
    let opaque = parsed
        .enums
        .iter()
        .find(|enumeration| enumeration.name == "Opaque")
        .expect("Opaque");
    assert!(opaque.opaque);
}

#[test]
fn records_qualified_type_names_as_member_accesses() {
    let parsed = summary(
        "import { Status } from \"./status\";\ntype Active = Status.Active;\nconst value = Status.Inactive;\n",
        "src/use.ts",
    );
    let declaration = parsed
        .declarations
        .iter()
        .find(|declaration| declaration.name == "Active")
        .expect("type alias");
    assert!(
        declaration
            .references
            .members
            .iter()
            .any(|member| member.object == "Status" && member.property.as_deref() == Some("Active")),
        "{:?}",
        declaration.references
    );
    assert!(
        !declaration.references.free.contains(&"Status".to_string()),
        "the qualified root is a member access, not a whole-object use: {:?}",
        declaration.references
    );
}

#[test]
fn harvests_unused_react_component_props() {
    let parsed = summary(
        r#"
function Card({ tone, title, children }) {
  return <div className={title}>{children}</div>;
}
const Badge = ({ count }) => <span>{count}</span>;
"#,
        "src/components/card.tsx",
    );
    assert_eq!(parsed.parse_status, ParseStatus::Ok);
    let prop = |component: &str, name: &str| {
        parsed
            .component_props
            .iter()
            .find(|record| record.component == component && record.prop == name)
            .unwrap_or_else(|| panic!("missing {component}.{name}: {:?}", parsed.component_props))
    };
    assert!(!prop("Card", "tone").used);
    assert!(prop("Card", "title").used);
    assert!(prop("Card", "children").used);
    assert!(prop("Badge", "count").used);
    assert_eq!(prop("Card", "tone").local, "tone");
    assert_eq!(prop("Card", "tone").span.line, 2);
}

#[test]
fn tracks_aliases_and_defaults_in_component_props() {
    let parsed = summary(
        r#"
const Card = ({ tone: shade = "red", title }) => (
  <div className={shade}>{title}</div>
);
"#,
        "src/components/card.tsx",
    );
    let tone = parsed
        .component_props
        .iter()
        .find(|record| record.prop == "tone")
        .expect("tone");
    assert_eq!(tone.local, "shade");
    assert!(tone.used);
    let title = parsed
        .component_props
        .iter()
        .find(|record| record.prop == "title")
        .expect("title");
    assert!(title.used);
}

#[test]
fn abstains_from_unharvestable_or_escaped_react_components() {
    for source in [
        // Exported declaration.
        "export function Card({ tone }) { return <div />; }\n",
        // Local export alias.
        "function Card({ tone }) { return <div />; }\nexport { Card };\n",
        // Default-export wrapper.
        "function Card({ tone }) { return <div />; }\nexport default memo(Card);\n",
        // Module registry.
        "function Card({ tone }) { return <div />; }\nconst registry = { Card };\nexport { registry };\n",
        // Rest parameter.
        "function Card({ tone, ...rest }) { return <div />; }\n",
        // Bare props parameter.
        "function Card(props) { return <div>{props.tone}</div>; }\n",
        // Nested destructure.
        "function Card({ user: { name } }) { return <div>{name}</div>; }\n",
        // Computed key.
        "function Card({ [key]: value }) { return <div>{value}</div>; }\n",
        // Not a renderer.
        "function Card({ tone }) { return 1; }\n",
        // Not a component name.
        "function helper({ tone }) { return <div>{tone}</div>; }\n",
    ] {
        let parsed = summary(source, "src/components/card.tsx");
        assert!(
            parsed.component_props.is_empty(),
            "unexpected props for {source}: {:?}",
            parsed.component_props
        );
    }
}

#[test]
fn reports_local_components_rendered_by_exported_components() {
    let source = r#"
type BadgeProps = {
  label: string;
  tone: string;
  size?: "sm" | "md";
};

function Badge({ label, tone, size = "md" }: BadgeProps) {
  return (
    <span className={size}>
      {label}
    </span>
  );
}

export function Widget() {
  return <Badge label="hello" tone="danger" size="sm" />;
}
"#;
    let parsed = summary(source, "components/Internal.tsx");
    let prop = |name: &str| {
        parsed
            .component_props
            .iter()
            .find(|record| record.component == "Badge" && record.prop == name)
            .unwrap_or_else(|| panic!("missing Badge.{name}: {:?}", parsed.component_props))
    };
    assert!(!prop("tone").used);
    assert!(prop("label").used);
    assert!(prop("size").used);
}

#[test]
fn harvests_same_file_typed_react_props() {
    let parsed = summary(
        r#"
function Card(props: CardProps) {
  return <div>{props.title}</div>;
}
interface CardProps {
  title: string;
  tone: string;
}
"#,
        "src/components/card.tsx",
    );
    let prop = |name: &str| {
        parsed
            .component_props
            .iter()
            .find(|record| record.component == "Card" && record.prop == name)
            .unwrap_or_else(|| panic!("missing Card.{name}: {:?}", parsed.component_props))
    };
    assert!(prop("title").used);
    assert!(!prop("tone").used);
    assert!(!prop("tone").destructured);
    assert_eq!(prop("tone").local, "props");
    assert_eq!(prop("tone").span.line, 7);
}

#[test]
fn harvests_typed_react_props_from_type_alias() {
    let parsed = summary(
        r#"
type BadgeProps = {
  label: string;
  count?: number;
};

const Badge = (props: BadgeProps) => <span>{props.label}</span>;
"#,
        "src/components/badge.tsx",
    );
    let label = parsed
        .component_props
        .iter()
        .find(|record| record.prop == "label")
        .expect("label");
    assert!(label.used);
    let count = parsed
        .component_props
        .iter()
        .find(|record| record.prop == "count")
        .expect("count");
    assert!(!count.used);
}

#[test]
fn credits_typed_props_destructured_from_the_binding() {
    let parsed = summary(
        r#"
interface Props {
  title: string;
  tone: string;
}

function Card(props: Props) {
  const { title } = props;
  return <div>{title}</div>;
}
"#,
        "src/components/card.tsx",
    );
    let prop = |name: &str| {
        parsed
            .component_props
            .iter()
            .find(|record| record.component == "Card" && record.prop == name)
            .unwrap_or_else(|| panic!("missing Card.{name}: {:?}", parsed.component_props))
    };
    assert!(prop("title").used);
    assert!(!prop("tone").used);
}

#[test]
fn abstains_on_whole_object_typed_props_use() {
    for source in [
        // Spread into a child.
        "interface Props { title: string; tone: string }\nfunction Card(props: Props) { return <Widget {...props} />; }\n",
        // Passed to a helper.
        "interface Props { title: string; tone: string }\nfunction Card(props: Props) { log(props); return <div />; }\n",
        // Returned.
        "interface Props { title: string; tone: string }\nfunction Card(props: Props) { use(props); return <div />; }\n",
        // Computed access.
        "interface Props { title: string; tone: string }\nfunction Card(props: Props) { return <div>{props[key]}</div>; }\n",
        // Rest destructure leaves the set opaque.
        "interface Props { title: string; tone: string }\nfunction Card(props: Props) { const { title, ...rest } = props; return <div>{title}{rest.tone}</div>; }\n",
    ] {
        let parsed = summary(source, "src/components/card.tsx");
        assert!(
            parsed.component_props.is_empty(),
            "unexpected props for {source}: {:?}",
            parsed.component_props
        );
    }
}

#[test]
fn abstains_on_unresolvable_typed_props() {
    for source in [
        // Imported type.
        "import type { Props } from \"./types\";\nfunction Card(props: Props) { return <div>{props.title}</div>; }\n",
        // Generic type reference.
        "type Props<T> = { value: T };\nfunction Card(props: Props<string>) { return <div>{props.value}</div>; }\n",
        // Inherited interface.
        "interface Base { tone: string }\ninterface Props extends Base { title: string }\nfunction Card(props: Props) { return <div>{props.title}</div>; }\n",
        // Intersection alias.
        "type Base = { tone: string };\ntype Props = Base & { title: string };\nfunction Card(props: Props) { return <div>{props.title}</div>; }\n",
        // Alias to another name.
        "interface Base { title: string }\ntype Props = Base;\nfunction Card(props: Props) { return <div>{props.title}</div>; }\n",
        // Index-only interface.
        "interface Props { [key: string]: string }\nfunction Card(props: Props) { return <div>{props.title}</div>; }\n",
        // Merged interface declarations.
        "interface Props { title: string }\ninterface Props { tone: string }\nfunction Card(props: Props) { return <div>{props.title}</div>; }\n",
        // Qualified annotation.
        "namespace Types { export interface Props { title: string } }\nfunction Card(props: Types.Props) { return <div>{props.title}</div>; }\n",
        // Inline type literal.
        "function Card(props: { title: string }) { return <div>{props.title}</div>; }\n",
        // Union with undefined.
        "interface Props { title: string }\nfunction Card(props: Props | undefined) { return <div>{props?.title}</div>; }\n",
    ] {
        let parsed = summary(source, "src/components/card.tsx");
        assert!(
            parsed.component_props.is_empty(),
            "unexpected props for {source}: {:?}",
            parsed.component_props
        );
    }
}

#[test]
fn harvests_local_memo_and_forward_ref_typed_props() {
    let parsed = summary(
        r#"
import { forwardRef, memo } from "react";

interface Props {
  title: string;
  tone: string;
}

const MemoCard = memo((props: Props) => <div>{props.title}</div>);
const ForwardCard = forwardRef<HTMLDivElement, Props>((props, ref) => (
  <div ref={ref}>{props.title}</div>
));
"#,
        "src/components/card.tsx",
    );
    let prop = |component: &str, name: &str| {
        parsed
            .component_props
            .iter()
            .find(|record| record.component == component && record.prop == name)
            .unwrap_or_else(|| panic!("missing {component}.{name}: {:?}", parsed.component_props))
    };
    assert!(prop("MemoCard", "title").used);
    assert!(!prop("MemoCard", "tone").used);
    assert!(prop("ForwardCard", "title").used);
    assert!(!prop("ForwardCard", "tone").used);
}

#[test]
fn forward_ref_inner_annotation_wins_over_wrapper_type() {
    let parsed = summary(
        r#"
import { forwardRef } from "react";

interface WrapperProps { wrapperOnly: string }
interface Props { title: string; tone: string }

const Card = forwardRef<HTMLDivElement, WrapperProps>((props: Props, ref) => (
  <div ref={ref}>{props.title}</div>
));
"#,
        "src/components/card.tsx",
    );
    assert!(
        parsed
            .component_props
            .iter()
            .any(|record| record.prop == "tone"),
        "{:?}",
        parsed.component_props
    );
    assert!(
        parsed
            .component_props
            .iter()
            .all(|record| record.prop != "wrapperOnly"),
        "{:?}",
        parsed.component_props
    );
}

#[test]
fn abstains_on_exported_or_comparator_wrapped_components() {
    for source in [
        // Exported wrapper binding.
        "import { memo } from \"react\";\ninterface Props { title: string; tone: string }\nexport const Card = memo((props: Props) => <div>{props.title}</div>);\n",
        // Memo comparator can read a prop the body does not.
        "import { memo } from \"react\";\ninterface Props { title: string; tone: string }\nconst Card = memo((props: Props) => <div>{props.title}</div>, (a, b) => a.tone === b.tone);\n",
        // forwardRef with no props type.
        "import { forwardRef } from \"react\";\nconst Card = forwardRef<HTMLDivElement>((props, ref) => <div ref={ref} />);\n",
    ] {
        let parsed = summary(source, "src/components/card.tsx");
        assert!(
            parsed.component_props.is_empty(),
            "unexpected props for {source}: {:?}",
            parsed.component_props
        );
    }
}

#[test]
fn harvests_component_contracts_with_optionality_and_defaults() {
    let parsed = summary(
        r#"
export function Card({ title, flag = false, tone }: { title: string; flag?: boolean; tone?: string }) {
  return <p>{title}{flag}{tone}</p>;
}

interface Props {
  title: string;
  size?: number;
  active?: boolean;
}

function Typed(props: Props) {
  return <p>{props.title}{props.size}{props.active}</p>;
}
export { Typed };
"#,
        "src/components/card.tsx",
    );
    assert_eq!(parsed.parse_status, ParseStatus::Ok);
    let contract = |component: &str| {
        parsed
            .component_contracts
            .iter()
            .find(|record| record.component == component)
            .unwrap_or_else(|| panic!("missing {component}: {:?}", parsed.component_contracts))
    };
    let prop = |component: &str, name: &str| {
        contract(component)
            .props
            .iter()
            .find(|prop| prop.name == name)
            .unwrap_or_else(|| panic!("missing {component}.{name}"))
    };
    assert!(!contract("Card").escaped);
    assert!(!contract("Typed").escaped);
    assert!(!prop("Card", "title").optional);
    assert!(prop("Card", "flag").optional);
    assert!(prop("Card", "flag").has_default);
    assert!(prop("Card", "tone").optional);
    assert!(!prop("Card", "tone").has_default);
    assert!(prop("Card", "flag").used);
    assert!(!prop("Typed", "title").optional);
    assert!(prop("Typed", "size").optional);
    assert!(prop("Typed", "active").optional);
    assert!(prop("Typed", "active").used);
}

#[test]
fn marks_component_contracts_escaped_only_for_value_uses() {
    let direct = summary(
        "export function Card({ flag }: { flag?: boolean }) { return <p>{flag}</p>; }\n",
        "src/components/card.tsx",
    );
    assert!(!direct.component_contracts[0].escaped);

    let default_export = summary(
        "function Card({ flag }: { flag?: boolean }) { return <p>{flag}</p>; }\nexport default Card;\n",
        "src/components/card.tsx",
    );
    assert!(!default_export.component_contracts[0].escaped);

    let rendered = summary(
        "function Card({ flag }: { flag?: boolean }) { return <p>{flag}</p>; }\nexport function App() { return <Card />; }\n",
        "src/components/card.tsx",
    );
    assert!(!rendered.component_contracts[0].escaped);

    for source in [
        "function Card({ flag }: { flag?: boolean }) { return <p>{flag}</p>; }\nconst registry = { Card };\nexport { registry };\n",
        "import { memo } from \"react\";\nfunction Card({ flag }: { flag?: boolean }) { return <p>{flag}</p>; }\nexport default memo(Card);\n",
        "function Card({ flag }: { flag?: boolean }) { return <p>{flag}</p>; }\nconst Alias = Card;\nexport { Alias };\n",
    ] {
        let parsed = summary(source, "src/components/card.tsx");
        assert!(
            parsed
                .component_contracts
                .iter()
                .find(|record| record.component == "Card")
                .is_some_and(|record| record.escaped),
            "expected an escaped Card for {source}: {:?}",
            parsed.component_contracts
        );
    }
}

#[test]
fn collects_jsx_uses_with_supplied_and_opaque_attributes() {
    let parsed = summary(
        r#"
export function App({ rest }: { rest: object }) {
  return (
    <>
      <Card title="a" flag />
      <Card {...rest} />
      <UI.Card />
      <svg:path />
      <div />
    </>
  );
}
"#,
        "src/app.tsx",
    );
    let cards: Vec<_> = parsed
        .jsx_uses
        .iter()
        .filter(|use_| use_.component == "Card")
        .collect();
    assert_eq!(cards.len(), 2, "{:?}", parsed.jsx_uses);
    assert_eq!(
        cards[0].supplied,
        vec!["title".to_string(), "flag".to_string()]
    );
    assert!(!cards[0].opaque);
    assert!(cards[1].opaque);
    assert!(
        parsed
            .jsx_uses
            .iter()
            .any(|use_| use_.component == "div" && !use_.opaque),
        "{:?}",
        parsed.jsx_uses
    );
    assert_eq!(
        parsed.jsx_namespace_uses.len(),
        1,
        "{:?}",
        parsed.jsx_namespace_uses
    );
    assert_eq!(parsed.jsx_namespace_uses[0].root, "UI");
}

#[test]
fn collects_component_value_uses_but_not_jsx_tags_or_direct_exports() {
    let rendered = summary(
        "function Card({ flag }: { flag?: boolean }) { return <p>{flag}</p>; }\nexport function App() { return <Card />; }\n",
        "src/app.tsx",
    );
    assert!(
        !rendered
            .component_value_uses
            .iter()
            .any(|name| name == "Card"),
        "a JSX tag is a render site, not a value escape: {:?}",
        rendered.component_value_uses
    );

    let escaped = summary(
        r#"
import { memo } from "react";
function Card({ flag }: { flag?: boolean }) { return <p>{flag}</p>; }
const registry = { Card };
export default memo(Card);
export { Card as Public };
"#,
        "src/app.tsx",
    );
    assert!(
        escaped
            .component_value_uses
            .iter()
            .any(|name| name == "Card"),
        "{:?}",
        escaped.component_value_uses
    );
    assert!(
        escaped
            .component_value_uses
            .iter()
            .any(|name| name == "memo"),
        "wrapper arguments are value uses: {:?}",
        escaped.component_value_uses
    );
}

#[test]
fn shadowed_jsx_tags_and_value_uses_are_not_collected() {
    let parsed = summary(
        r#"
import { Card } from "./Card";

function Shadow(Card: () => unknown) {
  return <Card />;
}

function Values(Card: unknown) {
  return <div title={String(Card)} />;
}

function Local() {
  const Card = () => null;
  return <Card />;
}

export const app = <Shadow />;
"#,
        "src/app.tsx",
    );
    assert!(
        parsed.jsx_uses.iter().all(|use_| use_.component != "Card"),
        "a shadowed JSX tag is not a render site of the import: {:?}",
        parsed.jsx_uses
    );
    assert!(
        !parsed
            .component_value_uses
            .iter()
            .any(|name| name == "Card"),
        "a shadowed value reference is not a use of the import: {:?}",
        parsed.component_value_uses
    );
    assert!(
        parsed
            .jsx_uses
            .iter()
            .any(|use_| use_.component == "Shadow"),
        "the outer render site is still recorded: {:?}",
        parsed.jsx_uses
    );
}
