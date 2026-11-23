//! Tests for dynamic entry detection.

use super::*;

#[test]
fn unused_files_require_no_entry_and_no_incoming_import() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "import { live } from '../src/live';\nexport default function Page() { return live; }\n",
        ),
        ("src/live.ts", "export const live = 1;\n"),
        ("src/dead.ts", "export const dead = 1;\n"),
        (
            "src/dead-importer.ts",
            "import { dependency } from './dependency';\nexport const dead = dependency;\n",
        ),
        ("src/dependency.ts", "export const dependency = 1;\n"),
        ("src/effects.ts", "console.log('side effect');\n"),
    ]);
    assert_eq!(
        snapshot.unused_files,
        vec![
            "src/dead-importer.ts".to_string(),
            "src/dead.ts".to_string(),
            "src/effects.ts".to_string(),
        ],
        "the page is an entry; the dependency has a dead importer"
    );
    assert!(
        snapshot.is_candidate("src/dead.ts#dead"),
        "{:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate("src/dead-importer.ts#dead"),
        "{:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate("src/dependency.ts#dependency"),
        "the dependency's only importer is itself dead"
    );
    assert!(
        !snapshot
            .unused_files
            .contains(&"src/dependency.ts".to_string()),
        "a file is unused only when no module imports it"
    );
    assert!(
        !snapshot.unused_files.contains(&"src/live.ts".to_string()),
        "an imported file is not unused"
    );
    assert!(
        !snapshot.unused_files.contains(&"app/page.tsx".to_string()),
        "an entry is not unused"
    );
}

#[test]
fn unimported_server_actions_are_candidates_by_default() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "import { importedAction } from '../src/actions';\n\
                 export default function Page() { return importedAction(); }\n",
        ),
        (
            "src/actions.ts",
            "\"use server\";\n\
                 export async function importedAction() { return 1; }\n\
                 export async function unimportedAction() { return 2; }\n",
        ),
    ]);
    let imported = "src/actions.ts#importedAction";
    let unimported = "src/actions.ts#unimportedAction";
    assert!(snapshot.is_live(imported), "{:?}", snapshot.live);
    assert!(
        !snapshot.is_candidate(imported),
        "{:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate(unimported),
        "an unreferenced action is dead under the default policy: {:?}",
        snapshot.candidates
    );
}

