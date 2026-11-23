//! Tests for expo entry detection.

use super::*;

#[test]
fn unused_test_exports_are_candidates_but_framework_exports_are_entries() {
    let snapshot = snapshot(&[
        (
            "tests/helper.ts",
            "export const usedHelper = 1;\nexport const unusedHelper = 2;\n",
        ),
        (
            "tests/use-helper.test.ts",
            "import { usedHelper } from './helper';\ntest('helper', () => { usedHelper; });\n",
        ),
        (
            "src/Button.stories.tsx",
            "export default { title: 'Button' };\nexport const Primary = {};\n",
        ),
        ("__mocks__/client.ts", "export const mockValue = 1;\n"),
        (
            "tests/types.test-d.ts",
            "export type Assertion = { value: number };\nexport const fixture: Assertion = { value: 1 };\n",
        ),
        (
            "__typetest__/assertions.ts",
            "export type Expect<T extends true> = T;\nexport type Equal<A, B> = A extends B ? true : false;\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(
        snapshot.is_candidate("tests/helper.ts#unusedHelper"),
        "an unreferenced test helper is dead: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_live("tests/helper.ts#usedHelper"),
        "an imported test helper stays live: {:?}",
        snapshot.live
    );
    for key in [
        "src/Button.stories.tsx#Primary",
        "src/Button.stories.tsx#default",
        "__mocks__/client.ts#mockValue",
        "tests/types.test-d.ts#Assertion",
        "tests/types.test-d.ts#fixture",
        "__typetest__/assertions.ts#Expect",
        "__typetest__/assertions.ts#Equal",
    ] {
        assert!(entries.contains(&key), "{key}: {entries:?}");
    }
}

#[test]
fn standalone_script_exports_follow_the_graph() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"app","scripts":{"build":"tsx scripts/build.ts"}}"#,
        ),
        (
            "scripts/build.ts",
            "export function usedByScript() { return 1; }\n\
                 export function unusedExport() { return 2; }\n\
                 usedByScript();\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(
        entries.contains(&"scripts/build.ts#__module__"),
        "a package script names the module: {entries:?}"
    );
    assert!(
        snapshot.is_live("scripts/build.ts#usedByScript"),
        "a helper called by the script stays live: {:?}",
        snapshot.live
    );
    assert!(
        snapshot.is_candidate("scripts/build.ts#unusedExport"),
        "an unreferenced script export is dead: {:?}",
        snapshot.candidates
    );
}

#[test]
fn config_file_exports_are_entries() {
    let snapshot = snapshot(&[
        ("src/index.ts", "export const live = 1;\n"),
        (
            "rollup.config.mjs",
            "export const external = ['react'];\nexport default { external };\n",
        ),
        ("eslint.config.mjs", "export default [];\n"),
        (
            "shrike.config.ts",
            "const config = {};\nexport default config;\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(
        entries.contains(&"rollup.config.mjs#external"),
        "{entries:?}"
    );
    assert!(
        entries.contains(&"rollup.config.mjs#default"),
        "{entries:?}"
    );
    assert!(
        entries.contains(&"eslint.config.mjs#default"),
        "{entries:?}"
    );
    assert!(entries.contains(&"shrike.config.ts#default"), "{entries:?}");
}

#[test]
fn unimported_client_boundary_exports_remain_candidates_by_default() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "export default function Page() { return null; }\n",
        ),
        (
            "src/client.ts",
            "\"use client\";\nexport function unusedClientExport() { return 1; }\n",
        ),
    ]);
    assert!(
        snapshot.is_candidate("src/client.ts#unusedClientExport"),
        "the client-boundary policy must still govern use client: {:?}",
        snapshot.candidates
    );
}

