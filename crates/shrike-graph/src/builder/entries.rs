use std::collections::BTreeSet;

use shrike_core::EntryPolicy;
use shrike_core::ids::module_symbol_key;
use shrike_syntax::ParseStatus;

use super::Builder;
use crate::types::EntryRecord;
use crate::util::route_of;

impl<'a> Builder<'a> {
    pub(super) fn detect_entries(&mut self) {
        let summaries = self.summaries.clone();
        for summary in &summaries {
            if summary.parse_status != ParseStatus::Ok {
                continue;
            }
            let path = summary.path.as_str();
            let module = module_symbol_key(path);
            let add = |builder: &mut Self,
                       symbol: String,
                       framework: &str,
                       kind: &str,
                       route: Option<String>,
                       runtime: &str| {
                if let Some(record) = builder
                    .symbols
                    .iter_mut()
                    .find(|record| record.key == symbol)
                {
                    record.entry = true;
                    builder.entries.push(EntryRecord {
                        symbol: symbol.clone(),
                        framework: framework.to_string(),
                        kind: kind.to_string(),
                        route,
                        runtime: runtime.to_string(),
                        certainty: "certain".to_string(),
                    });
                }
            };
            if summary.executable {
                add(self, module.clone(), "executable", "tool", None, "both");
            }
            let file_name = path.rsplit('/').next().unwrap_or(path);
            let stem = file_name
                .rsplit_once('.')
                .map_or(file_name, |(stem, _)| stem);
            let route = route_of(path);
            let is_app = path.starts_with("app/") || path.contains("/app/");
            let is_pages = path.starts_with("pages/") || path.contains("/pages/");
            // Next.js middleware/proxy live at the project root (or `src/`),
            // not under `app/`, but they are still framework entrypoints.
            let root_convention = matches!(stem, "middleware" | "proxy")
                && path
                    .strip_suffix(".tsx")
                    .or_else(|| path.strip_suffix(".ts"))
                    .or_else(|| path.strip_suffix(".jsx"))
                    .or_else(|| path.strip_suffix(".js"))
                    .is_some_and(|base| {
                        matches!(
                            base,
                            "middleware" | "proxy" | "src/middleware" | "src/proxy"
                        )
                    });
            let app_entry = (is_app
                && matches!(
                    stem,
                    "page"
                        | "layout"
                        | "template"
                        | "loading"
                        | "error"
                        | "global-error"
                        | "not-found"
                        | "route"
                        | "default"
                        | "sitemap"
                        | "robots"
                        | "manifest"
                        | "icon"
                        | "apple-icon"
                        | "opengraph-image"
                        | "twitter-image"
                        | "middleware"
                        | "proxy"
                        | "instrumentation"
                        | "instrumentation-client"
                ))
                || root_convention;
            let pages_entry = is_pages;
            if app_entry || pages_entry {
                for name in [
                    "default",
                    "page",
                    "layout",
                    "route",
                    "metadata",
                    "generateMetadata",
                    "viewport",
                    "generateViewport",
                    "generateStaticParams",
                    "generateSitemaps",
                    "getServerSideProps",
                    "getStaticProps",
                    "getStaticPaths",
                    "getInitialProps",
                    "register",
                    "onRequestError",
                    "onRequestInit",
                    "proxy",
                    "middleware",
                    "config",
                    "dynamic",
                    "revalidate",
                    "runtime",
                    "fetchCache",
                    "dynamicParams",
                    "preferredRegion",
                    "maxDuration",
                    "GET",
                    "POST",
                    "PUT",
                    "PATCH",
                    "DELETE",
                    "HEAD",
                    "OPTIONS",
                ] {
                    if let Some(key) = self.resolve_export(path, name, &mut BTreeSet::new()) {
                        add(
                            self,
                            key,
                            if app_entry { "next-app" } else { "next-pages" },
                            if name == "default" { "route" } else { "export" },
                            route.clone(),
                            "both",
                        );
                    }
                }
                add(
                    self,
                    module.clone(),
                    if app_entry { "next-app" } else { "next-pages" },
                    "module",
                    route,
                    "both",
                );
            }
            // Ambient declarations and declaration files describe types that
            // are visible program-wide (or are published API), not values that
            // can be proven unused from the module graph. Never propose them.
            let declaration_file = matches!(
                path.rsplit_once('.'),
                Some((_, "ts" | "mts" | "cts")) if path.contains(".d.")
            );
            for declaration in &summary.declarations {
                let ambient = summary.ambient_modules.contains(&declaration.name);
                if !ambient && !declaration_file {
                    continue;
                }
                let key = self
                    .locals
                    .get(path)
                    .and_then(|locals| locals.get(&declaration.name))
                    .cloned();
                if let Some(key) = key {
                    add(self, key, "ambient", "declaration", None, "both");
                }
            }
            if summary.has_directive("use server") || summary.has_directive("use client") {
                for (name, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    let _ = name;
                    add(self, key, "react", "client-boundary", None, "both");
                }
                add(self, module.clone(), "react", "module", None, "both");
            }
            if summary.has_directive("use server") {
                add(
                    self,
                    module.clone(),
                    "next-app",
                    "server-action",
                    None,
                    "server",
                );
            }
            let test = path.contains(".test.")
                || path.contains(".spec.")
                || path.contains("__tests__/")
                || path.contains("/__fixtures__/")
                || path.contains("/fixtures/")
                || path.contains("/e2e/")
                || path.starts_with("test/")
                || path.starts_with("tests/")
                || path.contains("/test/")
                || path.contains("/tests/");
            if self.config.test_entries
                && self.config.entry_policy != EntryPolicy::Production
                && test
            {
                add(self, module.clone(), "tests", "test", None, "both");
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(self, key, "tests", "test-export", None, "both");
                }
            }
            // Build/test tool configs are consumed directly by the tool, so
            // both the module and everything it exports are entries. Matching
            // on the `.config` suffix covers rollup, drizzle, shrike, and any
            // future tool without maintaining a name list.
            let config_file =
                stem.ends_with(".config") || matches!(stem, "tsconfig" | "instrumentation");
            if config_file {
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(self, key, "config", "config-export", None, "both");
                }
                add(self, module.clone(), "config", "config", None, "both");
            }
            if summary
                .module_refs
                .identifiers
                .iter()
                .any(|id| id == "createRoot" || id == "hydrateRoot")
            {
                add(self, module.clone(), "react", "root", None, "client");
                for identifier in &summary.module_refs.identifiers {
                    if let Some(target) = self.local_or_imported(path, identifier) {
                        add(self, target, "react", "root-reference", None, "client");
                    }
                }
            }
            let package_flags = self.resolver.package_for(path).map(|package| {
                (
                    package.script_files.contains(path),
                    package.public_sources.contains(path) || package.entry.as_deref() == Some(path),
                )
            });
            if let Some((is_script, is_public_source)) = package_flags {
                if is_script {
                    add(self, module.clone(), "scripts", "script", None, "both");
                }
                if is_public_source {
                    for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                        add(self, key, "packages", "public-export", None, "both");
                    }
                    add(
                        self,
                        module.clone(),
                        "packages",
                        "package-entry",
                        None,
                        "both",
                    );
                }
            }
            if self.public_files.contains(path) {
                for (_, key) in self.exports.get(path).cloned().unwrap_or_default() {
                    add(self, key, "packages", "public-api", None, "both");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use shrike_core::EngineConfig;
    use tempfile::tempdir;

    use crate::discovery::analyze_paths;
    use crate::types::GraphSnapshot;

    fn snapshot(files: &[(&str, &str)]) -> GraphSnapshot {
        let directory = tempdir().expect("tempdir");
        for (path, content) in files {
            let full = directory.path().join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).expect("create parents");
            }
            fs::write(full, content).expect("write fixture");
        }
        let paths: Vec<String> = files.iter().map(|(path, _)| (*path).to_string()).collect();
        analyze_paths(directory.path(), &EngineConfig::default(), &paths)
    }

    fn entry_symbols(snapshot: &GraphSnapshot) -> Vec<&str> {
        snapshot
            .entries
            .iter()
            .map(|entry| entry.symbol.as_str())
            .collect()
    }

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
    fn plain_test_directories_are_entries() {
        let snapshot = snapshot(&[
            ("source/index.ts", "export const api = 1;\n"),
            (
                "test/test.ts",
                "import { api } from '../source/index.ts';\nconsole.log(api);\n",
            ),
            ("tests/unit/helper.ts", "export const helper = 1;\n"),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(entries.contains(&"test/test.ts#__module__"), "{entries:?}");
        assert!(
            entries.contains(&"tests/unit/helper.ts#__module__"),
            "{entries:?}"
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
    fn documented_tool_scripts_are_entries() {
        let snapshot = snapshot(&[
            (
                "scripts/generate-report.ts",
                "/**\n * Report generator.\n *\n * Usage: npx tsx scripts/generate-report.ts\n */\nfunction main() {\n  console.log('report');\n}\nmain();\n",
            ),
            (
                "src/library.ts",
                "/** Usage: this is documentation, not an entry. */\nexport const value = 1;\n",
            ),
        ]);
        let entries = entry_symbols(&snapshot);
        assert!(
            entries.contains(&"scripts/generate-report.ts#__module__"),
            "{entries:?}"
        );
        assert!(
            snapshot
                .live
                .iter()
                .any(|key| key == "scripts/generate-report.ts#main"),
            "tool body must stay live: {:?}",
            snapshot.live
        );
        assert!(
            !entries.contains(&"src/library.ts#__module__"),
            "{entries:?}"
        );
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
}
