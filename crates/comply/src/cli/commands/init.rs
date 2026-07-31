// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::{Context, Result};
use chrono::Datelike;
use std::fs;
use std::path::Path;

#[allow(clippy::too_many_lines)]
pub(crate) fn run(cmd: &crate::Command) -> Result<()> {
    let (path, force, config_name, license, holders) = match cmd {
        crate::Command::Init {
            path,
            force,
            config_name,
            license,
            holder,
        } => (
            path.as_deref().unwrap_or("."),
            *force,
            config_name.as_str(),
            license.as_str(),
            holder.as_slice(),
        ),
        _ => return Ok(()),
    };

    let root = Path::new(path);
    let root = root
        .canonicalize()
        .context("failed to resolve project root path")?;

    let config_path = root.join(config_name);

    if config_path.exists() && !force {
        return Err(anyhow::anyhow!(
            "{} already exists. Use --force to overwrite",
            config_path.display()
        ));
    }

    let current_year = chrono::Utc::now().year().to_string();

    // Build copyright from holders or default
    let copyright = if holders.is_empty() {
        format!("{current_year} Your Name <you@example.com>")
    } else {
        holders
            .iter()
            .map(|h| format!("{current_year} {h}"))
            .collect::<Vec<_>>()
            .join(", ")
    };

    // Create REUSE.toml (or configured name)
    let reuse_toml = format!(
        r#"# REUSE compliance configuration
# See https://reuse.software/spec-3.3/ for format specification

version = 1

[[annotations]]
SPDX-FileCopyrightText = "{copyright}"
SPDX-License-Identifier = "{license}"
path = ["**"]
precedence = "aggregate"

[tool.comply]
# Custom comply settings
ignore = ["**/target/**", "**/*.lock"]

"#
    );

    fs::write(&config_path, reuse_toml).context("failed to write REUSE.toml")?;

    // Create LICENSE (MIT)
    let license_mit = format!(
        r#"MIT License

Copyright (c) {copyright}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"#
    );
    let license_path = root.join("LICENSE");
    if license_path.exists() && !force {
        return Err(anyhow::anyhow!(
            "LICENSE already exists. Use --force to overwrite"
        ));
    }
    fs::write(&license_path, license_mit).context("failed to write LICENSE")?;

    // Create LICENSE-APACHE (Apache-2.0)
    let license_apache = format!(
        r#"Apache License
Version 2.0, January 2004
https://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

"License" shall mean the terms and conditions for use, reproduction, and
distribution as defined by Sections 1 through 9 of this document.

[... truncated for brevity ...]

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

To apply the Apache License to your work, attach the following boilerplate
notice, with the fields enclosed by brackets "[]" replaced with your own
identifying information. (Don't include the brackets!) The text should
be enclosed in the appropriate comment syntax for the file format. We also
recommend that a file or class name and description of purpose be included
on the same "printed page" as the copyright notice for easier identification
within third-party archives.

Copyright {copyright}

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    https://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
"#
    );
    let license_apache_path = root.join("LICENSE-APACHE");
    if license_apache_path.exists() && !force {
        return Err(anyhow::anyhow!(
            "LICENSE-APACHE already exists. Use --force to overwrite"
        ));
    }
    fs::write(&license_apache_path, license_apache).context("failed to write LICENSE-APACHE")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_cmd(root: &Path, force: bool, holder: Vec<String>) -> crate::Command {
        crate::Command::Init {
            path: Some(root.to_str().unwrap().to_string()),
            force,
            config_name: "REUSE.toml".to_string(),
            license: "MIT OR Apache-2.0".to_string(),
            holder,
        }
    }

    fn empty_project() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn a_non_init_command_is_a_no_op() {
        let cmd = crate::Command::Fix {
            path: None,
            dry_run: true,
        };

        assert!(run(&cmd).is_ok());
    }

    #[test]
    fn test_init_creates_reuse_toml() {
        let tmp = empty_project();

        run(&init_cmd(tmp.path(), false, vec![])).unwrap();

        let reuse_toml = tmp.path().join("REUSE.toml");
        assert!(reuse_toml.exists(), "REUSE.toml should be created");

        let content = fs::read_to_string(&reuse_toml).unwrap();
        assert!(content.contains("SPDX-FileCopyrightText"));
        assert!(content.contains("SPDX-License-Identifier"));
        assert!(content.contains("[[annotations]]"));
    }

    #[test]
    fn test_init_creates_license_files() {
        let tmp = empty_project();

        run(&init_cmd(tmp.path(), false, vec![])).unwrap();

        assert!(tmp.path().join("LICENSE").exists());
        assert!(tmp.path().join("LICENSE-APACHE").exists());
    }

    #[test]
    fn holders_replace_the_placeholder_copyright() {
        let tmp = empty_project();

        run(&init_cmd(
            tmp.path(),
            false,
            vec!["Ada Lovelace".to_string(), "Grace Hopper".to_string()],
        ))
        .unwrap();

        let content = fs::read_to_string(tmp.path().join("REUSE.toml")).unwrap();
        let year = chrono::Utc::now().year();
        assert!(
            content.contains(&format!("{year} Ada Lovelace, {year} Grace Hopper")),
            "each --holder should be year-prefixed and comma-joined, got:\n{content}"
        );
        assert!(
            !content.contains("Your Name"),
            "the placeholder must be gone when holders are given"
        );
    }

    #[test]
    fn an_existing_config_is_not_overwritten_without_force() {
        let tmp = empty_project();
        fs::write(tmp.path().join("REUSE.toml"), "version = 1\n").unwrap();

        let err = run(&init_cmd(tmp.path(), false, vec![])).unwrap_err();

        assert!(
            err.to_string().contains("--force"),
            "the error should point at --force, got: {err}"
        );
        assert_eq!(
            fs::read_to_string(tmp.path().join("REUSE.toml")).unwrap(),
            "version = 1\n",
            "the existing config must survive"
        );
    }

    #[test]
    fn force_overwrites_an_existing_config() {
        let tmp = empty_project();
        fs::write(tmp.path().join("REUSE.toml"), "version = 1\n").unwrap();

        run(&init_cmd(tmp.path(), true, vec![])).unwrap();

        assert!(
            fs::read_to_string(tmp.path().join("REUSE.toml"))
                .unwrap()
                .contains("[[annotations]]"),
            "--force should replace the file wholesale"
        );
    }

    #[test]
    fn an_existing_mit_license_is_not_overwritten_without_force() {
        let tmp = empty_project();
        fs::write(tmp.path().join("LICENSE"), "not really a licence\n").unwrap();

        let err = run(&init_cmd(tmp.path(), false, vec![])).unwrap_err();

        assert!(
            err.to_string().contains("LICENSE already exists"),
            "got: {err}"
        );
        assert_eq!(
            fs::read_to_string(tmp.path().join("LICENSE")).unwrap(),
            "not really a licence\n"
        );
    }

    #[test]
    fn an_existing_apache_license_is_not_overwritten_without_force() {
        let tmp = empty_project();
        fs::write(tmp.path().join("LICENSE-APACHE"), "not really a licence\n").unwrap();

        let err = run(&init_cmd(tmp.path(), false, vec![])).unwrap_err();

        assert!(
            err.to_string().contains("LICENSE-APACHE already exists"),
            "got: {err}"
        );
        assert_eq!(
            fs::read_to_string(tmp.path().join("LICENSE-APACHE")).unwrap(),
            "not really a licence\n"
        );
    }
}