#[test]
fn invalid_client_exports_are_build_errors_in_client_routes() {
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
            "app/page.tsx",
            "\"use client\";\nexport const metadata = { title: \"page\" };\nexport const revalidate = 60;\nexport default function Page() { return null; }\n",
        ),
        (
            "app/layout.tsx",
            "\"use client\";\nexport const viewport = { width: \"device-width\" };\nexport function generateMetadata() { return { title: \"layout\" }; }\nexport default function Layout({ children }: { children: React.ReactNode }) { return <>{children}</>; }\n",
        ),
        (
            "app/api/route.ts",
            "\"use client\";\nexport const metadata = { title: \"api\" };\nexport const viewport = { width: \"device-width\" };\nexport const revalidate = 60;\nexport function GET() { return new Response(\"ok\"); }\n",
        ),
        (
            "app/aliased/page.tsx",
            "\"use client\";\nconst metadata = { title: \"x\" };\nexport { metadata };\nexport default function Page() { return null; }\n",
        ),
        (
            "app/renamed/page.tsx",
            "\"use client\";\nconst metadata = { title: \"x\" };\nexport { metadata as config };\nexport default function Page() { return null; }\n",
        ),
        (
            "app/direct/page.tsx",
            "\"use client\";\nexport const metadata = { title: \"x\" };\nexport { metadata as config };\nexport default function Page() { return null; }\n",
        ),
        (
            "app/type-specifier/page.tsx",
            "\"use client\";\ntype metadata = { title: string };\nexport type { metadata };\nexport default function Page() { return null; }\n",
        ),
        (
            "app/type-decl/page.tsx",
            "\"use client\";\nexport interface metadata { title: string }\nexport default function Page() { return null; }\n",
        ),
        (
            "app/reexport/page.tsx",
            "\"use client\";\nexport { metadata } from \"./server\";\nexport default function Page() { return null; }\n",
        ),
        (
            "app/reexport/server.ts",
            "export const metadata = { title: \"x\" };\n",
        ),
        (
            "app/star/page.tsx",
            "\"use client\";\nexport * as metadata from \"./server\";\nexport default function Page() { return null; }\n",
        ),
        (
            "app/star/server.ts",
            "export const metadata = { title: \"x\" };\n",
        ),
        (
            "app/non-failing/page.tsx",
            "\"use client\";\nexport const dynamic = \"force-static\";\nexport const dynamicParams = false;\nexport const fetchCache = \"force-cache\";\nexport const runtime = \"edge\";\nexport const preferredRegion = \"home\";\nexport const maxDuration = 10;\nexport function GET() { return new Response(\"ok\"); }\nexport default function Page() { return null; }\n",
        ),
        (
            "app/template.tsx",
            "\"use client\";\nexport const metadata = { title: \"template\" };\nexport default function Template({ children }: { children: React.ReactNode }) { return <>{children}</>; }\n",
        ),
        (
            "app/not-found.tsx",
            "\"use client\";\nexport const metadata = { title: \"missing\" };\nexport default function NotFound() { return null; }\n",
        ),
        (
            "app/widget.tsx",
            "\"use client\";\nexport const metadata = { title: \"widget\" };\nexport default function Widget() { return null; }\n",
        ),
        (
            "packages/plain/app/page.tsx",
            "\"use client\";\nexport const metadata = { title: \"plain\" };\nexport default function Page() { return null; }\n",
        ),
        (
            "app/server/page.tsx",
            "export const metadata = { title: \"server\" };\nexport default function Page() { return null; }\n",
        ),
    ]);
    let invalid: Vec<(&str, u32)> = next_snapshot
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "invalid_client_export")
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
            ("app/aliased/page.tsx", 3),
            ("app/api/route.ts", 2),
            ("app/api/route.ts", 4),
            ("app/direct/page.tsx", 2),
            ("app/layout.tsx", 2),
            ("app/layout.tsx", 3),
            ("app/page.tsx", 2),
            ("app/page.tsx", 3),
            ("app/reexport/page.tsx", 2),
            ("app/star/page.tsx", 2),
            ("app/type-specifier/page.tsx", 3),
        ],
        "{:?}",
        next_snapshot.diagnostics
    );
    assert!(
        next_snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "invalid_client_export")
            .all(|diagnostic| {
                diagnostic.severity == shrike_core::Severity::Error
                    && diagnostic.message.contains("fails the build")
            }),
        "invalid client exports are build errors: {:?}",
        next_snapshot.diagnostics
    );

    // A project without Next.js, and a tree with no manifest at all, are
    // not enough evidence to claim a Next.js build failure.
    let react_only = snapshot(&[
        (
            "package.json",
            r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
        ),
        (
            "app/page.tsx",
            "\"use client\";\nexport const metadata = { title: \"x\" };\nexport default function Page() { return null; }\n",
        ),
    ]);
    let missing_manifest = snapshot(&[(
        "app/page.tsx",
        "\"use client\";\nexport const metadata = { title: \"x\" };\nexport default function Page() { return null; }\n",
    )]);
    for control in [&react_only, &missing_manifest] {
        assert!(
            control
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "invalid_client_export"),
            "no positive Next.js evidence: {:?}",
            control.diagnostics
        );
    }
}

#[test]
fn invalid_server_exports_are_project_gated_warnings() {
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
            "app/actions.ts",
            "\"use server\";\nexport const count = 1;\nexport const action = async () => 2;\nexport function declared() { return 3; }\nexport class Direct {}\n",
        ),
        (
            "src/values.ts",
            "\"use server\";\nconst local = \"text\";\nexport { local as alias };\nexport enum Kind { A }\n",
        ),
        ("src/default.ts", "\"use server\";\nexport default 42;\n"),
        (
            "src/mixed.ts",
            "\"use server\";\nexport const wrapped = (1 as number);\nexport const later = makeAction();\nexport const maybe = someIdentifier;\nexport default class {}\nexport { remote } from \"./remote\";\nexport type Payload = string;\n",
        ),
        ("src/no-directive.ts", "export const count = 1;\n"),
        (
            "packages/plain/src/values.ts",
            "\"use server\";\nexport const count = 1;\n",
        ),
    ]);
    let mut invalid: Vec<(&str, u32)> = next_snapshot
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "invalid_server_export")
        .map(|diagnostic| {
            (
                diagnostic.file.as_deref().unwrap_or(""),
                diagnostic.span.map_or(0, |span| span.line),
            )
        })
        .collect();
    invalid.sort_unstable();
    assert_eq!(
        invalid,
        vec![
            ("app/actions.ts", 2),
            // A shared package inside an RSC project is compiled in the
            // consuming app's context, so the project-level gate applies.
            ("packages/plain/src/values.ts", 2),
            ("src/default.ts", 2),
            ("src/mixed.ts", 2),
            // The local alias is anchored on its export specifier, the enum
            // on its declaration.
            ("src/values.ts", 3),
            ("src/values.ts", 4),
        ],
        "{:?}",
        next_snapshot.diagnostics
    );
    assert!(
        next_snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "invalid_server_export")
            .all(|diagnostic| {
                diagnostic.severity == shrike_core::Severity::Warning
                    && diagnostic.message.contains("async function")
                    && diagnostic.message.contains("\"use server\"")
            }),
        "invalid server exports are warnings that name the boundary: {:?}",
        next_snapshot.diagnostics
    );

    // A project without an RSC bundler, and a tree with no manifest at
    // all, are not enough evidence to claim a Next.js build failure.
    let react_only = snapshot(&[
        (
            "package.json",
            r#"{"name":"plain","private":true,"dependencies":{"react":"^19.0.0"}}"#,
        ),
        (
            "app/actions.ts",
            "\"use server\";\nexport const count = 1;\n",
        ),
    ]);
    let missing_manifest = snapshot(&[(
        "app/actions.ts",
        "\"use server\";\nexport const count = 1;\n",
    )]);
    for control in [&react_only, &missing_manifest] {
        assert!(
            control
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "invalid_server_export"),
            "no positive RSC bundler evidence: {:?}",
            control.diagnostics
        );
    }
}

