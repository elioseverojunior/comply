// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::path::{Path, PathBuf};

use crate::error::Error;

mod dep5;
mod glob;
mod reuse_toml;

pub(crate) use glob::{glob_match, glob_to_regex};

/// The licensing that applies to one file, after any REUSE.toml annotation has
/// been combined with the file's own header according to its precedence.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Licensing {
    /// Copyright notices, in the order REUSE reports them.
    pub copyrights: Vec<String>,
    /// SPDX license expression, if anything declared one.
    pub license: Option<String>,
}

/// Precedence of an annotation per REUSE spec 3.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Precedence {
    /// File's own header takes precedence, REUSE.toml is fallback.
    #[default]
    Closest,
    /// Always aggregate with file's own licensing info.
    Aggregate,
    /// Override any file-local licensing info.
    Override,
}

/// An annotation for a set of files, specifying copyright and licensing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    /// Glob pattern matching the files (e.g. `"src/**"`).
    pub path: String,
    /// SPDX copyright text lines.
    pub copyright: Vec<String>,
    /// SPDX license identifier (e.g. `"MIT OR Apache-2.0"`).
    pub license: String,
    /// Precedence of this annotation.
    pub precedence: Precedence,
}

/// Parsed REUSE configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Annotations from REUSE.toml.
    pub annotations: Vec<Annotation>,
    /// Default license for files without explicit annotation.
    pub default_license: Option<String>,
    /// Default copyright for files without explicit annotation.
    pub default_copyright: Vec<String>,
    /// Glob patterns for files to ignore during REUSE analysis.
    pub ignore: Vec<String>,
    /// SPDX-PackageName (root-level REUSE.toml metadata).
    pub spdx_package_name: Option<String>,
    /// SPDX-PackageSupplier (root-level REUSE.toml metadata).
    pub spdx_package_supplier: Option<String>,
    /// Custom glob patterns for source files (extends default classification).
    pub source_patterns: Vec<String>,
}

impl Config {
    /// Find the first existing config file among the three allowed names:
    /// `.reuse.toml`, `reuse.toml`, `REUSE.toml`.
    fn find_config(root: &Path) -> Option<PathBuf> {
        let candidates = [".reuse.toml", "reuse.toml", "REUSE.toml"];
        for name in &candidates {
            let path = root.join(name);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    /// Load config from `REUSE.toml` (or `.reuse.toml` / `reuse.toml` fallbacks)
    /// in the project root.
    ///
    /// Per REUSE Specification 3.3, REUSE.toml and `.reuse/dep5` are mutually
    /// exclusive. If a REUSE.toml variant exists, `.reuse/dep5` is ignored.
    #[allow(clippy::missing_errors_doc)]
    pub fn load(root: &Path) -> Result<Self, Error> {
        let dep5_path = root.join(".reuse/dep5");

        if let Some(path) = Self::find_config(root) {
            // REUSE 3.3: a project declares licensing through REUSE.toml or
            // DEP5, never both -- merging them would silently invent a
            // precedence the spec does not define.
            if dep5_path.exists() {
                return Err(Error::Serde(format!(
                    "{} and .reuse/dep5 both declare licensing; \
                     REUSE 3.3 allows only one of them",
                    path.file_name()
                        .unwrap_or(path.as_os_str())
                        .to_string_lossy()
                )));
            }
            return Self::from_toml(&path);
        }

        if dep5_path.exists() {
            let annotations = Self::from_dep5(&dep5_path)?;
            return Ok(Self {
                annotations,
                default_license: None,
                default_copyright: vec![],
                ignore: vec![],
                spdx_package_name: None,
                spdx_package_supplier: None,
                source_patterns: vec![],
            });
        }

        Ok(Self {
            annotations: vec![],
            default_license: None,
            default_copyright: vec![],
            ignore: vec![],
            spdx_package_name: None,
            spdx_package_supplier: None,
            source_patterns: vec![],
        })
    }

    /// Find annotations matching the given relative path.
    #[must_use]
    pub fn match_annotations<'a>(&'a self, rel_path: &str) -> Vec<&'a Annotation> {
        // REUSE annotation globs are always forward-slash. Normalising here
        // means no caller has to remember to do it -- and none can forget.
        let rel_path = rel_path.replace('\\', "/");
        let mut matched: Vec<&Annotation> = self
            .annotations
            .iter()
            .filter(|a| glob_match(&a.path, &rel_path))
            .collect();

        // Sort: Override last (highest precedence), then aggregate, then closest
        matched.sort_by_key(|a| match a.precedence {
            Precedence::Override => 2,
            Precedence::Aggregate => 1,
            Precedence::Closest => 0,
        });

        matched
    }

