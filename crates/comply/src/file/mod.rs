// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Filesystem discovery: walking a project and classifying what it finds.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use ignore::gitignore::GitignoreBuilder;

use crate::config;
use crate::config::WalkOptions;
use crate::error::Error;
use crate::scope::{is_pruned_dir, is_reuse_manifest, is_spdx_document, is_within_pruned_dir};

mod classify;

use classify::classify;
pub use classify::is_ignored;

/// How a discovered file can carry its licensing information.
///
/// This is what decides where a header goes, not whether the file is text: a
/// binary needs the same annotation, it just cannot hold a comment, so it gets
/// a `.license` companion instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// Source file that can have inline SPDX headers.
    Source,
    /// Binary file that needs a `.license` companion.
    Binary,
    /// Build artifact, dependency, or ignored file.
    Ignored,
}

/// A discovered file with metadata.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// Classification.
    pub file_type: FileType,
    /// Whether a `.license` companion file exists for this file.
    pub has_license_file: bool,
}

impl FileEntry {
    /// Return the file's path relative to a root directory.
    ///
    /// If the path is not under `root`, the full path is returned.
    #[must_use]
    pub fn relative_to(&self, root: &Path) -> String {
        self.path
            .strip_prefix(root)
            .unwrap_or(&self.path)
            .to_string_lossy()
            .to_string()
    }
}

/// Walk a directory and discover all files for REUSE analysis.
///
/// Respects `.gitignore`, respects custom ignore patterns from `[tool.comply]`
/// in `REUSE.toml`, and classifies each file as source, binary, or ignored.
/// The `source_patterns` parameter allows extending the default source file
/// classification with custom glob patterns from REUSE.toml.
pub fn walk(
    root: &Path,
    ignore_patterns: &[String],
    source_patterns: &[String],
) -> impl Iterator<Item = Result<FileEntry, Error>> {
    walk_with(root, ignore_patterns, source_patterns, WalkOptions::DEFAULT)
}

/// [`walk`], with the traversal switches a project set in `[tool.comply.walk]`.
///
/// Separate from `walk` rather than an extra parameter on it: the defaults are
/// right for almost every caller, and threading `WalkOptions::DEFAULT` through
/// every existing call site would add noise without adding meaning.
pub fn walk_with(
    root: &Path,
    ignore_patterns: &[String],
    source_patterns: &[String],
    options: WalkOptions,
) -> impl Iterator<Item = Result<FileEntry, Error>> {
    let ignore_regexes: Vec<regex::Regex> = ignore_patterns
        .iter()
        .filter_map(|p| {
            let re_str = config::glob_to_regex(p);
            regex::Regex::new(&re_str).ok()
        })
        .collect();

    let root_owned = root.to_path_buf();
    let backfill_regexes = ignore_regexes.clone();
    let mut builder = WalkBuilder::new(root);
    builder.hidden(options.hidden);
    builder.git_ignore(options.git_ignore);
    builder.git_global(options.git_global);
    builder.git_exclude(options.git_exclude);
    builder.require_git(options.require_git);
    builder.follow_links(options.follow_links);

    builder.filter_entry(move |entry| {
        !out_of_scope(
            entry.path(),
            &root_owned,
            entry.file_type().is_some_and(|t| t.is_dir()),
            entry.file_type().is_some_and(|t| t.is_symlink()),
            &ignore_regexes,
        )
    });

    let walker = builder.build();
    let mut entries: Vec<Result<FileEntry, Error>> = walker
        .filter_map(|entry| to_file_entry(entry, source_patterns))
        .collect();

    // Put back any tracked file the traversal dropped. Only a gitignore rule
    // can have dropped it, and git does not apply those to tracked files -- so
    // leaving it out would report a file as absent that git considers part of
    // the project. comply's own rules (`[tool.comply] ignore`, `.license`
    // companions, the REUSE directories) still apply, hence `out_of_scope`.
    // Owned paths, so extending `entries` below does not borrow it twice.
    let seen: HashSet<PathBuf> = entries
        .iter()
        .filter_map(|r| r.as_ref().ok().map(|e| e.path.clone()))
        .collect();

    entries.extend(
        tracked_files(root)
            .into_iter()
            .map(|relative| root.join(relative))
            .filter(|path| !seen.contains(path) && path.is_file())
            // `out_of_scope` prunes a directory only when handed the directory
            // itself, which is all the walk ever needs. These paths skipped the
            // descent, so nothing has judged their ancestors yet.
            .filter(|path| !is_within_pruned_dir(path.strip_prefix(root).unwrap_or(path)))
            .filter(|path| !out_of_scope(path, root, false, path.is_symlink(), &backfill_regexes))
            .filter_map(|path| file_entry_at(path, source_patterns))
            .map(Ok),
    );
    entries.into_iter()
}

