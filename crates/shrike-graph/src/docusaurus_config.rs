//! Docusaurus documentation pages.
//!
//! Docusaurus renders MDX pages by directory convention instead of importing
//! them:
//!
//! - every `.mdx` file under `docs/`;
//! - every `.mdx` file under `blog/`;
//! - `versioned_docs/` pages, which may be MDX or React components.
//!
//! These modules are not imported by application code, so without the
//! convention their imports look dead. The package dependency gates the
//! convention; a non-Docusaurus `docs/` directory stays analyzable.

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

const PAGE_EXTENSIONS: &str = "{md,mdx,jsx,tsx}";

/// Compile the package-scoped Docusaurus page matcher.
#[must_use]
pub(crate) fn matcher(package_directory: &str) -> Option<GlobSet> {
    let prefix = if package_directory.is_empty() {
        String::new()
    } else {
        format!("{package_directory}/")
    };
    let patterns = [
        format!("{prefix}docs/**/*.{{md,mdx}}"),
        format!("{prefix}blog/**/*.{{md,mdx}}"),
        format!("{prefix}versioned_docs/**/*.{PAGE_EXTENSIONS}"),
    ];
    let mut builder = GlobSetBuilder::new();
    for pattern in &patterns {
        let Ok(glob) = GlobBuilder::new(pattern)
            .literal_separator(true)
            .empty_alternates(true)
            .build()
        else {
            continue;
        };
        builder.add(glob);
    }
    builder.build().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docs_blog_and_versioned_docs_are_pages() {
        let matcher = matcher("").expect("matcher");
        for path in [
            "docs/intro.mdx",
            "docs/guides/deploy.mdx",
            "docs/plain.md",
            "blog/hello.mdx",
            "blog/plain.md",
            "versioned_docs/version-1/guide.mdx",
            "versioned_docs/version-1/plain.md",
            "versioned_docs/version-1/Widget.tsx",
        ] {
            assert!(matcher.is_match(path), "{path} is a Docusaurus page");
        }
        for path in [
            "src/components/Callout.tsx",
            "docs/logo.svg",
            "blog/author.png",
        ] {
            assert!(
                !matcher.is_match(path),
                "{path} is not a Docusaurus MDX page"
            );
        }
    }

    #[test]
    fn pages_are_package_scoped() {
        let matcher = matcher("apps/docs").expect("matcher");
        assert!(matcher.is_match("apps/docs/docs/intro.mdx"));
        assert!(matcher.is_match("apps/docs/blog/hello.mdx"));
        assert!(!matcher.is_match("apps/site/docs/intro.mdx"));
        assert!(!matcher.is_match("docs/intro.mdx"));
    }
}
