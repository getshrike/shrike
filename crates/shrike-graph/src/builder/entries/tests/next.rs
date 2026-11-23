//! Tests for next entry detection.

use super::*;

#[test]
fn symbol_line_counts_use_the_declaration_span() {
    let snapshot = snapshot(&[(
        "source/math.ts",
        "export const add = (a: number, b: number) => {\n  return a + b;\n};\n\nexport const one = 1;\n",
    )]);
    let lines = |name: &str| {
        snapshot
            .symbols
            .iter()
            .find(|symbol| symbol.key == format!("source/math.ts#{name}"))
            .expect("symbol")
            .lines
    };
    assert_eq!(lines("add"), 3);
    assert_eq!(lines("one"), 1);
}

#[test]
fn root_next_proxy_is_an_entry() {
    let snapshot = snapshot(&[
        (
            "proxy.ts",
            "export const config = { matcher: ['/jobs/:slug'] };\nexport async function proxy() {}\n",
        ),
        ("src/index.ts", "export const live = 1;\n"),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(entries.contains(&"proxy.ts#proxy"), "{entries:?}");
    assert!(entries.contains(&"proxy.ts#config"), "{entries:?}");
}

#[test]
fn root_next_instrumentation_entries_keep_only_the_api_live() {
    let snapshot = snapshot(&[
        (
            "instrumentation.ts",
            "import { setup } from \"./lib/setup\";\n\
                 export async function register() { setup(); }\n\
                 export function onRequestError() {}\n\
                 export function unusedInstrumentationHelper() {}\n",
        ),
        (
            "instrumentation-client.ts",
            "export function onRouterTransitionStart() {}\n\
                 export const unusedClientHelper = 1;\n",
        ),
        ("lib/setup.ts", "export function setup() { return 1; }\n"),
        (
            "src/instrumentation.ts",
            "export function register() {}\n\
                 export const unusedSrcInstrumentationHelper = 1;\n",
        ),
        (
            "lib/instrumentation.ts",
            "export function unrelatedInstrumentation() {}\n",
        ),
        (
            "app/page.tsx",
            "export default function Page() { return null; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    for key in [
        "instrumentation.ts#__module__",
        "instrumentation.ts#register",
        "instrumentation.ts#onRequestError",
        "instrumentation-client.ts#__module__",
        "instrumentation-client.ts#onRouterTransitionStart",
        "src/instrumentation.ts#__module__",
        "src/instrumentation.ts#register",
        "app/page.tsx#default",
    ] {
        assert!(
            snapshot.is_live(key),
            "{key} must stay live: {:?}",
            snapshot.live
        );
        assert!(
            entries.contains(&key),
            "{key} must be an entry: {entries:?}"
        );
    }
    assert!(
        snapshot.is_live("lib/setup.ts#setup"),
        "an import from instrumentation must keep setup live: {:?}",
        snapshot.live
    );
    for key in [
        "instrumentation.ts#unusedInstrumentationHelper",
        "instrumentation-client.ts#unusedClientHelper",
        "src/instrumentation.ts#unusedSrcInstrumentationHelper",
        "lib/instrumentation.ts#unrelatedInstrumentation",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "{key} is unused and must remain a candidate: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn next_app_and_pages_are_package_relative() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"probe","private":true,"workspaces":["apps/*","packages/*"]}"#,
        ),
        (
            "apps/web/package.json",
            r#"{"name":"@probe/web","private":true,"dependencies":{"next":"^16.0.0","react":"^19.1.0"}}"#,
        ),
        (
            "packages/app/package.json",
            r#"{"name":"@probe/app","private":true,"dependencies":{"react":"^19.1.0"}}"#,
        ),
        (
            "packages/pages/package.json",
            r#"{"name":"@probe/pages","private":true,"dependencies":{"react":"^19.1.0"}}"#,
        ),
        (
            "apps/web/app/page.tsx",
            "export default function WebPage() { return null; }\nexport const unusedWebHelper = 1;\n",
        ),
        (
            "packages/app/src/page.tsx",
            "export default function DeadPage() { return null; }\nexport const unusedPageHelper = 2;\n",
        ),
        (
            "packages/app/src/layout.tsx",
            "export default function DeadLayout() { return null; }\nexport const unusedLayoutHelper = 3;\n",
        ),
        (
            "packages/pages/src/settings.tsx",
            "export default function DeadSettings() { return null; }\nexport const unusedSettingsHelper = 4;\n",
        ),
    ]);
    assert!(
        snapshot.is_live("apps/web/app/page.tsx#default"),
        "a package-relative app route is live: {:?}",
        snapshot.live
    );
    assert!(
        snapshot.is_candidate("apps/web/app/page.tsx#unusedWebHelper"),
        "a route helper is still analyzable: {:?}",
        snapshot.candidates
    );
    for key in [
        "packages/app/src/page.tsx#default",
        "packages/app/src/layout.tsx#default",
        "packages/pages/src/settings.tsx#default",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "{key} is not a Next route because the package path only contains `app`/`pages`: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn next_conventions_require_a_next_dependency() {
    let plain = snapshot(&[
        (
            "package.json",
            r#"{"name":"plain-react","dependencies":{"react":"^19.0.0"}}"#,
        ),
        (
            "app/page.tsx",
            "export default function Page() { return null; }\n",
        ),
        (
            "pages/index.tsx",
            "export default function Home() { return null; }\n",
        ),
        (
            "middleware.ts",
            "export function middleware() { return null; }\n",
        ),
    ]);
    for key in [
        "app/page.tsx#default",
        "pages/index.tsx#default",
        "middleware.ts#middleware",
    ] {
        assert!(
            plain.is_candidate(key),
            "{key} is not a Next convention without a next dependency: {:?}",
            plain.candidates
        );
    }

    let next = snapshot(&[
        (
            "package.json",
            r#"{"name":"next-app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "app/page.tsx",
            "export default function Page() { return null; }\nexport const unusedPageHelper = 1;\n",
        ),
        (
            "pages/index.tsx",
            "export default function Home() { return null; }\n",
        ),
        (
            "middleware.ts",
            "export function middleware() { return null; }\n",
        ),
    ]);
    for key in [
        "app/page.tsx#default",
        "pages/index.tsx#default",
        "middleware.ts#middleware",
    ] {
        assert!(
            next.is_live(key),
            "{key} is a Next convention when the package depends on next: {:?}",
            next.live
        );
    }
    assert!(
        next.is_candidate("app/page.tsx#unusedPageHelper"),
        "a route helper stays analyzable: {:?}",
        next.candidates
    );

    // An incomplete tree has no manifest to consult; keep the convention
    // rather than hiding a possible route.
    let unknown = snapshot(&[(
        "app/page.tsx",
        "export default function Page() { return null; }\n",
    )]);
    assert!(
        unknown.is_live("app/page.tsx#default"),
        "a missing manifest fails open: {:?}",
        unknown.candidates
    );
}

#[test]
fn server_actions_are_entries_under_protect_policy() {
    let config = EngineConfig {
        server_action_policy: ServerActionPolicy::Protect,
        ..EngineConfig::default()
    };
    let files = [
        (
            "package.json",
            r#"{"name":"app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "app/page.tsx",
            "import { importedAction } from '../src/actions';\n\
                 import { inlineAction } from '../src/inline';\n\
                 export default function Page() { return importedAction() + inlineAction(); }\n",
        ),
        (
            "src/actions.ts",
            "\"use server\";\n\
                 export async function importedAction() { return 1; }\n\
                 export async function unimportedAction() { return 2; }\n\
                 export const arrowAction = async () => 3;\n\
                 export const value = 4;\n",
        ),
        (
            "src/inline.ts",
            "export async function inlineAction() { \"use server\"; return 1; }\n\
                 export const inlineArrow = async () => { \"use server\"; return 2; };\n\
                 export default async function () { \"use server\"; return 3; }\n\
                 export const plain = () => 4;\n",
        ),
    ];
    let snapshot = snapshot_with_config(&files, &config);
    let entries = entry_symbols(&snapshot);
    for key in [
        "src/actions.ts#importedAction",
        "src/actions.ts#unimportedAction",
        "src/inline.ts#inlineAction",
        "src/inline.ts#inlineArrow",
        "src/inline.ts#default",
    ] {
        assert!(entries.contains(&key), "{key}: {entries:?}");
        assert!(
            snapshot.is_live(key),
            "{key} must stay live: {:?}",
            snapshot.live
        );
        assert!(
            !snapshot.is_candidate(key),
            "{key} must not be a candidate: {:?}",
            snapshot.candidates
        );
    }
    assert!(
        snapshot.is_candidate("src/inline.ts#plain"),
        "an ordinary export stays analyzable: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_live("src/actions.ts#value"),
        "a file-level use server directive protects every export: {:?}",
        snapshot.live
    );

    // Inline actions are only protected when the project has RSC bundler
    // evidence. Without it, the body directive has no special meaning.
    let no_bundler = snapshot_with_config(
        &[
            (
                "package.json",
                r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
            ),
            (
                "src/inline.ts",
                "export async function inlineAction() { \"use server\"; return 1; }\n",
            ),
        ],
        &config,
    );
    assert!(
        no_bundler.is_candidate("src/inline.ts#inlineAction"),
        "no RSC bundler evidence: {:?}",
        no_bundler.candidates
    );

    // Under the default `graph` policy, an inline action is dead unless
    // application code imports it, matching fallow's unused-action model.
    let graph = snapshot_with_config(&files, &EngineConfig::default());
    assert!(
        graph.is_candidate("src/inline.ts#inlineArrow"),
        "default graph policy: {:?}",
        graph.candidates
    );
    assert!(
        graph.is_candidate("src/inline.ts#default"),
        "an unreferenced default inline action is dead: {:?}",
        graph.candidates
    );
    assert!(
        graph.is_live("src/inline.ts#inlineAction"),
        "the imported inline action is live: {:?}",
        graph.live
    );
}

#[test]
fn misplaced_rsc_directives_are_project_gated_warnings() {
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
            "app/late-client.tsx",
            "import { Widget } from \"../components/widget\";\n\"use client\";\nexport function Page() { return <Widget />; }\n",
        ),
        (
            "app/late-server.ts",
            "const value = 1;\n\"use server\";\nexport async function action() { return value; }\n",
        ),
        // A leading directive is honored, even when imports follow it.
        (
            "app/leading.tsx",
            "\"use client\";\nimport { Widget } from \"../components/widget\";\nexport function Page() { return <Widget />; }\n",
        ),
        // A nested string and an out-of-scope `"use strict"` are not
        // directive statements.
        (
            "app/nested.ts",
            "export const labels = { directive: \"use client\" };\n",
        ),
        (
            "app/strict.ts",
            "import { Widget } from \"../components/widget\";\n\"use strict\";\nexport const Page = Widget;\n",
        ),
        // The gate is project-wide: a shared package inside an RSC project
        // is compiled in the consuming app's context.
        (
            "packages/plain/src/late-client.ts",
            "const value = 1;\n\"use client\";\nexport const Widget = value;\n",
        ),
        (
            "components/widget.tsx",
            "export function Widget() { return null; }\n",
        ),
    ]);
    let misplaced: Vec<(&str, u32, &str)> = next_snapshot
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "misplaced_directive")
        .map(|diagnostic| {
            (
                diagnostic.file.as_deref().unwrap_or(""),
                diagnostic.span.map_or(0, |span| span.line),
                diagnostic.message.as_str(),
            )
        })
        .collect();
    assert_eq!(
        misplaced.len(),
        3,
        "one warning per misplaced occurrence: {:?}",
        next_snapshot.diagnostics
    );
    assert_eq!(
        misplaced
            .iter()
            .map(|(file, line, _)| (*file, *line))
            .collect::<Vec<_>>(),
        vec![
            ("app/late-client.tsx", 2),
            ("app/late-server.ts", 2),
            ("packages/plain/src/late-client.ts", 2),
        ],
        "{:?}",
        next_snapshot.diagnostics
    );
    assert!(
        misplaced[0].2.contains("\"use client\"")
            && misplaced[1].2.contains("\"use server\"")
            && misplaced[0].2.contains("leading prologue"),
        "the warning names the ignored directive: {:?}",
        misplaced
    );
    assert!(
        next_snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "misplaced_directive")
            .all(|diagnostic| diagnostic.severity == shrike_core::Severity::Warning),
        "a misplaced directive is a warning, not an error: {:?}",
        next_snapshot.diagnostics
    );

    // Without an RSC bundler dependency the directives carry no special
    // meaning, so the same files produce no warning.
    let react_only = snapshot(&[
        (
            "package.json",
            r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
        ),
        (
            "app/late-client.tsx",
            "import { Widget } from \"../components/widget\";\n\"use client\";\nexport function Page() { return <Widget />; }\n",
        ),
        (
            "components/widget.tsx",
            "export function Widget() { return null; }\n",
        ),
    ]);
    assert!(
        react_only
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "misplaced_directive"),
        "no RSC bundler evidence: {:?}",
        react_only.diagnostics
    );

    // Any of the framework-agnostic RSC bundlers is enough.
    let waku = snapshot(&[
        (
            "package.json",
            r#"{"name":"app","dependencies":{"waku":"^0.21.0","react":"^19.0.0"}}"#,
        ),
        (
            "src/late-client.ts",
            "const value = 1;\n\"use client\";\nexport const Widget = value;\n",
        ),
    ]);
    assert!(
        waku.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "misplaced_directive"),
        "waku is an RSC bundler: {:?}",
        waku.diagnostics
    );
}

#[test]
fn next_route_collisions_and_dynamic_segment_conflicts_are_reported() {
    let routes = snapshot(&[
        (
            "package.json",
            r#"{"name":"app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "packages/plain/package.json",
            r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
        ),
        // Route groups are transparent: both pages own /about.
        (
            "app/(marketing)/about/page.tsx",
            "export default function A() { return null; }\n",
        ),
        (
            "app/(shop)/about/page.tsx",
            "export default function B() { return null; }\n",
        ),
        // A page and a route handler share the URL-owner namespace.
        (
            "app/api/thing/page.tsx",
            "export default function C() { return null; }\n",
        ),
        (
            "app/api/thing/route.ts",
            "export async function GET() { return new Response('ok'); }\n",
        ),
        // A grouped root page collides with the root page.
        (
            "app/page.tsx",
            "export default function D() { return null; }\n",
        ),
        (
            "app/(group)/page.tsx",
            "export default function E() { return null; }\n",
        ),
        // Parallel slots fork the URL namespace.
        (
            "app/@team/members/page.tsx",
            "export default function F() { return null; }\n",
        ),
        (
            "app/@analytics/members/page.tsx",
            "export default function G() { return null; }\n",
        ),
        // Different dynamic names at the same position conflict.
        (
            "app/shop/[id]/page.tsx",
            "export default function H() { return null; }\n",
        ),
        (
            "app/shop/[id]/layout.tsx",
            "export default function Layout() { return null; }\n",
        ),
        (
            "app/shop/[slug]/page.tsx",
            "export default function I() { return null; }\n",
        ),
        // Private and intercepting subtrees are not routable participants.
        (
            "app/_private/about/page.tsx",
            "export default function J() { return null; }\n",
        ),
        (
            "app/(.)photo/[id]/page.tsx",
            "export default function K() { return null; }\n",
        ),
        // A React-only package with an app-shaped directory is not a Next app.
        (
            "packages/plain/app/(a)/about/page.tsx",
            "export default function L() { return null; }\n",
        ),
        (
            "packages/plain/app/(b)/about/page.tsx",
            "export default function M() { return null; }\n",
        ),
    ]);
    let findings: Vec<(&str, &str)> = routes
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == "route_collision"
                || diagnostic.code == "dynamic_segment_name_conflict"
        })
        .map(|diagnostic| {
            (
                diagnostic.code.as_str(),
                diagnostic.file.as_deref().unwrap_or(""),
            )
        })
        .collect();
    assert_eq!(
        findings,
        vec![
            ("route_collision", "app/(group)/page.tsx"),
            ("route_collision", "app/page.tsx"),
            ("route_collision", "app/(marketing)/about/page.tsx"),
            ("route_collision", "app/(shop)/about/page.tsx"),
            ("route_collision", "app/api/thing/page.tsx"),
            ("route_collision", "app/api/thing/route.ts"),
            // Every file proving the conflicting position is reported,
            // including the layout that shares the [id] spelling.
            ("dynamic_segment_name_conflict", "app/shop/[id]/layout.tsx"),
            ("dynamic_segment_name_conflict", "app/shop/[id]/page.tsx"),
            ("dynamic_segment_name_conflict", "app/shop/[slug]/page.tsx"),
        ],
        "{:?}",
        routes.diagnostics
    );
    assert!(
        routes
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == "route_collision"
                    || diagnostic.code == "dynamic_segment_name_conflict"
            })
            .all(|diagnostic| diagnostic.severity == shrike_core::Severity::Error),
        "routing conflicts are build/runtime errors: {:?}",
        routes.diagnostics
    );

    // Without a manifest that declares Next.js, an app-shaped directory is
    // not a Next app and produces no routing diagnostics.
    let no_manifest = snapshot(&[
        (
            "apps/web/app/(a)/about/page.tsx",
            "export default function A() { return null; }\n",
        ),
        (
            "apps/web/app/(b)/about/page.tsx",
            "export default function B() { return null; }\n",
        ),
    ]);
    assert!(
        no_manifest.diagnostics.iter().all(|diagnostic| {
            diagnostic.code != "route_collision"
                && diagnostic.code != "dynamic_segment_name_conflict"
        }),
        "{:?}",
        no_manifest.diagnostics
    );
}

#[test]
fn next_route_conflicts_honor_src_app_and_dotted_page_extensions() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"app","dependencies":{"next":"^16.0.0"}}"#,
        ),
        (
            "next.config.ts",
            "const config = { pageExtensions: ['page.tsx'] };\nexport default config;\n",
        ),
        (
            "app/docs/page.page.tsx",
            "export default function A() { return null; }\n",
        ),
        (
            "app/docs/(alt)/page.page.tsx",
            "export default function B() { return null; }\n",
        ),
        (
            "src/app/shop/[id]/page.page.tsx",
            "export default function C() { return null; }\n",
        ),
        (
            "src/app/shop/[slug]/page.page.tsx",
            "export default function D() { return null; }\n",
        ),
    ]);
    let findings: Vec<(&str, &str)> = snapshot
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == "route_collision"
                || diagnostic.code == "dynamic_segment_name_conflict"
        })
        .map(|diagnostic| {
            (
                diagnostic.code.as_str(),
                diagnostic.file.as_deref().unwrap_or(""),
            )
        })
        .collect();
    assert_eq!(
        findings,
        vec![
            ("route_collision", "app/docs/(alt)/page.page.tsx"),
            ("route_collision", "app/docs/page.page.tsx"),
            (
                "dynamic_segment_name_conflict",
                "src/app/shop/[id]/page.page.tsx"
            ),
            (
                "dynamic_segment_name_conflict",
                "src/app/shop/[slug]/page.page.tsx"
            ),
        ],
        "{:?}",
        snapshot.diagnostics
    );
}

#[test]
fn next_pages_router_dynamic_name_conflicts_are_reported() {
    let routes = snapshot(&[
        (
            "package.json",
            r#"{"name":"pages","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "packages/plain/package.json",
            r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
        ),
        (
            "pages/index.tsx",
            "export default function Home() { return null; }\n",
        ),
        // Different regular dynamic names at the same position.
        (
            "pages/[id].tsx",
            "export default function A() { return null; }\n",
        ),
        (
            "pages/[slug].tsx",
            "export default function B() { return null; }\n",
        ),
        // The same conflict below a static segment.
        (
            "pages/blog/[id].tsx",
            "export default function C() { return null; }\n",
        ),
        (
            "pages/blog/[slug].tsx",
            "export default function D() { return null; }\n",
        ),
        // Catch-all siblings compare their stripped names too.
        (
            "pages/docs/[...id].tsx",
            "export default function E() { return null; }\n",
        ),
        (
            "pages/docs/[...slug].tsx",
            "export default function F() { return null; }\n",
        ),
        // A regular and a catch-all sibling use different tree slots and
        // build successfully.
        (
            "pages/guide/[id].tsx",
            "export default function G() { return null; }\n",
        ),
        (
            "pages/guide/[...slug].tsx",
            "export default function H() { return null; }\n",
        ),
        // API routes are pages in the same routing tree.
        (
            "pages/api/[id].ts",
            "export default function handler(req, res) { res.end('ok'); }\n",
        ),
        (
            "pages/api/[slug].ts",
            "export default function handler(req, res) { res.end('ok'); }\n",
        ),
        // A React-only package with a pages-shaped directory is not a
        // Next.js Pages Router.
        (
            "packages/plain/pages/[a].tsx",
            "export default function I() { return null; }\n",
        ),
        (
            "packages/plain/pages/[b].tsx",
            "export default function J() { return null; }\n",
        ),
    ]);
    assert_eq!(
        pages_route_findings(&routes),
        vec![
            ("dynamic_segment_name_conflict", "pages/[id].tsx"),
            ("dynamic_segment_name_conflict", "pages/[slug].tsx"),
            ("dynamic_segment_name_conflict", "pages/api/[id].ts"),
            ("dynamic_segment_name_conflict", "pages/api/[slug].ts"),
            ("dynamic_segment_name_conflict", "pages/blog/[id].tsx"),
            ("dynamic_segment_name_conflict", "pages/blog/[slug].tsx"),
            ("dynamic_segment_name_conflict", "pages/docs/[...id].tsx"),
            ("dynamic_segment_name_conflict", "pages/docs/[...slug].tsx"),
        ],
        "{:?}",
        routes.diagnostics
    );
    assert!(
        routes
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == "dynamic_segment_name_conflict"
                    || diagnostic.code == "pages_router_route_conflict"
            })
            .all(|diagnostic| diagnostic.severity == shrike_core::Severity::Error),
        "routing conflicts are build errors: {:?}",
        routes.diagnostics
    );
}

#[test]
fn next_pages_router_catch_all_and_specificity_conflicts_are_reported() {
    let routes = snapshot(&[
        (
            "package.json",
            r#"{"name":"pages","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "pages/index.tsx",
            "export default function Home() { return null; }\n",
        ),
        // A root optional catch-all also matches `/`, so the root index
        // page has the same specificity.
        (
            "pages/[[...slug]].tsx",
            "export default function A() { return null; }\n",
        ),
        // The same conflict below a static segment, from both a leaf file
        // and a directory index.
        (
            "pages/shop.tsx",
            "export default function B() { return null; }\n",
        ),
        (
            "pages/shop/[[...slug]].tsx",
            "export default function C() { return null; }\n",
        ),
        (
            "pages/guide/index.tsx",
            "export default function D() { return null; }\n",
        ),
        (
            "pages/guide/[[...slug]].tsx",
            "export default function E() { return null; }\n",
        ),
        // Required and optional catch-alls cannot share a level.
        (
            "pages/docs/[...id].tsx",
            "export default function F() { return null; }\n",
        ),
        (
            "pages/docs/[[...id]].tsx",
            "export default function G() { return null; }\n",
        ),
        (
            "pages/api/[...id].ts",
            "export default function handler(req, res) { res.end('ok'); }\n",
        ),
        (
            "pages/api/[[...id]].ts",
            "export default function handler(req, res) { res.end('ok'); }\n",
        ),
        // A regular and an optional catch-all sibling with no static leaf
        // build successfully.
        (
            "pages/blog/[id].tsx",
            "export default function H() { return null; }\n",
        ),
        (
            "pages/blog/[[...slug]].tsx",
            "export default function I() { return null; }\n",
        ),
        // A required catch-all with a static leaf builds successfully.
        (
            "pages/legal/[...slug].tsx",
            "export default function J() { return null; }\n",
        ),
        (
            "pages/legal/foo.tsx",
            "export default function K() { return null; }\n",
        ),
        // Reserved pages are not ordinary route leaves, but a real page
        // such as `404` is.
        (
            "pages/404.tsx",
            "export default function L() { return null; }\n",
        ),
        (
            "pages/404/[[...slug]].tsx",
            "export default function M() { return null; }\n",
        ),
        (
            "pages/_app.tsx",
            "export default function App({ Component, pageProps }) { return <Component {...pageProps} />; }\n",
        ),
    ]);
    assert_eq!(
        pages_route_findings(&routes),
        vec![
            ("pages_router_route_conflict", "pages/404/[[...slug]].tsx"),
            ("pages_router_route_conflict", "pages/[[...slug]].tsx"),
            ("pages_router_route_conflict", "pages/api/[...id].ts"),
            ("pages_router_route_conflict", "pages/api/[[...id]].ts"),
            ("pages_router_route_conflict", "pages/docs/[...id].tsx"),
            ("pages_router_route_conflict", "pages/docs/[[...id]].tsx"),
            ("pages_router_route_conflict", "pages/guide/[[...slug]].tsx"),
            ("pages_router_route_conflict", "pages/shop/[[...slug]].tsx"),
        ],
        "{:?}",
        routes.diagnostics
    );
}

#[test]
fn next_pages_router_repeated_and_malformed_segments_are_reported() {
    let routes = snapshot(&[
        (
            "package.json",
            r#"{"name":"pages","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "packages/plain/package.json",
            r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
        ),
        (
            "pages/index.tsx",
            "export default function Home() { return null; }\n",
        ),
        // A slug name cannot repeat within one route, across dynamic kinds.
        (
            "pages/repeat/[id]/[id].tsx",
            "export default function A() { return null; }\n",
        ),
        (
            "pages/repeat-catch/[slug]/[...slug].tsx",
            "export default function B() { return null; }\n",
        ),
        (
            "pages/repeat-optional/[slug]/[[...slug]].tsx",
            "export default function C() { return null; }\n",
        ),
        // Names that differ only by non-word symbols address the same
        // parameter.
        (
            "pages/repeat-nonword/[id]/[i-d].tsx",
            "export default function D() { return null; }\n",
        ),
        // A catch-all must be the final segment.
        (
            "pages/[...slug]/x.tsx",
            "export default function E() { return null; }\n",
        ),
        (
            "pages/[[...slug]]/x.tsx",
            "export default function F() { return null; }\n",
        ),
        // Optional parameters are only supported as catch-alls.
        (
            "pages/[[id]].tsx",
            "export default function G() { return null; }\n",
        ),
        // Different names at different positions are fine.
        (
            "pages/ok/[id]/[slug].tsx",
            "export default function H() { return null; }\n",
        ),
        // The same shapes in a React-only package are left alone.
        (
            "packages/plain/pages/[...bad]/x.tsx",
            "export default function I() { return null; }\n",
        ),
        (
            "packages/plain/pages/[[bad]].tsx",
            "export default function J() { return null; }\n",
        ),
    ]);
    assert_eq!(
        pages_route_findings(&routes),
        vec![
            (
                "dynamic_segment_name_conflict",
                "pages/repeat-catch/[slug]/[...slug].tsx"
            ),
            (
                "dynamic_segment_name_conflict",
                "pages/repeat-nonword/[id]/[i-d].tsx"
            ),
            (
                "dynamic_segment_name_conflict",
                "pages/repeat-optional/[slug]/[[...slug]].tsx"
            ),
            (
                "dynamic_segment_name_conflict",
                "pages/repeat/[id]/[id].tsx"
            ),
            ("pages_router_route_conflict", "pages/[...slug]/x.tsx"),
            ("pages_router_route_conflict", "pages/[[...slug]]/x.tsx"),
            ("pages_router_route_conflict", "pages/[[id]].tsx"),
        ],
        "{:?}",
        routes.diagnostics
    );
}

#[test]
fn next_pages_router_route_conflicts_honor_roots_and_page_extensions() {
    let routes = snapshot(&[
        (
            "package.json",
            r#"{"name":"pages","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "next.config.ts",
            "const config = { pageExtensions: ['page.tsx'] };\nexport default config;\n",
        ),
        // `pages` and `src/pages` are separate roots: Next.js uses one per
        // package, so names across the roots do not conflict.
        (
            "pages/[a].page.tsx",
            "export default function A() { return null; }\n",
        ),
        (
            "src/pages/[b].page.tsx",
            "export default function B() { return null; }\n",
        ),
        // Within one root, the configured dotted extension is the page
        // stem and the conflict is real.
        (
            "src/pages/shop/[id].page.tsx",
            "export default function C() { return null; }\n",
        ),
        (
            "src/pages/shop/[slug].page.tsx",
            "export default function D() { return null; }\n",
        ),
        // A file that does not use the configured extension is not a page,
        // so it cannot create an optional catch-all specificity conflict.
        (
            "pages/legacy.tsx",
            "export default function E() { return null; }\n",
        ),
        (
            "pages/legacy/[[...slug]].page.tsx",
            "export default function F() { return null; }\n",
        ),
    ]);
    assert_eq!(
        pages_route_findings(&routes),
        vec![
            (
                "dynamic_segment_name_conflict",
                "src/pages/shop/[id].page.tsx"
            ),
            (
                "dynamic_segment_name_conflict",
                "src/pages/shop/[slug].page.tsx"
            ),
        ],
        "{:?}",
        routes.diagnostics
    );
}

#[test]
fn next_segment_config_conflicts_are_build_errors() {
    let routes = snapshot(&[
        (
            "package.json",
            r#"{"name":"app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "app/edge-gsp/page.tsx",
            "export const runtime = 'edge';\nexport function generateStaticParams() { return []; }\nexport default function Page() { return null; }\n",
        ),
        (
            "app/edge-param/page.tsx",
            "export const runtime = 'edge';\nexport const unstable_paramMatching = {};\nexport default function Page() { return null; }\n",
        ),
        (
            "app/client-gsp/page.tsx",
            "\"use client\";\nexport const generateStaticParams = () => [];\nexport default function Page() { return null; }\n",
        ),
        (
            "app/client-instant/page.tsx",
            "\"use client\";\nexport const instant = true;\nexport default function Page() { return null; }\n",
        ),
        (
            "app/layout-stale/layout.tsx",
            "export const unstable_dynamicStaleTime = 60;\nexport default function Layout() { return null; }\n",
        ),
        (
            "app/stale-instant/page.tsx",
            "export const unstable_dynamicStaleTime = 60;\nexport const instant = true;\nexport default function Page() { return null; }\n",
        ),
        (
            "app/route-param/route.ts",
            "export const unstable_paramMatching = {};\nexport function GET() { return new Response('ok'); }\n",
        ),
        (
            "app/both-param/page.tsx",
            "export const unstable_paramMatching = {};\nexport const unstable_generateParamMatching = {};\nexport default function Page() { return null; }\n",
        ),
        (
            "app/asserted-runtime/page.tsx",
            "export const runtime = 'edge' as const;\nexport function generateStaticParams() { return []; }\nexport default function Page() { return null; }\n",
        ),
        (
            "app/type-reexport-gsp/page.tsx",
            "\"use client\";\nexport type { generateStaticParams } from './types';\nexport default function Page() { return null; }\n",
        ),
        (
            "src/app/src-edge/page.tsx",
            "export const runtime = 'edge';\nexport function generateStaticParams() { return []; }\nexport default function Page() { return null; }\n",
        ),
        // A function named `instant` is not a segment-config value.
        (
            "app/function-instant/page.tsx",
            "\"use client\";\nexport function instant() { return true; }\nexport default function Page() { return null; }\n",
        ),
        // A re-exported `runtime` is not read as segment config.
        (
            "app/runtime-reexport/page.tsx",
            "export { runtime } from './runtime';\nexport function generateStaticParams() { return []; }\nexport default function Page() { return null; }\n",
        ),
        // Parentheses defeat Next.js's const-value extractor.
        (
            "app/paren-runtime/page.tsx",
            "export const runtime = ('edge');\nexport function generateStaticParams() { return []; }\nexport default function Page() { return null; }\n",
        ),
        // A non-Next package with an app-shaped directory is not a Next app.
        (
            "packages/plain/package.json",
            r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
        ),
        (
            "packages/plain/app/plain/page.tsx",
            "export const runtime = 'edge';\nexport function generateStaticParams() { return []; }\nexport default function Page() { return null; }\n",
        ),
    ]);
    let findings: Vec<&str> = routes
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "invalid_route_segment_config")
        .map(|diagnostic| diagnostic.file.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(
        findings,
        vec![
            "app/asserted-runtime/page.tsx",
            "app/both-param/page.tsx",
            "app/client-gsp/page.tsx",
            "app/client-instant/page.tsx",
            "app/edge-gsp/page.tsx",
            "app/edge-param/page.tsx",
            "app/layout-stale/layout.tsx",
            "app/route-param/route.ts",
            "app/stale-instant/page.tsx",
            "app/type-reexport-gsp/page.tsx",
            "src/app/src-edge/page.tsx",
        ],
        "{:?}",
        routes.diagnostics
    );
    assert!(
        routes
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "invalid_route_segment_config")
            .all(|diagnostic| diagnostic.severity == shrike_core::Severity::Error),
        "segment-config conflicts are Next.js build errors: {:?}",
        routes.diagnostics
    );
    let messages: Vec<&str> = routes
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "invalid_route_segment_config")
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("generateStaticParams")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("parameter matching")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("unstable_dynamicStaleTime")),
        "{messages:?}"
    );

    // Without a manifest that declares Next.js, the same exports are inert.
    let no_manifest = snapshot(&[(
        "app/page.tsx",
        "export const runtime = 'edge';\nexport function generateStaticParams() { return []; }\nexport default function Page() { return null; }\n",
    )]);
    assert!(
        no_manifest
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "invalid_route_segment_config"),
        "{:?}",
        no_manifest.diagnostics
    );
}

#[test]
fn pages_router_data_and_runtime_conflicts_are_build_errors() {
    let next_snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"pages-app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "packages/plain/package.json",
            r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
        ),
        (
            "pages/both.tsx",
            "export default function Page() { return null; }\nexport const unusedHelper = 1;\nexport async function getStaticProps() { return { props: {} }; }\nexport async function getServerSideProps() { return { props: {} }; }\n",
        ),
        (
            "pages/gip-gsp.tsx",
            "function Page() { return null; }\nPage.getInitialProps = async () => ({});\nexport default Page;\nexport async function getStaticProps() { return { props: {} }; }\n",
        ),
        (
            "pages/gip-gssp.tsx",
            "export default class Page {\n  static async getInitialProps() { return {}; }\n  render() { return null; }\n}\nexport async function getServerSideProps() { return { props: {} }; }\n",
        ),
        (
            "pages/gip-default-export.tsx",
            "export default function Page() { return null; }\nPage.getInitialProps = async () => ({});\nexport async function getServerSideProps() { return { props: {} }; }\n",
        ),
        (
            "pages/gip-export-specifier.tsx",
            "function Page() { return null; }\nPage.getInitialProps = async () => ({});\nexport { Page as default };\nexport async function getStaticProps() { return { props: {} }; }\n",
        ),
        (
            "pages/static-paths.tsx",
            "export default function Page() { return null; }\nexport async function getStaticProps() { return { props: {} }; }\nexport async function getStaticPaths() { return { paths: [], fallback: false }; }\n",
        ),
        (
            "pages/[slug].tsx",
            "export default function Page() { return null; }\nexport async function getStaticProps() { return { props: {} }; }\n",
        ),
        (
            "pages/paths-only.tsx",
            "export default function Page() { return null; }\nexport async function getStaticPaths() { return { paths: [], fallback: false }; }\n",
        ),
        (
            "pages/[id].tsx",
            "export default function Page() { return null; }\nexport async function getStaticPaths() { return { paths: [], fallback: false }; }\n",
        ),
        (
            "pages/gssp-paths.tsx",
            "export default function Page() { return null; }\nexport async function getServerSideProps() { return { props: {} }; }\nexport async function getStaticPaths() { return { paths: [], fallback: false }; }\n",
        ),
        (
            "pages/valid/[id].tsx",
            "export default function Page() { return null; }\nexport async function getStaticProps() { return { props: {} }; }\nexport async function getStaticPaths() { return { paths: [], fallback: false }; }\n",
        ),
        // Next.js does not prerender a getInitialProps page, so
        // getStaticPaths without getStaticProps only fails at request time.
        (
            "pages/gip-paths.tsx",
            "function Page() { return null; }\nPage.getInitialProps = async () => ({});\nexport default Page;\nexport async function getStaticPaths() { return { paths: [], fallback: false }; }\n",
        ),
        // Only the default component's getInitialProps counts.
        (
            "pages/named-gip.tsx",
            "export const getInitialProps = async () => ({});\nexport default function Page() { return null; }\nexport async function getStaticProps() { return { props: {} }; }\n",
        ),
        (
            "pages/helper-gip.tsx",
            "const Helper = () => null;\nHelper.getInitialProps = async () => ({});\nexport default function Page() { return null; }\nexport async function getStaticProps() { return { props: {} }; }\n",
        ),
        // Type-only exports are erased before Next.js loads the page.
        (
            "pages/type-only.tsx",
            "type Data = { props: {} };\nexport type { Data as getStaticProps };\nexport type { Data as getServerSideProps };\nexport default function Page() { return null; }\n",
        ),
        // A default-exported component named getStaticProps is not the
        // named data-function export.
        (
            "pages/default-named.tsx",
            "export default function getStaticProps() { return null; }\nexport async function getServerSideProps() { return { props: {} }; }\n",
        ),
        (
            "pages/renamed.tsx",
            "const load = async () => ({ props: {} });\nexport { load as getStaticProps };\nexport async function getServerSideProps() { return { props: {} }; }\n",
        ),
        (
            "pages/reexport.tsx",
            "export { getStaticProps } from './data';\nexport async function getServerSideProps() { return { props: {} }; }\n",
        ),
        (
            "pages/data.ts",
            "export async function getStaticProps() { return { props: {} }; }\n",
        ),
        (
            "pages/edge.tsx",
            "export default function Page() { return null; }\nexport const runtime = 'edge';\n",
        ),
        (
            "pages/edge-config.tsx",
            "export default function Page() { return null; }\nexport const config = { runtime: 'edge' };\n",
        ),
        (
            "pages/experimental.tsx",
            "export default function Page() { return null; }\nexport const config = { runtime: 'experimental-edge' };\n",
        ),
        (
            "pages/_app.tsx",
            "export default function App({ Component, pageProps }) { return <Component {...pageProps} />; }\nexport const config = { runtime: 'edge' };\n",
        ),
        (
            "pages/_document.tsx",
            "export default function Document() { return null; }\nexport async function getStaticProps() { return { props: {} }; }\nexport async function getServerSideProps() { return { props: {} }; }\n",
        ),
        (
            "pages/api/edge.ts",
            "export default function handler(req, res) { res.end('ok'); }\nexport const config = { runtime: 'edge' };\n",
        ),
        (
            "pages/api/bad.ts",
            "export default function handler(req, res) { res.end('ok'); }\nexport const config = { runtime: 'bogus' };\n",
        ),
        (
            "pages/api/data.ts",
            "export default function handler(req, res) { res.end('ok'); }\nexport async function getStaticProps() { return { props: {} }; }\nexport async function getServerSideProps() { return { props: {} }; }\n",
        ),
        (
            "src/pages/src-both.tsx",
            "export default function Page() { return null; }\nexport async function getStaticProps() { return { props: {} }; }\nexport async function getServerSideProps() { return { props: {} }; }\n",
        ),
        (
            "packages/plain/pages/both.tsx",
            "export default function Page() { return null; }\nexport async function getStaticProps() { return { props: {} }; }\nexport async function getServerSideProps() { return { props: {} }; }\n",
        ),
    ]);
    let invalid: Vec<(&str, u32)> = next_snapshot
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "invalid_pages_router_export")
        .map(|diagnostic| {
            (
                diagnostic.file.as_deref().unwrap_or(""),
                diagnostic.span.map_or(0, |span| span.line),
            )
        })
        .collect();
    assert_eq!(
        invalid,
        vec![
            ("pages/[id].tsx", 2),
            ("pages/[slug].tsx", 2),
            ("pages/_app.tsx", 2),
            ("pages/api/bad.ts", 2),
            ("pages/both.tsx", 4),
            ("pages/edge-config.tsx", 2),
            ("pages/edge.tsx", 2),
            ("pages/gip-default-export.tsx", 3),
            ("pages/gip-export-specifier.tsx", 4),
            ("pages/gip-gsp.tsx", 4),
            ("pages/gip-gssp.tsx", 5),
            ("pages/gssp-paths.tsx", 2),
            ("pages/paths-only.tsx", 2),
            ("pages/reexport.tsx", 2),
            ("pages/renamed.tsx", 3),
            ("pages/static-paths.tsx", 3),
            ("src/pages/src-both.tsx", 3),
        ],
        "{:?}",
        next_snapshot.diagnostics
    );
    assert!(
        next_snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "invalid_pages_router_export")
            .all(|diagnostic| {
                diagnostic.severity == shrike_core::Severity::Error
                    && diagnostic.message.contains("fails the build")
            }),
        "Pages Router conflicts are build errors: {:?}",
        next_snapshot.diagnostics
    );
    assert!(
        next_snapshot
            .candidates
            .iter()
            .any(|key| key == "pages/both.tsx#unusedHelper"),
        "diagnostics must not change candidates: {:?}",
        next_snapshot.candidates
    );

    // A configured `pageExtensions` list replaces Next.js's defaults.
    let configured = snapshot(&[
        (
            "package.json",
            r#"{"name":"pages-app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "next.config.ts",
            "const config = { pageExtensions: ['page.tsx'] };\nexport default config;\n",
        ),
        (
            "pages/ignored.tsx",
            "export default function Page() { return null; }\nexport async function getStaticProps() { return { props: {} }; }\nexport async function getServerSideProps() { return { props: {} }; }\n",
        ),
        (
            "pages/real.page.tsx",
            "export default function Page() { return null; }\nexport async function getStaticProps() { return { props: {} }; }\nexport async function getServerSideProps() { return { props: {} }; }\n",
        ),
    ]);
    let configured_files: Vec<&str> = configured
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "invalid_pages_router_export")
        .map(|diagnostic| diagnostic.file.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(
        configured_files,
        vec!["pages/real.page.tsx"],
        "{:?}",
        configured.diagnostics
    );

    // Without a manifest that declares Next.js, the same page is inert.
    let no_manifest = snapshot(&[(
        "pages/both.tsx",
        "export default function Page() { return null; }\nexport async function getStaticProps() { return { props: {} }; }\nexport async function getServerSideProps() { return { props: {} }; }\n",
    )]);
    assert!(
        no_manifest
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "invalid_pages_router_export"),
        "{:?}",
        no_manifest.diagnostics
    );
}

#[test]
fn next_interrupt_routes_and_image_metadata_are_entries() {
    let snapshot = snapshot(&[
        (
            "app/layout.tsx",
            "export default function Layout() { return null; }\n",
        ),
        (
            "app/page.tsx",
            "export default function Page() { return null; }\nexport const helper = 1;\n",
        ),
        (
            "app/forbidden.tsx",
            "export default function Forbidden() { return null; }\n",
        ),
        (
            "app/unauthorized.tsx",
            "export default function Unauthorized() { return null; }\n",
        ),
        (
            "app/icon.tsx",
            "export const size = { width: 32 };\nexport const contentType = 'image/png';\nexport default function Icon() { return null; }\n",
        ),
        (
            "app/opengraph-image.tsx",
            "export const size = { width: 1200 };\nexport const contentType = 'image/png';\nexport default function Image() { return null; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(
        entries.contains(&"app/forbidden.tsx#default"),
        "{entries:?}"
    );
    assert!(
        entries.contains(&"app/unauthorized.tsx#default"),
        "{entries:?}"
    );
    assert!(entries.contains(&"app/icon.tsx#size"), "{entries:?}");
    assert!(entries.contains(&"app/icon.tsx#contentType"), "{entries:?}");
    assert!(
        entries.contains(&"app/opengraph-image.tsx#size"),
        "{entries:?}"
    );
    assert!(
        entries.contains(&"app/opengraph-image.tsx#contentType"),
        "{entries:?}"
    );
    assert!(
        !entries.contains(&"app/page.tsx#helper"),
        "a page's arbitrary export must not become public: {entries:?}"
    );
    assert!(
        snapshot
            .candidates
            .iter()
            .any(|key| key == "app/page.tsx#helper"),
        "the page helper should remain a candidate: {:?}",
        snapshot.candidates
    );
}

#[test]
fn next_root_only_conventions_require_the_app_root() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"probe","private":true,"dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "app/page.tsx",
            "export default function Page() { return null; }\n",
        ),
        (
            "app/robots.ts",
            "export default function robots() { return {}; }\n",
        ),
        (
            "app/manifest.ts",
            "export default function manifest() { return {}; }\n",
        ),
        (
            "app/global-error.tsx",
            "\"use client\";\nexport default function GlobalError() { return null; }\n",
        ),
        (
            "app/nested/robots.ts",
            "export default function robots() { return {}; }\n",
        ),
        (
            "app/nested/manifest.ts",
            "export default function manifest() { return {}; }\n",
        ),
        (
            "app/nested/global-error.tsx",
            "\"use client\";\nexport default function GlobalError() { return null; }\n",
        ),
        (
            "app/(group)/robots.ts",
            "export default function robots() { return {}; }\n",
        ),
        (
            "app/@slot/robots.ts",
            "export default function robots() { return {}; }\n",
        ),
        (
            "app/nested/sitemap.ts",
            "export default function sitemap() { return []; }\n",
        ),
        (
            "app/nested/middleware.ts",
            "export function middleware() { return null; }\n",
        ),
        (
            "app/nested/instrumentation.ts",
            "export function register() { return null; }\n",
        ),
        (
            "app/nested/instrumentation-client.ts",
            "export function onRouterTransitionStart() { return null; }\n",
        ),
        (
            "middleware.ts",
            "export function middleware() { return null; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    for key in [
        "app/robots.ts#default",
        "app/manifest.ts#default",
        "app/global-error.tsx#default",
        "app/nested/sitemap.ts#default",
        "middleware.ts#middleware",
    ] {
        assert!(
            entries.contains(&key),
            "{key} must be an entry: {entries:?}"
        );
    }
    for key in [
        "app/nested/robots.ts#default",
        "app/nested/manifest.ts#default",
        "app/nested/global-error.tsx#default",
        "app/(group)/robots.ts#default",
        "app/@slot/robots.ts#default",
        "app/nested/middleware.ts#middleware",
        "app/nested/instrumentation.ts#register",
        "app/nested/instrumentation-client.ts#onRouterTransitionStart",
    ] {
        assert!(
            !entries.contains(&key),
            "{key} is not a Next convention outside the app root: {entries:?}"
        );
        assert!(
            snapshot.is_candidate(key),
            "{key} should remain analyzable: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn next_cache_components_segment_config_is_public() {
    let snapshot = snapshot(&[
        (
            "app/layout.tsx",
            "export const ensureStatic = 'navigation';\nexport const instant = true;\nexport const prefetch = 'partial';\nexport const unstable_instant = true;\nexport const unstable_prefetch = 'force-disabled';\nexport function unusedLayoutHelper() { return 1; }\nexport default function Layout() { return null; }\n",
        ),
        (
            "app/page.tsx",
            "export const instant = { level: 'warning' };\nexport const unstable_dynamicStaleTime = 60;\nexport function unusedPageHelper() { return 1; }\nexport default function Page() { return null; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    for name in [
        "ensureStatic",
        "instant",
        "unstable_instant",
        "prefetch",
        "unstable_prefetch",
    ] {
        let key = format!("app/layout.tsx#{name}");
        assert!(
            entries.contains(&key.as_str()),
            "segment config {name} must be public: {entries:?}"
        );
    }
    assert!(
        entries.contains(&"app/page.tsx#instant"),
        "page segment config must be public: {entries:?}"
    );
    assert!(
        entries.contains(&"app/page.tsx#unstable_dynamicStaleTime"),
        "page dynamic stale time must be public: {entries:?}"
    );
    assert!(
        !entries.contains(&"app/layout.tsx#unusedLayoutHelper"),
        "an arbitrary layout export must not become public: {entries:?}"
    );
    assert!(
        !entries.contains(&"app/page.tsx#unusedPageHelper"),
        "an arbitrary page export must not become public: {entries:?}"
    );
    for key in [
        "app/layout.tsx#unusedLayoutHelper",
        "app/page.tsx#unusedPageHelper",
    ] {
        assert!(
            snapshot.candidates.iter().any(|candidate| candidate == key),
            "{key} should remain a candidate: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn next_intl_conventions_require_the_dependency() {
    let with_dependency = snapshot(&[
        (
            "package.json",
            r#"{"name":"app","dependencies":{"next":"^16.0.0","next-intl":"^4.0.0"}}"#,
        ),
        (
            "src/i18n/request.ts",
            "export function getRequestConfig() { return 1; }\nexport function unusedRequestHelper() { return 2; }\n",
        ),
        (
            "i18n/routing.ts",
            "export const routing = { locales: ['en'] };\nexport const unusedRoutingHelper = 1;\n",
        ),
        ("components/dead.ts", "export const deadThing = 1;\n"),
    ]);
    let entries = entry_symbols(&with_dependency);
    for key in [
        "src/i18n/request.ts#getRequestConfig",
        "src/i18n/request.ts#unusedRequestHelper",
        "i18n/routing.ts#routing",
        "i18n/routing.ts#unusedRoutingHelper",
    ] {
        assert!(
            entries.contains(&key),
            "{key} must be an entry: {entries:?}"
        );
    }
    assert!(
        with_dependency
            .candidates
            .iter()
            .any(|key| key == "components/dead.ts#deadThing"),
        "unrelated dead code must remain a candidate: {:?}",
        with_dependency.candidates
    );

    let without_dependency = snapshot(&[
        ("package.json", r#"{"name":"app"}"#),
        (
            "i18n/request.ts",
            "export function unusedRequestHelper() { return 2; }\n",
        ),
    ]);
    assert!(
        without_dependency
            .candidates
            .iter()
            .any(|key| key == "i18n/request.ts#unusedRequestHelper"),
        "the convention must not apply without next-intl: {:?}",
        without_dependency.candidates
    );
}

#[test]
fn dotted_page_extensions_keep_next_routes_live() {
    let snapshot = discovered_snapshot(&[
        (
            "package.json",
            r#"{"name":"next-app","dependencies":{"next":"^16.4.2","react":"^19.1.0"}}"#,
        ),
        (
            "next.config.ts",
            "const config = { pageExtensions: ['page.tsx'] };\nexport default config;\n",
        ),
        (
            "app/page.page.tsx",
            "import { Widget } from '../components/widget';\n\
                 export default function Page() { return <Widget />; }\n\
                 export const unusedPageHelper = 1;\n",
        ),
        (
            "app/layout.page.tsx",
            "export default function Layout() { return null; }\n\
                 export const unusedLayoutHelper = 2;\n",
        ),
        (
            "app/route.page.tsx",
            "export function GET() { return new Response('ok'); }\n\
                 export const unusedRouteHelper = 3;\n",
        ),
        (
            "components/widget.tsx",
            "export function Widget() { return <div />; }\n\
                 export const unusedWidgetHelper = 4;\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    for key in [
        "app/page.page.tsx#default",
        "app/layout.page.tsx#default",
        "app/route.page.tsx#GET",
    ] {
        assert!(
            entries.contains(&key),
            "the configured dotted extension is one extension, not a new stem: {key} missing from {entries:?}"
        );
    }
    assert!(
        !snapshot.is_candidate("components/widget.tsx#Widget"),
        "a component imported by a dotted-extension route stays live: {:?}",
        snapshot.candidates
    );
    for key in [
        "app/page.page.tsx#unusedPageHelper",
        "app/layout.page.tsx#unusedLayoutHelper",
        "app/route.page.tsx#unusedRouteHelper",
        "components/widget.tsx#unusedWidgetHelper",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "only genuinely unused exports are candidates: {key} missing from {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn next_page_extensions_make_markdown_routes_live() {
    let snapshot = discovered_snapshot(&next_page_extensions_fixture(true));
    let files: Vec<&str> = snapshot
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    for path in ["app/page.md", "pages/about.md"] {
        assert!(
            files.contains(&path),
            "{path} must be inventoried: {files:?}"
        );
    }
    assert!(
        !files.contains(&"app/blog/post.md"),
        "only App Router convention stems become routes: {files:?}"
    );
    let entries = entry_symbols(&snapshot);
    for key in ["app/page.md#__module__", "pages/about.md#__module__"] {
        assert!(
            entries.contains(&key),
            "{key} must be a route entry: {entries:?}"
        );
    }
    for key in [
        "components/header.tsx#Header",
        "components/footer.tsx#Footer",
    ] {
        assert!(
            snapshot.is_live(key),
            "{key} is rendered by a Markdown route and must be live: {:?}",
            snapshot.live
        );
    }
    for key in [
        "components/header.tsx#unusedHeaderHelper",
        "components/dead-card.tsx#DeadCard",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "{key} is unused and must remain a candidate: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn markdown_routes_require_page_extensions() {
    let snapshot = discovered_snapshot(&next_page_extensions_fixture(false));
    let files: Vec<&str> = snapshot
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert!(
        !files.iter().any(|path| path.ends_with(".md")),
        "no Markdown may be inventoried without pageExtensions: {files:?}"
    );
    for key in [
        "components/header.tsx#Header",
        "components/footer.tsx#Footer",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "{key} must not be protected without a Markdown route: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn next_config_referenced_modules_are_entries() {
    let snapshot = snapshot(&[
        (
            "next.config.ts",
            "// './lib/commented.ts' is documentation, not a loaded module\nconst config = {\n  images: { loaderFile: './image-loader.ts' },\n  cacheHandler: './cache-handler.ts',\n  turbopack: { rules: { '*.svg': { loaders: ['./loaders/svg.ts'] } } },\n};\nexport default config;\n",
        ),
        (
            "image-loader.ts",
            "export default function imageLoader() { return ''; }\n",
        ),
        ("cache-handler.ts", "export default class CacheHandler {}\n"),
        (
            "loaders/svg.ts",
            "export default function svgLoader() {}\nexport const raw = true;\n",
        ),
        (
            "lib/commented.ts",
            "export function commentedReference() { return 1; }\n",
        ),
        ("lib/dead.ts", "export function deadThing() { return 1; }\n"),
    ]);
    let entries = entry_symbols(&snapshot);
    for key in [
        "image-loader.ts#default",
        "cache-handler.ts#default",
        "loaders/svg.ts#default",
        "loaders/svg.ts#raw",
    ] {
        assert!(
            entries.contains(&key),
            "next.config reference {key} must be public: {entries:?}"
        );
        assert!(
            !snapshot.candidates.iter().any(|candidate| candidate == key),
            "next.config reference {key} must not be a candidate: {:?}",
            snapshot.candidates
        );
    }
    for key in [
        "lib/commented.ts#commentedReference",
        "lib/dead.ts#deadThing",
    ] {
        assert!(
            snapshot.candidates.iter().any(|candidate| candidate == key),
            "{key} must remain a candidate: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn mdx_component_map_requires_next_mdx_dependency() {
    let map =
        "export function useMDXComponents() { return {}; }\nexport const unusedMdxHelper = 1;\n";
    let plain = snapshot(&[
        (
            "package.json",
            r#"{"name":"next-app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        ("mdx-components.tsx", map),
    ]);
    assert!(
        plain.is_candidate("mdx-components.tsx#useMDXComponents"),
        "Next core never loads the component map: {:?}",
        plain.candidates
    );
    assert!(
        plain.is_candidate("mdx-components.tsx#unusedMdxHelper"),
        "{:?}",
        plain.candidates
    );

    let mdx = snapshot(&[
        (
            "package.json",
            r#"{"name":"next-app","dependencies":{"next":"^16.0.0","@next/mdx":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        ("mdx-components.tsx", map),
    ]);
    assert!(
        !mdx.is_candidate("mdx-components.tsx#useMDXComponents"),
        "@next/mdx aliases the component map: {:?}",
        mdx.candidates
    );
    assert!(
        mdx.is_candidate("mdx-components.tsx#unusedMdxHelper"),
        "only the framework API is protected: {:?}",
        mdx.candidates
    );

    // An incomplete tree has no manifest to consult; keep the convention.
    let unknown = snapshot(&[("mdx-components.tsx", map)]);
    assert!(
        !unknown.is_candidate("mdx-components.tsx#useMDXComponents"),
        "a missing manifest fails open: {:?}",
        unknown.candidates
    );

    // A workspace can hoist the MDX plugin to the root while its apps only
    // declare `next`. Keep the map live rather than reporting a false orphan.
    let hoisted = snapshot(&[
        (
            "package.json",
            r#"{"name":"workspace","devDependencies":{"@next/mdx":"^16.0.0"}}"#,
        ),
        (
            "apps/web/package.json",
            r#"{"name":"web","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        ("apps/web/mdx-components.tsx", map),
    ]);
    assert!(
        !hoisted.is_candidate("apps/web/mdx-components.tsx#useMDXComponents"),
        "a hoisted @next/mdx keeps the component map live: {:?}",
        hoisted.candidates
    );
}