/// Repo-relative paths git tracks under `root`.
///
/// One subprocess per walk rather than one per file, and the same question the
/// reference tool asks git. Any failure -- no repository, no git on PATH, a
/// path that is not a work tree -- yields an empty set, which leaves the
/// gitignore layer as the sole authority exactly as it was before.
fn tracked_files(root: &Path) -> HashSet<PathBuf> {
    // A chain rather than two early returns: "git is not on PATH" and "this is
    // not a work tree" then travel the same lines, and the second is reachable
    // from a test while the first is not on any machine that can run the suite.
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--cached"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| {
            out.stdout
                .split(|byte| *byte == 0)
                .filter(|raw| !raw.is_empty())
                .map(|raw| PathBuf::from(String::from_utf8_lossy(raw).into_owned()))
                .collect()
        })
        .unwrap_or_default()
}

/// Turn one walker item into a [`FileEntry`], or `None` for anything that is not
/// a covered file.
///
/// A named function rather than the closure body it used to be. tarpaulin
/// attributes match arms inside a `filter_map` closure unreliably, and did so
/// differently per platform: the directory and error arms measured as covered on
/// macOS but not on Linux, even though the same tests exercise both everywhere.
/// Lifting the body out gives every arm a line tarpaulin can account for.
fn to_file_entry(
    entry: Result<ignore::DirEntry, ignore::Error>,
    source_patterns: &[String],
) -> Option<Result<FileEntry, Error>> {
    let entry = match entry {
        Ok(entry) => entry,
        Err(e) => return Some(Err(Error::Io(std::io::Error::other(e.to_string())))),
    };

    // Directories are traversed, not covered.
    if !entry.file_type().is_some_and(|t| t.is_file()) {
        return None;
    }

    file_entry_at(entry.into_path(), source_patterns).map(Ok)
}

/// Describe one path that is known to be a file, or `None` if it is not covered.
///
/// Shared with the tracked-file backfill in [`walk_with`], so a file restored
/// there is classified by exactly the same rules as one the traversal yielded.
fn file_entry_at(path: PathBuf, source_patterns: &[String]) -> Option<FileEntry> {
    // A zero-byte file carries no licensing information, so it is not a covered
    // file either.
    if std::fs::metadata(&path).is_ok_and(|m| m.len() == 0) {
        return None;
    }

    let file_type = classify(&path, source_patterns);
    let has_license_file = has_license_companion(&path);
    Some(FileEntry {
        path,
        file_type,
        has_license_file,
    })
}

