//! Tests for frameworks entry detection.

use super::*;

#[test]
fn node_test_directories_and_test_files_are_entries() {
    let snapshot = snapshot(&[
        ("source/index.ts", "export const api = 1;\n"),
        (
            "test/test.ts",
            "import { api } from '../source/index.ts';\nconsole.log(api);\n",
        ),
        ("test/unit/helper.ts", "export const helper = 1;\n"),
        ("tests/unit/helper.ts", "export const helper = 1;\n"),
        (
            "tests/example.test.ts",
            "import { helper } from './unit/helper';\ntest('helper', () => helper);\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(entries.contains(&"test/test.ts#__module__"), "{entries:?}");
    assert!(
        entries.contains(&"test/unit/helper.ts#__module__"),
        "node --test executes every file under test/: {entries:?}"
    );
    assert!(
        entries.contains(&"tests/example.test.ts#__module__"),
        "{entries:?}"
    );
    assert!(
        !entries.contains(&"tests/unit/helper.ts#__module__"),
        "a tests/ support module is not executed by a runner: {entries:?}"
    );
    assert!(
        snapshot.is_live("tests/unit/helper.ts#helper"),
        "the imported support module stays live: {:?}",
        snapshot.live
    );
}

#[test]
fn test_config_references_are_entries() {
    let snapshot = snapshot(&[
        (
            "config/vitest.config.ts",
            "export default { test: { setupFiles: ['./tests/setup.ts'] } };\n",
        ),
        (
            "vitest.config.ts",
            "const path = require('node:path');\nexport default { resolve: { alias: { 'server-only': path.resolve(__dirname, 'tests/stubs/server-only.ts') } } };\n",
        ),
        (
            "tests/setup.ts",
            "import { configure } from './support/live';\nconfigure();\n",
        ),
        (
            "tests/support/live.ts",
            "export const configure = () => 1;\n",
        ),
        (
            "tests/stubs/server-only.ts",
            "export const serverOnly = true;\n",
        ),
        ("tests/support/dead.ts", "export const dead = 1;\n"),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(
        entries.contains(&"tests/setup.ts#__module__"),
        "a Vitest setup file is loaded by path: {entries:?}"
    );
    assert!(
        entries.contains(&"tests/stubs/server-only.ts#__module__"),
        "an alias target is loaded by the runner: {entries:?}"
    );
    assert!(
        snapshot.is_live("tests/support/live.ts#configure"),
        "the setup file's import stays live: {:?}",
        snapshot.live
    );
    assert!(
        !entries.contains(&"tests/support/dead.ts#__module__"),
        "an unreferenced support module is not an entry: {entries:?}"
    );
    assert!(
        snapshot.is_candidate("tests/support/dead.ts#dead"),
        "an unreferenced support module is dead: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot
            .unused_files
            .contains(&"tests/support/dead.ts".to_string()),
        "the dead support module is an unused file: {:?}",
        snapshot.unused_files
    );
}

#[test]
fn cypress_specs_and_support_are_entries_only_with_the_dependency() {
    let files = [
        (
            "package.json",
            r#"{"name":"web","private":true,"dependencies":{"next":"^16.0.0","react":"^19.1.0"},"devDependencies":{"cypress":"^15.0.0"}}"#,
        ),
        (
            "cypress.config.ts",
            "export default { e2e: { specPattern: ['cypress/e2e/**/*.cy.ts', 'cypress/e2e/**/*.spec.ts'] } };\n",
        ),
        (
            "cypress/e2e/home.cy.ts",
            "import { specHelper } from '../../lib/spec-helper';\nspecHelper();\n",
        ),
        (
            "cypress/e2e/checkout.spec.ts",
            "import { specHelper } from '../../lib/spec-helper';\nspecHelper();\n",
        ),
        (
            "cypress/support/e2e.ts",
            "import { commandHelper } from '../../lib/command-helper';\ncommandHelper();\n",
        ),
        (
            "lib/spec-helper.ts",
            "export const specHelper = () => 1;\nexport const unusedSpecHelper = 2;\n",
        ),
        (
            "lib/command-helper.ts",
            "export const commandHelper = () => 1;\nexport const unusedCommandHelper = 2;\n",
        ),
        ("lib/dead.ts", "export const dead = 1;\n"),
    ];
    let cypress = snapshot(&files);
    let entries = entry_symbols(&cypress);
    for key in [
        "cypress/e2e/home.cy.ts#__module__",
        "cypress/support/e2e.ts#__module__",
    ] {
        assert!(
            entries.contains(&key),
            "{key} is executed by Cypress: {entries:?}"
        );
    }
    for key in [
        "lib/spec-helper.ts#specHelper",
        "lib/command-helper.ts#commandHelper",
    ] {
        assert!(
            cypress.is_live(key),
            "{key} is imported by Cypress: {:?}",
            cypress.live
        );
    }
    for key in [
        "lib/spec-helper.ts#unusedSpecHelper",
        "lib/command-helper.ts#unusedCommandHelper",
        "lib/dead.ts#dead",
    ] {
        assert!(
            cypress.is_candidate(key),
            "{key} is genuinely unused: {:?}",
            cypress.candidates
        );
    }
    assert!(
        cypress.unused_files.contains(&"lib/dead.ts".to_string()),
        "only the genuinely dead file is unused: {:?}",
        cypress.unused_files
    );
    // A custom pattern may also match the generic `.spec.` convention.
    // The file is still one Cypress spec, not a Cypress spec plus a test.
    let checkout: Vec<&str> = cypress
        .entries
        .iter()
        .filter(|entry| entry.symbol == "cypress/e2e/checkout.spec.ts#__module__")
        .map(|entry| entry.framework.as_str())
        .collect();
    assert_eq!(
        checkout,
        ["cypress"],
        "a Cypress spec is not also a generic test: {entries:?}"
    );

    let plain = snapshot(&[
        (
            "package.json",
            r#"{"name":"plain","dependencies":{"react":"^19.0.0"}}"#,
        ),
        ("cypress.config.ts", "export default { e2e: {} };\n"),
        ("cypress/e2e/home.cy.ts", "export const home = 1;\n"),
        ("cypress/support/e2e.ts", "export const support = 1;\n"),
    ]);
    let entries = entry_symbols(&plain);
    for key in [
        "cypress/e2e/home.cy.ts#__module__",
        "cypress/support/e2e.ts#__module__",
    ] {
        assert!(
            !entries.contains(&key),
            "without a cypress dependency the config is inert: {key}"
        );
    }
    for path in ["cypress/e2e/home.cy.ts", "cypress/support/e2e.ts"] {
        assert!(
            plain.unused_files.contains(&path.to_string()),
            "unprotected Cypress files are dead: {:?}",
            plain.unused_files
        );
    }
}

#[test]
fn vite_index_html_module_scripts_are_entries() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"vite-app","devDependencies":{"vite":"^6.0.0"}}"#,
        ),
        (
            "index.html",
            "<div id=\"app\"></div>\n<script type=\"module\" src=\"/src/main.ts\"></script>\n",
        ),
        (
            "src/main.ts",
            "import { mount } from \"./app\";\nexport const bootFlag = 1;\nmount();\n",
        ),
        (
            "src/app.ts",
            "export function mount() { return 1; }\nexport function unusedMount() { return 2; }\n",
        ),
        ("src/dead.ts", "export function DeadThing() { return 3; }\n"),
    ]);
    let entries = entry_symbols(&snapshot);
    for key in [
        "index.html#__module__",
        "src/main.ts#__module__",
        "src/main.ts#bootFlag",
        "src/app.ts#mount",
    ] {
        assert!(
            entries.contains(&key) || snapshot.is_live(key),
            "{key} must be live: {entries:?}"
        );
    }
    for key in ["src/app.ts#unusedMount", "src/dead.ts#DeadThing"] {
        assert!(
            snapshot.candidates.iter().any(|candidate| candidate == key),
            "{key} must remain a candidate: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn vite_root_fallbacks_require_the_dependency() {
    let files: &[(&str, &str)] = &[
        (
            "src/main.ts",
            "import { helper } from \"./helpers\";\nexport const rootBoot = 1;\nhelper();\n",
        ),
        ("src/index.ts", "export const indexBoot = 1;\n"),
        ("src/helpers.ts", "export function helper() { return 1; }\n"),
        ("src/dead.ts", "export function DeadThing() { return 2; }\n"),
    ];
    let mut with_vite: Vec<(&str, &str)> = vec![(
        "package.json",
        r#"{"name":"vite-app","devDependencies":{"vite":"^6.0.0"}}"#,
    )];
    with_vite.extend_from_slice(files);
    let with_vite_snapshot = snapshot(&with_vite);
    let entries = entry_symbols(&with_vite_snapshot);
    for key in [
        "src/main.ts#__module__",
        "src/main.ts#rootBoot",
        "src/index.ts#__module__",
        "src/index.ts#indexBoot",
        "src/helpers.ts#helper",
    ] {
        assert!(
            entries.contains(&key) || with_vite_snapshot.is_live(key),
            "{key} must be live: {entries:?}"
        );
    }
    assert!(
        with_vite_snapshot
            .candidates
            .iter()
            .any(|candidate| candidate == "src/dead.ts#DeadThing"),
        "unrelated dead code must remain a candidate: {:?}",
        with_vite_snapshot.candidates
    );

    let mut without_vite: Vec<(&str, &str)> = vec![("package.json", r#"{"name":"plain-app"}"#)];
    without_vite.extend_from_slice(files);
    let without_vite_snapshot = snapshot(&without_vite);
    assert!(
        without_vite_snapshot
            .candidates
            .iter()
            .any(|candidate| candidate == "src/main.ts#rootBoot"),
        "the root fallback must not apply without Vite: {:?}",
        without_vite_snapshot.candidates
    );
}

#[test]
fn storybook_mdx_documents_are_entries() {
    let snapshot =
        storybook_mdx_app(r#"{"name":"docs-app","devDependencies":{"storybook":"^8.0.0"}}"#);
    let entries = entry_symbols(&snapshot);
    assert!(
        entries.contains(&"src/Docs.mdx#__module__"),
        "Storybook's default glob makes every MDX page an entry: {entries:?}"
    );
    assert!(
        snapshot.is_live("src/DocsCard.tsx#DocsCard"),
        "the component rendered by the docs page must be live: {:?}",
        snapshot.live
    );
    for key in [
        "src/DocsCard.tsx#unusedDocsHelper",
        "src/dead.tsx#DeadThing",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "{key} is unused and must remain a candidate: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn storybook_mdx_entries_require_the_dependency() {
    let snapshot = storybook_mdx_app(r#"{"name":"plain-app","dependencies":{"react":"^19.0.0"}}"#);
    for key in [
        "src/DocsCard.tsx#DocsCard",
        "src/DocsCard.tsx#unusedDocsHelper",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "without Storybook the MDX page must not protect its imports: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn storybook_custom_story_globs_are_entries() {
    let snapshot =
        custom_story_app(r#"{"name":"custom-stories","devDependencies":{"storybook":"^8.0.0"}}"#);
    let entries = entry_symbols(&snapshot);
    for key in [
        ".storybook/main.ts#__module__",
        "src/Button.demo.tsx#__module__",
        "src/Card.preview.tsx#__module__",
    ] {
        assert!(
            entries.contains(&key),
            "{key} is named by the Storybook config: {entries:?}"
        );
    }
    for key in [
        "src/Button.demo.tsx#default",
        "src/Button.demo.tsx#unusedDemoExport",
        "src/button.tsx#Button",
        "src/card.tsx#Card",
    ] {
        assert!(
            snapshot.is_live(key),
            "{key} is part of a story entry surface: {:?}",
            snapshot.live
        );
    }
    for key in ["src/button.tsx#unusedButtonHelper", "src/dead.tsx#Dead"] {
        assert!(
            snapshot.is_candidate(key),
            "{key} is unused and must remain a candidate: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn ladle_components_config_and_custom_stories_are_entries() {
    let snapshot = ladle_app(r#"{"name":"ladle-app","devDependencies":{"@ladle/react":"^4.1.2"}}"#);
    let entries = entry_symbols(&snapshot);
    for key in [
        ".ladle/config.mjs#__module__",
        ".ladle/components.tsx#__module__",
        ".ladle/components.tsx#Provider",
        ".ladle/components.tsx#unusedComponentsHelper",
        "ladle-vite.ts#__module__",
        "ladle-vite.ts#unusedViteHelper",
        "src/button.stories.tsx#__module__",
        "src/button.stories.tsx#Primary",
        "src/button.stories.tsx#unusedStoryHelper",
        "src/widget.demo.tsx#__module__",
        "src/widget.demo.tsx#DemoWidget",
        "src/widget.demo.tsx#unusedDemoHelper",
    ] {
        assert!(
            entries.contains(&key),
            "{key} is part of Ladle's entry surface: {entries:?}"
        );
    }
    for key in [
        ".ladle/shell.tsx#LadleShell",
        "ladle-plugin.ts#plugin",
        "src/components/button.tsx#Button",
    ] {
        assert!(
            snapshot.is_live(key),
            "{key} is imported by a Ladle entry: {:?}",
            snapshot.live
        );
    }
    for key in [
        ".ladle/shell.tsx#unusedShellHelper",
        "src/components/button.tsx#unusedButtonHelper",
        "src/dead.tsx#DeadThing",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "{key} is unused and must remain a candidate: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn ladle_entries_require_the_dependency() {
    let snapshot = ladle_app(r#"{"name":"plain-app","dependencies":{"react":"^19.0.0"}}"#);
    for key in [
        ".ladle/components.tsx#Provider",
        ".ladle/components.tsx#unusedComponentsHelper",
        ".ladle/shell.tsx#LadleShell",
        "ladle-vite.ts#unusedViteHelper",
        "ladle-plugin.ts#plugin",
        "src/widget.demo.tsx#DemoWidget",
        "src/widget.demo.tsx#unusedDemoHelper",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "without Ladle the entry surface must not protect {key}: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn ladle_default_story_glob_includes_mdx_pages() {
    let snapshot =
        ladle_default_app(r#"{"name":"ladle-app","devDependencies":{"@ladle/react":"^4.1.2"}}"#);
    let entries = entry_symbols(&snapshot);
    assert!(
        entries.contains(&"src/Intro.stories.mdx#__module__"),
        "Ladle's default story glob includes MDX under src: {entries:?}"
    );
    assert!(
        snapshot.is_live("src/MdxCard.tsx#MdxCard"),
        "the component rendered by the MDX story must be live: {:?}",
        snapshot.live
    );
    assert!(
        snapshot.is_candidate("src/MdxCard.tsx#unusedMdxHelper"),
        "unused helper exports stay candidates: {:?}",
        snapshot.candidates
    );
}

#[test]
fn storybook_custom_story_globs_require_the_dependency() {
    let snapshot = custom_story_app(r#"{"name":"plain-app","dependencies":{"react":"^19.0.0"}}"#);
    for key in [
        "src/Button.demo.tsx#default",
        "src/button.tsx#Button",
        "src/Card.preview.tsx#default",
        "src/card.tsx#Card",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "without Storybook the custom glob must not protect {key}: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn worker_modules_are_reachable_without_an_import() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"worker-app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "app/page.tsx",
            "import { WorkerHost } from \"../components/worker-host\";\n\nexport default function Page() {\n  return <WorkerHost />;\n}\n",
        ),
        (
            "components/worker-host.tsx",
            "export function WorkerHost() {\n  const worker = new Worker(new URL(\"./heavy-worker.ts\", import.meta.url), { type: \"module\" });\n  return <div>{worker ? \"ready\" : \"idle\"}</div>;\n}\n",
        ),
        (
            "components/heavy-worker.ts",
            "import { render } from \"./renderer\";\n\nconst state = { count: 0 };\n\nfunction handle(message: string) {\n  state.count += 1;\n  return render(message);\n}\n\nself.onmessage = (event) => handle(event.data);\n\nfunction deadWorkerHelper() {\n  return 1;\n}\n",
        ),
        (
            "components/renderer.ts",
            "export function render(value: string) {\n  return value;\n}\n\nexport function deadRenderer() {\n  return 2;\n}\n",
        ),
        (
            "components/dead.ts",
            "export function Dead() {\n  return null;\n}\n",
        ),
    ]);
    for key in [
        "components/worker-host.tsx#WorkerHost",
        "components/heavy-worker.ts#handle",
        "components/heavy-worker.ts#state",
        "components/renderer.ts#render",
    ] {
        assert!(
            snapshot.is_live(key),
            "worker-reachable symbol must be live: {key}: {:?}",
            snapshot.candidates
        );
    }
    for key in [
        "components/heavy-worker.ts#deadWorkerHelper",
        "components/renderer.ts#deadRenderer",
        "components/dead.ts#Dead",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "unreferenced symbol must remain a candidate: {key}: {:?}",
            snapshot.candidates
        );
    }
    assert!(
        snapshot
            .edges
            .iter()
            .any(|edge| edge.kind == "worker"
                && edge.target == "components/heavy-worker.ts#__module__"),
        "worker edge must be recorded: {:?}",
        snapshot.edges
    );
}

#[test]
fn public_worker_urls_do_not_widen_the_repository() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"worker-app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        (
            "app/page.tsx",
            "import { WorkerHost } from \"../components/worker-host\";\n\nexport default function Page() {\n  return <WorkerHost />;\n}\n",
        ),
        (
            "components/worker-host.tsx",
            "export function WorkerHost() {\n  void navigator.serviceWorker.register(\"/sw.js\", { scope: \"/\" });\n  return null;\n}\n",
        ),
        (
            "components/dead.ts",
            "export function Dead() {\n  return null;\n}\n",
        ),
    ]);
    assert!(
        snapshot.is_live("components/worker-host.tsx#WorkerHost"),
        "route-reachable component must stay live: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate("components/dead.ts#Dead"),
        "a public worker URL must not make unrelated symbols live: {:?}",
        snapshot.candidates
    );
    assert!(
        !snapshot
            .unknowns
            .iter()
            .any(|unknown| unknown.scope == "repo"),
        "public worker URL must not create a repository-wide unknown: {:?}",
        snapshot.unknowns
    );
    assert!(
        snapshot.modules.iter().all(|module| !module
            .external
            .iter()
            .any(|specifier| specifier == "/sw.js")),
        "a runtime worker URL is not an external package: {:?}",
        snapshot.modules
    );
}

#[test]
fn webpack_entries_are_roots() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"webpack-app","devDependencies":{"webpack":"^5.90.0","webpack-cli":"^5.1.0"}}"#,
        ),
        (
            "webpack.config.js",
            "const path = require(\"path\");\n\nmodule.exports = {\n  context: path.resolve(__dirname, \"src\"),\n  entry: { app: [\"./boot.js\"], admin: { import: \"./admin.js\" } },\n};\n",
        ),
        (
            "src/boot.js",
            "import { start } from \"./runtime\";\nimport { mountHeader } from \"./header\";\n\nstart();\nmountHeader();\n",
        ),
        (
            "src/admin.js",
            "import { renderAdmin } from \"./admin-panel\";\n\nrenderAdmin();\n",
        ),
        (
            "src/runtime.js",
            "export function start() { return \"started\"; }\nexport function unusedRuntime() { return \"dead\"; }\n",
        ),
        (
            "src/header.js",
            "export function mountHeader() { return \"header\"; }\n",
        ),
        (
            "src/admin-panel.js",
            "export function renderAdmin() { return \"admin\"; }\nexport function unusedAdminPanel() { return \"dead\"; }\n",
        ),
        (
            "src/dead.js",
            "export function DeadThing() { return \"dead\"; }\n",
        ),
    ]);
    let entries = entry_symbols(&snapshot);
    for key in ["src/boot.js#__module__", "src/admin.js#__module__"] {
        assert!(
            entries.contains(&key),
            "{key} is named by the webpack entry object: {entries:?}"
        );
    }
    for key in [
        "src/runtime.js#start",
        "src/header.js#mountHeader",
        "src/admin-panel.js#renderAdmin",
    ] {
        assert!(
            snapshot.is_live(key),
            "{key} is imported by a webpack entry and must be live: {:?}",
            snapshot.live
        );
    }
    for key in [
        "src/runtime.js#unusedRuntime",
        "src/admin-panel.js#unusedAdminPanel",
        "src/dead.js#DeadThing",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "{key} is unused and must remain a candidate: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn webpack_literal_context_is_package_relative() {
    let snapshot = snapshot(&[
        (
            "package.json",
            r#"{"name":"webpack-config-dir","devDependencies":{"webpack":"^5.90.0"}}"#,
        ),
        (
            "config/webpack.client.js",
            "module.exports = {\n  context: \"src\",\n  entry: \"./boot.js\",\n};\n",
        ),
        (
            "src/boot.js",
            "import { start } from \"./runtime\";\n\nstart();\n",
        ),
        (
            "src/runtime.js",
            "export function start() { return \"started\"; }\nexport function unusedRuntime() { return \"dead\"; }\n",
        ),
    ]);
    assert!(
        snapshot.is_live("src/boot.js#__module__"),
        "a literal context is resolved from the package root: {:?}",
        snapshot.live
    );
    assert!(
        snapshot.is_live("src/runtime.js#start"),
        "the config-directory entry must keep its imports live: {:?}",
        snapshot.live
    );
    assert!(
        snapshot.is_candidate("src/runtime.js#unusedRuntime"),
        "unused exports must still be reported: {:?}",
        snapshot.candidates
    );
}

#[test]
fn skipped_framework_files_are_entries() {
    let snapshot = snapshot(&[
        ("src/routes/+page.svelte", "<h1>Hello</h1>\n"),
        ("src/pages/index.astro", "---\n---\n<h1>Hello</h1>\n"),
        ("pages/index.vue", "<template><div /></template>\n"),
    ]);
    let entries = entry_symbols(&snapshot);
    assert!(
        entries.contains(&"src/routes/+page.svelte#__module__"),
        "{entries:?}"
    );
    assert!(
        entries.contains(&"src/pages/index.astro#__module__"),
        "{entries:?}"
    );
    assert!(
        entries.contains(&"pages/index.vue#__module__"),
        "{entries:?}"
    );
}

#[test]
fn cosmos_fixtures_and_decorators_are_entries() {
    let snapshot = discovered_snapshot(&[
        (
            "package.json",
            r#"{"name":"cosmos-app","private":true,"dependencies":{"react":"^19.0.0","react-dom":"^19.0.0"},"devDependencies":{"react-cosmos":"^7.0.0"}}"#,
        ),
        (
            "cosmos.config.json",
            "{\"plugins\":[\"react-cosmos-plugin-vite\"]}",
        ),
        (
            "components/Button.tsx",
            "export function Button() { return <button>ok</button>; }\nexport function unusedButtonHelper() { return 1; }\n",
        ),
        (
            "components/Card.tsx",
            "export function Card() { return <div>card</div>; }\nexport function unusedCardHelper() { return 2; }\n",
        ),
        (
            "components/Button.fixture.tsx",
            "import { Button } from './Button';\nimport { Card } from './Card';\nexport default function ButtonFixture() { return <Button />; }\nexport const CardFixture = () => <Card />;\n",
        ),
        (
            "components/__fixtures__/Nested.tsx",
            "import { NestedThing } from '../NestedThing';\nimport { Card } from '../Card';\nexport default function NestedFixture() { return <NestedThing card={<Card />} />; }\n",
        ),
        (
            "components/NestedThing.tsx",
            "export function NestedThing({ card }: { card: React.ReactNode }) { return <div>{card}</div>; }\nexport function unusedNestedHelper() { return 3; }\n",
        ),
        (
            "decorators/cosmos.decorator.tsx",
            "export default function CosmosDecorator({ children }: { children: React.ReactNode }) { return <div>{children}</div>; }\nexport function unusedDecoratorHelper() { return 4; }\n",
        ),
        ("dead.tsx", "export function DeadThing() { return 5; }\n"),
    ]);
    for key in [
        "components/Button.fixture.tsx#default",
        "components/Button.fixture.tsx#CardFixture",
        "components/__fixtures__/Nested.tsx#default",
        "components/Button.tsx#Button",
        "components/Card.tsx#Card",
        "components/NestedThing.tsx#NestedThing",
        "decorators/cosmos.decorator.tsx#default",
    ] {
        assert!(
            snapshot.is_live(key) || snapshot.entries.iter().any(|entry| entry.symbol == key),
            "{key} is a Cosmos user module or imported by one: {:?}",
            snapshot.candidates
        );
    }
    for key in [
        "components/Button.tsx#unusedButtonHelper",
        "components/Card.tsx#unusedCardHelper",
        "components/NestedThing.tsx#unusedNestedHelper",
        // Cosmos invokes only the decorator default export.
        "decorators/cosmos.decorator.tsx#unusedDecoratorHelper",
        "dead.tsx#DeadThing",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "{key} is unused and must remain a candidate: {:?}",
            snapshot.candidates
        );
    }
    assert!(
        snapshot.entries.iter().any(|entry| {
            entry.symbol == "components/Button.fixture.tsx#default"
                && entry.framework == "react-cosmos"
        }),
        "fixture exports are Cosmos entries: {:?}",
        snapshot.entries
    );
    assert!(
        snapshot.entries.iter().any(|entry| {
            entry.symbol == "decorators/cosmos.decorator.tsx#default"
                && entry.kind == "decorator-default"
        }),
        "the decorator default is a Cosmos entry: {:?}",
        snapshot.entries
    );

    let without_dependency = discovered_snapshot(&[
        (
            "package.json",
            r#"{"name":"plain-app","private":true,"dependencies":{"react":"^19.0.0","react-dom":"^19.0.0"}}"#,
        ),
        (
            "components/Button.tsx",
            "export function Button() { return <button>ok</button>; }\n",
        ),
        (
            "components/Button.fixture.tsx",
            "import { Button } from './Button';\nexport default function ButtonFixture() { return <Button />; }\n",
        ),
        (
            "decorators/cosmos.decorator.tsx",
            "export default function CosmosDecorator({ children }: { children: React.ReactNode }) { return <div>{children}</div>; }\n",
        ),
    ]);
    for key in [
        "components/Button.fixture.tsx#default",
        "decorators/cosmos.decorator.tsx#default",
    ] {
        assert!(
            without_dependency.is_candidate(key),
            "without react-cosmos {key} is not framework-invoked: {:?}",
            without_dependency.candidates
        );
    }
}

#[test]
fn cosmos_config_controls_directory_suffix_and_ignore() {
    let snapshot = discovered_snapshot(&[
        (
            "package.json",
            r#"{"name":"cosmos-custom","private":true,"dependencies":{"react":"^19.0.0","react-dom":"^19.0.0"},"devDependencies":{"react-cosmos":"^7.0.0"}}"#,
        ),
        (
            "cosmos.config.json",
            r#"{"fixturesDir":"src/examples","fixtureFileSuffix":"example","ignore":["**/*.skip.*"]}"#,
        ),
        (
            "src/Widget.tsx",
            "export function Widget() { return <div>widget</div>; }\nexport function unusedWidgetHelper() { return 1; }\n",
        ),
        (
            "src/Widget.example.tsx",
            "import { Widget } from './Widget';\nexport default function WidgetFixture() { return <Widget />; }\nexport const AnotherFixture = () => <Widget />;\n",
        ),
        (
            "src/Widget.fixture.tsx",
            "import { Legacy } from './Legacy';\nexport default function LegacyFixture() { return <Legacy />; }\n",
        ),
        (
            "src/Legacy.tsx",
            "export function Legacy() { return <div>legacy</div>; }\nexport function unusedLegacyHelper() { return 2; }\n",
        ),
        (
            "src/examples/Showcase.tsx",
            "import { Widget } from '../Widget';\nexport const Showcase = () => <Widget />;\n",
        ),
        (
            "src/examples/Thing.skip.tsx",
            "import { IgnoredThing } from '../IgnoredThing';\nexport const Skipped = () => <IgnoredThing />;\n",
        ),
        (
            "src/IgnoredThing.tsx",
            "export function IgnoredThing() { return <div>ignored</div>; }\n",
        ),
        (
            "src/cosmos.decorator.js",
            "import { Frame } from './Frame';\nexport default function Decorator({ children }) { return <Frame>{children}</Frame>; }\nexport function unusedDecoratorHelper() { return 3; }\n",
        ),
        (
            "src/Frame.tsx",
            "export function Frame({ children }) { return <div>{children}</div>; }\nexport function unusedFrameHelper() { return 4; }\n",
        ),
        (
            "src/dead.tsx",
            "export function DeadThing() { return 5; }\n",
        ),
    ]);
    for key in [
        "src/Widget.example.tsx#default",
        "src/Widget.example.tsx#AnotherFixture",
        "src/examples/Showcase.tsx#Showcase",
        "src/Widget.tsx#Widget",
        "src/cosmos.decorator.js#default",
        "src/Frame.tsx#Frame",
    ] {
        assert!(
            snapshot.is_live(key) || snapshot.entries.iter().any(|entry| entry.symbol == key),
            "{key} uses the configured Cosmos conventions: {:?}",
            snapshot.candidates
        );
    }
    for key in [
        // The configured suffix replaces `fixture`.
        "src/Widget.fixture.tsx#default",
        "src/Legacy.tsx#Legacy",
        "src/Legacy.tsx#unusedLegacyHelper",
        "src/Widget.tsx#unusedWidgetHelper",
        // Cosmos ignores the fixture directory entry.
        "src/examples/Thing.skip.tsx#Skipped",
        "src/IgnoredThing.tsx#IgnoredThing",
        "src/cosmos.decorator.js#unusedDecoratorHelper",
        "src/Frame.tsx#unusedFrameHelper",
        "src/dead.tsx#DeadThing",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "{key} is unused and must remain a candidate: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn docusaurus_docs_and_blog_pages_are_entries() {
    let snapshot = docusaurus_app(
        r#"{"name":"docs-site","private":true,"dependencies":{"@docusaurus/core":"^3.9.0","@docusaurus/preset-classic":"^3.9.0","react":"^19.0.0","react-dom":"^19.0.0"}}"#,
    );
    let entries = entry_symbols(&snapshot);
    for key in [
        "docs/intro.mdx#__module__",
        "blog/hello.mdx#__module__",
        "versioned_docs/version-1/guide.mdx#__module__",
    ] {
        assert!(
            entries.contains(&key),
            "Docusaurus pages are entries: {entries:?}"
        );
    }
    for key in [
        "src/components/Callout.tsx#default",
        "src/components/AuthorCard.tsx#default",
        "src/components/VersionNote.tsx#default",
    ] {
        assert!(
            snapshot.is_live(key),
            "the component rendered by a docs page must be live: {key}: {:?}",
            snapshot.candidates
        );
    }
    for key in [
        "src/components/Callout.tsx#unusedCalloutHelper",
        "src/components/AuthorCard.tsx#unusedAuthorHelper",
        "src/components/VersionNote.tsx#unusedVersionHelper",
        "src/components/dead.tsx#default",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "{key} is unused and must remain a candidate: {:?}",
            snapshot.candidates
        );
    }
    assert!(
        snapshot.entries.iter().any(|entry| {
            entry.symbol == "docs/intro.mdx#__module__" && entry.framework == "docusaurus"
        }),
        "the page records its framework: {:?}",
        snapshot.entries
    );
}

#[test]
fn docusaurus_pages_require_the_dependency() {
    let snapshot = docusaurus_app(
        r#"{"name":"plain-docs","private":true,"dependencies":{"react":"^19.0.0"}}"#,
    );
    for key in [
        "src/components/Callout.tsx#default",
        "src/components/AuthorCard.tsx#default",
        "src/components/VersionNote.tsx#default",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "without @docusaurus/core {key} is not framework-invoked: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn gatsby_create_page_templates_are_entries() {
    let snapshot = gatsby_app(
        r#"{"name":"gatsby-site","private":true,"dependencies":{"gatsby":"^5.14.0","react":"^18.3.1","react-dom":"^18.3.1"}}"#,
        "import path from \"path\";\n\nexport async function createPages({ actions }: { actions: any }) {\n  actions.createPage({\n    path: \"/blog/hello\",\n    component: path.resolve(\"./src/templates/blog-post.tsx\"),\n    context: { slug: \"hello\" },\n  });\n}\n",
    );
    let entries = entry_symbols(&snapshot);
    assert!(
        entries.contains(&"src/templates/blog-post.tsx#__module__"),
        "a template named by createPage is an entry: {entries:?}"
    );
    for key in [
        "src/templates/blog-post.tsx#default",
        "src/templates/blog-post.tsx#query",
        "src/templates/blog-post.tsx#Head",
        "src/components/post-body.tsx#PostBody",
    ] {
        assert!(
            snapshot.is_live(key),
            "{key} is invoked by Gatsby: {:?}",
            snapshot.candidates
        );
    }
    for key in [
        "src/components/post-body.tsx#unusedPostBodyHelper",
        "src/templates/blog-post.tsx#unusedTemplateHelper",
        "src/templates/dead-template.tsx#default",
        "src/templates/dead-template.tsx#query",
        "src/templates/dead-template.tsx#unusedDeadHelper",
        "src/other/dead.tsx#default",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "{key} is not registered by createPage and must remain a candidate: {:?}",
            snapshot.candidates
        );
    }
    assert!(
        snapshot.entries.iter().any(|entry| {
            entry.symbol == "src/templates/blog-post.tsx#default"
                && entry.framework == "gatsby"
                && entry.kind == "page-template-export"
        }),
        "the template records its framework: {:?}",
        snapshot.entries
    );
}

#[test]
fn gatsby_pages_are_entries_only_with_the_dependency() {
    let snapshot = gatsby_app(
        r#"{"name":"gatsby-site","private":true,"dependencies":{"gatsby":"^5.14.0","react":"^18.3.1"}}"#,
        "export {};\n",
    );
    assert!(
        snapshot.is_live("src/pages/index.tsx#default"),
        "Gatsby renders src/pages defaults: {:?}",
        snapshot.candidates
    );
    assert!(
        snapshot.is_candidate("src/pages/index.tsx#unusedHomeHelper"),
        "only the page API is framework-invoked: {:?}",
        snapshot.candidates
    );

    let plain = gatsby_app(
        r#"{"name":"plain-site","private":true,"dependencies":{"react":"^18.3.1"}}"#,
        "export {};\n",
    );
    assert!(
        plain.is_candidate("src/pages/index.tsx#default"),
        "without Gatsby, src/pages is not a framework page directory: {:?}",
        plain.candidates
    );
}

#[test]
fn gatsby_dynamic_components_fall_back_to_the_template_directory() {
    let snapshot = gatsby_app(
        r#"{"name":"gatsby-dynamic","private":true,"dependencies":{"gatsby":"^5.14.0"}}"#,
        "export async function createPages({ actions, getTemplate }) {\n  actions.createPage({\n    path: \"/dynamic\",\n    component: getTemplate(),\n  });\n}\n",
    );
    for key in [
        "src/templates/blog-post.tsx#default",
        "src/templates/blog-post.tsx#query",
        "src/templates/blog-post.tsx#Head",
        "src/templates/dead-template.tsx#default",
        "src/templates/dead-template.tsx#query",
    ] {
        assert!(
            snapshot.is_live(key),
            "an opaque component protects the conventional directory: {key}: {:?}",
            snapshot.candidates
        );
    }
    for key in [
        "src/templates/blog-post.tsx#unusedTemplateHelper",
        "src/templates/dead-template.tsx#unusedDeadHelper",
        "src/other/dead.tsx#default",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "{key} is not a Gatsby page API export: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn gatsby_templates_require_the_dependency() {
    let snapshot = gatsby_app(
        r#"{"name":"plain-node","private":true,"dependencies":{"react":"^18.3.1"}}"#,
        "import path from \"path\";\nexport const createPages = ({ actions }: { actions: any }) => {\n  actions.createPage({ path: \"/blog\", component: path.resolve(\"./src/templates/blog-post.tsx\") });\n};\n",
    );
    for key in [
        "src/templates/blog-post.tsx#default",
        "src/templates/blog-post.tsx#query",
        "src/templates/blog-post.tsx#Head",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "without gatsby {key} is not framework-invoked: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn gatsby_plugin_templates_resolve_from_the_site_and_the_plugin() {
    let snapshot = discovered_snapshot(&[
        (
            "package.json",
            r#"{"name":"gatsby-plugin-site","private":true,"dependencies":{"gatsby":"^5.14.0"}}"#,
        ),
        (
            "plugins/local/gatsby-node.js",
            "const path = require(\"path\");\nexports.createPages = ({ actions }) => {\n  actions.createPage({ path: \"/site\", component: path.resolve(\"./src/templates/site-post.tsx\") });\n  actions.createPage({ path: \"/plugin\", component: path.resolve(__dirname, \"templates/plugin-post.tsx\") });\n};\n",
        ),
        (
            "src/templates/site-post.tsx",
            "export default function SitePost() { return <div>site</div>; }\n",
        ),
        (
            "plugins/local/templates/plugin-post.tsx",
            "export default function PluginPost() { return <div>plugin</div>; }\n",
        ),
    ]);
    for key in [
        "src/templates/site-post.tsx#default",
        "plugins/local/templates/plugin-post.tsx#default",
    ] {
        assert!(
            snapshot.is_live(key),
            "{key} is named by the plugin's createPage call: {:?}",
            snapshot.candidates
        );
    }
}

#[test]
fn gatsby_nested_node_files_are_not_registrations() {
    let snapshot = discovered_snapshot(&[
        (
            "package.json",
            r#"{"name":"gatsby-nested-node","private":true,"dependencies":{"gatsby":"^5.14.0"}}"#,
        ),
        (
            "src/gatsby-node.ts",
            "import path from \"path\";\nexport const createPages = ({ actions }: { actions: any }) => {\n  actions.createPage({ path: \"/blog\", component: path.resolve(\"./src/templates/blog-post.tsx\") });\n};\n",
        ),
        (
            "src/templates/blog-post.tsx",
            "export default function BlogPost() { return <div>post</div>; }\n",
        ),
    ]);
    assert!(
        snapshot.is_candidate("src/templates/blog-post.tsx#default"),
        "Gatsby does not execute src/gatsby-node.ts: {:?}",
        snapshot.candidates
    );
}