#[test]
fn mixed_client_server_barrels_are_project_gated_warnings() {
    let next_snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "packages/plain/package.json",
            r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
        ),
        (
            "components/client.tsx",
            "\"use client\";\nexport function Button() { return null; }\n",
        ),
        (
            "lib/server.ts",
            "import \"server-only\";\nexport function readFile() { return null; }\n",
        ),
        (
            "lib/action.ts",
            "\"use server\";\nexport async function action() { return 1; }\n",
        ),
        (
            "lib/headers.ts",
            "import { cookies } from \"next/headers\";\nexport function getCookie() { return cookies(); }\n",
        ),
        (
            "lib/headers-namespace.ts",
            "import * as headers from \"next/headers\";\nexport function getHeader() { return headers.headers(); }\n",
        ),
        (
            "lib/headers-type-only.ts",
            "import type { cookies } from \"next/headers\";\nexport const util = 1;\n",
        ),
        (
            "lib/headers-side-effect.ts",
            "import \"next/headers\";\nexport const util = 1;\n",
        ),
        (
            "lib/format.ts",
            "export function format() { return \"\"; }\n",
        ),
        // Explicit re-exports, in both source orders.
        (
            "src/barrel.ts",
            "export { Button } from \"../components/client\";\nexport { readFile } from \"../lib/server\";\n",
        ),
        (
            "src/barrel-server-first.ts",
            "export { readFile } from \"../lib/server\";\nexport { Button } from \"../components/client\";\n",
        ),
        (
            "src/barrel-order-client.ts",
            "export const note = 1;\nexport { Button } from \"../components/client\";\nexport { readFile } from \"../lib/server\";\n",
        ),
        (
            "src/barrel-order-server.ts",
            "export const note = 1;\nexport const other = 2;\nexport { readFile } from \"../lib/server\";\nexport { Button } from \"../components/client\";\n",
        ),
        // Whole-module re-exports.
        (
            "src/barrel-star.ts",
            "export * from \"../components/client\";\nexport * from \"../lib/server\";\n",
        ),
        (
            "src/barrel-namespace.ts",
            "export * as client from \"../components/client\";\nexport * as server from \"../lib/server\";\n",
        ),
        // `"use server"` and a named `next/headers` API are server-only.
        (
            "src/barrel-action.ts",
            "export { Button } from \"../components/client\";\nexport { action } from \"../lib/action\";\n",
        ),
        (
            "src/barrel-headers.ts",
            "export { Button } from \"../components/client\";\nexport { getCookie } from \"../lib/headers\";\n",
        ),
        (
            "src/barrel-headers-namespace.ts",
            "export { Button } from \"../components/client\";\nexport { getHeader } from \"../lib/headers-namespace\";\n",
        ),
        // An ordinary utility, an external package, a type-only
        // re-export, and a type-only or side-effect server import are not
        // mixed boundaries.
        (
            "src/barrel-plain.ts",
            "export { Button } from \"../components/client\";\nexport { format } from \"../lib/format\";\n",
        ),
        (
            "src/barrel-external.ts",
            "export { Button } from \"../components/client\";\nexport { something } from \"some-package\";\n",
        ),
        (
            "src/barrel-type-only-reexport.ts",
            "export type { Button } from \"../components/client\";\nexport { readFile } from \"../lib/server\";\n",
        ),
        (
            "src/barrel-type-only-server.ts",
            "export { Button } from \"../components/client\";\nexport { util } from \"../lib/headers-type-only\";\n",
        ),
        (
            "src/barrel-side-effect-server.ts",
            "export { Button } from \"../components/client\";\nexport { util } from \"../lib/headers-side-effect\";\n",
        ),
        (
            "src/barrel-no-client.ts",
            "export { readFile } from \"../lib/server\";\nexport { format } from \"../lib/format\";\n",
        ),
        (
            "src/barrel-no-server.ts",
            "export { Button } from \"../components/client\";\n",
        ),
        // The gate is project-wide: a shared barrel in a React-only
        // workspace package is compiled by the sibling Next app.
        (
            "packages/plain/src/barrel.ts",
            "export { Button } from \"../../../components/client\";\nexport { readFile } from \"../../../lib/server\";\n",
        ),
    ]);
    let mut mixed: Vec<(String, u32)> = next_snapshot
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "mixed_client_server_barrel")
        .map(|diagnostic| {
            (
                diagnostic.file.clone().unwrap_or_default(),
                diagnostic.span.map_or(0, |span| span.line),
            )
        })
        .collect();
    mixed.sort();
    assert_eq!(
        mixed,
        vec![
            ("packages/plain/src/barrel.ts".to_string(), 1),
            ("src/barrel-action.ts".to_string(), 1),
            ("src/barrel-headers-namespace.ts".to_string(), 1),
            ("src/barrel-headers.ts".to_string(), 1),
            ("src/barrel-namespace.ts".to_string(), 1),
            ("src/barrel-order-client.ts".to_string(), 2),
            ("src/barrel-order-server.ts".to_string(), 3),
            ("src/barrel-server-first.ts".to_string(), 1),
            ("src/barrel-star.ts".to_string(), 1),
            ("src/barrel.ts".to_string(), 1),
        ],
        "{:?}",
        next_snapshot.diagnostics
    );
    let first = next_snapshot
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "mixed_client_server_barrel")
        .expect("a mixed barrel");
    assert_eq!(first.severity, shrike_core::Severity::Warning);
    assert!(
        first.message.contains("../components/client")
            && first.message.contains("../lib/server")
            && first.message.contains("\"use client\"")
            && first.message.contains("server-only"),
        "the warning names both origins: {}",
        first.message
    );

    // Without an RSC bundler the directives carry no special meaning.
    let react_only = snapshot(&[
        (
            "package.json",
            r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
        ),
        (
            "components/client.tsx",
            "\"use client\";\nexport function Button() { return null; }\n",
        ),
        (
            "lib/server.ts",
            "import \"server-only\";\nexport function readFile() { return null; }\n",
        ),
        (
            "src/barrel.ts",
            "export { Button } from \"../components/client\";\nexport { readFile } from \"../lib/server\";\n",
        ),
    ]);
    assert!(
        react_only
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "mixed_client_server_barrel"),
        "no RSC bundler evidence: {:?}",
        react_only.diagnostics
    );

    // Any framework-agnostic RSC bundler enables the rule.
    let waku = snapshot(&[
        (
            "package.json",
            r#"{"name":"app","dependencies":{"waku":"^0.21.0","react":"^19.0.0"}}"#,
        ),
        (
            "components/client.tsx",
            "\"use client\";\nexport function Button() { return null; }\n",
        ),
        (
            "lib/server.ts",
            "import \"server-only\";\nexport function readFile() { return null; }\n",
        ),
        (
            "src/barrel.ts",
            "export { Button } from \"../components/client\";\nexport { readFile } from \"../lib/server\";\n",
        ),
    ]);
    assert_eq!(
        waku.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "mixed_client_server_barrel")
            .count(),
        1,
        "{:?}",
        waku.diagnostics
    );
}

