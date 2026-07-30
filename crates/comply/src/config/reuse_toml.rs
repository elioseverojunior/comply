// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::{Annotation, CommentStyleRule, Config, Precedence, WalkOptions};
use crate::error::Error;

impl Config {
    /// Parse REUSE.toml format.
    ///
    /// # Panics
    ///
    /// Panics if `SPDX-License-Identifier` is a single-element list that produces
    /// a `None` from `Iterator::next` (should be impossible by construction).
    #[allow(clippy::missing_errors_doc)]
    pub fn from_toml(path: &Path) -> Result<Self, Error> {
        // Errors name the file: a malformed manifest otherwise surfaces as
        // "missing copyright" on every file in the project, which points the
        // reader at entirely the wrong thing.
        let name = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .to_string();
        let content = fs::read_to_string(path).map_err(Error::Io)?;
        let raw: ReuseToml = toml::from_str(&content)
            .map_err(|e| Error::Serde(format!("failed to parse {name}: {e}")))?;

        // REUSE 3.3: the `version` key is REQUIRED.
        if raw.version.is_none() {
            return Err(Error::Serde(format!(
                "{name} is missing the required `version` key"
            )));
        }

        // Collect annotations from both `[[annotation]]` and `[[annotations]]`
        let mut annotations: Vec<Annotation> = Vec::new();
        for mut a in raw.annotation.into_iter().chain(raw.annotations) {
            let copyright = std::mem::take(&mut a.spdx_copyright_text);
            let licenses = std::mem::take(&mut a.spdx_license_identifier);
            let precedence = parse_precedence(a.precedence.as_deref());

            let license = match licenses.len() {
                0 => String::new(),
                1 => {
                    // Safety: matched on `1`, so there is exactly one element
                    #[allow(clippy::unwrap_used)]
                    licenses.into_iter().next().unwrap()
                }
                _ => licenses.join(" AND "),
            };

            // Expand multiple paths into separate annotations
            for p in a.path {
                annotations.push(Annotation {
                    path: p,
                    copyright: copyright
                        .iter()
                        .filter_map(|s| match s {
                            StringOrTable::String(s) => Some(s.clone()),
                            StringOrTable::Table(_) => None,
                        })
                        .collect(),
                    license: license.clone(),
                    precedence,
                });
            }
        }

        let default_license = raw.default.as_ref().and_then(|d| {
            d.license
                .clone()
                .or_else(|| d.spdx_license_identifier.clone())
        });

        let default_copyright = raw.default.map(|d| d.copyright_lines()).unwrap_or_default();

        let comply = raw.tool_comply.and_then(|t| t.comply);
        let walk = comply
            .as_ref()
            .and_then(|c| c.walk.as_ref())
            .map_or_else(WalkOptions::default, RawWalk::apply);
        let (ignore, comment_styles) = match comply {
            Some(c) => {
                let styles = c
                    .comment_styles
                    .into_iter()
                    .map(RawCommentStyle::apply)
                    .collect::<Result<Vec<_>, _>>()?;
                reject_contested_keys(&styles)?;
                (c.ignore, styles)
            }
            None => (Vec::new(), Vec::new()),
        };

        Ok(Self {
            spdx_package_name: raw.spdx_package_name,
            spdx_package_supplier: raw.spdx_package_supplier,
            annotations,
            default_license,
            default_copyright,
            ignore,
            source_patterns: raw.source_patterns,
            walk,
            comment_styles,
        })
    }
}

/// Refuse a manifest where two rules claim the same extension or filename.
///
/// Lookup resolves by position, so leaving a contested key legal would make the
/// order of a `Vec` the de facto precedence -- invisible in the file and easy
/// to change by accident. A duplicate is far more likely a typo than an intent,
/// so it is reported rather than silently resolved.
///
/// Extensions are compared lowercased, matching how `detect_style_with` looks
/// them up; filenames are compared exactly, since a filesystem may distinguish
/// them. The two are separate namespaces: a `makefile` extension does not
/// contest a `makefile` filename.
fn reject_contested_keys(rules: &[CommentStyleRule]) -> Result<(), Error> {
    let mut extensions = HashSet::new();
    let mut filenames = HashSet::new();

    for rule in rules {
        for ext in &rule.extensions {
            if !extensions.insert(ext.to_lowercase()) {
                return Err(Error::Serde(format!(
                    "two [[tool.comply.comment-styles]] rules claim the extension \
                     `{ext}`; remove one, or lookup order would silently decide"
                )));
            }
        }
        for name in &rule.filenames {
            if !filenames.insert(name.clone()) {
                return Err(Error::Serde(format!(
                    "two [[tool.comply.comment-styles]] rules claim the filename \
                     `{name}`; remove one, or lookup order would silently decide"
                )));
            }
        }
    }

    Ok(())
}

