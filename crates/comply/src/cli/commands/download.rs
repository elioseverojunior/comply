// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fetch licence texts into `LICENSES/`.
//!
//! The transfer is delegated to `curl` rather than linking an HTTP/TLS stack.
//! That is a deliberate supply-chain choice: this project carries zero
//! `cargo-vet` exemptions and audits every dependency through imported audit
//! sets, and a TLS client would add a large subtree needing fresh audits for a
//! feature used once per licence. `--source` accepts a local template, so the
//! network is not on the tested path.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use anyhow::{Context, Result, bail};
use comply::spdx;

use crate::cli::commands::lint::lint_project;
use crate::cli::shared::{load_config, resolve_root};

/// `{}` is replaced with the SPDX identifier.
const DEFAULT_SOURCE: &str =
    "https://raw.githubusercontent.com/spdx/license-list-data/main/text/{}.txt";

/// What to fetch and where to put it.
pub(crate) struct Request<'a> {
    pub(crate) ids: &'a [String],
    pub(crate) all: bool,
    pub(crate) output: Option<&'a String>,
    pub(crate) source: Option<&'a String>,
    pub(crate) force: bool,
}

pub(crate) fn run(path: Option<&String>, request: &Request<'_>) -> Result<()> {
    let root = resolve_root(path)?;
    let target = request
        .output
        .map_or_else(|| root.join("LICENSES"), PathBuf::from);
    let template = request.source.map_or(DEFAULT_SOURCE, String::as_str);

    let ids = resolve_ids(&root, request)?;
    if ids.is_empty() {
        println!("Nothing to download: every referenced licence already has a file.");
        return Ok(());
    }

    fs::create_dir_all(&target)
        .with_context(|| format!("failed to create {}", target.display()))?;

    for id in &ids {
        // `GPL-3.0+` and `GPL-3.0` are one document -- the `+` means "or
        // later", a property of the expression, not of the text -- so it is
        // stripped before the id is used for anything.
        let id = id.strip_suffix('+').unwrap_or(id);
        let destination = target.join(format!("{id}.txt"));

        // A `LicenseRef-` identifier is defined by the project, so the SPDX
        // list cannot know it and there is no upstream text to fetch.
        let project_local = is_license_ref(id);

        // An unknown identifier has no upstream text; saying so beats a 404.
        if !project_local && !spdx::is_known_license(id) && !spdx::is_known_exception(id) {
            bail!("'{id}' is not a known SPDX identifier; nothing to download");
        }

        // One skip check for both kinds -- it used to be written twice, and the
        // copy in the LicenseRef- branch was unreachable by any test.
        if destination.exists() && !request.force {
            println!("{id}: already present, skipping");
            continue;
        }

        // The file still has to exist or `lint` keeps reporting the licence
        // missing, so a project-local one gets an empty placeholder to fill in.
        if project_local {
            fs::write(&destination, "")
                .with_context(|| format!("failed to write {}", destination.display()))?;
            println!(
                "{id}: empty placeholder written to {}",
                destination.display()
            );
            continue;
        }

        // Retired identifiers are published as `deprecated_<id>`; the plain
        // path 404s for them. The file is still stored under the plain id,
        // which is what REUSE.toml and the headers reference.
        let source_id = if spdx::deprecated_licenses().contains(&id) {
            format!("deprecated_{id}")
        } else {
            id.to_string()
        };

        let text = fetch(template, &source_id)?;
        if text.trim().is_empty() {
            bail!("'{id}': the source returned an empty document");
        }
        fs::write(&destination, text)
            .with_context(|| format!("failed to write {}", destination.display()))?;
        println!("{id}: written to {}", destination.display());
    }
    Ok(())
}

/// Whether an identifier is project-local rather than from the SPDX list.
///
/// REUSE 3.3 section 2.1.2: `LicenseRef-` names a licence the project defines
/// itself, so nothing upstream can supply its text. Matched case-insensitively
/// on the prefix only, which is what the reference tool's pattern does.
fn is_license_ref(id: &str) -> bool {
    id.len() > "LicenseRef-".len()
        && id
            .get(.."LicenseRef-".len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("LicenseRef-"))
}