#[test]
fn react_dom_namespace_and_named_roots_are_recognized() {
    assert_legacy_react_app_is_live(&legacy_react_app(
        "import * as ReactDOM from \"react-dom\";\nimport App from \"./App\";\nimport { formatting } from \"./format\";\n\nReactDOM.hydrate(<App label={formatting()} />, document.getElementById(\"root\"));\n",
    ));
    assert_legacy_react_app_is_live(&legacy_react_app(
        "import { render } from \"react-dom\";\nimport App from \"./App\";\nimport { formatting } from \"./format\";\n\nrender(<App label={formatting()} />, document.getElementById(\"root\"));\n",
    ));
}

#[test]
fn namespace_values_widen_the_target_module() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "import * as UI from '../components/ui';\nconst all = { ...UI };\nexport default function Page() { return <div>{Object.keys(all).length}</div>; }\n",
        ),
        (
            "components/ui.tsx",
            "export function Button() { return null; }\nexport function DeadButton() { return null; }\n",
        ),
    ]);
    assert!(
        !snapshot
            .candidates
            .iter()
            .any(|key| key == "components/ui.tsx#DeadButton"),
        "a namespace value use must conservatively protect every export: {:?}",
        snapshot.candidates
    );
}

#[test]
fn type_position_imports_keep_the_named_type_live() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "import { JsDocValue } from '../src/jsdoc.js';\nexport const inline: import('../src/types').InlineType = { id: 'x' };\nexport default function Page() { return <div>{JsDocValue.name}</div>; }\n",
        ),
        (
            "src/jsdoc.js",
            "/** @type {import('./types').JSDocType} */\nexport const JsDocValue = { name: 'x' };\n",
        ),
        (
            "src/types.ts",
            "export interface JSDocType { name: string }\nexport interface InlineType { id: string }\nexport interface DeadType { dead: true }\n",
        ),
    ]);
    for live in ["src/types.ts#JSDocType", "src/types.ts#InlineType"] {
        assert!(
            !snapshot.candidates.iter().any(|key| key == live),
            "{live} is used from a type position: {:?}",
            snapshot.candidates
        );
    }
    assert!(
        snapshot
            .candidates
            .iter()
            .any(|key| key == "src/types.ts#DeadType"),
        "an unused type must remain a candidate: {:?}",
        snapshot.candidates
    );
}

#[test]
fn mdx_imports_and_component_map_are_reachable() {
    let snapshot = snapshot(&[
        (
            "app/layout.tsx",
            "export default function Layout() { return null; }\n",
        ),
        (
            "app/page.mdx",
            "import { Widget } from '../components/widget';\n\n# Hello\n\n<Widget />\n",
        ),
        (
            "components/widget.tsx",
            "export function Widget() { return null; }\n",
        ),
        (
            "mdx-components.tsx",
            "export function useMDXComponents(components: Record<string, unknown>) { return components; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(
        entries.contains(&"mdx-components.tsx#useMDXComponents"),
        "{entries:?}"
    );
    assert!(
        !snapshot
            .candidates
            .iter()
            .any(|key| key == "components/widget.tsx#Widget"),
        "MDX imports must keep the component live: {:?}",
        snapshot.candidates
    );
    assert!(
        !snapshot
            .candidates
            .iter()
            .any(|key| key == "mdx-components.tsx#useMDXComponents"),
        "{:?}",
        snapshot.candidates
    );
}

#[test]
fn unused_barrel_aliases_are_candidates_even_when_their_target_is_dead() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"barrel-app","private":true,"dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "app/page.tsx",
            "import { used } from \"../src/impl\";\n\nexport default function Page() {\n  return used;\n}\n",
        ),
        ("src/barrel.ts", "export { used, dead } from \"./impl\";\n"),
        (
            "src/impl.ts",
            "export const used = 1;\nexport const dead = 2;\n",
        ),
    ]);
    assert!(
        snapshot.is_live("src/impl.ts#used"),
        "directly imported declaration must be live: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate("src/barrel.ts#used"),
        "unused barrel alias with a live target is actionable: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate("src/impl.ts#dead"),
        "dead declaration must be reported: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate("src/barrel.ts#dead"),
        "a dead alias must be reported so its barrel line is removed: {:?}",
        snapshot.candidates
    );
}