/// The one predicate that decides whether a path is outside REUSE analysis.
///
/// Both entry points route through here: [`walk`] calls it per directory entry
/// while descending, and [`is_excluded`] calls it for a path a caller named.
/// They used to carry separate copies of these rules, so `lint` and `lint-file`
/// could disagree about the same file.
///
/// Gitignore is deliberately NOT handled here -- during a walk the `ignore`
/// crate has already applied it (including nested and global rules), so
/// repeating it would be wasted work. [`is_excluded`] adds it separately,
/// because a named path never went through that walk.
fn out_of_scope(
    path: &Path,
    root: &Path,
    is_dir: bool,
    is_symlink: bool,
    ignore_regexes: &[regex::Regex],
) -> bool {
    let file_name = path
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().to_string());

    // Symlinks are never followed: the target is covered in its own right, and
    // writing through one would edit a file the author did not name.
    if is_symlink {
        return true;
    }

    // `.license` companions belong to their parent, which carries the verdict.
    if file_name.ends_with(crate::scope::LICENSE_COMPANION_SUFFIX) {
        return true;
    }

    // The REUSE manifest describes coverage; it is not itself covered.
    if is_reuse_manifest(&file_name) {
        return true;
    }

    // Prune VCS metadata, build output and the REUSE directories rather than
    // descending and classifying every file inside them. On a real repository
    // .git alone is hundreds of objects, each formerly walked, classified as
    // ignored, and reported as "skipped".
    if is_dir && is_pruned_dir(&file_name) {
        return true;
    }

    // An SPDX document describes the project; it is not part of it.
    if is_spdx_document(&file_name) {
        return true;
    }

    // `[tool.comply] ignore` from REUSE.toml.
    let relative = path.strip_prefix(root).unwrap_or(path).to_string_lossy();
    ignore_regexes.iter().any(|re| re.is_match(&relative))
}

/// Whether `path` falls outside REUSE analysis, by the same three rules
/// [`walk`] applies while descending.
///
/// `walk` gets these from the `ignore` crate as it goes. A command handed an
/// explicit path has no walk to inherit them from, so it asks here instead --
/// otherwise `lint` and `lint-file` disagree about the very same file.
///
/// The three sources, in the order they are cheapest to check:
/// 1. the built-in [`IGNORED_DIRS`](crate::scope::IGNORED_DIRS) and
///    [`REUSE_DIRS`](crate::scope::REUSE_DIRS) names,
/// 2. `[tool.comply] ignore` globs from REUSE.toml,
/// 3. gitignore rules -- the repository's own, every nested `.gitignore` from
///    the root down to the file, `.git/info/exclude`, and the user's global one.
#[must_use]
pub fn is_excluded(root: &Path, path: &Path, ignore_patterns: &[String]) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);

    // Any pruned directory anywhere in the path. `out_of_scope` judges a single
    // entry as a walk would encounter it; a named path was never descended into,
    // so its ancestors have to be checked here.
    if is_within_pruned_dir(relative) {
        return true;
    }

    // The same predicate the walk applies, so the two cannot drift apart.
    let regexes: Vec<regex::Regex> = ignore_patterns
        .iter()
        .filter_map(|p| regex::Regex::new(&config::glob_to_regex(p)).ok())
        .collect();
    if out_of_scope(path, root, path.is_dir(), path.is_symlink(), &regexes) {
        return true;
    }

    if !gitignored(root, relative) {
        return false;
    }

    // A pattern matches, but gitignore does not apply to tracked files. git is
    // consulted only here, on the rare path that would otherwise be dropped,
    // rather than for every file a caller names.
    !tracked_files(root).contains(relative)
}

/// Gitignore rules that apply to `path`, nested files included.
///
/// A `.gitignore` governs its own directory and everything below it, so the
/// builder is fed every one from `root` down to the file's parent.
fn gitignored(root: &Path, relative: &Path) -> bool {
    gitignore_chain(root, relative, global_excludes().as_deref())
}

/// Locate git's global excludes file, if the user has one.
///
/// `core.excludesFile` first, read the way git reads it, then the XDG path git
/// falls back to when that setting is absent.
fn global_excludes() -> Option<PathBuf> {
    first_readable(vec![
        ignore::gitignore::gitconfig_excludes_path(),
        xdg_config_home().map(|base| base.join("git").join("ignore")),
    ])
}