    /// Combine a file's own header with the REUSE.toml annotation covering it,
    /// honouring that annotation's precedence (REUSE 3.3 section 2.3.2).
    ///
    /// The three modes differ in more than emphasis: `closest` falls back field
    /// by field, `aggregate` unions field by field, but `override` replaces the
    /// header outright -- so an override annotation that omits a copyright
    /// leaves the file with none.
    #[must_use]
    pub fn resolve_licensing(
        &self,
        rel_path: &str,
        header_copyrights: &[String],
        header_license: Option<&str>,
    ) -> Licensing {
        let matched = self.match_annotations(rel_path);
        let header = || Licensing {
            copyrights: header_copyrights.to_vec(),
            license: header_license.map(ToString::to_string),
        };

        let Some(anno) = matched.last() else {
            return header();
        };
        let anno_license = (!anno.license.is_empty()).then(|| anno.license.clone());

        match anno.precedence {
            Precedence::Override => Licensing {
                copyrights: anno.copyright.clone(),
                license: anno_license,
            },
            Precedence::Aggregate => {
                let mut copyrights = anno.copyright.clone();
                for c in header_copyrights {
                    if !copyrights.contains(c) {
                        copyrights.push(c.clone());
                    }
                }
                let license = match (anno_license, header_license) {
                    (Some(a), Some(h)) if a != h => Some(format!("{a} AND {h}")),
                    (Some(a), _) => Some(a),
                    (None, h) => h.map(ToString::to_string),
                };
                Licensing {
                    copyrights,
                    license,
                }
            }
            Precedence::Closest => Licensing {
                copyrights: if header_copyrights.is_empty() {
                    anno.copyright.clone()
                } else {
                    header_copyrights.to_vec()
                },
                license: header_license.map(ToString::to_string).or(anno_license),
            },
        }
    }

    /// Create a new `ConfigBuilder` for fluent construction.
    #[must_use]
    pub const fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }
}

/// A fluent builder for constructing `Config` instances.
#[derive(Debug, Default)]
pub struct ConfigBuilder {
    annotations: Vec<Annotation>,
    default_license: Option<String>,
    default_copyright: Vec<String>,
    ignore: Vec<String>,
    spdx_package_name: Option<String>,
    spdx_package_supplier: Option<String>,
    source_patterns: Vec<String>,
}

impl ConfigBuilder {
    const fn new() -> Self {
        Self {
            annotations: Vec::new(),
            default_license: None,
            default_copyright: Vec::new(),
            ignore: Vec::new(),
            spdx_package_name: None,
            spdx_package_supplier: None,
            source_patterns: Vec::new(),
        }
    }

    /// Add an annotation to the config.
    #[must_use]
    pub fn add_annotation(mut self, annotation: Annotation) -> Self {
        self.annotations.push(annotation);
        self
    }

    /// Set the default license for files without explicit annotation.
    #[must_use]
    pub fn default_license(mut self, license: impl Into<String>) -> Self {
        self.default_license = Some(license.into());
        self
    }

    /// Add a default copyright line.
    #[must_use]
    pub fn add_default_copyright(mut self, copyright: impl Into<String>) -> Self {
        self.default_copyright.push(copyright.into());
        self
    }

    /// Set the ignore patterns for files to skip during analysis.
    #[must_use]
    pub fn ignore(mut self, patterns: Vec<String>) -> Self {
        self.ignore = patterns;
        self
    }

