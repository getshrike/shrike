use std::fs;

use shrike_core::EngineConfig;

use super::*;
use crate::config::strip_json_comments;

#[test]
fn resolves_relative_and_alias() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src/lib")).unwrap();
    fs::write(
        temp.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
    )
    .unwrap();
    let files = vec!["src/lib/a.ts".to_string(), "src/lib/index.ts".to_string()];
    let resolver = Resolver::new(temp.path(), &files, &EngineConfig::default());
    let resolved = resolver.resolve("src/app/page.ts", "../lib/a");
    assert_eq!(resolved.target.as_deref(), Some("src/lib/a.ts"));
    let alias = resolver.resolve("src/app/page.ts", "@/lib/a");
    assert_eq!(alias.target.as_deref(), Some("src/lib/a.ts"));
    let barrel = resolver.resolve("src/app/page.ts", "@/lib");
    assert_eq!(barrel.target.as_deref(), Some("src/lib/index.ts"));
    assert!(resolver.resolve("src/app/page.ts", "react").external);
}

#[test]
fn metro_expands_platform_variants_for_neutral_imports() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src/components")).unwrap();
    fs::create_dir_all(temp.path().join("src/screens")).unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{"name":"metro-app","dependencies":{"react-native":"^0.81.0"}}"#,
    )
    .unwrap();
    let files = vec![
        "src/App.tsx".to_string(),
        "src/components/Button.tsx".to_string(),
        "src/components/Button.ios.tsx".to_string(),
        "src/components/Button.android.tsx".to_string(),
        "src/components/Icon.ios.tsx".to_string(),
        "src/components/Icon.android.tsx".to_string(),
        "src/components/Card/index.ios.tsx".to_string(),
        "src/components/Card/index.android.tsx".to_string(),
        "src/screens/Home.native.tsx".to_string(),
    ];
    let resolver = Resolver::new(temp.path(), &files, &EngineConfig::default());

    let base = resolver.resolve("src/App.tsx", "./components/Button");
    assert_eq!(base.target.as_deref(), Some("src/components/Button.tsx"));
    assert_eq!(
        base.alternatives,
        vec![
            "src/components/Button.ios.tsx".to_string(),
            "src/components/Button.android.tsx".to_string(),
        ]
    );

    let variants_only = resolver.resolve("src/App.tsx", "./components/Icon");
    assert_eq!(
        variants_only.target.as_deref(),
        Some("src/components/Icon.ios.tsx")
    );
    assert_eq!(
        variants_only.alternatives,
        vec!["src/components/Icon.android.tsx".to_string()]
    );

    let index_variants = resolver.resolve("src/App.tsx", "./components/Card");
    assert_eq!(
        index_variants.target.as_deref(),
        Some("src/components/Card/index.ios.tsx")
    );
    assert_eq!(
        index_variants.alternatives,
        vec!["src/components/Card/index.android.tsx".to_string()]
    );

    let native = resolver.resolve("src/App.tsx", "./screens/Home");
    assert_eq!(
        native.target.as_deref(),
        Some("src/screens/Home.native.tsx")
    );
    assert!(native.alternatives.is_empty());

    let explicit = resolver.resolve("src/App.tsx", "./components/Button.ios");
    assert_eq!(
        explicit.target.as_deref(),
        Some("src/components/Button.ios.tsx")
    );
    assert!(explicit.alternatives.is_empty());
}

#[test]
fn platform_variants_require_a_metro_package() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src/components")).unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{"name":"plain-app","dependencies":{"react":"^19.0.0"}}"#,
    )
    .unwrap();
    let files = vec![
        "src/App.tsx".to_string(),
        "src/components/Button.tsx".to_string(),
        "src/components/Button.ios.tsx".to_string(),
        "src/components/Icon.ios.tsx".to_string(),
    ];
    let resolver = Resolver::new(temp.path(), &files, &EngineConfig::default());
    let base = resolver.resolve("src/App.tsx", "./components/Button");
    assert_eq!(base.target.as_deref(), Some("src/components/Button.tsx"));
    assert!(base.alternatives.is_empty());
    let icon = resolver.resolve("src/App.tsx", "./components/Icon");
    assert_eq!(icon.target, None);
    assert!(icon.alternatives.is_empty());
}

#[test]
fn resolves_root_package_self_reference() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{"name":"@corpus/pkg","exports":{".":"./src/index.ts"}}"#,
    )
    .unwrap();
    fs::write(temp.path().join("src/index.ts"), "export const api = 1;").unwrap();
    let files = vec!["src/index.ts".to_string()];
    let resolver = Resolver::new(temp.path(), &files, &EngineConfig::default());
    assert_eq!(resolver.packages().len(), 1);
    assert_eq!(
        resolver
            .package_for("src/index.ts")
            .map(|p| p.name.as_str()),
        Some("@corpus/pkg")
    );
    let resolved = resolver.resolve("app/page.ts", "@corpus/pkg");
    assert_eq!(resolved.target.as_deref(), Some("src/index.ts"));
}

