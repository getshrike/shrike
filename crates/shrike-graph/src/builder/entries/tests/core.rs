//! Tests for core entry detection.

use super::*;

#[test]
fn documented_tool_scripts_are_not_entries_without_a_reference() {
    let snapshot = snapshot(&[
        (
            "scripts/generate-report.ts",
            "/**\n * Report generator.\n *\n * Usage: npx tsx scripts/generate-report.ts\n */\nfunction main() {\n  console.log('report');\n}\nmain();\n",
        ),
        (
            "scripts/tool.mjs",
            "#!/usr/bin/env node\nconsole.log('tool');\n",
        ),
        (
            "src/library.ts",
            "/** Usage: this is documentation, not an entry. */\nexport const value = 1;\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(
        !entries.contains(&"scripts/generate-report.ts#__module__"),
        "a documented header is not a reference: {entries:?}"
    );
    assert!(
        !entries.contains(&"scripts/tool.mjs#__module__"),
        "a shebang is not a reference: {entries:?}"
    );
    assert!(
        snapshot
            .unused_files
            .contains(&"scripts/generate-report.ts".to_string()),
        "an unreferenced tool is an unused file: {:?}",
        snapshot.unused_files
    );
    assert!(
        snapshot
            .unused_files
            .contains(&"scripts/tool.mjs".to_string()),
        "an unreferenced shebang tool is an unused file: {:?}",
        snapshot.unused_files
    );
    assert!(
        !entries.contains(&"src/library.ts#__module__"),
        "{entries:?}"
    );
}

#[test]
fn client_server_leaks_follow_the_use_client_cone() {
    let next_snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        // Direct server-only import in a client module.
        (
            "app/direct/page.tsx",
            "\"use client\";\n\nimport { headers } from \"next/headers\";\nexport default function Direct() { return null; }\n",
        ),
        // Transitive static import and a re-export chain.
        (
            "lib/transitive.ts",
            "import fs from \"node:fs\";\nexport function read() { return fs.readFileSync(\"x\", \"utf8\"); }\n",
        ),
        (
            "app/transitive/page.tsx",
            "\"use client\";\n\nimport { read } from \"../../lib/transitive\";\nexport default function Page() { return <main>{read()}</main>; }\n",
        ),
        ("lib/chain.ts", "export { read } from \"./transitive\";\n"),
        (
            "app/reexport/page.tsx",
            "\"use client\";\n\nimport { read } from \"../../lib/chain\";\nexport default function Page() { return <main>{read()}</main>; }\n",
        ),
        // A client module that re-exports server-only code directly.
        (
            "app/star/page.tsx",
            "\"use client\";\n\nexport { read } from \"../../lib/transitive\";\n",
        ),
        // An all-action Server Action module is the sanctioned boundary.
        (
            "lib/action.ts",
            "\"use server\";\nimport fs from \"node:fs\";\nexport async function action() { return fs.readFileSync(\"x\", \"utf8\"); }\n",
        ),
        (
            "app/action/page.tsx",
            "\"use client\";\n\nimport { action } from \"../../lib/action\";\nexport default function Page() { return <button onClick={() => action()}>run</button>; }\n",
        ),
        // A non-action export keeps the server module in the client cone.
        (
            "lib/action-mixed.ts",
            "\"use server\";\nimport fs from \"node:fs\";\nexport async function action() { return fs.readFileSync(\"x\", \"utf8\"); }\nexport const value = 1;\n",
        ),
        (
            "app/action-mixed/page.tsx",
            "\"use client\";\n\nimport { action, value } from \"../../lib/action-mixed\";\nexport default function Page() { return <button onClick={() => action()}>{value}</button>; }\n",
        ),
        // An async arrow export is action-shaped too.
        (
            "lib/action-arrow.ts",
            "\"use server\";\nimport fs from \"node:fs\";\nexport const action = async () => fs.readFileSync(\"x\", \"utf8\");\n",
        ),
        (
            "app/action-arrow/page.tsx",
            "\"use client\";\n\nimport { action } from \"../../lib/action-arrow\";\nexport default function Page() { return <button onClick={() => action()}>run</button>; }\n",
        ),
        // One non-action export keeps an otherwise all-arrow module in the
        // client cone.
        (
            "lib/action-arrow-mixed.ts",
            "\"use server\";\nimport fs from \"node:fs\";\nexport const action = async () => fs.readFileSync(\"x\", \"utf8\");\nexport const value = 1;\n",
        ),
        (
            "app/action-arrow-mixed/page.tsx",
            "\"use client\";\n\nimport { action, value } from \"../../lib/action-arrow-mixed\";\nexport default function Page() { return <button onClick={() => action()}>{value}</button>; }\n",
        ),
        // Type-only edges are erased and cannot leak.
        (
            "lib/type-server.ts",
            "import fs from \"node:fs\";\nexport type ServerType = string;\nexport function read() { return fs.readFileSync(\"x\", \"utf8\"); }\n",
        ),
        (
            "app/type-only/page.tsx",
            "\"use client\";\n\nimport type { ServerType } from \"../../lib/type-server\";\nexport default function Page() { const value: ServerType = \"ok\"; return <main>{value}</main>; }\n",
        ),
        (
            "app/type-reexport/page.tsx",
            "\"use client\";\n\nexport type { ServerType } from \"../../lib/type-server\";\n",
        ),
        // The sanctioned next/dynamic client-only escape hatch.
        (
            "app/dynamic-ssr/page.tsx",
            "\"use client\";\n\nimport dynamic from \"next/dynamic\";\nconst Chart = dynamic(() => import(\"../../lib/transitive\"), { ssr: false });\nexport default function Page() { return <Chart />; }\n",
        ),
        // Without ssr:false the dynamic import is an ordinary client edge.
        (
            "app/dynamic-no-ssr/page.tsx",
            "\"use client\";\n\nimport dynamic from \"next/dynamic\";\nconst Chart = dynamic(() => import(\"../../lib/transitive\"));\nexport default function Page() { return <Chart />; }\n",
        ),
        // A static edge of the same specifier is not an escape hatch.
        (
            "app/dynamic-static/page.tsx",
            "\"use client\";\n\nimport { read } from \"../../lib/transitive\";\nimport dynamic from \"next/dynamic\";\nconst Chart = dynamic(() => import(\"../../lib/transitive\"), { ssr: false });\nexport default function Page() { return <main>{read()}<Chart /></main>; }\n",
        ),
        // A server component is not a client root.
        (
            "app/server-component/page.tsx",
            "import { read } from \"../../lib/transitive\";\nexport default function Page() { return <main>{read()}</main>; }\n",
        ),
        // A namespace next/headers import is conservative; a side-effect
        // import and a default import are not server APIs.
        (
            "app/headers-namespace/page.tsx",
            "\"use client\";\n\nimport * as headers from \"next/headers\";\nexport default function Page() { return <main>{headers.headers().get(\"host\")}</main>; }\n",
        ),
        (
            "app/headers-side-effect/page.tsx",
            "\"use client\";\n\nimport \"next/headers\";\nexport default function Page() { return null; }\n",
        ),
        (
            "app/server-only/page.tsx",
            "\"use client\";\n\nimport \"server-only\";\nexport default function Page() { return null; }\n",
        ),
    ]);
    let mut leaks: Vec<(String, u32)> = next_snapshot
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "client_server_leak")
        .map(|diagnostic| {
            (
                diagnostic.file.clone().unwrap_or_default(),
                diagnostic.span.map_or(0, |span| span.line),
            )
        })
        .collect();
    leaks.sort();
    assert_eq!(
        leaks,
        vec![
            ("app/action-arrow-mixed/page.tsx".to_string(), 3),
            ("app/action-mixed/page.tsx".to_string(), 3),
            ("app/direct/page.tsx".to_string(), 3),
            ("app/dynamic-no-ssr/page.tsx".to_string(), 4),
            ("app/dynamic-static/page.tsx".to_string(), 3),
            ("app/headers-namespace/page.tsx".to_string(), 3),
            ("app/reexport/page.tsx".to_string(), 3),
            ("app/server-only/page.tsx".to_string(), 3),
            ("app/star/page.tsx".to_string(), 3),
            ("app/transitive/page.tsx".to_string(), 3),
        ],
        "{:?}",
        next_snapshot.diagnostics
    );
    let transitive = next_snapshot
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code == "client_server_leak"
                && diagnostic.file.as_deref() == Some("app/transitive/page.tsx")
        })
        .expect("transitive leak");
    assert_eq!(transitive.severity, shrike_core::Severity::Warning);
    assert!(
        !transitive.message.contains("lib/chain")
            && transitive.message.contains("lib/transitive.ts")
            && transitive.message.contains("node:fs"),
        "the warning names the sink and path: {}",
        transitive.message
    );
    let dynamic = next_snapshot
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code == "client_server_leak"
                && diagnostic.file.as_deref() == Some("app/dynamic-no-ssr/page.tsx")
        })
        .expect("dynamic leak");
    assert_eq!(dynamic.span.map(|span| span.line), Some(4));

    // Without an RSC bundler the directives carry no special meaning.
    let react_only = snapshot(&[
        (
            "package.json",
            r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
        ),
        (
            "app/client.tsx",
            "\"use client\";\nimport \"server-only\";\nexport function Widget() { return null; }\n",
        ),
    ]);
    assert!(
        react_only
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "client_server_leak"),
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
            "app/client.tsx",
            "\"use client\";\nimport \"server-only\";\nexport function Widget() { return null; }\n",
        ),
    ]);
    assert_eq!(
        waku.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "client_server_leak")
            .count(),
        1,
        "{:?}",
        waku.diagnostics
    );
}