#[test]
fn library_build_entry_maps_to_source_exports() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"@corpus/is","exports":{"types":"./distribution/index.d.ts","default":"./distribution/index.js"}}"#,
        ),
        (
            "source/index.ts",
            "export function isString(value: unknown) { return typeof value === 'string'; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(entries.contains(&"source/index.ts#isString"), "{entries:?}");
    assert!(
        entries.contains(&"source/index.ts#__module__"),
        "{entries:?}"
    );
}

#[test]
fn next_param_matching_exports_are_public_on_pages_and_layouts() {
    let snapshot = snapshot(&[
        (
            "app/layout.tsx",
            "export const unstable_paramMatching = { id: 'dynamic' };\nexport function unstable_generateParamMatching() { return {}; }\nexport function unusedLayoutHelper() { return 1; }\nexport default function Layout() { return null; }\n",
        ),
        (
            "app/page.tsx",
            "export const unstable_paramMatching = { id: 'dynamic' };\nexport function unstable_generateParamMatching() { return {}; }\nexport function unusedPageHelper() { return 1; }\nexport default function Page() { return null; }\n",
        ),
        (
            "app/route.ts",
            "export const unstable_paramMatching = { id: 'dynamic' };\nexport function unstable_generateParamMatching() { return {}; }\nexport function GET() { return new Response('ok'); }\n",
        ),
        (
            "app/loading.tsx",
            "export const unstable_paramMatching = { id: 'dynamic' };\nexport function unstable_generateParamMatching() { return {}; }\nexport default function Loading() { return null; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    for file in ["app/layout.tsx", "app/page.tsx"] {
        for name in ["unstable_paramMatching", "unstable_generateParamMatching"] {
            let key = format!("{file}#{name}");
            assert!(
                entries.contains(&key.as_str()),
                "Next.js reads {name} from {file}: {entries:?}"
            );
        }
    }
    for file in ["app/route.ts", "app/loading.tsx"] {
        for name in ["unstable_paramMatching", "unstable_generateParamMatching"] {
            let key = format!("{file}#{name}");
            assert!(
                !entries.contains(&key.as_str()),
                "{name} is not a Next.js export on {file}: {entries:?}"
            );
            assert!(
                snapshot
                    .candidates
                    .iter()
                    .any(|candidate| candidate == &key),
                "{key} should remain a candidate: {:?}",
                snapshot.candidates
            );
        }
    }
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
fn next_framework_exports_are_scoped_to_each_file_type() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"app","dependencies":{"next":"^16.0.0","react":"^19.0.0","@next/mdx":"^16.0.0"}}"#,
        ),
        (
            "app/page.tsx",
            "export const metadata = {};\nexport async function generateMetadata() { return {}; }\nexport const getStaticProps = async () => ({ props: {} });\nexport function Head() { return null; }\nexport const pageQuery = 'q';\nexport function getInitialProps() { return {}; }\nexport function middleware() { return null; }\nexport function register() { return null; }\nexport function GET() { return null; }\nexport const config = {};\nexport const instant = 'auto';\nexport const prefetch = 'auto';\nexport const ensureStatic = 'auto';\nexport function unusedPageHelper() { return 1; }\nexport default function Page() { return null; }\n",
        ),
        (
            "app/layout.tsx",
            "export const preferredRegion = 'syd1';\nexport const unstable_dynamicStaleTime = 5;\nexport const instant = 'auto';\nexport const prefetch = 'auto';\nexport const ensureStatic = 'auto';\nexport const config = {};\nexport function unusedLayoutHelper() { return 1; }\nexport default function Layout() { return null; }\n",
        ),
        (
            "app/route.ts",
            "export async function GET() { return new Response('ok'); }\nexport const metadata = {};\nexport const config = {};\nexport const dynamicParams = true;\nexport const instant = 'auto';\nexport const prefetch = 'auto';\nexport const ensureStatic = 'auto';\nexport default function handler() { return null; }\nexport const generateStaticParams = () => [];\nexport function unusedRouteHelper() { return 1; }\n",
        ),
        (
            "app/not-found.tsx",
            "export const metadata = { title: 'missing' };\nexport const generateStaticParams = () => [];\nexport default function NotFound() { return null; }\n",
        ),
        (
            "app/global-not-found.tsx",
            "export const metadata = { title: 'missing' };\nexport default function GlobalNotFound() { return null; }\n",
        ),
        (
            "app/loading.tsx",
            "export const metadata = {};\nexport const config = {};\nexport default function Loading() { return null; }\n",
        ),
        (
            "app/sitemap.ts",
            "export function generateSitemaps() { return []; }\nexport const metadata = {};\nexport const instant = 'auto';\nexport const prefetch = 'auto';\nexport const ensureStatic = 'auto';\nexport function unusedSitemapHelper() { return 1; }\nexport default function sitemap() { return []; }\n",
        ),
        (
            "app/icon.tsx",
            "export const size = { width: 32, height: 32 };\nexport const contentType = 'image/png';\nexport const alt = 'icon';\nexport function generateImageMetadata() { return []; }\nexport default function Icon() { return null; }\n",
        ),
        (
            "app/opengraph-image.tsx",
            "export const alt = 'og';\nexport const size = { width: 1200, height: 630 };\nexport const contentType = 'image/png';\nexport function generateImageMetadata() { return []; }\nexport default function Image() { return null; }\n",
        ),
        (
            "pages/index.tsx",
            "export async function getStaticProps() { return { props: {} }; }\nexport async function getServerSideProps() { return { props: {} }; }\nexport async function getStaticPaths() { return { paths: [], fallback: false }; }\nexport const config = { runtime: 'edge' };\nexport const runtime = 'edge';\nexport const maxDuration = 10;\nexport const metadata = {};\nexport const dynamic = 'force-static';\nexport function GET() { return null; }\nexport function Head() { return null; }\nexport const pageQuery = 'q';\nexport function getInitialProps() { return {}; }\nexport function unusedPagesHelper() { return 1; }\nexport default function Page() { return null; }\n",
        ),
        (
            "pages/_app.tsx",
            "export function reportWebVitals() {}\nexport async function getStaticProps() { return { props: {} }; }\nexport default function App() { return null; }\n",
        ),
        (
            "pages/api/hello.ts",
            "export const config = { runtime: 'edge' };\nexport async function getStaticProps() { return { props: {} }; }\nexport function GET() { return null; }\nexport default function handler() { return null; }\n",
        ),
        (
            "pages/api.ts",
            "export const config = { runtime: 'edge' };\nexport async function getStaticProps() { return { props: {} }; }\nexport default function handler() { return null; }\n",
        ),
        (
            "pages/blog/_app.tsx",
            "export function reportWebVitals() {}\nexport async function getStaticProps() { return { props: {} }; }\nexport default function Page() { return null; }\n",
        ),
        (
            "pages/_document.tsx",
            "export const config = { runtime: 'edge' };\nexport async function getStaticProps() { return { props: {} }; }\nexport default function Document() { return null; }\n",
        ),
        (
            "middleware.ts",
            "export function middleware() {}\nexport const config = {};\nexport function register() {}\nexport const metadata = {};\n",
        ),
        (
            "proxy.ts",
            "export function proxy() {}\nexport const config = {};\nexport function middleware() {}\n",
        ),
        (
            "instrumentation.ts",
            "export function register() {}\nexport function onRequestError() {}\nexport function onRequestInit() {}\nexport default function init() {}\n",
        ),
        (
            "instrumentation-client.ts",
            "export function onRouterTransitionStart() {}\nexport function register() {}\n",
        ),
        (
            "mdx-components.tsx",
            "export function useMDXComponents() { return {}; }\nexport const metadata = {};\nexport default function Components() { return null; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    for key in [
        "app/page.tsx#default",
        "app/page.tsx#metadata",
        "app/page.tsx#generateMetadata",
        "app/page.tsx#instant",
        "app/page.tsx#prefetch",
        "app/page.tsx#ensureStatic",
        "app/layout.tsx#default",
        "app/layout.tsx#preferredRegion",
        "app/layout.tsx#instant",
        "app/layout.tsx#prefetch",
        "app/layout.tsx#ensureStatic",
        "app/route.ts#GET",
        "app/route.ts#generateStaticParams",
        "app/route.ts#dynamicParams",
        "app/not-found.tsx#default",
        "app/not-found.tsx#metadata",
        "app/global-not-found.tsx#default",
        "app/loading.tsx#default",
        "app/sitemap.ts#default",
        "app/sitemap.ts#generateSitemaps",
        "app/icon.tsx#default",
        "app/icon.tsx#size",
        "app/icon.tsx#contentType",
        "app/icon.tsx#alt",
        "app/icon.tsx#generateImageMetadata",
        "app/opengraph-image.tsx#default",
        "app/opengraph-image.tsx#alt",
        "pages/index.tsx#default",
        "pages/index.tsx#getStaticProps",
        "pages/index.tsx#getServerSideProps",
        "pages/index.tsx#getStaticPaths",
        "pages/index.tsx#config",
        "pages/index.tsx#runtime",
        "pages/index.tsx#maxDuration",
        "pages/_app.tsx#default",
        "pages/_app.tsx#reportWebVitals",
        "pages/api/hello.ts#default",
        "pages/api/hello.ts#config",
        "pages/api.ts#default",
        "pages/api.ts#config",
        "pages/blog/_app.tsx#default",
        "pages/blog/_app.tsx#getStaticProps",
        "pages/_document.tsx#default",
        "pages/_document.tsx#config",
        "middleware.ts#middleware",
        "middleware.ts#config",
        "proxy.ts#proxy",
        "proxy.ts#config",
        "instrumentation.ts#register",
        "instrumentation.ts#onRequestError",
        "instrumentation-client.ts#onRouterTransitionStart",
        "mdx-components.tsx#useMDXComponents",
    ] {
        assert!(
            entries.contains(&key),
            "{key} is framework API and must be an entry: {entries:?}"
        );
    }
    for key in [
        "app/page.tsx#getStaticProps",
        "app/page.tsx#Head",
        "app/page.tsx#pageQuery",
        "app/page.tsx#getInitialProps",
        "app/page.tsx#middleware",
        "app/page.tsx#register",
        "app/page.tsx#GET",
        "app/page.tsx#config",
        "app/page.tsx#unusedPageHelper",
        "app/layout.tsx#unstable_dynamicStaleTime",
        "app/layout.tsx#config",
        "app/layout.tsx#unusedLayoutHelper",
        "app/route.ts#metadata",
        "app/route.ts#config",
        "app/route.ts#default",
        "app/route.ts#instant",
        "app/route.ts#prefetch",
        "app/route.ts#ensureStatic",
        "app/route.ts#unusedRouteHelper",
        "app/not-found.tsx#generateStaticParams",
        "app/global-not-found.tsx#metadata",
        "app/loading.tsx#metadata",
        "app/loading.tsx#config",
        "app/sitemap.ts#metadata",
        "app/sitemap.ts#instant",
        "app/sitemap.ts#prefetch",
        "app/sitemap.ts#ensureStatic",
        "app/sitemap.ts#unusedSitemapHelper",
        "pages/index.tsx#metadata",
        "pages/index.tsx#dynamic",
        "pages/index.tsx#GET",
        "pages/index.tsx#Head",
        "pages/index.tsx#pageQuery",
        "pages/index.tsx#getInitialProps",
        "pages/index.tsx#unusedPagesHelper",
        "pages/_app.tsx#getStaticProps",
        "pages/api/hello.ts#getStaticProps",
        "pages/api/hello.ts#GET",
        "pages/api.ts#getStaticProps",
        "pages/blog/_app.tsx#reportWebVitals",
        "pages/_document.tsx#getStaticProps",
        "middleware.ts#register",
        "middleware.ts#metadata",
        "proxy.ts#middleware",
        "instrumentation.ts#onRequestInit",
        "instrumentation.ts#default",
        "instrumentation-client.ts#register",
        "mdx-components.tsx#metadata",
        "mdx-components.tsx#default",
    ] {
        assert!(
            !entries.contains(&key),
            "{key} is not read by Next.js and must stay a candidate: {entries:?}"
        );
        assert!(
            snapshot.candidates.iter().any(|candidate| candidate == key),
            "{key} should remain a candidate: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn expo_router_root_is_read_from_app_config() {
    assert_eq!(
        expo_router_root(r#"{"expo":{"plugins":[["expo-router",{"root":"./src/routes"}]]}}"#)
            .as_deref(),
        Some("src/routes")
    );
    assert_eq!(
        expo_router_root(
            "export default { expo: { plugins: [['expo-router', { root: 'src/features' }]] } };"
        )
        .as_deref(),
        Some("src/features")
    );
    assert_eq!(
        expo_router_root(
            "export default { expo: { plugins: [['expo-router', { root: someValue }]] } };"
        ),
        None
    );
    assert_eq!(
        expo_router_root(r#"{"expo":{"plugins":["expo-router"]}}"#),
        None
    );
}

#[test]
fn expo_config_plugin_paths_are_static_only() {
    assert_eq!(
        expo_config_plugin_paths(
            r#"{"expo":{"plugins":["./plugins/with-thing",["expo-router",{"root":"./app"}]]}}"#
        ),
        vec!["./plugins/with-thing"]
    );
    assert_eq!(
        expo_config_plugin_paths(
            "const plugins = ['./plugins/a', ['expo-router', { root: './app' }]];\nexport default { plugins };\n"
        ),
        vec!["./plugins/a"]
    );
    assert_eq!(
        expo_config_plugin_paths("export default { plugins: [require.resolve('./plugins/b')] };\n"),
        vec!["./plugins/b"]
    );
    assert_eq!(
        expo_config_plugin_paths("// plugins: ['./commented']\nexport default { name: 'x' };\n"),
        Vec::<String>::new()
    );
    assert_eq!(
        expo_config_plugin_paths(
            "export default { name: './plugins/not-a-plugin', plugins: [someValue] };\n"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn expo_router_custom_root_routes_are_entries() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"expo-app","main":"expo-router/entry","dependencies":{"expo":"^54.0.0","expo-router":"^6.0.0"}}"#,
        ),
        (
            "app.config.ts",
            "export default { expo: { plugins: [['expo-router', { root: './src/routes' }]] } };\n",
        ),
        (
            "src/routes/_layout.tsx",
            "import { Stack } from 'expo-router';\nexport const unstable_settings = { initialRouteName: 'index' };\nexport default function Layout() { return <Stack />; }\nexport function unusedLayoutHelper() { return 1; }\n",
        ),
        (
            "src/routes/index.tsx",
            "export default function Home() { return null; }\nexport function unusedHomeHelper() { return 1; }\n",
        ),
        (
            "src/routes/user/[id].tsx",
            "export default function User() { return null; }\nexport function unusedUserHelper() { return 1; }\n",
        ),
        (
            "app/dead.tsx",
            "export function deadOutsideRoot() { return 1; }\n",
        ),
        (
            "src/components/dead.tsx",
            "export function deadThing() { return 1; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    for key in [
        "src/routes/_layout.tsx#default",
        "src/routes/_layout.tsx#unstable_settings",
        "src/routes/index.tsx#default",
        "src/routes/user/[id].tsx#default",
    ] {
        assert!(
            entries.contains(&key),
            "custom Expo root entry {key} must be public: {entries:?}"
        );
        assert!(
            !snapshot.candidates.iter().any(|candidate| candidate == key),
            "custom Expo root entry {key} must not be a candidate: {:?}",
            snapshot.candidates
        );
    }
    for key in [
        "src/routes/_layout.tsx#unusedLayoutHelper",
        "src/routes/index.tsx#unusedHomeHelper",
        "src/routes/user/[id].tsx#unusedUserHelper",
        "app/dead.tsx#deadOutsideRoot",
        "src/components/dead.tsx#deadThing",
    ] {
        assert!(
            snapshot.candidates.iter().any(|candidate| candidate == key),
            "{key} must remain a candidate: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn expo_config_plugins_are_entries() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"expo-plugin-app","main":"expo-router/entry","dependencies":{"expo":"^54.0.0","expo-router":"^6.0.0","react":"^19.0.0","react-native":"^0.81.0"}}"#,
        ),
        (
            "app.json",
            r#"{"expo":{"plugins":["./plugins/with-thing",["expo-router",{"root":"./app"}]]}}"#,
        ),
        (
            "app/_layout.tsx",
            "import { Stack } from 'expo-router';\nexport default function Layout() { return <Stack />; }\n",
        ),
        (
            "app/index.tsx",
            "export default function Index() { return null; }\n",
        ),
        (
            "plugins/with-thing.ts",
            "export default function withThing(config: object) { return config; }\nexport function helper() { return true; }\nexport function unusedPluginExport() { return 1; }\n",
        ),
        ("src/dead.ts", "export function deadThing() { return 1; }\n"),
    ]);
    let entries = entry_symbols(&snapshot);
    for key in [
        "plugins/with-thing.ts#__module__",
        "plugins/with-thing.ts#default",
        "plugins/with-thing.ts#helper",
        "plugins/with-thing.ts#unusedPluginExport",
    ] {
        assert!(
            entries.contains(&key),
            "local Expo config plugin surface {key} must be an entry: {entries:?}"
        );
        assert!(
            !snapshot.candidates.iter().any(|candidate| candidate == key),
            "local Expo config plugin surface {key} must not be a candidate: {:?}",
            snapshot.candidates
        );
    }
    assert!(
        snapshot
            .candidates
            .iter()
            .any(|candidate| candidate == "src/dead.ts#deadThing"),
        "an unrelated module must remain a candidate: {:?}",
        snapshot.candidates
    );
}

#[test]
fn expo_api_routes_and_native_intent_exports_are_entries() {
    let snapshot = snapshot(&[
        (
            "app/_layout.tsx",
            "export default function Layout() { return null; }\n",
        ),
        (
            "app/hello+api.ts",
            "export function GET() { return new Response('ok'); }\nexport function POST() { return new Response('ok'); }\nexport function unusedApiHelper() { return 1; }\n",
        ),
        (
            "app/+native-intent.tsx",
            "export function redirectSystemPath({ path }: { path: string }) { return path; }\nexport function legacy_subscribe() {}\nexport function unusedIntentHelper() { return 1; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    for name in ["GET", "POST"] {
        let key = format!("app/hello+api.ts#{name}");
        assert!(
            entries.contains(&key.as_str()),
            "Expo API method {name} must be public: {entries:?}"
        );
    }
    for name in ["redirectSystemPath", "legacy_subscribe"] {
        let key = format!("app/+native-intent.tsx#{name}");
        assert!(
            entries.contains(&key.as_str()),
            "Expo native intent export {name} must be public: {entries:?}"
        );
    }
    for key in [
        "app/hello+api.ts#unusedApiHelper",
        "app/+native-intent.tsx#unusedIntentHelper",
    ] {
        assert!(
            !entries.contains(&key),
            "an arbitrary Expo route export must not become public: {entries:?}"
        );
        assert!(
            snapshot.candidates.iter().any(|candidate| candidate == key),
            "{key} should remain a candidate: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn jsx_namespace_members_keep_the_used_export_live() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "import * as UI from '../components/ui';\nexport default function Page() { return <UI.Button />; }\n",
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
            .any(|key| key == "components/ui.tsx#Button"),
        "the JSX namespace member must stay live: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot
            .candidates
            .iter()
            .any(|key| key == "components/ui.tsx#DeadButton"),
        "an unused namespace export must remain a candidate: {:?}",
        snapshot.candidates
    );
}

#[test]
fn namespace_widening_propagates_through_exported_dependencies() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "import * as UI from '../components/ui';\nconst all = { ...UI };\nexport default function Page() { return <div>{Object.keys(all).length}</div>; }\n",
        ),
        (
            "components/ui.tsx",
            "function leaf() { return 1; }\n\
                 function helper() { return leaf(); }\n\
                 export function Button() { return helper(); }\n\
                 export function DeadButton() { return null; }\n",
        ),
    ]);
    for key in ["components/ui.tsx#helper", "components/ui.tsx#leaf"] {
        assert!(
            !snapshot.candidates.iter().any(|candidate| candidate == key),
            "a live symbol reached through namespace widening must keep its own \
                 dependencies live: {key} in {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn destructured_dynamic_imports_keep_only_named_exports_live() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "import { load } from '../src/loader';\nexport default function Page() { return <div>{load()}</div>; }\n",
        ),
        (
            "src/loader.ts",
            "export async function load() {\n  const { used } = await import('./mod');\n  return used;\n}\n",
        ),
        (
            "src/mod.ts",
            "export const used = 1;\nexport const dead = 2;\n",
        ),
    ]);
    assert!(
        !snapshot
            .candidates
            .iter()
            .any(|key| key == "src/mod.ts#used"),
        "the destructured export must stay live: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot
            .candidates
            .iter()
            .any(|key| key == "src/mod.ts#dead"),
        "an export the destructuring does not read must be a candidate: {:?}",
        snapshot.candidates
    );
}

#[test]
fn promise_all_destructuring_keeps_only_named_exports_live() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "import { load } from '../src/loader';\nexport default function Page() { return <div>{load()}</div>; }\n",
        ),
        (
            "src/loader.ts",
            "export async function load() {\n  const [{ used }, other] = await Promise.all([import('./mod'), import('./other')]);\n  return used + (other ? 1 : 0);\n}\n",
        ),
        (
            "src/mod.ts",
            "export const used = 1;\nexport const dead = 2;\n",
        ),
        (
            "src/other.ts",
            "export const any = 1;\nexport const alsoAny = 2;\n",
        ),
    ]);
    assert!(
        !snapshot
            .candidates
            .iter()
            .any(|key| key == "src/mod.ts#used"),
        "the destructured export must stay live: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot
            .candidates
            .iter()
            .any(|key| key == "src/mod.ts#dead"),
        "an export the destructuring does not read must be a candidate: {:?}",
        snapshot.candidates
    );
    for key in ["src/other.ts#any", "src/other.ts#alsoAny"] {
        assert!(
            !snapshot.candidates.iter().any(|candidate| candidate == key),
            "a namespace binding can be used in unknown ways: {key} {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn member_dynamic_imports_keep_only_the_accessed_export_live() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "import { load } from '../src/loader';\nexport default function Page() { return <div>{load()}</div>; }\n",
        ),
        (
            "src/loader.ts",
            "export async function load() {\n  const used = (await import('./mod')).used;\n  return used;\n}\n",
        ),
        (
            "src/mod.ts",
            "export const used = 1;\nexport const dead = 2;\n",
        ),
    ]);
    assert!(
        !snapshot
            .candidates
            .iter()
            .any(|key| key == "src/mod.ts#used"),
        "{:?}",
        snapshot.candidates
    );
    assert!(
        snapshot
            .candidates
            .iter()
            .any(|key| key == "src/mod.ts#dead"),
        "{:?}",
        snapshot.candidates
    );
}

