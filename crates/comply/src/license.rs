// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `LICENSES/` directory: which texts a project bundles, and which it needs.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::error::Error;
use crate::hash;

/// All SPDX license identifiers from the SPDX License List.
use crate::spdx::{DEPRECATED_LICENSES, KNOWN_LICENSES};

/// How the project's `LICENSES/` directory lines up with the licenses its
/// files actually reference (REUSE 3.3 sections 2.2 and 2.4).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LicenseAudit {
    /// Referenced by a file or annotation, but absent from `LICENSES/`.
    pub missing: Vec<String>,
    /// Present in `LICENSES/`, but nothing in the project refers to it.
    pub unused: Vec<String>,
    /// Present, but the file name carries no extension.
    pub without_extension: Vec<String>,
    /// Neither an SPDX identifier nor a `LicenseRef-` custom identifier.
    pub bad: Vec<String>,
    /// A known identifier the SPDX License List has since deprecated.
    pub deprecated: Vec<String>,
}

impl LicenseAudit {
    /// True when the directory and the project agree on every license.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.missing.is_empty()
            && self.unused.is_empty()
            && self.without_extension.is_empty()
            && self.bad.is_empty()
            && self.deprecated.is_empty()
    }
}

/// Whether an identifier is one REUSE will accept at all.
///
/// `LicenseRef-` is the spec's escape hatch for licences absent from the SPDX
/// List, so it is valid by construction; exceptions must exist in the list.
fn is_recognised_identifier(id: &str) -> bool {
    id.starts_with("LicenseRef-")
        || crate::spdx::is_known_license(id)
        || crate::spdx::is_known_exception(id)
}

/// Cross-check `<root>/LICENSES/` against the identifiers `used` by the project.
///
/// An absent `LICENSES/` directory is not an error -- it simply means every
/// referenced license is missing -- but any other read failure is propagated
/// rather than being reported as an empty directory.
///
/// # Errors
///
/// Returns [`Error::Io`] if `LICENSES/` exists but cannot be read.
pub fn audit(root: &Path, used: &BTreeSet<String>) -> Result<LicenseAudit, Error> {
    let dir = root.join("LICENSES");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LicenseAudit {
                missing: used.iter().cloned().collect(),
                ..LicenseAudit::default()
            });
        }
        Err(e) => return Err(Error::Io(e)),
    };

    let mut present = BTreeSet::new();
    let mut without_extension = BTreeSet::new();
    for entry in entries {
        let path = entry.map_err(Error::Io)?.path();
        if !path.is_file() {
            continue;
        }
        // The identifier is the file name with any extension removed, so
        // `MIT.txt` and a bare `MIT` both declare MIT.
        //
        // `file_stem` is None only for a path ending in `..`, which `read_dir`
        // never yields; the previous `else { continue }` was therefore dead and
        // no test could reach it.
        let id = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if path.extension().is_none() {
            without_extension.insert(id.clone());
        }
        present.insert(id);
    }

    // `GPL-2.0-or-later` style is spelled `GPL-2.0+` in the older syntax, and
    // the License File is named for the base license, so the `+` is not part of
    // the file name to look for.
    let used: BTreeSet<String> = used
        .iter()
        .map(|id| id.trim_end_matches('+').to_string())
        .collect();

    // Identifier validity is judged over both sources: an unrecognised name is
    // just as wrong sitting in `LICENSES/` as it is in a file's header.
    let all: BTreeSet<&String> = used.union(&present).collect();

    Ok(LicenseAudit {
        missing: used.difference(&present).cloned().collect(),
        unused: present.difference(&used).cloned().collect(),
        without_extension: without_extension.into_iter().collect(),
        bad: all
            .iter()
            .filter(|id| !is_recognised_identifier(id))
            .map(|id| (*id).clone())
            .collect(),
        deprecated: all
            .iter()
            .filter(|id| DEPRECATED_LICENSES.contains(&id.as_str()))
            .map(|id| (*id).clone())
            .collect(),
    })
}

/// A database of known license identifiers and texts for identification.
#[derive(Debug, Clone)]
pub struct LicenseDb {
    /// Map of SPDX identifier -> (`normalized_text`, hash) for text matching.
    entries: HashMap<String, LicenseEntry>,
    /// Set of all known SPDX license identifiers (including without bundled text).
    known_ids: HashSet<String>,
}