fn parse_precedence(s: Option<&str>) -> Precedence {
    match s {
        Some("override") => Precedence::Override,
        Some("aggregate") => Precedence::Aggregate,
        _ => Precedence::Closest,
    }
}

/// A field TOML may spell as one value or a list of them.
///
/// `untagged` rather than a hand-written `Visitor`: the two `impl Visitor` blocks
/// this replaced were declaration lines that tarpaulin counted as executable but
/// no test could ever run, so they held coverage below 100 permanently.
#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    fn into_vec(self) -> Vec<T> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values,
        }
    }
}

/// Deserialize a TOML field that can be either a single string or a list of strings.
fn deserialize_one_or_many<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // `untagged` would otherwise report "data did not match any variant of
    // untagged enum OneOrMany", which tells the author nothing about their
    // REUSE.toml. The accepted shapes are named instead.
    OneOrMany::<String>::deserialize(deserializer)
        .map(OneOrMany::into_vec)
        .map_err(|_| serde::de::Error::custom("expected a string or list of strings"))
}

/// Deserialize `SPDX-FileCopyrightText` which can be a string, list of strings,
/// or list of inline tables (rare).
fn deserialize_copyright<'de, D>(deserializer: D) -> Result<Vec<StringOrTable>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    OneOrMany::<StringOrTable>::deserialize(deserializer)
        .map(OneOrMany::into_vec)
        .map_err(|_| serde::de::Error::custom("expected a string or list of strings/tables"))
}

#[derive(Debug, Deserialize)]
struct ReuseToml {
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    annotation: Vec<ReuseAnnotation>,
    #[serde(default)]
    annotations: Vec<ReuseAnnotation>,
    default: Option<ReuseDefault>,
    #[serde(rename = "tool")]
    tool_comply: Option<ToolComply>,
    #[serde(rename = "SPDX-PackageName")]
    spdx_package_name: Option<String>,
    #[serde(rename = "SPDX-PackageSupplier")]
    spdx_package_supplier: Option<String>,
    #[serde(default, rename = "source-patterns")]
    source_patterns: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ToolComply {
    comply: Option<ToolComplySection>,
}

#[derive(Debug, Deserialize)]
struct ToolComplySection {
    #[serde(default)]
    ignore: Vec<String>,
    walk: Option<RawWalk>,
    #[serde(default, rename = "comment-styles")]
    comment_styles: Vec<RawCommentStyle>,
}

/// One `[[tool.comply.comment-styles]]` rule as written.
#[derive(Debug, Deserialize)]
struct RawCommentStyle {
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    filenames: Vec<String>,
    open: String,
    #[serde(default)]
    line: String,
    #[serde(default)]
    close: String,
}

impl RawCommentStyle {
    /// Validate and convert. A rule claiming neither an extension nor a
    /// filename can never match anything, so it is an error rather than a
    /// silent no-op -- the author would otherwise believe a style was
    /// configured while every file kept its built-in.
    fn apply(self) -> Result<CommentStyleRule, Error> {
        if self.extensions.is_empty() && self.filenames.is_empty() {
            return Err(Error::Serde(
                "a [[tool.comply.comment-styles]] rule needs `extensions` or `filenames`; \
                 one claiming neither can never match a file"
                    .to_string(),
            ));
        }
        Ok(CommentStyleRule {
            extensions: self.extensions,
            filenames: self.filenames,
            open: self.open,
            line: self.line,
            close: self.close,
        })
    }
}

/// `[tool.comply.walk]`, every switch optional.
///
/// `Option<bool>` rather than `bool`: an absent key has to keep the built-in
/// default, and `#[serde(default)]` on a plain bool would silently turn every
/// unnamed switch off.
#[derive(Debug, Deserialize)]
struct RawWalk {
    hidden: Option<bool>,
    #[serde(rename = "git-ignore")]
    git_ignore: Option<bool>,
    #[serde(rename = "git-global")]
    git_global: Option<bool>,
    #[serde(rename = "git-exclude")]
    git_exclude: Option<bool>,
    #[serde(rename = "require-git")]
    require_git: Option<bool>,
    #[serde(rename = "follow-links")]
    follow_links: Option<bool>,
}

impl RawWalk {
    fn apply(&self) -> WalkOptions {
        let base = WalkOptions::default();
        WalkOptions {
            hidden: self.hidden.unwrap_or(base.hidden),
            git_ignore: self.git_ignore.unwrap_or(base.git_ignore),
            git_global: self.git_global.unwrap_or(base.git_global),
            git_exclude: self.git_exclude.unwrap_or(base.git_exclude),
            require_git: self.require_git.unwrap_or(base.require_git),
            follow_links: self.follow_links.unwrap_or(base.follow_links),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ReuseAnnotation {
    #[serde(default, deserialize_with = "deserialize_one_or_many")]
    path: Vec<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_copyright",
        rename = "SPDX-FileCopyrightText",
        alias = "copyright"
    )]
    spdx_copyright_text: Vec<StringOrTable>,
    #[serde(
        default,
        rename = "SPDX-License-Identifier",
        alias = "license",
        deserialize_with = "deserialize_one_or_many"
    )]
    spdx_license_identifier: Vec<String>,
    #[serde(default)]
    precedence: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReuseDefault {
    #[serde(
        default,
        deserialize_with = "deserialize_copyright",
        rename = "SPDX-FileCopyrightText",
        alias = "copyright"
    )]
    spdx_copyright_text: Vec<StringOrTable>,
    #[serde(default, rename = "SPDX-License-Identifier")]
    spdx_license_identifier: Option<String>,
    #[serde(default)]
    license: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StringOrTable {
    String(String),
    #[expect(dead_code)]
    Table(HashMap<String, String>),
}