#[test]
fn then_callback_keeps_only_accessed_exports_live() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "import { load } from '../src/loader';\nexport default function Page() { return <div>{load()}</div>; }\n",
        ),
        (
            "src/loader.ts",
            "export async function load() {\n  const used = await import('./mod').then((module) => module.used);\n  const escaped = await import('./escaped').then((module) => use(module));\n  return used + (escaped ? 1 : 0);\n}\nfunction use(value: unknown) { return value ? 1 : 0; }\n",
        ),
        (
            "src/mod.ts",
            "export const used = 1;\nexport const dead = 2;\n",
        ),
        (
            "src/escaped.ts",
            "export const any = 1;\nexport const alsoAny = 2;\n",
        ),
    ]);
    assert!(
        !snapshot
            .candidates
            .iter()
            .any(|key| key == "src/mod.ts#used"),
        "the member read by the .then callback must stay live: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot
            .candidates
            .iter()
            .any(|key| key == "src/mod.ts#dead"),
        "an export the callback does not read must be a candidate: {:?}",
        snapshot.candidates
    );
    for key in ["src/escaped.ts#any", "src/escaped.ts#alsoAny"] {
        assert!(
            !snapshot.candidates.iter().any(|candidate| candidate == key),
            "a namespace passed as a value can be used in unknown ways: {key} {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn const_namespace_dynamic_import_keeps_only_accessed_exports_live() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "import { load } from '../src/loader';\nexport default function Page() { return <div>{load()}</div>; }\n",
        ),
        (
            "src/loader.ts",
            "export async function load() {\n  const used = await import('./mod');\n  const escaped = await import('./escaped');\n  return used.used + (escaped ? 1 : 0);\n}\nfunction use(value: unknown) { return value ? 1 : 0; }\n",
        ),
        (
            "src/mod.ts",
            "export const used = 1;\nexport const dead = 2;\n",
        ),
        (
            "src/escaped.ts",
            "export const any = 1;\nexport const alsoAny = 2;\n",
        ),
    ]);
    assert!(
        !snapshot
            .candidates
            .iter()
            .any(|key| key == "src/mod.ts#used"),
        "the member read through the const namespace must stay live: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot
            .candidates
            .iter()
            .any(|key| key == "src/mod.ts#dead"),
        "an export the const namespace does not read must be a candidate: {:?}",
        snapshot.candidates
    );
    for key in ["src/escaped.ts#any", "src/escaped.ts#alsoAny"] {
        assert!(
            !snapshot.candidates.iter().any(|candidate| candidate == key),
            "a namespace used as a value can be used in unknown ways: {key} {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn unknown_dynamic_import_usage_keeps_all_exports_live() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "import { load } from '../src/loader';\nexport default function Page() { return <div>{load()}</div>; }\n",
        ),
        (
            "src/loader.ts",
            "export async function load() {\n  const mod = await import('./mod');\n  return mod;\n}\n",
        ),
        (
            "src/mod.ts",
            "export const used = 1;\nexport const other = 2;\n",
        ),
    ]);
    for key in ["src/mod.ts#used", "src/mod.ts#other"] {
        assert!(
            !snapshot.candidates.iter().any(|candidate| candidate == key),
            "a namespace binding can be used in unknown ways: {key} {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn unresolved_dynamic_import_name_keeps_all_exports_live() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "import { load } from '../src/loader';\nexport default function Page() { return <div>{load()}</div>; }\n",
        ),
        (
            "src/loader.ts",
            "export async function load() {\n  const { missing } = await import('./mod');\n  return missing;\n}\n",
        ),
        (
            "src/mod.ts",
            "export const used = 1;\nexport const other = 2;\n",
        ),
    ]);
    for key in ["src/mod.ts#used", "src/mod.ts#other"] {
        assert!(
            !snapshot.candidates.iter().any(|candidate| candidate == key),
            "an unresolved destructured name must widen instead of narrowing: {key} {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn pages_app_report_web_vitals_is_a_framework_export() {
    let snapshot = snapshot(&[(
        "pages/_app.tsx",
        "export default function App() { return null; }\nexport function reportWebVitals() {}\nexport function unusedAppExport() {}\n",
    )]);
    let entries = entry_symbols(&snapshot);
    assert!(
        entries.contains(&"pages/_app.tsx#reportWebVitals"),
        "{entries:?}"
    );
    assert!(
        snapshot
            .candidates
            .iter()
            .any(|key| key == "pages/_app.tsx#unusedAppExport"),
        "an arbitrary _app export must remain a candidate: {:?}",
        snapshot.candidates
    );
}

#[test]
fn exported_symbols_used_only_in_their_own_file_are_reported_separately() {
    let snapshot = snapshot(&[
        (
            "app/page.tsx",
            "import { bootstrap } from '../src/registry';\nexport default function Page() { bootstrap(); return null; }\n",
        ),
        (
            "src/registry.ts",
            "export function helper() { return 1; }\n\
                 export function publicApi() { return helper(); }\n\
                 export function bootstrap() { return { publicApi }; }\n",
        ),
    ]);
    assert!(
        snapshot
            .internal_only
            .iter()
            .any(|key| key == "src/registry.ts#publicApi"),
        "{:?}",
        snapshot.internal_only
    );
    assert!(
        snapshot
            .internal_only
            .iter()
            .any(|key| key == "src/registry.ts#helper"),
        "{:?}",
        snapshot.internal_only
    );
    assert!(
        !snapshot
            .candidates
            .iter()
            .any(|key| key == "src/registry.ts#publicApi"),
        "{:?}",
        snapshot.candidates
    );
}

#[test]
fn imports_from_dead_modules_do_not_make_exports_external() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"internal-dead-import","private":true,"dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "app/page.tsx",
            "import { bootstrap } from '../src/registry';\nexport default function Page() { bootstrap(); return null; }\n",
        ),
        (
            "src/registry.ts",
            "export function helper() { return 1; }\nexport function bootstrap() { return helper(); }\n",
        ),
        (
            "src/dead.ts",
            "import { helper } from './registry';\nexport function deadConsumer() { return helper(); }\n",
        ),
        ("src/barrel.ts", "export { helper } from './registry';\n"),
    ]);
    assert!(
        snapshot.is_candidate("src/dead.ts#deadConsumer"),
        "dead importer must remain a candidate: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot
            .internal_only
            .iter()
            .any(|key| key == "src/registry.ts#helper"),
        "a use from a dead module must not count as external visibility: {:?}",
        snapshot.internal_only
    );
    assert!(
        !snapshot.is_candidate("src/registry.ts#helper"),
        "the helper is still used by live bootstrap code: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate("src/barrel.ts#helper"),
        "the dead alias to the live helper is actionable: {:?}",
        snapshot.candidates
    );
}