#[test]
fn unused_barrel_alias_chains_report_every_link_to_a_dead_target() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"barrel-chain","private":true,"dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "app/page.tsx",
            "import { used } from \"../src/impl\";\n\nexport default function Page() {\n  return used;\n}\n",
        ),
        ("src/barrel.ts", "export { used, dead } from \"./impl\";\n"),
        ("src/outer.ts", "export { used, dead } from \"./barrel\";\n"),
        (
            "src/impl.ts",
            "export const used = 1;\nexport const dead = 2;\n",
        ),
    ]);
    assert!(
        snapshot.is_candidate("src/barrel.ts#used"),
        "inner alias with a live target is actionable: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate("src/outer.ts#used"),
        "outer alias whose chain reaches a live target is actionable: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate("src/barrel.ts#dead"),
        "dead inner alias must be reported so the barrel line is removable: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate("src/outer.ts#dead"),
        "dead outer alias must be reported so the barrel line is removable: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate("src/impl.ts#dead"),
        "dead declaration must be reported: {:?}",
        snapshot.candidates
    );
}

#[test]
fn framework_component_imports_do_not_create_dangling_edges() {
    let snapshot = snapshot(&[
        ("src/main.ts", "import App from './App.vue';\nApp();\n"),
        ("src/App.vue", "<template><div /></template>\n"),
    ]);
    for edge in &snapshot.edges {
        assert!(
            snapshot
                .symbols
                .iter()
                .any(|symbol| symbol.key == edge.source),
            "missing source symbol for edge: {edge:?}"
        );
        assert!(
            snapshot
                .symbols
                .iter()
                .any(|symbol| symbol.key == edge.target),
            "missing target symbol for edge: {edge:?}"
        );
    }
}

#[test]
fn internal_only_aliases_follow_external_use() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "import { publicName } from '../src/registry';\nexport default function Page() { publicName(); return null; }\n",
        ),
        (
            "src/registry.ts",
            "function helper() { return 1; }\nexport { helper as publicName };\nexport function unused() { return 2; }\n",
        ),
    ]);
    assert!(
        !snapshot
            .internal_only
            .iter()
            .any(|key| key == "src/registry.ts#helper"),
        "an exported alias makes the local symbol externally visible: {:?}",
        snapshot.internal_only
    );
    assert!(
        snapshot
            .candidates
            .iter()
            .any(|key| key == "src/registry.ts#unused"),
        "{:?}",
        snapshot.candidates
    );
}

#[test]
fn types_reachable_through_an_externally_used_type_stay_live() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"type-graph","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "app/page.tsx",
            "import type { Host } from '../src/host';\n\nexport default function Page(props: { host: Host }) {\n  return <div>{props.host.label}</div>;\n}\n",
        ),
        (
            "src/host.ts",
            "export interface Host {\n  label: string;\n  image: Image | null;\n}\n\nexport interface Image {\n  width: number;\n  height: number;\n}\n\nexport function unusedHostHelper() {\n  return 1;\n}\n",
        ),
    ]);
    for key in ["src/host.ts#Host", "src/host.ts#Image"] {
        assert!(snapshot.is_live(key), "{key}: {:?}", snapshot.candidates);
        assert!(
            !snapshot
                .internal_only
                .iter()
                .any(|candidate| candidate == key),
            "{key} is used through an externally imported type: {:?}",
            snapshot.internal_only
        );
    }
    assert!(
        snapshot.is_candidate("src/host.ts#unusedHostHelper"),
        "{:?}",
        snapshot.candidates
    );
}