impl ReuseDefault {
    fn copyright_lines(&self) -> Vec<String> {
        self.spdx_copyright_text
            .iter()
            .filter_map(|s| match s {
                StringOrTable::String(s) => Some(s.clone()),
                StringOrTable::Table(_) => None,
            })
            .collect()
    }
}

// --- Glob matching --------------------------------------------------------

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// The REUSE spec permits table-valued copyright entries; comply reads only
    /// the string form, so a table must drop out rather than abort the parse.
    #[test]
    fn a_table_valued_copyright_is_dropped_from_an_annotation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        fs::write(
            &path,
            concat!(
                "version = 1\n\n[[annotation]]\n",
                "path = \"src/**\"\n",
                "SPDX-FileCopyrightText = [{ name = \"Acme\" }]\n",
                "SPDX-License-Identifier = \"MIT\"\n"
            ),
        )
        .unwrap();

        let config = Config::from_toml(&path).unwrap();

        assert!(config.annotations[0].copyright.is_empty());
        assert_eq!(config.annotations[0].license, "MIT");
    }

    #[test]
    fn a_table_valued_copyright_is_dropped_from_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        fs::write(
            &path,
            "version = 1\n\n[default]\nSPDX-FileCopyrightText = [{ name = \"Acme\" }]\n",
        )
        .unwrap();

        let config = Config::from_toml(&path).unwrap();

        assert!(config.default_copyright.is_empty());
    }

    #[test]
    fn an_annotation_with_no_licence_parses_to_an_empty_expression() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        fs::write(
            &path,
            "version = 1\n\n[[annotation]]\npath = \"src/**\"\nSPDX-FileCopyrightText = \"2026 Acme\"\n",
        )
        .unwrap();

        let config = Config::from_toml(&path).unwrap();

        assert_eq!(config.annotations[0].license, "");
    }

    #[test]
    fn a_non_string_path_reports_what_was_expected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        fs::write(&path, "version = 1\n\n[[annotation]]\npath = 42\n").unwrap();

        let err = Config::from_toml(&path).unwrap_err().to_string();

        assert!(
            err.contains("a string or list of strings"),
            "the error should name the accepted shapes, got: {err}"
        );
    }

    #[test]
    fn a_non_string_copyright_reports_what_was_expected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        fs::write(
            &path,
            "version = 1\n\n[[annotation]]\npath = \"src/**\"\nSPDX-FileCopyrightText = 42\n",
        )
        .unwrap();

        let err = Config::from_toml(&path).unwrap_err().to_string();

        assert!(
            err.contains("a string or list of strings/tables"),
            "the error should name the accepted shapes, got: {err}"
        );
    }

    /// `[[tool.comply.comment-styles]]` -- a project teaching comply a syntax
    /// it does not ship. Built-ins load first and a user rule wins on the same
    /// key, so a project can retarget an extension without editing the crate.
    mod comment_styles {
        use super::*;

        fn parse(body: &str) -> Result<Config, Error> {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("REUSE.toml");
            fs::write(&path, body).unwrap();
            Config::from_toml(&path)
        }

        #[test]
        fn a_rule_groups_every_extension_that_shares_a_syntax() {
            let config = parse(
                "version = 1\n\n\
                 [[tool.comply.comment-styles]]\n\
                 extensions = [\"vue\", \"svelte\"]\n\
                 open = \"<!--\"\n\
                 close = \"-->\"\n",
            )
            .unwrap();

            assert_eq!(config.comment_styles.len(), 1, "one rule, not one per ext");
            let rule = &config.comment_styles[0];
            assert_eq!(rule.extensions, ["vue", "svelte"]);
            assert_eq!((rule.open.as_str(), rule.close.as_str()), ("<!--", "-->"));
        }

        #[test]
        fn a_line_style_needs_only_an_opener() {
            let config = parse(
                "version = 1\n\n\
                 [[tool.comply.comment-styles]]\n\
                 filenames = [\"Jenkinsfile\"]\n\
                 open = \"//\"\n",
            )
            .unwrap();

            let rule = &config.comment_styles[0];
            assert_eq!(rule.filenames, ["Jenkinsfile"]);
            assert!(rule.close.is_empty(), "no close means a line style");
            assert!(rule.line.is_empty());
        }

        #[test]
        fn a_rule_matching_nothing_is_rejected() {
            // Silently ignoring it would leave the author believing a style was
            // configured while every file kept its built-in.
            let err = parse(
                "version = 1\n\n\
                 [[tool.comply.comment-styles]]\n\
                 open = \"//\"\n",
            )
            .expect_err("a rule that can never match is a mistake, not a no-op");

            assert!(
                err.to_string().contains("extensions") || err.to_string().contains("filenames"),
                "the error must say what is missing, got: {err}"
            );
        }

        #[test]
        fn two_rules_claiming_one_extension_are_rejected() {
            // Lookup would otherwise resolve by position, making the iteration
            // order of a Vec the de facto rule. A duplicated extension is far
            // more likely a mistake than an intent, so it is refused rather
            // than silently resolved one way.
            let err = parse(
                "version = 1\n\n\
                 [[tool.comply.comment-styles]]\n\
                 extensions = [\"vue\"]\n\
                 open = \"<!--\"\n\n\
                 [[tool.comply.comment-styles]]\n\
                 extensions = [\"vue\"]\n\
                 open = \"//\"\n",
            )
            .expect_err("a contested extension must not resolve by position");

            assert!(err.to_string().contains("vue"), "name it: {err}");
        }

        #[test]
        fn a_contested_extension_is_caught_case_insensitively() {
            // Matching lowercases the extension, so `VUE` and `vue` are the
            // same key and must collide rather than shadow each other.
            let err = parse(
                "version = 1\n\n\
                 [[tool.comply.comment-styles]]\n\
                 extensions = [\"VUE\"]\n\
                 open = \"<!--\"\n\n\
                 [[tool.comply.comment-styles]]\n\
                 extensions = [\"vue\"]\n\
                 open = \"//\"\n",
            )
            .expect_err("case cannot be used to smuggle a duplicate past the check");

            assert!(err.to_string().to_lowercase().contains("vue"), "got: {err}");
        }

        #[test]
        fn a_contested_filename_is_rejected_too() {
            let err = parse(
                "version = 1\n\n\
                 [[tool.comply.comment-styles]]\n\
                 filenames = [\"Jenkinsfile\"]\n\
                 open = \"//\"\n\n\
                 [[tool.comply.comment-styles]]\n\
                 filenames = [\"Jenkinsfile\"]\n\
                 open = \"#\"\n",
            )
            .expect_err("filenames need the same guard as extensions");

            assert!(err.to_string().contains("Jenkinsfile"), "got: {err}");
        }

        #[test]
        fn an_extension_and_a_filename_may_share_a_spelling() {
            // Different namespaces: a rule claiming the `makefile` extension
            // does not contest one claiming the `makefile` filename.
            let config = parse(
                "version = 1\n\n\
                 [[tool.comply.comment-styles]]\n\
                 extensions = [\"makefile\"]\n\
                 open = \"#\"\n\n\
                 [[tool.comply.comment-styles]]\n\
                 filenames = [\"makefile\"]\n\
                 open = \"//\"\n",
            )
            .expect("these do not collide");

            assert_eq!(config.comment_styles.len(), 2);
        }

        #[test]
        fn one_rule_may_repeat_nothing_within_itself() {
            let err = parse(
                "version = 1\n\n\
                 [[tool.comply.comment-styles]]\n\
                 extensions = [\"vue\", \"vue\"]\n\
                 open = \"<!--\"\n",
            )
            .expect_err("a rule contesting itself is still a mistake");

            assert!(err.to_string().contains("vue"), "got: {err}");
        }

        #[test]
        fn no_table_means_no_rules_not_an_error() {
            let config = parse("version = 1\n").unwrap();
            assert!(config.comment_styles.is_empty());
        }
    }

    #[test]
    fn walk_switches_fall_back_to_the_built_in_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "version = 1").unwrap();

        let config = Config::from_toml(&path).unwrap();
        assert_eq!(config.walk, WalkOptions::default());
    }

    #[test]
    fn a_named_walk_switch_overrides_only_itself() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r"
