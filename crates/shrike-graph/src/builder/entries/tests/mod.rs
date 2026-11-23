//! Entry-detection regression tests, grouped by framework or rule.

use std::fs;

use shrike_core::{EngineConfig, ServerActionPolicy};
use tempfile::tempdir;

use super::expo_config_plugin_paths;
use super::expo_router_root;
use crate::discovery::{analyze_paths, discover_files};
use crate::types::GraphSnapshot;

fn snapshot(files: &[(&str, &str)]) -> GraphSnapshot {
    snapshot_with_config(files, &EngineConfig::default())
}

fn snapshot_with_config(files: &[(&str, &str)], config: &EngineConfig) -> GraphSnapshot {
    let directory = tempdir().expect("tempdir");
    for (path, content) in files {
        let full = directory.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create parents");
        }
        fs::write(full, content).expect("write fixture");
    }
    let paths: Vec<String> = files.iter().map(|(path, _)| (*path).to_string()).collect();
    analyze_paths(directory.path(), config, &paths)
}

/// Analyze only the files the production discovery walk finds. Use this
/// when the feature under test is inventory, not entry classification.
fn discovered_snapshot(files: &[(&str, &str)]) -> GraphSnapshot {
    let directory = tempdir().expect("tempdir");
    for (path, content) in files {
        let full = directory.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create parents");
        }
        fs::write(full, content).expect("write fixture");
    }
    let paths = discover_files(directory.path(), &EngineConfig::default());
    analyze_paths(directory.path(), &EngineConfig::default(), &paths)
}

fn entry_symbols(snapshot: &GraphSnapshot) -> Vec<&str> {
    snapshot
        .entries
        .iter()
        .map(|entry| entry.symbol.as_str())
        .collect()
}

fn pages_route_findings(snapshot: &GraphSnapshot) -> Vec<(&str, &str)> {
    let mut findings: Vec<(&str, &str)> = snapshot
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == "dynamic_segment_name_conflict"
                || diagnostic.code == "pages_router_route_conflict"
        })
        .map(|diagnostic| {
            (
                diagnostic.code.as_str(),
                diagnostic.file.as_deref().unwrap_or(""),
            )
        })
        .collect();
    findings.sort();
    findings
}

fn legacy_react_app(bootstrap: &str) -> GraphSnapshot {
    snapshot(&[
        (
            "package.json",
            r#"{"name":"legacy-react-app","dependencies":{"react":"^17.0.2","react-dom":"^17.0.2"}}"#,
        ),
        ("src/client.js", bootstrap),
        (
            "src/App.js",
            "export default function App({ label }) { return <div>{label}</div>; }\nexport function unusedAppHelper() { return 1; }\n",
        ),
        (
            "src/format.js",
            "export function formatting() { return \"ready\"; }\nexport function unusedFormatter() { return 2; }\n",
        ),
        ("src/dead.js", "export function DeadThing() { return 3; }\n"),
    ])
}

fn assert_legacy_react_app_is_live(snapshot: &GraphSnapshot) {
    let entries = entry_symbols(snapshot);
    assert!(
        entries.contains(&"src/client.js#__module__"),
        "the render call must make the bootstrap a root: {entries:?}"
    );
    for key in ["src/App.js#default", "src/format.js#formatting"] {
        assert!(
            entries.contains(&key) || snapshot.is_live(key),
            "{key} is rendered by the root and must be live: {entries:?}"
        );
    }
    for key in [
        "src/App.js#unusedAppHelper",
        "src/format.js#unusedFormatter",
        "src/dead.js#DeadThing",
    ] {
        assert!(
            snapshot.is_candidate(key),
            "{key} is unused and must remain a candidate: {:?}",
            snapshot.candidates
        );
    }
}

fn storybook_mdx_app(package_json: &str) -> GraphSnapshot {
    snapshot(&[
        ("package.json", package_json),
        (
            ".storybook/main.ts",
            "export default { stories: [\"../src/**/*.mdx\"] };\n",
        ),
        (
            "src/DocsCard.tsx",
            "export function DocsCard() { return <div>docs</div>; }\nexport function unusedDocsHelper() { return 1; }\n",
        ),
        (
            "src/Docs.mdx",
            "import { DocsCard } from \"./DocsCard\";\n\n# Documentation\n\n<DocsCard />\n",
        ),
        (
            "src/dead.tsx",
            "export function DeadThing() { return 2; }\n",
        ),
    ])
}