#[derive(Debug, Clone)]
struct LicenseEntry {
    /// Normalized text for exact matching.
    text: String,
    /// SHA-256 hash of the normalized text.
    hash: Vec<u8>,
}

impl LicenseDb {
    /// Create a new builder for constructing a `LicenseDb`.
    #[must_use]
    pub fn builder() -> LicenseDbBuilder {
        LicenseDbBuilder::new()
    }

    /// Try to identify a license from its text content.
    ///
    /// Returns the SPDX identifier if a match is found.
    #[must_use]
    pub fn identify(&self, text: &str) -> Option<&str> {
        let normalized = normalize(text);
        let input_hash = hash::hash_bytes(normalized.as_bytes());

        for (id, entry) in &self.entries {
            if entry.hash == input_hash || entry.text == normalized {
                return Some(id);
            }
        }
        None
    }

    /// Check whether a given SPDX identifier is in this database.
    #[must_use]
    pub fn is_known(&self, id: &str) -> bool {
        self.entries.contains_key(id) || self.known_ids.contains(id)
    }

    /// Return the number of license entries (with bundled text).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the project bundles no license text at all.
    ///
    /// Counts entries with text, so this stays true for a project whose
    /// identifiers are all known but whose `LICENSES/` directory is missing --
    /// which is the case worth reporting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return the total number of known SPDX identifiers (includes textless IDs).
    #[must_use]
    pub fn total_known(&self) -> usize {
        self.entries.len() + self.known_ids.len()
    }

    /// Get an iterator over all license identifiers that have bundled text.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

/// A fluent builder for constructing a `LicenseDb`.
#[derive(Debug, Default)]
pub struct LicenseDbBuilder {
    entries: HashMap<String, LicenseEntry>,
    known_ids: HashSet<String>,
}

impl LicenseDbBuilder {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            known_ids: HashSet::new(),
        }
    }

    /// Add a license by its SPDX identifier and text content.
    #[must_use]
    pub fn add(mut self, id: impl Into<String>, text: impl Into<String>) -> Self {
        let id = id.into();
        let raw = text.into();
        let normalized = normalize(&raw);
        let hash = hash::hash_bytes(normalized.as_bytes());
        self.entries.insert(
            id,
            LicenseEntry {
                text: normalized,
                hash,
            },
        );
        self
    }

    /// Register a known SPDX license identifier without bundling its text.
    /// Adds the ID to the known-IDs set so `is_known()` returns true.
    #[must_use]
    pub fn add_known_id(mut self, id: impl Into<String>) -> Self {
        self.known_ids.insert(id.into());
        self
    }

    /// Register all known SPDX license identifiers from the SPDX License List
    /// without bundling their full texts.  This enables `is_known()` for the
    /// complete SPDX license list without the binary-size cost of all texts.
    #[must_use]
    pub fn add_all_known_ids(self) -> Self {
        KNOWN_LICENSES
            .iter()
            .fold(self, |b, id| b.add_known_id(*id))
    }

    /// Add all built-in standard SPDX licenses with bundled texts.
    #[must_use]
    pub fn add_standard(self) -> Self {
        self.add("MIT", LICENSE_MIT)
            .add("Apache-2.0", LICENSE_APACHE_2)
            .add("CC0-1.0", LICENSE_CC0_1)
            .add("GPL-3.0-only", LICENSE_GPL_3)
            .add("BSD-2-Clause", LICENSE_BSD_2)
            .add("BSD-3-Clause", LICENSE_BSD_3)
            .add("Unlicense", LICENSE_UNLICENSE)
            .add("MPL-2.0", LICENSE_MPL_2)
    }

    /// Add a license by reading from a file at the given path.
    #[allow(clippy::missing_errors_doc)]
    pub fn add_from_file(self, id: impl Into<String>, path: &Path) -> Result<Self, Error> {
        let text = fs::read_to_string(path)?;
        Ok(self.add(id, text))
    }

    /// Consume the builder and produce a `LicenseDb`.
    #[must_use]
    pub fn build(self) -> LicenseDb {
        LicenseDb {
            entries: self.entries,
            known_ids: self.known_ids,
        }
    }
}

/// Normalize license text for comparison.
fn normalize(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase()
}