    /// Add a single ignore pattern.
    #[must_use]
    pub fn add_ignore(mut self, pattern: impl Into<String>) -> Self {
        self.ignore.push(pattern.into());
        self
    }

    /// Set SPDX-PackageName.
    #[must_use]
    pub fn spdx_package_name(mut self, name: impl Into<String>) -> Self {
        self.spdx_package_name = Some(name.into());
        self
    }

    /// Set SPDX-PackageSupplier.
    #[must_use]
    pub fn spdx_package_supplier(mut self, supplier: impl Into<String>) -> Self {
        self.spdx_package_supplier = Some(supplier.into());
        self
    }

    /// Add source patterns to extend default source file classification.
    #[must_use]
    pub fn source_patterns(mut self, patterns: Vec<String>) -> Self {
        self.source_patterns = patterns;
        self
    }

    /// Add a single source pattern.
    #[must_use]
    pub fn add_source_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.source_patterns.push(pattern.into());
        self
    }

    /// Consume the builder and produce a `Config`.
    #[must_use]
    pub fn build(self) -> Config {
        Config {
            annotations: self.annotations,
            default_license: self.default_license,
            default_copyright: self.default_copyright,
            ignore: self.ignore,
            spdx_package_name: self.spdx_package_name,
            spdx_package_supplier: self.spdx_package_supplier,
            source_patterns: self.source_patterns,
        }
    }
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    mod config_guards {
        use super::*;
        use tempfile::TempDir;

        fn project(files: &[(&str, &str)]) -> TempDir {
            let tmp = TempDir::new().unwrap();
            for (rel, body) in files {
                let path = tmp.path().join(rel);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).unwrap();
                }
                fs::write(path, body).unwrap();
            }
            tmp
        }

        const VALID: &str = "version = 1\n";
        const DEP5: &str = concat!(
            "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n\n",
            "Files: src/*\nCopyright: 2026 A\nLicense: MIT\n"
        );

        #[test]
        fn a_reuse_toml_without_a_version_key_is_rejected() {
            let tmp = project(&[("REUSE.toml", "[[annotations]]\npath = [\"src/**\"]\n")]);

            let err = Config::load(tmp.path()).unwrap_err().to_string();

            assert!(
                err.contains("version"),
                "the spec makes `version` REQUIRED; got: {err}"
            );
        }

        #[test]
        fn a_reuse_toml_with_a_version_key_loads() {
            let tmp = project(&[("REUSE.toml", VALID)]);

            assert!(Config::load(tmp.path()).is_ok());
        }

        #[test]
        fn declaring_licensing_in_both_reuse_toml_and_dep5_is_rejected() {
            let tmp = project(&[("REUSE.toml", VALID), (".reuse/dep5", DEP5)]);

            // Lower-cased: `find_config` reports whichever spelling it matched,
            // and a case-insensitive filesystem may match `reuse.toml` first.
            let err = Config::load(tmp.path())
                .unwrap_err()
                .to_string()
                .to_lowercase();

            assert!(
                err.contains("reuse.toml") && err.contains("dep5"),
                "the error should name both files; got: {err}"
            );
        }

        #[test]
        fn dep5_alone_still_loads() {
            let tmp = project(&[(".reuse/dep5", DEP5)]);

            assert!(Config::load(tmp.path()).is_ok());
        }

        #[test]
        fn a_malformed_reuse_toml_names_the_file_it_could_not_parse() {
            let tmp = project(&[("REUSE.toml", "this is = = not toml\n")]);

            let err = Config::load(tmp.path())
                .unwrap_err()
                .to_string()
                .to_lowercase();

            assert!(
                err.contains("reuse.toml"),
                "a broken manifest must be named, not reported as missing copyrights; got: {err}"
            );
        }
    }

    /// Expected values in these tests are the observed output of the reference
    /// `reuse` 6.2.0 implementation on equivalent fixtures, since REUSE 3.3
    /// describes precedence in prose only.
    mod precedence {
        use super::*;

        fn config_with(precedence: Precedence, copyright: &[&str], license: &str) -> Config {
            Config::builder()
                .add_annotation(Annotation {
                    path: "src/**".into(),
                    copyright: copyright.iter().map(ToString::to_string).collect(),
                    license: license.into(),
                    precedence,
                })
                .build()
        }

        #[test]
        fn closest_prefers_the_header_field_by_field() {
            let config = config_with(Precedence::Closest, &["2026 New"], "MIT");

            // A header supplying only a copyright keeps it, and takes the
            // licence from the annotation.
            let r = config.resolve_licensing("src/a.rs", &["2001 Old".into()], None);
            assert_eq!(r.copyrights, vec!["2001 Old"]);
            assert_eq!(r.license.as_deref(), Some("MIT"));

            // ... and the mirror image: the licence is the header's, the
            // copyright comes from the annotation.
            let r = config.resolve_licensing("src/a.rs", &[], Some("GPL-3.0-only"));
            assert_eq!(r.copyrights, vec!["2026 New"]);
            assert_eq!(r.license.as_deref(), Some("GPL-3.0-only"));
        }

        #[test]
        fn closest_falls_back_entirely_when_the_file_declares_nothing() {
            let config = config_with(Precedence::Closest, &["2026 New"], "MIT");

            let r = config.resolve_licensing("src/a.rs", &[], None);

            assert_eq!(r.copyrights, vec!["2026 New"]);
            assert_eq!(r.license.as_deref(), Some("MIT"));
        }

        #[test]
        fn aggregate_unions_both_with_the_annotation_first() {
            let config = config_with(Precedence::Aggregate, &["2026 New"], "MIT");

            let r =
                config.resolve_licensing("src/a.rs", &["2001 Old".into()], Some("GPL-3.0-only"));

            assert_eq!(r.copyrights, vec!["2026 New", "2001 Old"]);
            assert_eq!(
                r.license.as_deref(),
                Some("MIT AND GPL-3.0-only"),
                "a file covered by both licences is the conjunction of them"
            );
        }

        #[test]
        fn aggregate_does_not_repeat_a_copyright_both_sides_declare() {
            let config = config_with(Precedence::Aggregate, &["2026 Acme"], "MIT");

            let r = config.resolve_licensing("src/a.rs", &["2026 Acme".into()], None);

            assert_eq!(
                r.copyrights,
                vec!["2026 Acme"],
                "the same holder declared twice is still one copyright"
            );
        }

        #[test]
        fn override_replaces_the_header_wholesale() {
            let config = config_with(Precedence::Override, &["2026 New"], "MIT");

            let r =
                config.resolve_licensing("src/a.rs", &["2001 Old".into()], Some("GPL-3.0-only"));

            assert_eq!(r.copyrights, vec!["2026 New"]);
            assert_eq!(r.license.as_deref(), Some("MIT"));
        }

        #[test]
        fn override_discards_a_field_it_does_not_itself_supply() {
            // Deliberately counter-intuitive, and confirmed against `reuse`:
            // an override annotation carrying only a licence leaves the file
            // with NO copyright, which makes it non-compliant.
            let config = config_with(Precedence::Override, &[], "MIT");

            let r =
                config.resolve_licensing("src/a.rs", &["2001 Old".into()], Some("GPL-3.0-only"));

            assert!(r.copyrights.is_empty());
            assert_eq!(r.license.as_deref(), Some("MIT"));
        }

        #[test]
        fn a_file_no_annotation_matches_keeps_its_own_header() {
            let config = config_with(Precedence::Override, &["2026 New"], "MIT");

            let r =
                config.resolve_licensing("docs/x.md", &["2001 Old".into()], Some("GPL-3.0-only"));

            assert_eq!(r.copyrights, vec!["2001 Old"]);
            assert_eq!(r.license.as_deref(), Some("GPL-3.0-only"));
        }
    }

    #[test]
    fn config_builder_chain() {
        let config = Config::builder()
            .add_annotation(Annotation {
                path: "*.rs".into(),
                copyright: vec!["2026 Author".into()],
                license: "MIT".into(),
                precedence: Precedence::Closest,
            })
            .default_license("MIT")
            .add_default_copyright("2026 Project")
            .build();
        assert_eq!(config.annotations.len(), 1);
        assert_eq!(config.default_license.as_deref(), Some("MIT"));
        assert_eq!(config.default_copyright, vec!["2026 Project"]);
    }

    #[test]
    fn config_builder_with_spdx_metadata() {
        let config = Config::builder()
            .spdx_package_name("comply")
            .spdx_package_supplier("COMPLY contributors")
            .build();
        assert_eq!(config.spdx_package_name.as_deref(), Some("comply"));
        assert_eq!(
            config.spdx_package_supplier.as_deref(),
            Some("COMPLY contributors")
        );
    }

    #[test]
    fn config_builder_empty() {
        let config = Config::builder().build();
        assert!(config.annotations.is_empty());
        assert!(config.default_license.is_none());
        assert!(config.default_copyright.is_empty());
    }

    #[test]
    fn match_no_annotation() {
        let config = Config {
            annotations: vec![],
            default_license: None,
            default_copyright: vec![],
            ignore: vec![],
            spdx_package_name: None,
            spdx_package_supplier: None,
            source_patterns: vec![],
        };

        let matched = config.match_annotations("src/lib.rs");
        assert!(matched.is_empty());
    }

    #[test]
    fn parse_annotations_plural() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
