//! Deterministic glob matching for include/ignore patterns.
//!
//! Supported syntax:
//! - `*` matches within one path segment;
//! - `?` matches one character within a segment;
//! - `**` matches zero or more whole segments;
//! - everything else is literal and case-sensitive.
//!
//! The matcher is intentionally small. It is used for repository inclusion,
//! generated-file detection, public-API patterns, and dynamic-import prefixes.

/// Match a normalized relative path against a glob pattern.
#[must_use]
pub fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim();
    let pattern = pattern.strip_prefix("./").unwrap_or(pattern);
    let pattern = pattern.strip_suffix('/').unwrap_or(pattern);
    let path = path.strip_prefix("./").unwrap_or(path);
    if pattern.is_empty() {
        return path.is_empty();
    }
    let pat: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let seg: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match_segments(&pat, &seg)
}

fn match_segments(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => {
            if match_segments(rest, path) {
                return true;
            }
            if let Some((_, tail)) = path.split_first() {
                return match_segments(pattern, tail);
            }
            false
        }
        Some((head, rest)) => match path.split_first() {
            None => false,
            Some((segment, tail)) => match_segment(head, segment) && match_segments(rest, tail),
        },
    }
}

fn match_segment(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut dp = vec![vec![false; t.len() + 1]; p.len() + 1];
    dp[0][0] = true;
    for i in 0..p.len() {
        if p[i] == '*' {
            dp[i + 1][0] = dp[i][0];
        }
        for j in 0..t.len() {
            dp[i + 1][j + 1] = match p[i] {
                '*' => dp[i][j + 1] || dp[i + 1][j],
                '?' => dp[i][j],
                ch => dp[i][j] && ch == t[j],
            };
        }
    }
    dp[p.len()][t.len()]
}

/// True when `path` matches any include pattern (or the include list is empty)
/// and no ignore pattern.
#[must_use]
pub fn is_included(path: &str, include: &[String], ignore: &[String]) -> bool {
    if ignore.iter().any(|pattern| glob_match(pattern, path)) {
        return false;
    }
    include.is_empty() || include.iter().any(|pattern| glob_match(pattern, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_double_star() {
        assert!(glob_match("src/**", "src/a.ts"));
        assert!(glob_match("src/**", "src/nested/deep/a.ts"));
        assert!(glob_match("**/*.ts", "a.ts"));
        assert!(glob_match("**/*.ts", "src/a.ts"));
        assert!(!glob_match("src/**", "test/a.ts"));
        assert!(!glob_match("*.ts", "src/a.ts"));
    }

    #[test]
    fn matches_directory_ignore() {
        assert!(glob_match("node_modules/**", "node_modules/pkg/index.js"));
        assert!(glob_match(
            "**/node_modules/**",
            "apps/web/node_modules/x.js"
        ));
        assert!(!glob_match("node_modules/**", "src/node_modules.ts"));
    }

    #[test]
    fn include_and_ignore_precedence() {
        let include = vec!["src/**".to_string()];
        let ignore = vec!["src/generated/**".to_string()];
        assert!(is_included("src/a.ts", &include, &ignore));
        assert!(!is_included("src/generated/a.ts", &include, &ignore));
        assert!(!is_included("test/a.ts", &include, &ignore));
        assert!(is_included("anything.ts", &[], &ignore));
    }

    #[test]
    fn question_mark_matches_one_character() {
        assert!(glob_match("src/?.ts", "src/a.ts"));
        assert!(!glob_match("src/?.ts", "src/ab.ts"));
    }
}