#[test]
fn nested_root_only_app_stems_do_not_decorate_routes() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "app/shop/[id]/page.tsx",
            "export default function Page() { return null; }\n",
        ),
        // Next.js ignores `robots` under a dynamic segment, a route group,
        // or a parallel slot, so it must not introduce a competing
        // `[slug]` segment or a route participant.
        (
            "app/shop/[slug]/robots.ts",
            "export default function robots() { return {}; }\n",
        ),
        (
            "app/(group)/robots.ts",
            "export default function robots() { return {}; }\n",
        ),
        (
            "app/@slot/robots.ts",
            "export default function robots() { return {}; }\n",
        ),
    ]);
    assert!(
        snapshot
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "dynamic_segment_name_conflict"),
        "{:?}",
        snapshot.diagnostics
    );
}

#[test]
fn package_script_modules_are_entries() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"app","scripts":{"seed":"TZ=UTC npx tsx db/seeds/seed.ts"}}"#,
        ),
        (
            "db/seeds/seed.ts",
            "function main() {\n  console.log('seed');\n}\nmain();\n",
        ),
        ("src/index.ts", "export const live = 1;\n"),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(
        entries.contains(&"db/seeds/seed.ts#__module__"),
        "{entries:?}"
    );
    assert!(
        snapshot
            .live
            .iter()
            .any(|key| key == "db/seeds/seed.ts#main"),
        "the script body must stay live: {:?}",
        snapshot.live
    );
}