version = 1

[[annotations]]
path = ["src/*.rs", "src/*.py"]
SPDX-FileCopyrightText = "2026 COMPLY contributors"
SPDX-License-Identifier = "MIT OR Apache-2.0"

[[annotations]]
path = "tests/*"
SPDX-FileCopyrightText = "2026 Testers"
SPDX-License-Identifier = "MIT"
"#
        )
        .unwrap();

        let config = Config::from_toml(&path).unwrap();
        assert_eq!(config.annotations.len(), 3);
        assert_eq!(config.annotations[0].path, "src/*.rs");
        assert_eq!(config.annotations[1].path, "src/*.py");
        assert_eq!(config.annotations[2].path, "tests/*");
    }

    #[test]
    fn parse_license_id_as_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
version = 1

[[annotations]]
path = "dual/*"
SPDX-FileCopyrightText = "2026 Author"
SPDX-License-Identifier = ["MIT", "Apache-2.0"]
"#
        )
        .unwrap();

        let config = Config::from_toml(&path).unwrap();
        assert_eq!(config.annotations.len(), 1);
        assert_eq!(config.annotations[0].license, "MIT AND Apache-2.0");
    }

    #[test]
    fn parse_aggregate_precedence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
version = 1

[[annotations]]
path = "agg/*"
SPDX-FileCopyrightText = "2026 Aggregator"
SPDX-License-Identifier = "MIT"
precedence = "aggregate"
"#
        )
        .unwrap();

        let config = Config::from_toml(&path).unwrap();
        assert_eq!(config.annotations[0].precedence, Precedence::Aggregate);
    }

    #[test]
    fn parse_both_annotation_and_annotations_merged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REUSE.toml");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