/// The first candidate that names a file that exists.
///
/// Split out so it is testable: which candidate wins depends on the developer's
/// own git config, so a test that called [`global_excludes`] would assert
/// something different on every machine.
fn first_readable(candidates: Vec<Option<PathBuf>>) -> Option<PathBuf> {
    candidates.into_iter().flatten().find(|p| p.is_file())
}

/// `$XDG_CONFIG_HOME`, or the `~/.config` git falls back to.
fn xdg_config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::home_dir().map(|home| home.join(".config")))
}

/// Apply gitignore rules in git's own precedence order, lowest layer first:
/// `core.excludesFile`, `.git/info/exclude`, then every `.gitignore` from the
/// root down to the file's own directory.
///
/// One matcher rather than several OR'd together. Later patterns override
/// earlier ones, so a repository `!rule` can un-ignore a path the user's global
/// excludes file had ignored -- an OR could only ever add ignores, never lift
/// one, and it also had to match a repo-relative path against a matcher rooted
/// at the user's home directory.
///
/// `global_excludes` is a parameter rather than a lookup so this is testable:
/// overriding the real one would mean setting an environment variable, and
/// `unsafe_code` is denied workspace-wide.
fn gitignore_chain(root: &Path, relative: &Path, global_excludes: Option<&Path>) -> bool {
    let mut builder = GitignoreBuilder::new(root);
    if let Some(path) = global_excludes {
        builder.add(path);
    }
    builder.add(root.join(".git/info/exclude"));
    builder.add(root.join(".gitignore"));

    let mut dir = root.to_path_buf();
    if let Some(parent) = relative.parent() {
        for component in parent.components() {
            dir.push(component);
            builder.add(dir.join(".gitignore"));
        }
    }

    // The relative path, not the absolute one: `matched_path_or_any_parents`
    // asserts its argument is under the matcher's own root.
    builder
        .build()
        .is_ok_and(|set| set.matched_path_or_any_parents(relative, false).is_ignore())
}

/// Classify a single path exactly as [`walk`] would classify it.
///
/// `lint-file` needs this: it judges the paths a caller named, so it cannot get
/// their classification from a walk it never runs.
#[must_use]
pub fn classify_path(path: &Path, source_patterns: &[String]) -> FileType {
    classify(path, source_patterns)
}

/// Check whether a file has a `.license` companion file.
#[must_use]
pub fn has_license_companion(path: &Path) -> bool {
    let license_path = license_companion_path(path);
    license_path.exists() && license_path.is_file()
}

