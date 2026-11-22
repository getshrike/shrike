//! Next.js Pages Router unused page-data diagnostics.
//!
//! This is a warning, never a deletion candidate: a returned prop key is not a
//! symbol, and removing it changes the page's data contract. The parser has
//! already abstained unless both the data function's return value and the page
//! component's props are statically readable, so a record here is a key that
//! the page component never declares.

use std::collections::BTreeSet;

use shrike_core::Diagnostic;
use shrike_syntax::ParseStatus;

use super::Builder;

impl Builder<'_> {
    /// Warn about props returned by a Pages Router data function but never
    /// consumed by the page component. Only files that Next.js detects as
    /// Pages Router routes are checked; an ordinary module that happens to
    /// export `getStaticProps` is not a page.
    pub(super) fn detect_unused_page_data_keys(&mut self) {
        let page_extensions = self.configured_page_extensions();
        let pages: BTreeSet<&str> = self
            .entries
            .iter()
            .filter(|entry| entry.framework == "next-pages" && entry.kind == "route")
            .map(|entry| entry.symbol.split('#').next().unwrap_or(&entry.symbol))
            .collect();
        if pages.is_empty() {
            return;
        }
        let summaries = self.summaries.clone();
        for summary in summaries.iter() {
            if summary.parse_status != ParseStatus::Ok || summary.unused_page_data_keys.is_empty() {
                continue;
            }
            if !pages.contains(summary.path.as_str()) {
                continue;
            }
            // API routes and `_app`/`_document`/`_error` are entries too, but
            // Next.js does not treat their data functions as page props. A
            // nested `_app` or `_error` is an ordinary page, so the classifier
            // checks the root-anchored reserved names.
            let package_directory = self
                .resolver
                .package_for(&summary.path)
                .map_or_else(String::new, |package| package.directory.clone());
            if !super::files::is_pages_router_data_page(
                &summary.path,
                &package_directory,
                page_extensions.get(&package_directory),
            ) {
                continue;
            }
            if self
                .file_generated
                .get(&summary.path)
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            for record in &summary.unused_page_data_keys {
                self.diagnostics.push(
                    Diagnostic::warning(
                        "unused_page_data_key",
                        format!(
                            "`{}` returns prop `{}` but the page component never reads it",
                            record.function, record.key
                        ),
                    )
                    .in_file(&summary.path)
                    .at(record.span),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use shrike_core::{Diagnostic, EngineConfig};
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

    fn data_diagnostics(snapshot: &GraphSnapshot) -> Vec<&Diagnostic> {
        snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "unused_page_data_key")
            .collect()
    }

    fn next_package() -> &'static str {
        r#"{"name":"probe","private":true,"dependencies":{"next":"^16.0.0","react":"^19.0.0"}}"#
    }

    #[test]
    fn reports_returned_key_the_page_never_reads() {
        let snapshot = snapshot(&[
            ("package.json", next_package()),
            (
                "pages/index.tsx",
                r#"
export async function getStaticProps() {
  return { props: { used: 1, unused: 2 } };
}
export default function Home({ used }: { used: number }) {
  return <div>{used}</div>;
}
"#,
            ),
        ]);
        let findings = data_diagnostics(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].file.as_deref(), Some("pages/index.tsx"));
        assert!(findings[0].message.contains("`getStaticProps`"));
        assert!(findings[0].message.contains("`unused`"));
        assert_eq!(findings[0].span.map(|span| span.line), Some(3));
    }

    #[test]
    fn supports_typed_props_and_get_initial_props() {
        let snapshot = snapshot(&[
            ("package.json", next_package()),
            (
                "pages/about.tsx",
                r#"
interface AboutProps { used: number; unused: string; }
function About(props: AboutProps) {
  return <div>{props.used}</div>;
}
About.getInitialProps = async () => ({ used: 1, unused: "x" });
export default About;
"#,
            ),
        ]);
        let findings = data_diagnostics(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].message.contains("`getInitialProps`"));
        assert!(findings[0].message.contains("`unused`"));
    }

    #[test]
    fn does_not_report_outside_pages_router_routes() {
        let snapshot = snapshot(&[
            ("package.json", next_package()),
            (
                "src/data.ts",
                r#"
export async function getStaticProps() {
  return { props: { used: 1, unused: 2 } };
}
export default function Home({ used }: { used: number }) {
  return <div>{used}</div>;
}
"#,
            ),
        ]);
        assert!(data_diagnostics(&snapshot).is_empty());
    }

    #[test]
    fn does_not_report_api_routes_or_reserved_pages() {
        let snapshot = snapshot(&[
            ("package.json", next_package()),
            (
                "pages/api.ts",
                r#"
export async function getStaticProps() {
  return { props: { unused: 1 } };
}
export default function api({ used }: { used: number }) {
  return used;
}
"#,
            ),
            (
                "pages/api/hello.ts",
                r#"
export async function getStaticProps() {
  return { props: { unused: 1 } };
}
export default function handler({ used }: { used: number }) {
  return used;
}
"#,
            ),
            (
                "pages/_app.tsx",
                r#"
function App({ Component, pageProps }: any) {
  return <Component {...pageProps} />;
}
App.getInitialProps = async () => ({ pageProps: {}, unused: 1 });
export default App;
"#,
            ),
            (
                "pages/_document.tsx",
                r#"
function Document({ used }: any) {
  return <html><body>{used}</body></html>;
}
Document.getInitialProps = async () => ({ used: 1, unused: 2 });
export default Document;
"#,
            ),
            (
                "pages/_error.tsx",
                r#"
function ErrorPage({ statusCode }: any) {
  return <div>{statusCode}</div>;
}
ErrorPage.getInitialProps = async () => ({ statusCode: 404, unused: 1 });
export default ErrorPage;
"#,
            ),
            // Only root-anchored `_app`/`_document`/`_error` are reserved; a
            // nested one is an ordinary page and stays analyzable.
            (
                "pages/guide/_error.tsx",
                r#"
function GuideError({ used }: any) {
  return <div>{used}</div>;
}
GuideError.getInitialProps = async () => ({ used: 1, unused: 2 });
export default GuideError;
"#,
            ),
        ]);
        let findings = data_diagnostics(&snapshot);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].file.as_deref(), Some("pages/guide/_error.tsx"));
        assert!(findings[0].message.contains("`unused`"));
    }

    #[test]
    fn abstains_on_opaque_return_and_whole_object_props() {
        let snapshot = snapshot(&[
            ("package.json", next_package()),
            (
                "pages/opaque.tsx",
                r#"
const data = { used: 1, unused: 2 };
export async function getStaticProps() {
  return { props: data };
}
export default function Opaque({ used }: { used: number }) {
  return <div>{used}</div>;
}
"#,
            ),
            (
                "pages/spread.tsx",
                r#"
export async function getStaticProps() {
  return { props: { ...base, unused: 2 } };
}
export default function Spread({ used }: { used: number }) {
  return <div>{used}</div>;
}
"#,
            ),
            (
                "pages/whole.tsx",
                r#"
export async function getStaticProps() {
  return { props: { used: 1, unused: 2 } };
}
export default function Whole(props: { used: number; unused: number }) {
  return <pre>{JSON.stringify(props)}</pre>;
}
"#,
            ),
        ]);
        assert!(data_diagnostics(&snapshot).is_empty());
    }
}