version = 1

[[annotation]]
path = "old/*"
SPDX-FileCopyrightText = "2026 Old"
SPDX-License-Identifier = "MIT"

[[annotations]]
path = "new/*"
SPDX-FileCopyrightText = "2026 New"
SPDX-License-Identifier = "Apache-2.0"
"#
        )
        .unwrap();

        let config = Config::from_toml(&path).unwrap();
        assert_eq!(config.annotations.len(), 2);
        assert_eq!(config.annotations[0].path, "old/*");
        assert_eq!(config.annotations[1].path, "new/*");
    }

    #[test]
    fn load_no_config_files() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load(dir.path()).unwrap();
        assert!(config.annotations.is_empty());
        assert!(config.default_license.is_none());
        assert!(config.ignore.is_empty());
    }

    #[test]
    fn config_builder_ignore() {
        let config = Config::builder()
            .ignore(vec!["target/**".to_string()])
            .build();
        assert_eq!(config.ignore, vec!["target/**"]);
    }

    #[test]
    fn config_builder_add_ignore() {
        let config = Config::builder()
            .add_ignore("target/**".to_string())
            .add_ignore("node_modules/**".to_string())
            .build();
        assert_eq!(config.ignore, vec!["target/**", "node_modules/**"]);
    }

    #[test]
    fn config_builder_source_patterns() {
        let config = Config::builder()
            .source_patterns(vec!["config/**".to_string()])
            .build();
        assert_eq!(config.source_patterns, vec!["config/**"]);
    }

    #[test]
    fn config_builder_add_source_pattern() {
        let config = Config::builder()
            .add_source_pattern("config/**".to_string())
            .add_source_pattern("scripts/**".to_string())
            .build();
        assert_eq!(config.source_patterns, vec!["config/**", "scripts/**"]);
    }

    #[test]
    fn match_annotations_multiple_patterns() {
        let config = Config {
            annotations: vec![
                Annotation {
                    path: "src/**".to_string(),
                    copyright: vec!["2026 Author".into()],
                    license: "MIT".into(),
                    precedence: Precedence::Closest,
                },
                Annotation {
                    path: "tests/**".to_string(),
                    copyright: vec!["2026 Tester".into()],
                    license: "Apache-2.0".into(),
                    precedence: Precedence::Closest,
                },
            ],
            default_license: None,
            default_copyright: vec![],
            ignore: vec![],
            spdx_package_name: None,
            spdx_package_supplier: None,
            source_patterns: vec![],
        };

        let matched = config.match_annotations("src/lib.rs");
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].license, "MIT");

        let matched = config.match_annotations("tests/integration.rs");
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].license, "Apache-2.0");

        let matched = config.match_annotations("docs/readme.md");
        assert!(matched.is_empty());
    }

    #[test]
    fn match_annotations_precedence_order() {
        let config = Config {
            annotations: vec![
                Annotation {
                    path: "src/**".to_string(),
                    copyright: vec!["2026 Base".into()],
                    license: "MIT".into(),
                    precedence: Precedence::Closest,
                },
                Annotation {
                    path: "src/special.rs".to_string(),
                    copyright: vec!["2026 Special".into()],
                    license: "Apache-2.0".into(),
                    precedence: Precedence::Override,
                },
                Annotation {
                    path: "src/**".to_string(),
                    copyright: vec!["2026 Aggregate".into()],
                    license: "GPL-3.0".into(),
                    precedence: Precedence::Aggregate,
                },
            ],
            default_license: None,
            default_copyright: vec![],
            ignore: vec![],
            spdx_package_name: None,
            spdx_package_supplier: None,
            source_patterns: vec![],
        };

        // Override comes LAST (highest precedence value = last in sorted order)
        let matched = config.match_annotations("src/special.rs");
        assert_eq!(matched.len(), 3);
        assert_eq!(matched[0].license, "MIT"); // Closest first
        assert_eq!(matched[1].license, "GPL-3.0"); // Aggregate second
        assert_eq!(matched[2].license, "Apache-2.0"); // Override last
    }

    #[test]
    fn match_annotations_empty() {
        let config = Config {
            annotations: vec![],
            default_license: None,
            default_copyright: vec![],
            ignore: vec![],
            spdx_package_name: None,
            spdx_package_supplier: None,
            source_patterns: vec![],
        };

        let matched = config.match_annotations("src/lib.rs");
        assert!(matched.is_empty());
    }

    #[test]
    fn match_annotations_normalises_windows_separators() {
        let config = Config::builder()
            .add_annotation(Annotation {
                path: "src/**".to_string(),
                copyright: vec!["2026 Acme Inc".to_string()],
                license: "MIT".to_string(),
                precedence: Precedence::Aggregate,
            })
            .build();

        let matched = config.match_annotations(r"src\main.rs");

        assert_eq!(
            matched.len(),
            1,
            "a backslash-separated path must match the same annotation"
        );
    }
}
// REUSE-IgnoreEnd