// --- Built-in license texts (abridged for initial support) ----------------

const LICENSE_MIT: &str = r#"Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE."#;

const LICENSE_APACHE_2: &str = r#"Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at

http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License."#;

const LICENSE_CC0_1: &str = r#"Creative Commons Legal Code

CC0 1.0 Universal

CREATIVE COMMONS CORPORATION IS NOT A LAW FIRM AND DOES NOT PROVIDE LEGAL SERVICES. DISTRIBUTION OF THIS DOCUMENT DOES NOT CREATE AN ATTORNEY-CLIENT RELATIONSHIP. CREATIVE COMMONS PROVIDES THIS INFORMATION ON AN "AS-IS" BASIS. CREATIVE COMMONS MAKES NO WARRANTIES REGARDING THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS PROVIDED HEREUNDER, AND DISCLAIMS LIABILITY FOR DAMAGES RESULTING FROM THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS PROVIDED HEREUNDER.

Statement of Purpose

The laws of most jurisdictions throughout the world automatically confer exclusive Copyright and Related Rights (defined below) upon the creator and subsequent owner(s) (each and all, an "owner") of an original work of authorship and/or a database (each, a "Work").

Certain owners wish to permanently relinquish those rights to a Work for the purpose of contributing to a commons of creative, cultural and scientific works ("Commons") that the public can reliably and without fear of later claims of infringement build upon, modify, incorporate in other works, reuse and redistribute as freely as possible in any form whatsoever and for any purposes, including without limitation commercial purposes. These owners may contribute to the Commons to promote the ideal of a free culture and the further production of creative, cultural and scientific works, or to gain reputation or greater distribution for their Work in part through the use and efforts of others.

For these and/or other purposes and motivations, and without any expectation of additional consideration or compensation, the person associating CC0 with a Work (the "Affirmer"), to the extent that he or she is an owner of Copyright and Related Rights in the Work, voluntarily elects to apply CC0 to the Work and publicly distribute the Work under its terms, with knowledge of his or her Copyright and Related Rights in the Work and the meaning and intended legal effect of CC0 on those rights.

1. Copyright and Related Rights. A Work made available under CC0 may be protected by copyright and related or neighboring rights ("Copyright and Related Rights"). Copyright and Related Rights include, but are not limited to, the following:

i. the right to reproduce, adapt, distribute, perform, display, communicate, and translate a Work;

ii. moral rights retained by the original author(s) and/or performer(s);

iii. publicity and privacy rights pertaining to a person's image or likeness depicted in a Work;

iv. rights protecting against unfair competition in regards to a Work, subject to the limitations in paragraph 4(a), below;

v. rights protecting the extraction, dissemination, use and reuse of data in a Work;

vi. database rights (such as those arising under Directive 96/9/EC of the European Parliament and of the Council of 11 March 1996 on the legal protection of databases, and under any national implementation thereof, including any amended or successor version of such directive); and

vii. other similar, equivalent or corresponding rights throughout the world based on applicable law or treaty, and any national implementations thereof.

2. Waiver. To the greatest extent permitted by, but not in contravention of, applicable law, Affirmer hereby overtly, fully, permanently, irrevocably and unconditionally waives, abandons, and surrenders all of Affirmer's Copyright and Related Rights and associated claims and causes of action, whether now known or unknown (including existing as well as future claims and causes of action), in the Work (i) in all territories worldwide, (ii) for the maximum duration provided by applicable law or treaty (including future time extensions), (iii) in any current or future medium or number of copies, and (iv) for any purpose whatsoever, including without limitation commercial, advertising or promotional purposes (the "Waiver"). Affirmer makes the Waiver for the benefit of each member of the public at large and to the detriment of Affirmer's heirs and successors, fully intending that such Waiver shall not be subject to revocation, rescission, cancellation, termination, or any other legal or equitable action to disrupt the quiet enjoyment of the Work by the public as contemplated by Affirmer's express Statement of Purpose.