#[test]
fn maps_conditional_build_exports_to_source_entry() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("source")).unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{"name":"@corpus/is","exports":{"types":"./distribution/index.d.ts","default":"./distribution/index.js"}}"#,
    )
    .unwrap();
    fs::write(temp.path().join("source/index.ts"), "export const api = 1;").unwrap();
    let files = vec!["source/index.ts".to_string()];
    let resolver = Resolver::new(temp.path(), &files, &EngineConfig::default());
    let package = resolver.package_for("source/index.ts").unwrap();
    assert_eq!(package.entry.as_deref(), Some("source/index.ts"));
    let resolved = resolver.resolve("test/test.ts", "@corpus/is");
    assert_eq!(resolved.target.as_deref(), Some("source/index.ts"));
    assert!(!resolved.external);
}

#[test]
fn maps_subpath_build_exports_to_source() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{"name":"@corpus/store","exports":{".":{"import":"./esm/index.mjs"},"./react":{"import":"./esm/react.mjs"}}}"#,
    )
    .unwrap();
    fs::write(temp.path().join("src/index.ts"), "export const api = 1;").unwrap();
    fs::write(temp.path().join("src/react.ts"), "export const react = 1;").unwrap();
    let files = vec!["src/index.ts".to_string(), "src/react.ts".to_string()];
    let resolver = Resolver::new(temp.path(), &files, &EngineConfig::default());
    let root = resolver.resolve("examples/demo/src/App.tsx", "@corpus/store");
    assert_eq!(root.target.as_deref(), Some("src/index.ts"));
    let subpath = resolver.resolve("examples/demo/src/App.tsx", "@corpus/store/react");
    assert_eq!(subpath.target.as_deref(), Some("src/react.ts"));
    let package = resolver.package_for("src/index.ts").unwrap();
    assert!(package.public_sources.contains("src/index.ts"));
    assert!(package.public_sources.contains("src/react.ts"));
}

#[test]
fn maps_conditional_export_flavors_to_public_sources() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{"name":"@corpus/forms","exports":{".":{"types":"./dist/index.d.ts","react-server":"./dist/react-server.esm.mjs","import":"./dist/index.esm.mjs","require":"./dist/index.cjs.js"}}}"#,
    )
    .unwrap();
    fs::write(temp.path().join("src/index.ts"), "export const api = 1;").unwrap();
    fs::write(
        temp.path().join("src/index.react-server.ts"),
        "export const serverApi = 1;",
    )
    .unwrap();
    let files = vec![
        "src/index.ts".to_string(),
        "src/index.react-server.ts".to_string(),
    ];
    let resolver = Resolver::new(temp.path(), &files, &EngineConfig::default());
    let package = resolver.package_for("src/index.ts").unwrap();
    assert_eq!(package.entry.as_deref(), Some("src/index.ts"));
    assert!(package.public_sources.contains("src/index.ts"));
    assert!(package.public_sources.contains("src/index.react-server.ts"));
}

#[test]
fn wildcard_exports_map_to_source_files() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src/middleware")).unwrap();
    fs::create_dir_all(temp.path().join("tests")).unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{"name":"@corpus/store","exports":{".":{"import":"./esm/index.mjs"},"./*":{"import":"./esm/*.mjs"}}}"#,
    )
    .unwrap();
    fs::write(temp.path().join("src/index.ts"), "export const api = 1;").unwrap();
    fs::write(
        temp.path().join("src/middleware/immer.ts"),
        "export const immer = 1;",
    )
    .unwrap();
    fs::write(
        temp.path().join("tests/example.test.ts"),
        "export const notPublic = 1;",
    )
    .unwrap();
    let files = vec![
        "src/index.ts".to_string(),
        "src/middleware/immer.ts".to_string(),
        "tests/example.test.ts".to_string(),
    ];
    let resolver = Resolver::new(temp.path(), &files, &EngineConfig::default());
    let package = resolver.package_for("src/index.ts").unwrap();
    assert!(package.public_sources.contains("src/index.ts"));
    assert!(package.public_sources.contains("src/middleware/immer.ts"));
    assert!(!package.public_sources.contains("tests/example.test.ts"));
}

#[test]
fn package_scripts_and_bin_mark_script_files() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("db/seeds")).unwrap();
    fs::create_dir_all(temp.path().join("scripts")).unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{
            "name": "app",
            "scripts": {
                "seed": "TZ=UTC npx tsx db/seeds/seed.ts",
                "check": "node --conditions=react-server scripts/check.mjs",
                "test": "vitest run tests/unit"
            },
            "bin": { "tool": "./cli.js" }
        }"#,
    )
    .unwrap();
    for path in [
        "db/seeds/seed.ts",
        "scripts/check.mjs",
        "cli.js",
        "src/index.ts",
    ] {
        fs::write(temp.path().join(path), "export const value = 1;").unwrap();
    }
    let files = vec![
        "db/seeds/seed.ts".to_string(),
        "scripts/check.mjs".to_string(),
        "cli.js".to_string(),
        "src/index.ts".to_string(),
    ];
    let resolver = Resolver::new(temp.path(), &files, &EngineConfig::default());
    let package = resolver.package_for("src/index.ts").unwrap();
    assert!(package.script_files.contains("db/seeds/seed.ts"));
    assert!(package.script_files.contains("scripts/check.mjs"));
    assert!(package.script_files.contains("cli.js"));
    assert!(!package.script_files.contains("src/index.ts"));
}

#[test]
fn strips_comments_and_trailing_commas() {
    let input = r#"{"a":1, // x
    "b":"// not a comment", /* y */}"#;
    let output = strip_json_comments(input);
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["b"], "// not a comment");
}