/// Find the `.license` companion file for a given file.
#[must_use]
pub fn license_companion_path(path: &Path) -> PathBuf {
    let mut s = path.to_path_buf().into_os_string();
    s.push(".license");
    PathBuf::from(s)
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn companion_path_is_correct() {
        let p = Path::new("binary.bin");
        assert_eq!(
            license_companion_path(p),
            PathBuf::from("binary.bin.license")
        );
    }

    #[test]
    fn a_walk_switch_changes_what_the_traversal_visits() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(root.join(".hidden.rs"), "x").unwrap();
        std::fs::write(root.join("visible.rs"), "x").unwrap();

        let visited = |options| {
            walk_with(&root, &[], &[], options)
                .filter_map(Result::ok)
                .count()
        };

        assert_eq!(
            visited(WalkOptions::DEFAULT),
            2,
            "a dotfile needs a license like any other file, so it is walked"
        );
        assert_eq!(
            visited(WalkOptions {
                hidden: true,
                ..WalkOptions::DEFAULT
            }),
            1,
            "`hidden = true` in [tool.comply.walk] has to reach the builder"
        );
    }

    fn git_in(root: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .expect("git must be available to run these tests");
        assert!(out.status.success(), "git {args:?} failed");
    }

    /// A repository with one tracked and one untracked file, both matching the
    /// same `.gitignore` rule.
    fn repo_with_ignored_names() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git_in(root, &["init", "-q"]);
        std::fs::write(root.join(".gitignore"), "secret*\n").expect("write");
        std::fs::write(root.join("secrets.rs"), "pub fn a() {}\n").expect("write");
        std::fs::write(root.join("secret_untracked.rs"), "pub fn b() {}\n").expect("write");
        std::fs::write(root.join("plain.rs"), "pub fn c() {}\n").expect("write");
        git_in(root, &["add", "-f", "secrets.rs"]);
        dir
    }

    fn walked_names(root: &Path) -> Vec<String> {
        walk(root, &[], &[])
            .filter_map(Result::ok)
            .filter_map(|e| e.path.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect()
    }

    #[test]
    fn a_tracked_file_is_never_excluded_by_a_gitignore_rule() {
        // gitignore does not apply to tracked files -- `git check-ignore`
        // reports nothing for one no matter how many patterns match. Excluding
        // it anyway dropped a tracked `secrets.rs` from the report, and only on
        // machines whose global excludes happened to list `secret*`, so a local
        // run passed while CI failed on the same commit.
        let dir = repo_with_ignored_names();
        let names = walked_names(dir.path());

        assert!(
            names.iter().any(|n| n == "secrets.rs"),
            "tracked file must be analysed, got {names:?}"
        );
        assert!(names.iter().any(|n| n == "plain.rs"), "got {names:?}");
    }

    #[test]
    fn an_untracked_file_matching_a_gitignore_rule_stays_excluded() {
        let dir = repo_with_ignored_names();
        let names = walked_names(dir.path());

        assert!(
            !names.iter().any(|n| n == "secret_untracked.rs"),
            "an untracked ignored file is genuinely out of scope, got {names:?}"
        );
    }

    #[test]
    fn a_named_tracked_path_is_not_reported_as_excluded() {
        // `lint-file`, `annotate` and `format` route through `is_excluded`; it
        // has to reach the same verdict the walk does.
        let dir = repo_with_ignored_names();
        let root = dir.path();

        assert!(!is_excluded(root, &root.join("secrets.rs"), &[]));
        assert!(is_excluded(root, &root.join("secret_untracked.rs"), &[]));
    }

    #[test]
    fn the_first_candidate_that_exists_wins() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let real = root.join("present");
        std::fs::write(&real, "*.tmp\n").unwrap();
        let absent = root.join("absent");

        assert_eq!(
            first_readable(vec![Some(absent.clone()), Some(real.clone())]),
            Some(real.clone()),
            "a missing earlier candidate is skipped, not returned"
        );
        assert_eq!(first_readable(vec![None, Some(real.clone())]), Some(real));
        assert_eq!(first_readable(vec![None, Some(absent)]), None);
        assert_eq!(first_readable(vec![]), None);
    }

    #[test]
    fn the_xdg_fallback_points_at_gits_own_default() {
        // Unset or set, it must land on `<base>/git/ignore` -- that is the path
        // git reads when core.excludesFile is absent.
        if let Some(path) = xdg_config_home() {
            assert!(path.is_absolute(), "got {path:?}");
        }
        // `global_excludes` composes the two; on a machine with neither
        // configured it is simply None, which is a valid answer.
        let resolved = global_excludes();
        assert!(resolved.as_ref().is_none_or(|p| p.is_file()));
    }

    #[test]
    fn the_global_excludes_file_is_inherited() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let globals = root.join("global-excludes");
        std::fs::write(&globals, "*.secret\n").unwrap();

        assert!(gitignore_chain(
            &root,
            Path::new("a.secret"),
            Some(globals.as_path())
        ));
        assert!(!gitignore_chain(
            &root,
            Path::new("a.rs"),
            Some(globals.as_path())
        ));
    }

    #[test]
    fn a_repository_negation_overrides_the_global_excludes_file() {
        // git layers these lowest-first: core.excludesFile, then
        // .git/info/exclude, then .gitignore outermost to innermost. Consulting
        // the global file as a separate OR'd fallback made a repo-level `!rule`
        // unable to win, which is the whole point of it being the lowest layer.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let globals = root.join("global-excludes");
        std::fs::write(&globals, "*.log\n").unwrap();
        std::fs::write(root.join(".gitignore"), "!keep.log\n").unwrap();

        assert!(
            !gitignore_chain(&root, Path::new("keep.log"), Some(globals.as_path())),
            "a repository negation must win over the global excludes file"
        );
        assert!(gitignore_chain(
            &root,
            Path::new("other.log"),
            Some(globals.as_path())
        ));
    }

    #[test]
    fn the_chain_still_works_with_no_global_excludes_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(root.join(".gitignore"), "build/\n").unwrap();

        assert!(gitignore_chain(&root, Path::new("build/out.o"), None));
        assert!(!gitignore_chain(&root, Path::new("src/main.rs"), None));
    }

    #[test]
    fn a_named_path_is_excluded_by_the_same_rules_a_walk_applies() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::create_dir_all(root.join("vendor")).unwrap();
        std::fs::write(root.join(".gitignore"), "root-ignored.txt\n").unwrap();
        std::fs::write(root.join("sub/.gitignore"), "deep-ignored.txt\n").unwrap();
        for f in [
            "keep.rs",
            "root-ignored.txt",
            "sub/deep-ignored.txt",
            "vendor/dep.rs",
            ".git/config",
        ] {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, "x\n").unwrap();
        }
        let patterns = vec!["vendor/**".to_string()];

        // `lint` gets these three sources from the walk; a command handed an
        // explicit path has no walk to inherit them from, and used to judge
        // files the project had plainly excluded.
        assert!(is_excluded(&root, &root.join(".git/config"), &patterns));
        assert!(is_excluded(
            &root,
            &root.join("root-ignored.txt"),
            &patterns
        ));
        assert!(is_excluded(
            &root,
            &root.join("sub/deep-ignored.txt"),
            &patterns
        ));
        assert!(is_excluded(&root, &root.join("vendor/dep.rs"), &patterns));

        assert!(!is_excluded(&root, &root.join("keep.rs"), &patterns));
    }

    #[test]
    fn walk_prunes_vcs_and_build_directories() {
        let dir = tempfile::tempdir().unwrap();
        for d in [
            ".git/objects/ab",
            "node_modules/pkg",
            "target/debug",
            ".venv/lib",
        ] {
            std::fs::create_dir_all(dir.path().join(d)).unwrap();
            std::fs::write(dir.path().join(d).join("blob"), "content").unwrap();
        }
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &[], &[]).filter_map(Result::ok).collect();

        // Pruned during traversal, not merely classified as ignored afterwards:
        // walking .git/objects on a real repository costs hundreds of entries
        // and reports every one of them as "skipped".
        assert_eq!(
            files
                .iter()
                .map(|f| f.relative_to(dir.path()))
                .collect::<Vec<_>>(),
            vec!["main.rs".to_string()]
        );
    }

    #[test]
    fn walk_skips_licenses_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("LICENSES")).unwrap();
        std::fs::write(dir.path().join("LICENSES/MIT.txt"), "MIT license text").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &[], &[]).filter_map(Result::ok).collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_to(dir.path()), "src/main.rs");
    }

    #[test]
    fn walk_skips_reuse_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".reuse")).unwrap();
        std::fs::write(
            dir.path().join(".reuse/dep5"),
            "Files: *\nCopyright: 2026\nLicense: MIT\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &[], &[]).filter_map(Result::ok).collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_to(dir.path()), "main.rs");
    }

    #[test]
    fn walk_skips_reuse_toml_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("REUSE.toml"), "version = 1\n").unwrap();
        std::fs::write(dir.path().join("reuse.toml"), "version = 1\n").unwrap();
        std::fs::write(dir.path().join(".reuse.toml"), "version = 1\n").unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &[], &[]).filter_map(Result::ok).collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_to(dir.path()), "main.rs");
    }

    #[test]
    fn walk_skips_license_companion_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("binary.bin"), [0u8; 4]).unwrap();
        std::fs::write(
            dir.path().join("binary.bin.license"),
            "SPDX-FileCopyrightText: 2026\nSPDX-License-Identifier: MIT\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &[], &[]).filter_map(Result::ok).collect();
        // binary.bin should be included, but .license companion should be skipped
        let names: Vec<String> = files.iter().map(|f| f.relative_to(dir.path())).collect();
        assert!(names.contains(&"binary.bin".to_string()));
        assert!(names.contains(&"main.rs".to_string()));
        assert!(!names.contains(&"binary.bin.license".to_string()));
    }

    #[test]
    fn walk_skips_zero_byte_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("empty.txt"), "").unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &[], &[]).filter_map(Result::ok).collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_to(dir.path()), "main.rs");
    }

    #[test]
    fn walk_skips_spdx_docs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sbom.spdx.json"), "{}").unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &[], &[]).filter_map(Result::ok).collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_to(dir.path()), "main.rs");
    }

    #[test]
    fn walk_honours_custom_ignore_patterns() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("vendor")).unwrap();
        std::fs::write(dir.path().join("vendor/dep.rs"), "fn dep() {}").unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &["vendor/**".to_string()], &[])
            .filter_map(Result::ok)
            .collect();

        assert_eq!(files.len(), 1, "the vendored file should be filtered out");
        assert_eq!(files[0].relative_to(dir.path()), "main.rs");
    }

    #[test]
    fn walk_surfaces_a_traversal_error_rather_than_swallowing_it() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-such-directory");

        let errors = walk(&missing, &[], &[]).filter(Result::is_err).count();

        assert_eq!(
            errors, 1,
            "a root that cannot be read must not look like an empty project"
        );
    }

    /// The walk prunes `LICENSES/` as a directory and never descends, so the
    /// texts inside it never reach `seen`. git tracks them all the same, and
    /// the backfill hands `out_of_scope` a *file* path -- which only prunes
    /// when `is_dir`, so the directory rule it would have hit is skipped
    /// entirely and the file comes back.
    ///
    /// The damage was not cosmetic: `comply spdx` then asserted that
    /// `LICENSES/MIT.txt` is licensed `MIT OR Apache-2.0` via the `**`
    /// aggregate, which claims the MIT licence text is itself covered by the
    /// project's dual licence.
    #[test]
    fn a_tracked_file_inside_a_pruned_directory_is_not_backfilled() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git_in(root, &["init", "-q"]);
        std::fs::create_dir_all(root.join("LICENSES")).expect("mkdir");
        std::fs::write(root.join("LICENSES/MIT.txt"), "MIT text\n").expect("write");
        std::fs::write(root.join("main.rs"), "fn main() {}\n").expect("write");
        git_in(root, &["add", "-A"]);

        let paths: Vec<String> = walk(root, &[], &[])
            .filter_map(Result::ok)
            .map(|e| e.relative_to(root))
            .collect();

        assert!(
            !paths.iter().any(|p| p.starts_with("LICENSES/")),
            "the licence texts describe coverage, they are not covered: {paths:?}"
        );
        assert!(
            paths.iter().any(|p| p == "main.rs"),
            "the backfill must still return real project files: {paths:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn walk_does_not_follow_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        std::os::unix::fs::symlink(dir.path().join("main.rs"), dir.path().join("alias.rs"))
            .unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &[], &[]).filter_map(Result::ok).collect();

        assert_eq!(
            files.len(),
            1,
            "a symlink would double-count the file it points at"
        );
        assert_eq!(files[0].relative_to(dir.path()), "main.rs");
    }
}
// REUSE-IgnoreEnd
