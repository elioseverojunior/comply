// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::{Annotation, Config, Precedence};
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

        let ignore = raw
            .tool_comply
            .and_then(|t| t.comply)
            .map(|c| c.ignore)
            .unwrap_or_default();

        Ok(Self {
            spdx_package_name: raw.spdx_package_name,
            spdx_package_supplier: raw.spdx_package_supplier,
            annotations,
            default_license,
            default_copyright,
            ignore,
            source_patterns: raw.source_patterns,
        })
    }
}

fn parse_precedence(s: Option<&str>) -> Precedence {
    match s {
        Some("override") => Precedence::Override,
        Some("aggregate") => Precedence::Aggregate,
        _ => Precedence::Closest,
    }
}

/// Deserialize a TOML field that can be either a single string or a list of strings.
fn deserialize_one_or_many<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct OneOrManyVisitor;
    impl<'de> de::Visitor<'de> for OneOrManyVisitor {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a string or list of strings")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Vec<String>, E> {
            Ok(vec![v.to_string()])
        }

        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<String>, A::Error> {
            let mut out = Vec::new();
            while let Some(s) = seq.next_element::<String>()? {
                out.push(s);
            }
            Ok(out)
        }
    }

    deserializer.deserialize_any(OneOrManyVisitor)
}

/// Deserialize `SPDX-FileCopyrightText` which can be a string, list of strings,
/// or list of inline tables (rare).
fn deserialize_copyright<'de, D>(deserializer: D) -> Result<Vec<StringOrTable>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct CopyrightVisitor;
    impl<'de> de::Visitor<'de> for CopyrightVisitor {
        type Value = Vec<StringOrTable>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a string or list of strings/tables")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Vec<StringOrTable>, E> {
            Ok(vec![StringOrTable::String(v.to_string())])
        }

        fn visit_seq<A: de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Vec<StringOrTable>, A::Error> {
            let mut out = Vec::new();
            while let Some(elem) = seq.next_element::<CopyrightElem>()? {
                out.push(match elem {
                    CopyrightElem::String(s) => StringOrTable::String(s),
                    CopyrightElem::Table(t) => StringOrTable::Table(t),
                });
            }
            Ok(out)
        }
    }

    deserializer.deserialize_any(CopyrightVisitor)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CopyrightElem {
    String(String),
    Table(HashMap<String, String>),
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