3. Public License Fallback. Should any part of the Waiver for any reason be judged legally invalid or ineffective under applicable law, then the Waiver shall be preserved to the maximum extent permitted taking into account Affirmer's express Statement of Purpose. In addition, to the extent the Waiver is so judged Affirmer hereby grants to each affected person a royalty-free, non transferable, non sublicensable, non exclusive, irrevocable and unconditional license to exercise Affirmer's Copyright and Related Rights in the Work (i) in all territories worldwide, (ii) for the maximum duration provided by applicable law or treaty (including future time extensions), (iii) in any current or future medium or number of copies, and (iv) for any purpose whatsoever, including without limitation commercial, advertising or promotional purposes (the "License"). The License shall be deemed effective as of the date CC0 was applied by Affirmer to the Work. Should any part of the License for any reason be judged legally invalid or ineffective under applicable law, such partial invalidity or ineffectiveness shall not invalidate the remainder of the License, and in such case Affirmer hereby affirms that he or she will not (i) exercise any of his or her remaining Copyright and Related Rights in the Work or (ii) assert any associated claims and causes of action with respect to the Work, in either case contrary to Affirmer's express Statement of Purpose.

4. Limitations and Disclaimers.

a. No trademark or patent rights held by Affirmer are waived, abandoned, surrendered, licensed or otherwise affected by this document.

b. Affirmer offers the Work as-is and makes no representations or warranties of any kind concerning the Work, express, implied, statutory or otherwise, including without limitation warranties of title, merchantability, fitness for a particular purpose, non infringement, or the absence of latent or other defects, accuracy, or the present or absence of errors, whether or not discoverable, all to the greatest extent permissible under applicable law.

c. Affirmer disclaims responsibility for clearing rights of other persons that may apply to the Work or any use thereof, including without limitation any person's Copyright and Related Rights in the Work. Further, Affirmer disclaims responsibility for obtaining any necessary consents, permissions or other rights required for any use of the Work.

d. Affirmer understands and acknowledges that Creative Commons is not a party to this document and has no duty or obligation with respect to this CC0 or use of the Work."#;

const LICENSE_GPL_3: &str = r"This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.

You should have received a copy of the GNU General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.";

const LICENSE_BSD_2: &str = r#"Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE."#;

const LICENSE_BSD_3: &str = r#"Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE."#;

const LICENSE_UNLICENSE: &str = r#"This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or distribute this software, either in source code form or as a compiled binary, for any purpose, commercial or non-commercial, and by any means.

In jurisdictions that recognize copyright laws, the author or authors of this software dedicate any and all copyright interest in the software to the public domain. We make this dedication for the benefit of the public at large and to the detriment of our heirs and successors. We intend this dedication to be an overt act of relinquishment in perpetuity of all present and future rights to this software under copyright law.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

For more information, please refer to <https://unlicense.org/>"#;

const LICENSE_MPL_2: &str = r"This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of the MPL was not distributed with this file, You can obtain one at https://mozilla.org/MPL/2.0/.";

// --- Helper: build the standard license DB ---------------------------------

