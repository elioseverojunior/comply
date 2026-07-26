// SPDX-FileCopyrightText: 2026 COMPLY contributors
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
                regex.push_str(".*");
                i += 2;
                // Skip optional trailing slash
                if i < chars.len() && chars[i] == '/' {
                    regex.push('/');
                    i += 1;
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
    use crate::config::{Annotation, Config, Precedence};

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
    fn glob_star_star_slash_anchors_the_remainder_to_a_segment() {
        assert!(glob_match("**/*.png", "assets/img/logo.png"));
        assert!(
            !glob_match("**/*.png", "logo.png"),
            "`**/` requires at least one leading separator"
        );
        assert_eq!(glob_to_regex("**/*.png"), r"^.*/[^/]*\.png$");
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
