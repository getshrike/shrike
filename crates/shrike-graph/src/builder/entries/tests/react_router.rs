//! Tests for react_router entry detection.

use super::*;

#[test]
fn react_router_custom_app_directory_is_detected() {
    let snapshot = snapshot(&[
        (
            "react-router.config.ts",
            "import type { Config } from '@react-router/dev/config';\nexport default { appDirectory: 'src/frontend' } satisfies Config;\n",
        ),
        (
            "src/frontend/root.tsx",
            "export function Layout() { return null; }\nexport default function Root() { return null; }\nexport function unusedRootHelper() { return 1; }\n",
        ),
        ("src/frontend/routes.ts", "export default [];\n"),
        (
            "src/frontend/routes/home.tsx",
            "export function loader() { return null; }\nexport default function Home() { return null; }\nexport function unusedRouteHelper() { return 1; }\n",
        ),
        (
            "src/components/dead.tsx",
            "export function deadThing() { return 1; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    for key in [
        "src/frontend/root.tsx#default",
        "src/frontend/root.tsx#Layout",
        "src/frontend/routes/home.tsx#default",
        "src/frontend/routes/home.tsx#loader",
    ] {
        assert!(
            entries.contains(&key),
            "custom appDirectory entry {key} must be public: {entries:?}"
        );
        assert!(
            !snapshot.candidates.iter().any(|candidate| candidate == key),
            "custom appDirectory entry {key} must not be a candidate: {:?}",
            snapshot.candidates
        );
    }
    for key in [
        "src/frontend/root.tsx#unusedRootHelper",
        "src/frontend/routes/home.tsx#unusedRouteHelper",
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
fn react_router_route_config_targets_outside_routes_are_entries() {
    let snapshot = snapshot(&[
        (
            "react-router.config.ts",
            "export default { appDirectory: 'src/frontend' };\n",
        ),
        (
            "src/frontend/routes.ts",
            "import { index, layout, route as r } from '@react-router/dev/routes';\n\
                 // route('ignored', 'route-modules/comment-only.tsx');\n\
                 export default [\n\
                   index('route-modules/home.tsx'),\n\
                   r('about', './route-modules/about.tsx'),\n\
                   layout('shell.tsx', [r('login', 'route-modules/login.tsx')]),\n\
                   { path: 'contact', file: 'route-modules/contact.tsx' },\n\
                 ];\n",
        ),
        (
            "src/frontend/root.tsx",
            "export default function Root() { return null; }\n",
        ),
        (
            "src/frontend/route-modules/home.tsx",
            "export default function Home() { return null; }\nexport function loader() { return null; }\nexport function unusedHomeHelper() { return 1; }\n",
        ),
        (
            "src/frontend/route-modules/about.tsx",
            "export default function About() { return null; }\nexport function loader() { return null; }\nexport function unusedAboutHelper() { return 1; }\n",
        ),
        (
            "src/frontend/route-modules/login.tsx",
            "export default function Login() { return null; }\n",
        ),
        (
            "src/frontend/route-modules/contact.tsx",
            "export default function Contact() { return null; }\n",
        ),
        (
            "src/frontend/shell.tsx",
            "export default function Shell() { return null; }\n",
        ),
        (
            "src/frontend/route-modules/comment-only.tsx",
            "export default function CommentOnly() { return null; }\n",
        ),
        (
            "src/frontend/route-modules/unreferenced.tsx",
            "export default function Unreferenced() { return null; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    for key in [
        "src/frontend/route-modules/home.tsx#default",
        "src/frontend/route-modules/home.tsx#loader",
        "src/frontend/route-modules/about.tsx#default",
        "src/frontend/route-modules/about.tsx#loader",
        "src/frontend/route-modules/login.tsx#default",
        "src/frontend/route-modules/contact.tsx#default",
        "src/frontend/shell.tsx#default",
    ] {
        assert!(
            entries.contains(&key),
            "route config target {key} must be public: {entries:?}"
        );
        assert!(
            !snapshot.candidates.iter().any(|candidate| candidate == key),
            "route config target {key} must not be a candidate: {:?}",
            snapshot.candidates
        );
    }
    for key in [
        "src/frontend/route-modules/home.tsx#unusedHomeHelper",
        "src/frontend/route-modules/about.tsx#unusedAboutHelper",
        "src/frontend/route-modules/comment-only.tsx#default",
        "src/frontend/route-modules/unreferenced.tsx#default",
    ] {
        assert!(
            snapshot.candidates.iter().any(|candidate| candidate == key),
            "{key} must remain a candidate: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn router_and_meta_framework_routes_are_entries() {
    let snapshot = snapshot(&[
        (
            "app/root.tsx",
            "import { Outlet } from '@remix-run/react';\nexport default function Root() { return <Outlet />; }\nexport function loader() { return null; }\n",
        ),
        (
            "app/routes/users.$id.tsx",
            "import { useLoaderData } from '@remix-run/react';\nexport async function loader() { return null; }\nexport default function User() { return null; }\n",
        ),
        (
            "src/routes/posts.tsx",
            "import { createFileRoute } from '@tanstack/react-router';\nexport const Route = createFileRoute('/posts')({});\n",
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
            "app/profile.tsx",
            "export default function Profile() { return null; }\n",
        ),
        (
            "src/routes/+page.ts",
            "export function load() { return {}; }\n",
        ),
        (
            "src/pages/health.ts",
            "import type { APIRoute } from 'astro';\nexport const GET: APIRoute = () => new Response('ok');\n",
        ),
        ("nuxt.config.ts", "export default defineNuxtConfig({});\n"),
        (
            "server/api/hello.ts",
            "export default defineEventHandler(() => 'hello');\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(entries.contains(&"app/root.tsx#default"), "{entries:?}");
    assert!(
        entries.contains(&"app/routes/users.$id.tsx#loader"),
        "{entries:?}"
    );
    assert!(
        entries.contains(&"src/routes/posts.tsx#Route"),
        "{entries:?}"
    );
    assert!(entries.contains(&"app/_layout.tsx#default"), "{entries:?}");
    assert!(entries.contains(&"app/index.tsx#default"), "{entries:?}");
    assert!(
        entries.contains(&"app/profile.tsx#default"),
        "every Expo app module is a route: {entries:?}"
    );
    assert!(entries.contains(&"src/routes/+page.ts#load"), "{entries:?}");
    assert!(entries.contains(&"src/pages/health.ts#GET"), "{entries:?}");
    assert!(
        entries.contains(&"server/api/hello.ts#default"),
        "{entries:?}"
    );
}

#[test]
fn react_router_route_modules_with_generated_types_are_entries() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"rr7-app","type":"module","dependencies":{"react-router":"^7.9.0","@react-router/dev":"^7.9.0"}}"#,
        ),
        (
            "app/routes.ts",
            "import { type RouteConfig, index, route } from '@react-router/dev/routes';\n\
                 export default [index('routes/home.tsx'), route('about', 'routes/about.tsx')] satisfies RouteConfig;\n",
        ),
        (
            "app/root.tsx",
            "import { Outlet } from 'react-router';\n\
                 export function Layout({ children }: { children: React.ReactNode }) { return <html><body>{children}</body></html>; }\n\
                 export default function Root() { return <Outlet />; }\n",
        ),
        (
            "app/routes/home.tsx",
            "import type { Route } from './+types/home';\n\
                 export function loader({}: Route.LoaderArgs) { return { title: 'Home' }; }\n\
                 export const middleware = [];\n\
                 export const unstable_middleware = [];\n\
                 export default function Home({ loaderData }: Route.ComponentProps) { return <main>{loaderData.title}</main>; }\n",
        ),
        (
            "app/routes/about.tsx",
            "import type { Route } from './+types/about';\n\
                 export async function action({ request }: Route.ActionArgs) { return request.formData(); }\n\
                 export const clientMiddleware = [];\n\
                 export const unstable_clientMiddleware = [];\n\
                 export default function About() { return <main>About</main>; }\n",
        ),
        (
            "app/components/dead.tsx",
            "export function deadThing() { return 1; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    for symbol in [
        "app/root.tsx#Layout",
        "app/routes/home.tsx#loader",
        "app/routes/home.tsx#middleware",
        "app/routes/home.tsx#unstable_middleware",
        "app/routes/home.tsx#default",
        "app/routes/about.tsx#action",
        "app/routes/about.tsx#clientMiddleware",
        "app/routes/about.tsx#unstable_clientMiddleware",
        "app/routes/about.tsx#default",
    ] {
        assert!(
            entries.contains(&symbol),
            "missing entry {symbol}: {entries:?}"
        );
    }
    assert!(
        snapshot
            .candidates
            .iter()
            .any(|key| key == "app/components/dead.tsx#deadThing"),
        "non-route modules must still be analyzed: {:?}",
        snapshot.candidates
    );
}

#[test]
fn remix_route_modules_without_imports_are_entries() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"remix-app","type":"module","dependencies":{"@remix-run/node":"^2.15.0","@remix-run/react":"^2.15.0"}}"#,
        ),
        (
            "app/root.tsx",
            "import { Outlet } from '@remix-run/react';\n\
                 export default function App() { return <Outlet />; }\n\
                 export function CatchBoundary() { return <main>Caught</main>; }\n\
                 export const shouldReload = () => false;\n\
                 export const unstable_shouldReload = () => false;\n",
        ),
        (
            "app/routes/about._index.tsx",
            "export default function About() { return <main>About</main>; }\n",
        ),
        (
            "app/components/dead.tsx",
            "export function deadThing() { return 1; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    for symbol in [
        "app/root.tsx#default",
        "app/root.tsx#CatchBoundary",
        "app/root.tsx#shouldReload",
        "app/root.tsx#unstable_shouldReload",
        "app/routes/about._index.tsx#default",
    ] {
        assert!(
            entries.contains(&symbol),
            "a Remix route export is framework-invoked even with no imports: {symbol} missing from {entries:?}"
        );
    }
    assert!(
        snapshot
            .candidates
            .iter()
            .any(|key| key == "app/components/dead.tsx#deadThing"),
        "non-route modules must still be analyzed: {:?}",
        snapshot.candidates
    );
}

#[test]
fn remix_custom_server_module_is_an_entry() {
    let remix = snapshot(&[
        (
            "package.json",
            r#"{"name":"remix-app","dependencies":{"@remix-run/node":"^2.15.0","@remix-run/react":"^2.15.0","react":"^18.3.1"}}"#,
        ),
        (
            "app/root.tsx",
            "import { Outlet } from '@remix-run/react';\nexport default function App() { return <Outlet />; }\n",
        ),
        (
            "server.ts",
            "import { startServer } from './server-lib';\nstartServer();\nexport function createServer() { return 1; }\nexport const unusedServerHelper = 2;\n",
        ),
        (
            "server-lib.ts",
            "export function startServer() { return 1; }\n",
        ),
        (
            "packages/other/server.ts",
            "export function otherServer() { return 1; }\n",
        ),
    ]);
    let entries = entry_symbols(&remix);
    assert!(
        entries.contains(&"server.ts#__module__"),
        "a Remix custom server is executed directly: {entries:?}"
    );
    assert!(
        !remix.is_candidate("server-lib.ts#startServer"),
        "the server module's top-level call keeps its import live: {:?}",
        remix.candidates
    );
    assert!(
        remix.is_candidate("server.ts#createServer")
            && remix.is_candidate("server.ts#unusedServerHelper"),
        "nothing reads the server module's exports: {:?}",
        remix.candidates
    );
    assert!(
        remix.is_candidate("packages/other/server.ts#otherServer"),
        "the convention is scoped to the app package root: {:?}",
        remix.candidates
    );

    let plain = snapshot(&[
        (
            "package.json",
            r#"{"name":"plain-react","dependencies":{"react":"^18.3.1"}}"#,
        ),
        (
            "server.ts",
            "export function createServer() { return 1; }\n",
        ),
    ]);
    assert!(
        plain.is_candidate("server.ts#createServer"),
        "a plain React app has no Remix custom server: {:?}",
        plain.candidates
    );
}

#[test]
fn next_custom_server_module_is_an_entry() {
    let next = snapshot(&[
        (
            "package.json",
            r#"{"name":"next-app","type":"module","dependencies":{"next":"^16.4.2","react":"^19.1.0"}}"#,
        ),
        (
            "app/page.tsx",
            "export default function Page() { return null; }\n",
        ),
        (
            "server.js",
            "import { boot } from './server-lib';\nboot();\nexport function start() { return 1; }\nexport const unusedServerHelper = 2;\n",
        ),
        ("server-lib.js", "export function boot() { return 1; }\n"),
    ]);
    let entries = entry_symbols(&next);
    assert!(
        entries.contains(&"server.js#__module__"),
        "a Next custom server is executed directly: {entries:?}"
    );
    assert!(
        !next.is_candidate("server-lib.js#boot"),
        "the server module's top-level call keeps its import live: {:?}",
        next.candidates
    );
    assert!(
        next.is_candidate("server.js#start") && next.is_candidate("server.js#unusedServerHelper"),
        "nothing reads the custom server's exports: {:?}",
        next.candidates
    );

    let plain = snapshot(&[
        (
            "package.json",
            r#"{"name":"plain-react","type":"module","dependencies":{"react":"^19.1.0"}}"#,
        ),
        ("server.js", "export function start() { return 1; }\n"),
    ]);
    assert!(
        plain.is_candidate("server.js#start"),
        "a plain React app has no Next custom server: {:?}",
        plain.candidates
    );
}

#[test]
fn tanstack_route_factories_are_entries_in_custom_directories() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"tsr-app","type":"module","dependencies":{"@tanstack/react-router":"^1.130.0"}}"#,
        ),
        (
            "tsr.config.json",
            r#"{"routesDirectory":"src/pages","generatedRouteTree":"src/routeTree.gen.ts"}"#,
        ),
        (
            "src/pages/index.tsx",
            "import { createFileRoute } from '@tanstack/react-router';\n\
                 export const Route = createFileRoute('/')({ component: Home });\n\
                 function Home() { return <main>Home</main>; }\n",
        ),
        (
            "src/components/dead.tsx",
            "export function deadThing() { return 1; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(
        entries.contains(&"src/pages/index.tsx#Route"),
        "a route factory marks a route in a configured directory: {entries:?}"
    );
    assert!(
        !snapshot
            .candidates
            .iter()
            .any(|key| key == "src/pages/index.tsx#Home"),
        "the route's component must stay live: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot
            .candidates
            .iter()
            .any(|key| key == "src/components/dead.tsx#deadThing"),
        "non-route modules must still be analyzed: {:?}",
        snapshot.candidates
    );
}