/// Explicit identifiers, or -- with `--all` -- the ones the project references
/// but has no file for. The latter is what makes this the remedy for the
/// missing-licence lint rather than a separate chore.
fn resolve_ids(root: &Path, request: &Request<'_>) -> Result<Vec<String>> {
    if !request.all {
        return Ok(request.ids.to_vec());
    }

    let config = load_config(root)?;
    let report = lint_project(root, &config)?;
    let mut ids = report.licenses().missing.clone();
    ids.extend(request.ids.iter().cloned());
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

/// Resolve one identifier's text from `template`.
///
/// A template naming a local path is read directly, which is what keeps the
/// tests off the network.
fn fetch(template: &str, id: &str) -> Result<String> {
    let location = template.replace("{}", id);

    if let Some(local) = location.strip_prefix("file://") {
        return fs::read_to_string(local)
            .with_context(|| format!("failed to read {local} for '{id}'"));
    }
    if !location.starts_with("http://") && !location.starts_with("https://") {
        return fs::read_to_string(&location)
            .with_context(|| format!("failed to read {location} for '{id}'"));
    }

    let output = Process::new("curl")
        .args(["-sfL", "--max-time", "30", &location])
        .output()
        .context("could not run `curl`; install it or pass --source with a local path")?;

    if !output.status.success() {
        bail!("failed to download '{id}' from {location}");
    }
    // Lossy rather than strict: a licence text with one stray byte upstream is
    // still worth writing and inspecting, and failing the whole fetch over it
    // helps nobody.
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// A directory of licence texts, used as a `--source` so no test needs the
    /// network.
    fn source_dir() -> (TempDir, String) {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("MIT.txt"), "MIT License text\n").unwrap();
        fs::write(tmp.path().join("Apache-2.0.txt"), "Apache text\n").unwrap();
        fs::write(tmp.path().join("Empty.txt"), "   \n").unwrap();
        let template = format!("{}/{{}}.txt", tmp.path().display());
        (tmp, template)
    }

    fn project() -> (TempDir, PathBuf, String) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::write(root.join("REUSE.toml"), "version = 1\n").unwrap();
        let arg = root.to_str().unwrap().to_string();
        (tmp, root, arg)
    }

    fn request<'a>(ids: &'a [String], source: &'a String, all: bool, force: bool) -> Request<'a> {
        Request {
            ids,
            all,
            output: None,
            source: Some(source),
            force,
        }
    }

    /// Behaviours checked against `reuse` 6.2.0 on the same inputs. Each of
    /// these was an error in comply where the reference tool wrote a file, so
    /// a project referencing such a licence could not be made compliant with
    /// `download --all`.
    mod reference_parity {
        use super::*;

        #[test]
        fn a_plus_suffix_resolves_to_the_base_licence() {
            // `GPL-3.0+` and `GPL-3.0` share one text upstream; the suffix says
            // "or later", which is a property of the expression, not the file.
            // Passing it through produced a request for `GPL-3.0+.txt` -- 404.
            // A licence that is NOT deprecated, so this isolates `+` handling
            // from the deprecated-path fallback below.
            let (_src, template) = source_dir();
            let (_tmp, root, arg) = project();
            let ids = vec!["MIT+".to_string()];

            run(Some(&arg), &request(&ids, &template, false, false)).unwrap();

            assert_eq!(
                fs::read_to_string(root.join("LICENSES/MIT.txt")).unwrap(),
                "MIT License text\n",
                "the `+` belongs to the expression, so the file is the base id"
            );
            assert!(
                !root.join("LICENSES/MIT+.txt").exists(),
                "nothing should be stored under the suffixed name"
            );
        }

        #[test]
        fn a_deprecated_identifier_falls_back_to_the_deprecated_path() {
            // The SPDX list publishes retired identifiers under
            // `deprecated_<id>.txt`; `<id>.txt` is a 404 for them. Verified
            // live: GPL-3.0.txt is 404, deprecated_GPL-3.0.txt is 200.
            let tmp = TempDir::new().unwrap();
            fs::write(tmp.path().join("deprecated_GPL-3.0.txt"), "old GPL\n").unwrap();
            let template = format!("{}/{{}}.txt", tmp.path().display());
            let (_tmp, root, arg) = project();
            let ids = vec!["GPL-3.0".to_string()];

            run(Some(&arg), &request(&ids, &template, false, false)).unwrap();

            assert_eq!(
                fs::read_to_string(root.join("LICENSES/GPL-3.0.txt")).unwrap(),
                "old GPL\n",
                "fetched from the deprecated path, stored under the plain id"
            );
        }

        #[test]
        fn a_license_ref_gets_an_empty_placeholder() {
            // A `LicenseRef-` identifier is project-local: there is nothing to
            // download, but the file has to exist or `lint` keeps reporting the
            // licence missing. The reference tool touches an empty file.
            let (_src, template) = source_dir();
            let (_tmp, root, arg) = project();
            let ids = vec!["LicenseRef-Custom".to_string()];

            run(Some(&arg), &request(&ids, &template, false, false)).unwrap();

            let written = root.join("LICENSES/LicenseRef-Custom.txt");
            assert!(written.exists(), "placeholder must be created");
            assert_eq!(
                fs::read_to_string(&written).unwrap(),
                "",
                "nothing to download, so the placeholder is empty"
            );
        }

        #[test]
        fn an_unknown_identifier_is_still_refused() {
            // The three cases above widen what is accepted; this pins that a
            // genuine typo is still rejected rather than silently fetched.
            let (_src, template) = source_dir();
            let (_tmp, _root, arg) = project();
            let ids = vec!["Not-A-Licence".to_string()];

            let err = run(Some(&arg), &request(&ids, &template, false, false)).unwrap_err();

            assert!(
                err.to_string().contains("not a known SPDX identifier"),
                "got: {err}"
            );
        }
    }

    #[test]
    fn a_named_licence_lands_in_the_licenses_directory() {
        let (_src, template) = source_dir();
        let (_tmp, root, arg) = project();
        let ids = vec!["MIT".to_string()];

        run(Some(&arg), &request(&ids, &template, false, false)).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("LICENSES/MIT.txt")).unwrap(),
            "MIT License text\n"
        );
    }

    #[test]
    fn an_existing_file_is_kept_unless_force_is_given() {
        let (_src, template) = source_dir();
        let (_tmp, root, arg) = project();
        fs::create_dir_all(root.join("LICENSES")).unwrap();
        fs::write(root.join("LICENSES/MIT.txt"), "hand-edited\n").unwrap();
        let ids = vec!["MIT".to_string()];

        run(Some(&arg), &request(&ids, &template, false, false)).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("LICENSES/MIT.txt")).unwrap(),
            "hand-edited\n",
            "a local edit must not be clobbered silently"
        );

        run(Some(&arg), &request(&ids, &template, false, true)).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("LICENSES/MIT.txt")).unwrap(),
            "MIT License text\n",
            "--force is the way to replace it"
        );
    }

    #[test]
    fn all_fetches_exactly_what_the_lint_reports_missing() {
        let (_src, template) = source_dir();
        let (_tmp, root, arg) = project();
        // A file referencing Apache-2.0 with no matching text: precisely the
        // missing-licence finding this flag is meant to clear.
        fs::write(
            root.join("a.py"),
            "# SPDX-FileCopyrightText: 2026 A\n# SPDX-License-Identifier: Apache-2.0\nx = 1\n",
        )
        .unwrap();

        run(Some(&arg), &request(&[], &template, true, false)).unwrap();

        assert!(root.join("LICENSES/Apache-2.0.txt").exists());
        assert!(
            !root.join("LICENSES/MIT.txt").exists(),
            "--all must not pull licences the project never references"
        );
    }

    #[test]
    fn an_unknown_identifier_is_refused_before_any_transfer() {
        let (_src, template) = source_dir();
        let (_tmp, _root, arg) = project();
        let ids = vec!["Not-A-Licence".to_string()];

        let err = run(Some(&arg), &request(&ids, &template, false, false)).unwrap_err();

        assert!(
            format!("{err:#}").contains("not a known SPDX identifier"),
            "got: {err:#}"
        );
    }

    #[test]
    fn an_empty_document_is_an_error_rather_than_an_empty_licence_file() {
        let (_src, template) = source_dir();
        let (_tmp, root, arg) = project();
        // `Empty` is not a real identifier, so use a real one pointed at the
        // blank fixture by renaming the source entry.
        fs::write(PathBuf::from(template.replace("{}", "0BSD")), "   \n").unwrap();
        let ids = vec!["0BSD".to_string()];

        let err = run(Some(&arg), &request(&ids, &template, false, false)).unwrap_err();

        assert!(
            format!("{err:#}").contains("empty document"),
            "got: {err:#}"
        );
        assert!(
            !root.join("LICENSES/0BSD.txt").exists(),
            "nothing should be written when the source is blank"
        );
    }

    #[test]
    fn a_missing_source_file_names_what_could_not_be_read() {
        let (_src, template) = source_dir();
        let (_tmp, _root, arg) = project();
        let ids = vec!["0BSD".to_string()];

        let err = run(Some(&arg), &request(&ids, &template, false, false)).unwrap_err();

        assert!(format!("{err:#}").contains("0BSD"), "got: {err:#}");
    }

    /// A one-shot HTTP server on an ephemeral loopback port.
    ///
    /// Exercises the real `curl` path -- process spawn, response body, decode --
    /// without reaching the network or adding an HTTP dependency to the tree.
    fn serve_once(body: &'static str) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read as _, Write as _};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut discard = [0_u8; 1024];
                let _ = stream.read(&mut discard);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (format!("http://127.0.0.1:{port}/{{}}.txt"), handle)
    }

    #[test]
    fn a_successful_transfer_writes_exactly_what_the_server_returned() {
        let (template, server) = serve_once("MIT text off the wire\n");
        let (_tmp, root, arg) = project();
        let ids = vec!["MIT".to_string()];

        run(Some(&arg), &request(&ids, &template, false, false)).unwrap();
        server.join().unwrap();

        assert_eq!(
            fs::read_to_string(root.join("LICENSES/MIT.txt")).unwrap(),
            "MIT text off the wire\n"
        );
    }

    #[test]
    fn a_transfer_that_cannot_connect_reports_the_identifier_and_location() {
        let (_tmp, root, arg) = project();
        // Port 1 refuses immediately, so this exercises the curl path without
        // reaching the network.
        let template = "http://127.0.0.1:1/{}.txt".to_string();
        let ids = vec!["MIT".to_string()];

        let err = run(Some(&arg), &request(&ids, &template, false, false)).unwrap_err();

        let message = format!("{err:#}");
        assert!(message.contains("MIT"), "got: {message}");
        assert!(message.contains("127.0.0.1"), "got: {message}");
        assert!(
            !root.join("LICENSES/MIT.txt").exists(),
            "a failed transfer must leave nothing behind"
        );
    }

    #[test]
    fn a_file_url_template_is_read_from_disk() {
        let (src, _template) = source_dir();
        let (_tmp, root, arg) = project();
        let template = format!("file://{}/{{}}.txt", src.path().display());
        let ids = vec!["MIT".to_string()];

        run(Some(&arg), &request(&ids, &template, false, false)).unwrap();

        assert!(root.join("LICENSES/MIT.txt").exists());
    }

    #[test]
    fn an_output_directory_overrides_the_default() {
        let (_src, template) = source_dir();
        let (_tmp, root, arg) = project();
        let elsewhere = root.join("vendor/licences");
        let ids = vec!["MIT".to_string()];

        run(
            Some(&arg),
            &Request {
                ids: &ids,
                all: false,
                output: Some(&elsewhere.to_str().unwrap().to_string()),
                source: Some(&template),
                force: false,
            },
        )
        .unwrap();

        assert!(elsewhere.join("MIT.txt").exists());
    }

    #[test]
    fn a_project_with_nothing_missing_reports_that_and_writes_nothing() {
        let (_src, template) = source_dir();
        let (_tmp, root, arg) = project();

        run(Some(&arg), &request(&[], &template, true, false)).unwrap();

        assert!(
            !root.join("LICENSES").exists(),
            "no directory should be created when there is nothing to fetch"
        );
    }

    #[test]
    fn an_exception_identifier_is_accepted_as_well_as_a_licence() {
        let (src, template) = source_dir();
        let (_tmp, root, arg) = project();
        fs::write(
            src.path().join("Classpath-exception-2.0.txt"),
            "exception text\n",
        )
        .unwrap();
        let ids = vec!["Classpath-exception-2.0".to_string()];

        run(Some(&arg), &request(&ids, &template, false, false)).unwrap();

        assert!(root.join("LICENSES/Classpath-exception-2.0.txt").exists());
    }
}
// REUSE-IgnoreEnd
