// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

pub(crate) fn glob_match(pattern: &str, path: &str) -> bool {
    let regex_pattern = glob_to_regex(pattern);
    regex_like_match(&regex_pattern, path)
}

pub(crate) fn glob_to_regex(pattern: &str) -> String {
    let mut regex = String::with_capacity(pattern.len() + 2);
    regex.push('^');

    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                // `**` matches any number of path segments
                i += 2;
                if i < chars.len() && chars[i] == '/' {
                    // `**/` is ZERO or more segments, so the separator is part
                    // of the optional group. Emitting `.*/` instead required at
                    // least one leading segment, so `**/target/**` matched
                    // `a/target/x` but not `target/x` -- and root-level paths
                    // are exactly where `target/` and `Cargo.lock` live, so
                    // `[tool.comply] ignore` silently did nothing.
                    regex.push_str("(?:.*/)?");
                    i += 1;
                } else {
                    regex.push_str(".*");
                }
            }
            '*' => {
                regex.push_str("[^/]*");
                i += 1;
            }
            '?' => {
                regex.push_str("[^/]");
                i += 1;
            }
            '.' | '+' | '^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\' => {
                regex.push('\\');
                regex.push(chars[i]);
                i += 1;
            }
            c => {
                regex.push(c);
                i += 1;
            }
        }
    }

    // A pattern naming a directory covers what is under it, the way the same
    // line in a `.gitignore` would. Without this `dist/` compiled to `^dist/$`,
    // which matches no file at all, so every `ignore` entry written in that
    // form silently did nothing.
    if pattern.ends_with('/') {
        regex.push_str(".*");
    }

    regex.push('$');
    regex
}

fn regex_like_match(pattern: &str, text: &str) -> bool {
    // Simple regex match without pulling in the full regex crate at this layer.
    // We use a basic approach since our patterns are simple globs.
    let Ok(re) = regex::Regex::new(pattern) else {
        return false;
    };
    re.is_match(text)
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Annotation, Config, Precedence, WalkOptions};

    #[test]
    fn a_trailing_slash_covers_everything_beneath_the_directory() {
        // `.gitignore` spells a directory `dist/`, and an `ignore` entry copied
        // from one has to mean the same. Matching only the literal `dist/` left
        // every file under it unignored, which is how a project that ignored
        // `dist/` had its build output annotated -- and the vendored
        // third-party headers inside it rewritten.
        assert!(glob_match("dist/", "dist/index.js"));
        assert!(glob_match("dist/", "dist/sub/deep.js"));
        assert!(
            !glob_match("dist/", "distant.js"),
            "the slash must not be optional"
        );
    }

    #[test]
    fn a_leading_double_star_matches_at_the_root_too() {
        // `**/target/**` must cover `target/debug/x`, not only `a/target/debug/x`.
        // gitignore and globset both treat a leading `**/` as zero-or-more
        // segments; requiring one made `[tool.comply] ignore` silently miss
        // every top-level path, which is exactly where `target/` and
        // `Cargo.lock` live.
        let re = regex::Regex::new(&glob_to_regex("**/target/**")).unwrap();
        assert!(
            re.is_match("target/debug/x"),
            "root-level target must match"
        );
        assert!(
            re.is_match("crates/a/target/debug/x"),
            "nested must still match"
        );

        let lock = regex::Regex::new(&glob_to_regex("**/*.lock")).unwrap();
        assert!(lock.is_match("Cargo.lock"), "root-level lock must match");
        assert!(
            lock.is_match("crates/a/Cargo.lock"),
            "nested must still match"
        );
    }

    #[test]
    fn match_simple_glob() {
        let config = Config {
            annotations: vec![Annotation {
                path: "src/**".to_string(),
                copyright: vec!["2026 A".to_string()],
                license: "MIT".to_string(),
                precedence: Precedence::Closest,
            }],
            default_license: None,
            default_copyright: vec![],
            ignore: vec![],
            spdx_package_name: None,
            spdx_package_supplier: None,
            source_patterns: vec![],
            walk: WalkOptions::default(),
            comment_styles: vec![],
        };

        let matched = config.match_annotations("src/lib.rs");
        assert_eq!(matched.len(), 1);
    }

    #[test]
    fn glob_star_star_matches_subdirs() {
        assert!(glob_match("src/**", "src/lib.rs"));
        assert!(glob_match("src/**", "src/sub/mod.rs"));
        assert!(!glob_match("src/**", "other/lib.rs"));
    }

    #[test]
    fn glob_star_star_slash_spans_zero_segments_as_well() {
        assert!(glob_match("**/*.png", "assets/img/logo.png"));
        // This used to assert the opposite -- "`**/` requires at least one
        // leading separator" -- which disagrees with the reference tool. Checked
        // against reuse 6.2.0: a REUSE.toml annotation with `path = "**/*.png"`
        // covers a root-level `logo.png`, and `reuse lint` reports nothing.
        assert!(
            glob_match("**/*.png", "logo.png"),
            "`**/` spans zero or more segments, so a root-level file matches"
        );
        assert_eq!(glob_to_regex("**/*.png"), r"^(?:.*/)?[^/]*\.png$");
    }

    #[test]
    fn glob_star_does_not_cross_dir() {
        assert!(glob_match("*.rs", "lib.rs"));
        assert!(!glob_match("*.rs", "src/lib.rs"));
    }

    #[test]
    fn glob_question_matches_single_char() {
        assert!(glob_match("?.rs", "a.rs"));
        assert!(!glob_match("?.rs", "ab.rs"));
    }

    #[test]
    fn regex_like_match_basic() {
        assert!(regex_like_match("^test$", "test"));
        assert!(!regex_like_match("^test$", "testing"));
    }

    #[test]
    fn regex_like_match_with_wildcards() {
        // Test that glob patterns are converted to regex correctly
        assert!(regex_like_match("^src/.*", "src/lib.rs"));
        assert!(regex_like_match("^src/.*", "src/main.rs"));
        assert!(!regex_like_match("^src/.*", "other/lib.rs"));
    }

    #[test]
    fn regex_like_match_invalid_regex() {
        // Invalid regex should return false, not panic
        assert!(!regex_like_match("[invalid", "test"));
    }
}
// REUSE-IgnoreEnd