version = 1

[tool.comply.walk]
hidden = true
follow-links = true
"
        )
        .unwrap();

        let config = Config::from_toml(&path).unwrap();
        assert!(config.walk.hidden, "the named switch flips");
        assert!(config.walk.follow_links, "the named switch flips");
        // Everything left unnamed keeps its default rather than becoming false.
        assert!(config.walk.git_ignore);
        assert!(config.walk.git_global);
        assert!(config.walk.git_exclude);
        assert!(!config.walk.require_git);
    }

    #[test]
    fn walk_switches_coexist_with_the_ignore_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
version = 1

[tool.comply]
ignore = ["**/*.lock"]

[tool.comply.walk]
git-ignore = false
"#
        )
        .unwrap();

        let config = Config::from_toml(&path).unwrap();
        assert_eq!(config.ignore, vec!["**/*.lock".to_string()]);
        assert!(!config.walk.git_ignore);
    }

    #[test]
    fn parse_reuse_toml_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
version = 1

[[annotation]]
path = "src/**"
SPDX-FileCopyrightText = ["2026 COMPLY contributors"]
SPDX-License-Identifier = "MIT OR Apache-2.0"
"#
        )
        .unwrap();

        let config = Config::from_toml(&path).unwrap();
        assert_eq!(config.annotations.len(), 1);
        assert_eq!(config.annotations[0].path, "src/**");
        assert_eq!(config.annotations[0].license, "MIT OR Apache-2.0");
        assert_eq!(
            config.annotations[0].copyright,
            vec!["2026 COMPLY contributors"]
        );
    }

    #[test]
    fn parse_reuse_toml_with_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