/// Build a `LicenseDb` with all built-in standard licenses and all known
/// SPDX License List IDs.
#[must_use]
pub fn standard_db() -> LicenseDb {
    LicenseDb::builder()
        .add_standard()
        .add_all_known_ids()
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected values match the observed output of the reference `reuse`
    /// 6.2.0 implementation on equivalent fixtures.
    mod directory_audit {
        use super::*;
        use tempfile::TempDir;

        fn project(license_files: &[&str]) -> TempDir {
            let tmp = TempDir::new().unwrap();
            if !license_files.is_empty() {
                fs::create_dir_all(tmp.path().join("LICENSES")).unwrap();
                for name in license_files {
                    fs::write(tmp.path().join("LICENSES").join(name), "text").unwrap();
                }
            }
            tmp
        }

        fn used(ids: &[&str]) -> BTreeSet<String> {
            ids.iter().map(ToString::to_string).collect()
        }

        #[test]
        fn a_referenced_licence_with_no_file_is_missing() {
            let tmp = project(&[]);

            let audit = audit(tmp.path(), &used(&["MIT"])).unwrap();

            assert_eq!(audit.missing, ["MIT"]);
            assert!(audit.unused.is_empty());
        }

        #[test]
        fn a_file_nothing_references_is_unused() {
            let tmp = project(&["MIT.txt", "GPL-3.0-only.txt"]);

            let audit = audit(tmp.path(), &used(&["MIT"])).unwrap();

            assert_eq!(audit.unused, ["GPL-3.0-only"]);
            assert!(audit.missing.is_empty());
        }

        #[test]
        fn a_licence_file_without_an_extension_is_flagged_but_still_counts() {
            let tmp = project(&["MIT"]);

            let audit = audit(tmp.path(), &used(&["MIT"])).unwrap();

            assert_eq!(audit.without_extension, ["MIT"]);
            assert!(
                audit.missing.is_empty(),
                "the file is present, just badly named"
            );
        }

        #[test]
        fn a_stray_file_reads_as_an_unused_licence() {
            let tmp = project(&["MIT.txt", "README.md"]);

            let audit = audit(tmp.path(), &used(&["MIT"])).unwrap();

            assert_eq!(audit.unused, ["README"]);
        }

        #[test]
        fn an_unrecognised_identifier_is_a_bad_licence() {
            let tmp = project(&["NOT-A-LICENCE.txt"]);

            let audit = audit(tmp.path(), &used(&["NOT-A-LICENCE"])).unwrap();

            assert_eq!(audit.bad, ["NOT-A-LICENCE"]);
        }

        #[test]
        fn a_licenseref_identifier_is_not_bad() {
            let tmp = project(&["LicenseRef-Custom.txt"]);

            let audit = audit(tmp.path(), &used(&["LicenseRef-Custom"])).unwrap();

            assert!(
                audit.bad.is_empty(),
                "the spec reserves LicenseRef- for licences absent from the SPDX list"
            );
            assert!(audit.is_clean(), "got {audit:?}");
        }

        #[test]
        fn a_deprecated_identifier_is_reported_but_is_not_bad() {
            let tmp = project(&["GPL-3.0.txt"]);

            let audit = audit(tmp.path(), &used(&["GPL-3.0"])).unwrap();

            assert_eq!(audit.deprecated, ["GPL-3.0"]);
            assert!(
                audit.bad.is_empty(),
                "a deprecated id is still a known id, just one to migrate off"
            );
        }

        #[test]
        fn an_exception_identifier_is_recognised() {
            let tmp = project(&["GPL-3.0-only.txt", "Autoconf-exception-3.0.txt"]);

            let audit = audit(
                tmp.path(),
                &used(&["GPL-3.0-only", "Autoconf-exception-3.0"]),
            )
            .unwrap();

            assert!(audit.is_clean(), "got {audit:?}");
        }

        #[test]
        fn a_stray_file_in_licenses_is_bad_as_well_as_unused() {
            let tmp = project(&["MIT.txt", "README.md"]);

            let audit = audit(tmp.path(), &used(&["MIT"])).unwrap();

            assert_eq!(audit.bad, ["README"]);
            assert_eq!(audit.unused, ["README"]);
        }

        #[test]
        fn a_project_whose_files_and_usage_line_up_is_clean() {
            let tmp = project(&["MIT.txt", "Apache-2.0.txt"]);

            let audit = audit(tmp.path(), &used(&["MIT", "Apache-2.0"])).unwrap();

            assert!(audit.is_clean(), "got {audit:?}");
        }

        #[test]
        fn an_absent_licenses_directory_makes_everything_missing() {
            let tmp = project(&[]);

            let audit = audit(tmp.path(), &used(&["MIT", "Apache-2.0"])).unwrap();

            assert_eq!(audit.missing, ["Apache-2.0", "MIT"]);
            assert!(!audit.is_clean());
        }

        #[test]
        fn a_subdirectory_inside_licenses_is_skipped_not_read_as_an_identifier() {
            let tmp = project(&["MIT.txt"]);
            fs::create_dir(tmp.path().join("LICENSES").join("nested")).unwrap();

            let audit = audit(tmp.path(), &used(&["MIT"])).unwrap();

            // `nested` would otherwise surface as both unused and bad.
            assert!(audit.is_clean(), "{audit:?}");
        }

        #[test]
        fn a_licenses_path_that_is_not_a_directory_is_an_io_error() {
            let tmp = TempDir::new().unwrap();
            // A regular file where the directory should be: `read_dir` fails
            // with something other than NotFound, which must not be mistaken
            // for "no LICENSES/ directory".
            fs::write(tmp.path().join("LICENSES"), "not a directory").unwrap();

            let err = audit(tmp.path(), &used(&["MIT"])).unwrap_err();

            assert!(matches!(err, Error::Io(_)), "{err:?}");
        }
    }

    #[test]
    fn license_db_identifies_mit() {
        let db = standard_db();
        assert!(db.identify(LICENSE_MIT).is_some());
        assert_eq!(db.identify(LICENSE_MIT), Some("MIT"));
    }

    #[test]
    fn license_db_identifies_apache() {
        let db = standard_db();
        assert_eq!(db.identify(LICENSE_APACHE_2), Some("Apache-2.0"));
    }

    #[test]
    fn license_db_identifies_cc0() {
        let db = standard_db();
        assert_eq!(db.identify(LICENSE_CC0_1), Some("CC0-1.0"));
    }

    #[test]
    fn license_db_identifies_gpl3() {
        let db = standard_db();
        assert_eq!(db.identify(LICENSE_GPL_3), Some("GPL-3.0-only"));
    }

    #[test]
    fn license_db_identifies_bsd2() {
        let db = standard_db();
        assert_eq!(db.identify(LICENSE_BSD_2), Some("BSD-2-Clause"));
    }

    #[test]
    fn license_db_identifies_bsd3() {
        let db = standard_db();
        assert_eq!(db.identify(LICENSE_BSD_3), Some("BSD-3-Clause"));
    }

    #[test]
    fn license_db_identifies_unlicense() {
        let db = standard_db();
        assert_eq!(db.identify(LICENSE_UNLICENSE), Some("Unlicense"));
    }

    #[test]
    fn license_db_identifies_mpl2() {
        let db = standard_db();
        assert_eq!(db.identify(LICENSE_MPL_2), Some("MPL-2.0"));
    }

    #[test]
    fn license_db_unknown_text_returns_none() {
        let db = standard_db();
        assert!(db.identify("completely unknown license text").is_none());
    }

    #[test]
    fn license_db_whitespace_insensitive() {
        let db = standard_db();
        // Add extra whitespace/newlines
        let noisy = format!("\n\n  {LICENSE_MIT}  \n\n");
        assert_eq!(db.identify(&noisy), Some("MIT"));
    }

    #[test]
    fn license_db_case_insensitive() {
        let db = standard_db();
        let upper = LICENSE_MIT.to_uppercase();
        // After normalization (to_lowercase), this should match
        assert_eq!(db.identify(&upper), Some("MIT"));
    }

    #[test]
    fn license_db_builder_chain() {
        let db = LicenseDb::builder()
            .add("Custom", "custom text")
            .add("Other", "other text")
            .build();
        assert!(db.is_known("Custom"));
        assert!(db.is_known("Other"));
    }

    #[test]
    fn license_db_is_known() {
        let db = LicenseDb::builder().add("MIT", LICENSE_MIT).build();
        assert!(db.is_known("MIT"));
        assert!(!db.is_known("GPL-3.0-only"));
    }

    #[test]
    fn license_db_len() {
        let db = standard_db();
        assert_eq!(db.len(), 8);
        assert!(!db.is_empty());
    }

    #[test]
    fn license_db_total_known() {
        let db = standard_db();
        // total_known includes both text-entries AND known-ids
        assert!(db.total_known() > 700);
        assert!(db.is_known("MIT"));
        assert!(db.is_known("Apache-2.0"));
        assert!(db.is_known("GPL-3.0-only"));
        assert!(db.is_known("BSL-1.0"));
        assert!(db.is_known("OFL-1.1"));
        assert!(!db.is_known("NONEXISTENT-LICENSE"));
    }

    #[test]
    fn license_db_empty_builder() {
        let db = LicenseDb::builder().build();
        assert!(db.is_empty());
        assert_eq!(db.len(), 0);
    }

    #[test]
    fn ids_iterator() {
        let db = LicenseDb::builder()
            .add("MIT", "x")
            .add("Apache-2.0", "y")
            .build();
        let ids: Vec<&str> = db.ids().collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"MIT"));
        assert!(ids.contains(&"Apache-2.0"));
    }

    #[test]
    fn license_db_builder_add_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("LICENSE-MIT");
        fs::write(&path, LICENSE_MIT).unwrap();

        let db = LicenseDb::builder()
            .add_from_file("MIT", &path)
            .unwrap()
            .build();

        assert!(db.is_known("MIT"));
        assert_eq!(db.identify(LICENSE_MIT), Some("MIT"));
    }
}