fn custom_story_app(package_json: &str) -> GraphSnapshot {
    snapshot(&[
        ("package.json", package_json),
        (
            ".storybook/main.ts",
            r#"const config = {
  stories: [
    "../src/**/*.demo.tsx",
    { directory: "../src", files: "**/*.preview.tsx" },
  ],
  framework: "@storybook/react-vite",
};

export default config;
"#,
        ),
        (
            "src/Button.demo.tsx",
            "import { Button } from \"./button\";\n\nexport default function ButtonDemo() {\n  return <Button />;\n}\n\nexport function unusedDemoExport() {\n  return 1;\n}\n",
        ),
        (
            "src/button.tsx",
            "export function Button() {\n  return <button />;\n}\n\nexport function unusedButtonHelper() {\n  return 2;\n}\n",
        ),
        (
            "src/Card.preview.tsx",
            "import { Card } from \"./card\";\n\nexport default function CardPreview() {\n  return <Card />;\n}\n",
        ),
        (
            "src/card.tsx",
            "export function Card() {\n  return <div />;\n}\n",
        ),
        (
            "src/dead.tsx",
            "export function Dead() {\n  return null;\n}\n",
        ),
    ])
}

fn ladle_app(package_json: &str) -> GraphSnapshot {
    snapshot(&[
        ("package.json", package_json),
        (
            ".ladle/config.mjs",
            r#"export default {
  stories: ["src/**/*.stories.tsx", "src/**/*.demo.tsx"],
  viteConfig: "./ladle-vite.ts",
};
"#,
        ),
        (
            ".ladle/components.tsx",
            "import { LadleShell } from \"./shell\";\nexport const Provider = ({ children }) => <LadleShell>{children}</LadleShell>;\nexport function unusedComponentsHelper() { return 0; }\n",
        ),
        (
            ".ladle/shell.tsx",
            "export function LadleShell({ children }) { return <div>{children}</div>; }\nexport function unusedShellHelper() { return 1; }\n",
        ),
        (
            "ladle-vite.ts",
            "import { plugin } from \"./ladle-plugin\";\nexport default { plugins: [plugin()] };\nexport function unusedViteHelper() { return 2; }\n",
        ),
        (
            "ladle-plugin.ts",
            "export function plugin() { return { name: \"ladle-fixture\" }; }\n",
        ),
        (
            "src/components/button.tsx",
            "export function Button() { return <button />; }\nexport function unusedButtonHelper() { return 3; }\n",
        ),
        (
            "src/button.stories.tsx",
            "import { Button } from \"./components/button\";\nexport const Primary = () => <Button />;\nexport const unusedStoryHelper = () => 4;\n",
        ),
        (
            "src/widget.demo.tsx",
            "export function DemoWidget() { return <section />; }\nexport function unusedDemoHelper() { return 5; }\n",
        ),
        (
            "src/dead.tsx",
            "export function DeadThing() { return 6; }\n",
        ),
    ])
}

fn ladle_default_app(package_json: &str) -> GraphSnapshot {
    snapshot(&[
        ("package.json", package_json),
        (
            ".ladle/components.tsx",
            "import { LadleShell } from \"./shell\";\nexport const Provider = ({ children }) => <LadleShell>{children}</LadleShell>;\n",
        ),
        (
            ".ladle/shell.tsx",
            "export function LadleShell({ children }) { return <div>{children}</div>; }\n",
        ),
        (
            "src/MdxCard.tsx",
            "export function MdxCard() { return <div>docs</div>; }\nexport function unusedMdxHelper() { return 1; }\n",
        ),
        (
            "src/Intro.stories.mdx",
            "import { MdxCard } from \"./MdxCard\";\n\n# Introduction\n\n<MdxCard />\n",
        ),
    ])
}