version = 1

[default]
SPDX-FileCopyrightText = ["2026 Project"]
SPDX-License-Identifier = "CC0-1.0"

[[annotation]]
path = "code/**"
SPDX-FileCopyrightText = ["2026 Author"]
SPDX-License-Identifier = "MIT"
"#
        )
        .unwrap();

        let config = Config::from_toml(&path).unwrap();
        assert_eq!(config.annotations.len(), 1);
        assert_eq!(config.default_copyright, vec!["2026 Project"]);
        assert_eq!(config.default_license.as_deref(), Some("CC0-1.0"));
    }

    #[test]
    fn parse_reuse_toml_with_override() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
version = 1

[[annotation]]
path = "src/main.rs"
SPDX-FileCopyrightText = ["2026 Override Author"]
SPDX-License-Identifier = "GPL-3.0-only"
precedence = "override"
"#
        )
        .unwrap();

        let config = Config::from_toml(&path).unwrap();
        assert_eq!(config.annotations[0].precedence, Precedence::Override);
    }

    #[test]
    fn parse_reuse_toml_with_spdx_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
version = 1
SPDX-PackageName = "comply"
SPDX-PackageSupplier = "COMPLY contributors"

[[annotations]]
path = "*"
SPDX-FileCopyrightText = "2026 Project"
SPDX-License-Identifier = "MIT"
"#
        )
        .unwrap();

        let config = Config::from_toml(&path).unwrap();
        assert_eq!(config.spdx_package_name.as_deref(), Some("comply"));
        assert_eq!(
            config.spdx_package_supplier.as_deref(),
            Some("COMPLY contributors")
        );
        assert_eq!(config.annotations.len(), 1);
    }
}
// REUSE-IgnoreEnd