#[test]
fn ambient_module_augmentations_and_declaration_files_are_entries() {
    let snapshot = snapshot(&[
        (
            "src/vanilla.ts",
            "export interface StoreMutators<S, A> {}\nexport type StateCreator<T> = T;\n",
        ),
        (
            "src/immer.ts",
            "import type { StateCreator } from './vanilla.ts';\n\
                 type Write<T, U> = Omit<T, keyof U> & U;\n\
                 declare module './vanilla.ts' {\n\
                   interface StoreMutators<S, A> { immer: Write<S, A> }\n\
                 }\n\
                 export const immer = 1;\n",
        ),
        (
            "src/types.d.ts",
            "declare interface ImportMeta { env?: { MODE: string } }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(
        entries.contains(&"src/immer.ts#./vanilla.ts"),
        "{entries:?}"
    );
    assert!(
        entries.contains(&"src/types.d.ts#ImportMeta"),
        "{entries:?}"
    );
    assert!(
        snapshot.live.iter().any(|key| key == "src/immer.ts#Write"),
        "types referenced by a module augmentation must stay live: {:?}",
        snapshot.live
    );
}

#[test]
fn react_dom_render_is_a_legacy_root() {
    assert_legacy_react_app_is_live(&legacy_react_app(
        "import React from \"react\";\nimport ReactDOM from \"react-dom\";\nimport App from \"./App\";\nimport { formatting } from \"./format\";\n\nReactDOM.render(<App label={formatting()} />, document.getElementById(\"root\"));\n",
    ));
}

#[test]
fn testing_library_render_is_not_a_react_root() {
    let snapshot = legacy_react_app(
        "import { render } from \"@testing-library/react\";\nimport App from \"./App\";\n\nrender(<App label=\"test\" />);\n",
    );
    assert!(
        snapshot.is_candidate("src/App.js#default"),
        "testing-library render must not make the application tree live: {:?}",
        snapshot.candidates
    );
}