fn next_page_extensions_fixture(page_extensions: bool) -> Vec<(&'static str, &'static str)> {
    let config = if page_extensions {
        "const nextConfig = { pageExtensions: ['js', 'jsx', 'ts', 'tsx', 'md', 'mdx'] };\nexport default nextConfig;\n"
    } else {
        "const nextConfig = { reactStrictMode: true };\nexport default nextConfig;\n"
    };
    vec![
        (
            "package.json",
            r#"{"name":"docs-app","dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#,
        ),
        ("next.config.ts", config),
        (
            "app/page.md",
            "import { Header } from \"../components/header\";\n\n# Home\n\n<Header />\n",
        ),
        (
            "components/header.tsx",
            "export function Header() { return <h1>hi</h1>; }\nexport function unusedHeaderHelper() { return 1; }\n",
        ),
        (
            "pages/about.md",
            "import { Footer } from \"../components/footer\";\n\n# About\n\n<Footer />\n",
        ),
        (
            "components/footer.tsx",
            "export function Footer() { return <footer />; }\n",
        ),
        (
            "app/blog/post.md",
            "import { DeadCard } from \"../../components/dead-card\";\n\n# Post\n\n<DeadCard />\n",
        ),
        (
            "components/dead-card.tsx",
            "export function DeadCard() { return <div />; }\n",
        ),
    ]
}

fn docusaurus_app(package_json: &str) -> GraphSnapshot {
    discovered_snapshot(&[
        ("package.json", package_json),
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","jsx":"react-jsx","paths":{"@site/*":["./*"]}}}"#,
        ),
        (
            "docs/intro.mdx",
            "import Callout from \"@site/src/components/Callout\";\n\n# Introduction\n\n<Callout>Read this.</Callout>\n",
        ),
        (
            "blog/hello.mdx",
            "import AuthorCard from \"@site/src/components/AuthorCard\";\n\n# Hello\n\n<AuthorCard name=\"Mark\" />\n",
        ),
        (
            "versioned_docs/version-1/guide.mdx",
            "import VersionNote from \"@site/src/components/VersionNote\";\n\n<VersionNote />\n",
        ),
        (
            "src/components/Callout.tsx",
            "export default function Callout({ children }: { children: React.ReactNode }) { return <aside>{children}</aside>; }\nexport function unusedCalloutHelper() { return 1; }\n",
        ),
        (
            "src/components/AuthorCard.tsx",
            "export default function AuthorCard({ name }: { name: string }) { return <div>{name}</div>; }\nexport function unusedAuthorHelper() { return 2; }\n",
        ),
        (
            "src/components/VersionNote.tsx",
            "export default function VersionNote() { return <p>version</p>; }\nexport function unusedVersionHelper() { return 3; }\n",
        ),
        (
            "src/components/dead.tsx",
            "export default function DeadComponent() { return <div>dead</div>; }\n",
        ),
    ])
}

fn gatsby_app(package_json: &str, node_source: &str) -> GraphSnapshot {
    discovered_snapshot(&[
        ("package.json", package_json),
        (
            "gatsby-config.ts",
            "export default { siteMetadata: { title: \"fixture\" }, plugins: [] };\n",
        ),
        ("gatsby-node.ts", node_source),
        (
            "src/pages/index.tsx",
            "import { PostBody } from \"../components/post-body\";\nexport default function Home() { return <PostBody title=\"home\" />; }\nexport function unusedHomeHelper() { return 4; }\n",
        ),
        (
            "src/components/post-body.tsx",
            "export function PostBody({ title }: { title: string }) { return <article>{title}</article>; }\nexport function unusedPostBodyHelper() { return 1; }\n",
        ),
        (
            "src/templates/blog-post.tsx",
            "import { PostBody } from \"../components/post-body\";\nexport default function BlogPost() { return <PostBody title=\"post\" />; }\nexport const query = `query BlogPost { post { title } }`;\nexport function Head() { return <title>Post</title>; }\nexport function unusedTemplateHelper() { return 2; }\n",
        ),
        (
            "src/templates/dead-template.tsx",
            "export default function DeadTemplate() { return <div>dead</div>; }\nexport const query = `query Dead { post { title } }`;\nexport function unusedDeadHelper() { return 3; }\n",
        ),
        (
            "src/other/dead.tsx",
            "export default function DeadComponent() { return <div>dead</div>; }\n",
        ),
    ])
}

mod core;
mod dynamic;
mod expo;
mod frameworks;
mod next;
mod react_router;