#[test]
fn live_external_imports_keep_exports_out_of_internal_only() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"internal-live-import","private":true,"dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "app/page.tsx",
            "import { helper } from '../src/registry';\nexport default function Page() { return helper(); }\n",
        ),
        (
            "src/registry.ts",
            "export function helper() { return 1; }\n",
        ),
    ]);
    assert!(
        snapshot.is_live("src/registry.ts#helper"),
        "a live import must keep the export live"
    );
    assert!(
        !snapshot
            .internal_only
            .iter()
            .any(|key| key == "src/registry.ts#helper"),
        "a live external import is external visibility: {:?}",
        snapshot.internal_only
    );
    assert!(!snapshot.is_candidate("src/registry.ts#helper"));
}

#[test]
fn local_exports_of_imported_bindings_are_alias_candidates() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"local-alias","private":true,"dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "app/page.tsx",
            "import { used } from \"../src/impl\";\n\nexport default function Page() {\n  return used;\n}\n",
        ),
        (
            "src/barrel.ts",
            "import { used as localUsed, dead } from \"./impl\";\nexport { localUsed as used, dead };\n",
        ),
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
    let alias = snapshot
        .symbols
        .iter()
        .find(|symbol| symbol.key == "src/barrel.ts#used")
        .expect("local export of an import must create an alias symbol");
    assert_eq!(alias.kind, "re-export");
    assert!(
        snapshot.is_candidate("src/barrel.ts#used"),
        "unused local alias with a live target is actionable: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate("src/impl.ts#dead"),
        "dead declaration must be reported: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate("src/barrel.ts#dead"),
        "a dead local alias must be reported so its export line is removable: {:?}",
        snapshot.candidates
    );
}

#[test]
fn importing_a_local_export_alias_keeps_the_alias_and_target_live() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"local-alias-live","private":true,"dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "app/page.tsx",
            "import { used } from \"../src/barrel\";\n\nexport default function Page() {\n  return used;\n}\n",
        ),
        (
            "src/barrel.ts",
            "import { used } from \"./impl\";\nexport { used };\n",
        ),
        (
            "src/impl.ts",
            "export const used = 1;\nexport const dead = 2;\n",
        ),
    ]);
    assert!(
        snapshot.is_live("src/barrel.ts#used"),
        "importing the alias must keep it live: {:?}",
        snapshot.candidates
    );
    assert!(
        !snapshot.is_candidate("src/barrel.ts#used"),
        "a live alias is not a candidate: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_live("src/impl.ts#used"),
        "the alias chain must keep the target live: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate("src/impl.ts#dead"),
        "dead declaration must be reported: {:?}",
        snapshot.candidates
    );
}

#[test]
fn nuxt_app_dir_and_expo_src_app_are_entries() {
    let snapshot = snapshot(&[
        ("nuxt.config.ts", "export default defineNuxtConfig({});\n"),
        ("app/pages/index.vue", "<template><div /></template>\n"),
        (
            "src/app/_layout.tsx",
            "import { Stack } from 'expo-router';\nexport default function Layout() { return <Stack />; }\n",
        ),
        (
            "src/app/index.tsx",
            "export default function Index() { return null; }\n",
        ),
        (
            "app/routes.ts",
            "import { type RouteConfig, index } from '@react-router/dev/routes';\nexport default [index('routes/home.tsx')] satisfies RouteConfig;\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(
        entries.contains(&"app/pages/index.vue#__module__"),
        "{entries:?}"
    );
    assert!(
        entries.contains(&"src/app/_layout.tsx#default"),
        "{entries:?}"
    );
    assert!(
        entries.contains(&"src/app/index.tsx#default"),
        "{entries:?}"
    );
    assert!(entries.contains(&"app/routes.ts#default"), "{entries:?}");
}
