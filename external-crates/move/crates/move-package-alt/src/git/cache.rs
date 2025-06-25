// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    io::BufRead,
    path::{Path, PathBuf},
    process::{Output, Stdio},
};

use tokio::process::Command;
use tracing::{debug, info};

use super::{
    errors::{GitError, GitResult},
    sha::GitSha,
};

use once_cell::sync::OnceCell;

static CONFIG: OnceCell<String> = OnceCell::new();

// TODO: this should be moved into [crate::dependency::git]
fn get_cache_path() -> &'static str {
    CONFIG.get_or_init(|| {
        #[cfg(test)]
        {
            let tempdir = tempfile::tempdir().expect("failed to create temp dir");
            tempdir.path().to_string_lossy().to_string()
        }

        #[cfg(not(test))]
        {
            move_command_line_common::env::MOVE_HOME.to_string()
        }
    })
}

/// A cache that manages a collection of downloaded git trees
#[derive(Debug)]
pub struct GitCache {
    root_dir: PathBuf,
}

/// A subdirectory within a particular commit of a git repository. The files may or may not have
/// been downloaded, but you can ensure that they have by calling `fetch()`
#[derive(Clone, Debug)]
pub struct GitTree {
    /// Repository URL
    repo: String,

    /// Commit-ish (branch, tag, or SHA)
    sha: GitSha,

    /// relative path inside the repository to use for sparse checkout
    path_in_repo: PathBuf,

    /// Absolute path to the root of the repository
    path_to_repo: PathBuf,
}

impl GitCache {
    pub fn new() -> Self {
        Self {
            root_dir: get_cache_path().into(),
        }
    }
    /// Create or load the cache at `root_dir`
    pub fn new_from_dir(root_dir: impl AsRef<Path>) -> Self {
        Self {
            root_dir: root_dir.as_ref().to_path_buf(),
        }
    }

    /// Resolve the git committish `rev` (branch, tag, or sha) from a repository at the remote
    /// `repo` to a commit hash. This will make a remote call so network is required.
    pub async fn find_sha(repo: &str, rev: &Option<String>) -> GitResult<GitSha> {
        find_sha(repo, rev).await
    }

    /// Helper function to find the sha and then construct a [GitTree]
    pub async fn resolve_to_tree(
        &self,
        repo: &str,
        rev: &Option<String>,
        path_in_repo: Option<PathBuf>,
    ) -> GitResult<GitTree> {
        let sha = Self::find_sha(repo, rev).await?;
        Ok(self.tree_for_sha(repo.to_string(), sha.clone(), path_in_repo.clone()))
    }

    /// Construct a tree in `self` for the repository `repo` with the provided `sha` and
    /// `path_in_repo`.
    pub fn tree_for_sha(
        &self,
        repo: String,
        sha: GitSha,
        path_in_repo: Option<PathBuf>,
    ) -> GitTree {
        let filename = url_to_file_name(repo.as_str());
        let path_to_repo = self.root_dir.join(format!("{filename}_{sha}"));
        let path_in_repo = path_in_repo.unwrap_or_default();

        GitTree {
            repo,
            sha,
            path_in_repo,
            path_to_repo,
        }
    }
}

impl GitTree {
    /// The absolute path on the filesystem where this tree will be downloaded when `fetch` is
    /// called
    pub fn path_to_tree(&self) -> PathBuf {
        self.path_to_repo.join(&self.path_in_repo)
    }

    /// Ensure that the files are downloaded to `self.path_to_tree()`. Fails if there was already a
    /// dirty checkout there (call [Self::fetch_allow_dirty] if you don't want to
    /// fail). Returns `self.path_to_tree()`.
    pub async fn fetch(&self) -> GitResult<PathBuf> {
        self.checkout_repo(false).await
    }

    /// Ensure that there are files downloaded to `self.path_to_tree()`. Has no effect if
    /// `self.path_to_tree()` already exists. Returns `self.path_to_tree()`
    pub async fn fetch_allow_dirty(&self) -> GitResult<PathBuf> {
        self.checkout_repo(true).await
    }

    /// The url of the repository for this commit
    pub fn repo_url(&self) -> &str {
        &self.repo
    }

    /// The relative path to the subtree within the repository
    pub fn path_in_repo(&self) -> &Path {
        &self.path_in_repo
    }

    /// The git sha for the tree
    pub fn sha(&self) -> &GitSha {
        &self.sha
    }

    /// Checkout the directory using a sparse checkout. It will try to clone without checkout, set
    /// sparse checkout directory, and then checkout the folder specified by `self.path_in_repo` at the
    /// given sha.
    ///
    /// Fails if `allow_dirty` is false and a dirty checkout of the directory already exists
    async fn checkout_repo(&self, allow_dirty: bool) -> GitResult<PathBuf> {
        let tree_path = self.path_to_tree();
        let mut fresh = false;

        // create repo if necessary
        if !self.path_to_repo.exists() {
            // git clone --sparse --filter=blob:none --no-checkout <url> <path>
            run_git_cmd_with_args(
                &[
                    "-c",
                    "advice.detachedHead=false",
                    "clone",
                    "--quiet",
                    "--sparse",
                    "--filter=blob:none",
                    "--no-checkout",
                    "--depth",
                    "1",
                    &self.repo,
                    &self.path_to_repo.to_string_lossy(),
                ],
                None,
            )
            .await?;

            fresh = true;
        }

        // Checkout directory if it does not exist already or if it exists but it has not been
        // checked out yet
        if !tree_path.exists() || fresh {
            // git sparse-checkout add <path>
            let path_in_repo = self.path_in_repo().to_string_lossy();

            self.run_git(&["sparse-checkout", "add", &path_in_repo])
                .await?;

            // git checkout
            self.run_git(&["checkout", "--quiet", self.sha.as_ref()])
                .await?;
            let cmd = Command::new("ls")
                .arg(&self.path_to_repo)
                .output()
                .await
                .unwrap();
        }

        // check for dirt
        if !allow_dirty && self.is_dirty().await {
            Err(GitError::dirty(
                self.path_to_tree().to_string_lossy().as_ref(),
            ))
        } else {
            Ok(tree_path)
        }
    }

    /// Run `git <args>` in working directory `self.path_to_repo`
    async fn run_git(&self, args: &[&str]) -> GitResult<String> {
        run_git_cmd_with_args(args, Some(&self.path_to_repo)).await
    }

    /// Return true if the directory exists and is dirty
    async fn is_dirty(&self) -> bool {
        if !self.path_to_repo.join(".git").exists() {
            // git directory has been removed - it's dirty!
            return true;
        }

        // for passing the path to `git status`, path in repo should be `.` if it's empty
        // here's the error msg from git
        // fatal: empty string is not a valid pathspec. please use . instead if you meant to
        // match all paths
        let path_in_repo = if self.path_in_repo.as_os_str().is_empty() {
            "."
        } else {
            &self.path_in_repo.to_string_lossy()
        };

        let Ok(output) = self
            .run_git(&[
                "status",
                "--porcelain",
                "--untracked-files=no",
                path_in_repo,
            ])
            .await
        else {
            // if there's an error, the git repo has probably been tampered with - it's dirty
            return true;
        };

        if !output.is_empty() {
            debug!("Tree {self:?} is dirty");
            return true;
        }

        false
    }

    /// The path to the folder containing the cached repo (without the addition of the path within
    /// the repo)
    #[cfg(test)]
    pub fn repo_fs_path(&self) -> &Path {
        &self.path_to_repo
    }
}

/// Transform a repository URL into a directory name
fn url_to_file_name(url: &str) -> String {
    regex::Regex::new(r"/|:|\.|@")
        .unwrap()
        .replace_all(url, "_")
        .to_string()
}

/// Resolve the git committish `rev` (branch, tag, or sha) from a repository at the remote
/// `repo` to a commit SHA. This will make a remote call so network is required.
async fn find_sha(repo: &str, rev: &Option<String>) -> GitResult<GitSha> {
    if let Some(r) = rev {
        if let Ok(sha) = GitSha::try_from(r.to_string()) {
            return Ok(sha);
        }

        // if there is some revision which is likely a branch, a tag, or a wrong SHA (e.g., capital
        // letter), then we have a different set of arguments than if there is no revision. In no
        // revision case, we need to find the default branch of that remote.

        // we have a branch or tag
        // git ls-remote https://github.com/user/repo.git refs/heads/main
        let stdout =
            run_git_cmd_with_args(&["ls-remote", repo, &format!("refs/heads/{r}")], None).await?;

        let sha = stdout
            .split_whitespace()
            .next()
            .ok_or(GitError::no_sha(repo, r))?
            .to_string()
            .try_into()
            .expect("git returns valid shas");

        Ok(sha)
    } else {
        // nothing specified, so we need to find the default branch
        find_default_branch_and_get_sha(repo).await
    }
}

/// Find the default branch and return the SHA
async fn find_default_branch_and_get_sha(repo_url: &str) -> GitResult<GitSha> {
    let stdout = run_git_cmd_with_args(&["ls-remote", "--symref", repo_url, "HEAD"], None).await?;

    let lines: Vec<_> = stdout.lines().collect();

    // TODO: default_branch is ignored here; are we guaranteed that the sha is always the
    // second line?
    let _default_branch = lines[0]
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| GitError::no_sha(repo_url, "HEAD"))?;

    let sha_str = lines[1]
        .split_whitespace()
        .next()
        .ok_or_else(|| GitError::no_sha(repo_url, "HEAD"))?;

    let sha =
        GitSha::try_from(sha_str.to_string()).expect("Git should return correctly formatted shas");

    Ok(sha)
}

/// Runs `git <args>` in `cwd`. Fails if there is an io failure or if `git` returns a non-zero
/// exit status; returns the standard output and logs standard error to `info!`
pub async fn run_git_cmd_with_args(args: &[&str], cwd: Option<&PathBuf>) -> GitResult<String> {
    // Run the git command

    let mut cmd = Command::new("git");
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }

    debug!("running `{}`", display_cmd(&cmd));
    debug!("  in directory `{:?}`", cmd.as_std().get_current_dir());

    let output = cmd
        .output()
        .await
        .map_err(|e| GitError::io_error(&cmd, &cwd, e))?;

    if !output.stderr.is_empty() {
        info!("output from `{}`", display_cmd(&cmd));
        for line in output.stderr.lines() {
            info!("  │ {}", line.expect("vector read can't fail"));
        }
    }

    if !output.stdout.is_empty() {
        debug!("stdout from `{}`", display_cmd(&cmd));
        for line in output.stdout.lines() {
            debug!("  │ {}", line.expect("vector read can't fail"));
        }
    }

    if !output.status.success() {
        return Err(GitError::nonzero_exit_status(&cmd, &cwd, output.status));
    }

    String::from_utf8(output.stdout).map_err(|e| GitError::non_utf_output(&cmd, &cwd))
}

/// Output the `cmd` and its args in a concise form (without quoting or showing the working directory)
fn display_cmd(cmd: &Command) -> String {
    let mut result: String = cmd.as_std().get_program().to_string_lossy().into();
    for arg in cmd.as_std().get_args() {
        result.push(' ');
        result.push_str(arg.to_string_lossy().as_ref());
    }
    result
}

// TODO: add more tests
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::env;
    use std::fs;
    use std::path::Path;
    use tempfile::{TempDir, tempdir};
    use test_log::test;
    use walkdir::DirEntry;
    use walkdir::WalkDir;

    fn setup_temp_dir() -> TempDir {
        tempdir().unwrap()
    }

    pub async fn run_git_cmd(args: &[&str], repo_path: &PathBuf) -> Output {
        Command::new("git")
            .args(args)
            .current_dir(repo_path)
            .output()
            .await
            .unwrap()
    }

    /// Sets up a test Move project with git repository
    /// It returns the temporary directory, the path to the directory containing the repository,
    /// the first commit sha, and and the second commit sha.
    ///
    /// The first commit contains the following files:
    ///  - packages/pkg_a/Move.toml (copied from tests/data/basic_move_project/config
    ///
    /// The second commit contains the following files:
    ///  - packages/pkg_a/Move.toml (copied from tests/data/basic_move_project/pkg_a/Move.toml
    ///  - packages/pkg_b/Move.toml (copied from tests/data/basic_move_project/pkg_b/Move.toml
    pub async fn setup_test_move_project() -> (TempDir, String, String, String) {
        // Create a temporary directory
        let temp_dir = tempdir().unwrap();
        let root_path = temp_dir.path().to_path_buf();

        // Create the root directory for the Move project
        fs::create_dir_all(&root_path).unwrap();

        // Initialize git repository with main as default branch
        run_git_cmd(&["init", "--initial-branch=main"], &root_path).await;

        fs::copy(
            "tests/data/basic_move_project/config",
            root_path.join(".git").join("config"),
        )
        .unwrap();

        // Create directory structure
        let pkg_a_path = root_path.join("packages").join("pkg_a");
        let pkg_b_path = root_path.join("packages").join("pkg_b");
        fs::create_dir_all(&pkg_a_path).unwrap();
        fs::copy(
            "tests/data/basic_move_project/pkg_a/Move.toml",
            pkg_a_path.join("Move.toml"),
        )
        .unwrap();

        // Initial commit
        run_git_cmd(&["add", "."], &root_path).await;
        run_git_cmd(&["commit", "-m", "Initial commit"], &root_path).await;

        fs::create_dir_all(&pkg_b_path).unwrap();
        fs::copy(
            "tests/data/basic_move_project/pkg_b/Move.toml",
            pkg_b_path.join("Move.toml"),
        )
        .unwrap();

        // Commit updates
        run_git_cmd(&["add", "."], &root_path).await;
        run_git_cmd(&["commit", "-m", "Second commit"], &root_path).await;

        // Get commits SHA
        let output = run_git_cmd(&["log"], &root_path).await;
        debug!("{output:?}");
        let commits = run_git_cmd(&["log", "--pretty=format:%H"], &root_path).await;
        let commits = String::from_utf8_lossy(&commits.stdout);
        let commits: Vec<_> = commits.lines().collect();

        let branch = run_git_cmd(&["rev-parse", "--abbrev-ref", "HEAD"], &root_path).await;

        (
            temp_dir,
            format!("file://{}", root_path.to_string_lossy()),
            commits[1].to_string(),
            commits[0].to_string(),
        )
    }

    /// Asserts that `root/path` exists for each path in `paths`, and that no other files exist
    /// inside `root`. Ignores empty directories and files that start with `.` (in particular, `.git`)
    fn assert_exactly_paths(root: &Path, paths: impl IntoIterator<Item = impl AsRef<Path>>) {
        fn is_hidden(entry: &DirEntry) -> bool {
            entry
                .file_name()
                .to_str()
                .map(|s| s.starts_with("."))
                .unwrap_or(false)
        }

        let mut files: BTreeSet<PathBuf> = WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| !is_hidden(e))
            .filter_map(|e| {
                let path = e.unwrap().into_path();
                debug!("  found path: {path:?}");
                if path.is_file() { Some(path) } else { None }
            })
            .collect();

        for path in paths {
            let fullpath = root.join(&path);
            debug!("removing {fullpath:?}");
            assert!(
                files.remove(&fullpath),
                "missing file {:?}",
                path.as_ref().to_string_lossy()
            );
        }

        assert!(files.is_empty(), "extra files: {files:?}");
    }

    /// Ensure that loading a package into an empty cache outputs only the correct files
    #[test(tokio::test)]
    async fn test_sparse_checkout_branch() {
        let (repo_dir, repo_path, _, _) = setup_test_move_project().await;
        let cache_dir = tempdir().unwrap();
        let cache = GitCache::new_from_dir(cache_dir.path());

        // Pass in a branch name
        let git_tree = cache
            .resolve_to_tree(
                &repo_path,
                &Some("main".into()),
                Some(PathBuf::from("packages/pkg_a")),
            )
            .await
            .unwrap();

        // Fetch the dependency
        let checkout_path = git_tree.fetch().await.unwrap();

        // Verify only packages/pkg_a was checked out
        assert_exactly_paths(git_tree.repo_fs_path(), ["packages/pkg_a/Move.toml"]);
        assert_exactly_paths(&checkout_path, ["Move.toml"]);
    }

    /// Ensure that loading a package into an empty cache from a SHA outputs only the correct files
    #[test(tokio::test)]
    async fn test_sparse_checkout_sha() {
        let (repo_dir, repo_path, _, second_sha) = setup_test_move_project().await;
        let cache_dir = tempdir().unwrap();
        let cache = GitCache::new_from_dir(cache_dir.path());

        // Pass in a commit SHA
        let git_tree = cache
            .resolve_to_tree(
                &repo_path,
                &Some(second_sha),
                Some(PathBuf::from("packages/pkg_a")),
            )
            .await
            .unwrap();

        // Fetch the dependency
        let checkout_path = git_tree.fetch().await.unwrap();

        // Verify only packages/pkg_b was checked out
        assert_exactly_paths(git_tree.repo_fs_path(), ["packages/pkg_a/Move.toml"]);
    }

    /// Ensure that checking out two different paths from the same repo / sha works
    #[test(tokio::test)]
    async fn test_multi_checkout() {
        let (repo_dir, repo_path, _, second_sha) = setup_test_move_project().await;
        let cache_dir = tempdir().unwrap();
        let cache = GitCache::new_from_dir(cache_dir.path());

        let git_tree_a = cache
            .resolve_to_tree(&repo_path, &None, Some(PathBuf::from("packages/pkg_a")))
            .await
            .unwrap();

        let git_tree_b = cache
            .resolve_to_tree(&repo_path, &None, Some(PathBuf::from("packages/pkg_b")))
            .await
            .unwrap();

        // Fetch the dependencies
        git_tree_a.fetch().await.unwrap();
        git_tree_b.fetch().await.unwrap();

        assert_eq!(git_tree_a.repo_fs_path(), git_tree_b.repo_fs_path());
        assert_exactly_paths(
            git_tree_a.repo_fs_path(),
            ["packages/pkg_a/Move.toml", "packages/pkg_b/Move.toml"],
        );
    }

    /// Creating a git tree should fail if the sha doesn't exist
    #[test(tokio::test)]
    async fn test_wrong_sha() {
        let (repo_dir, repo_path, _, _) = setup_test_move_project().await;
        let cache_dir = tempdir().unwrap();
        let cache = GitCache::new_from_dir(cache_dir.path());

        // valid sha, but incorrect for repo:
        let wrong_sha = "0".repeat(40);

        // note: in current implementation, find_sha succeeds with an invalid sha; it doesn't
        // contact the server - we only fail when we try to fetch (which seems reasonable)
        let git_tree = cache
            .resolve_to_tree(
                &repo_path,
                &Some(wrong_sha),
                Some(PathBuf::from("packages/pkg_a")),
            )
            .await
            .unwrap();

        let result = git_tree.fetch().await;

        assert!(result.is_err());
    }

    /// Creating a git tree should fail if the branch doesn't exist
    #[test(tokio::test)]
    async fn test_wrong_branch_name() {
        let (repo_dir, repo_path, _, _) = setup_test_move_project().await;
        let cache_dir = tempdir().unwrap();
        let cache = GitCache::new_from_dir(cache_dir.path());

        let wrong_branch = "test";
        let git_tree = cache
            .resolve_to_tree(
                &repo_path,
                &Some("nonexisting_branch".to_string()),
                Some(PathBuf::from("packages/pkg_a")),
            )
            .await;

        assert!(git_tree.is_err());
    }

    /// Fetching should succeeed if the path is `None`
    #[test(tokio::test)]
    async fn test_fetch_no_path() {
        let (repo_dir, repo_path, _, _) = setup_test_move_project().await;
        let cache_dir = tempdir().unwrap();
        let cache = GitCache::new_from_dir(cache_dir.path());

        let git_tree = cache
            .resolve_to_tree(&repo_path, &None, None)
            .await
            .unwrap();

        git_tree.fetch().await.unwrap();
    }

    /// Fetching should fail if a dirty checkout exists
    #[test(tokio::test)]
    async fn test_fetch_dirty_fail() {
        let (repo_dir, repo_path, _, _) = setup_test_move_project().await;
        let cache_dir = tempdir().unwrap();
        let cache = GitCache::new_from_dir(cache_dir.path());

        let git_tree = cache
            .resolve_to_tree(&repo_path, &None, Some(PathBuf::from("packages/pkg_a")))
            .await
            .unwrap();

        fs::create_dir_all(git_tree.path_to_tree());
        fs::write(
            git_tree.path_to_tree().join("garbage.txt"),
            "something to dirty the repo",
        );

        let result = git_tree.fetch().await;
        assert!(result.is_err());
    }

    /// `fetch_allow_dirty` should succeed with a dirty checkout
    #[test(tokio::test)]
    async fn test_fetch_allow_dirty() {
        let (repo_dir, repo_url, _, _) = setup_test_move_project().await;
        let cache_dir = tempdir().unwrap();
        let cache = GitCache::new_from_dir(cache_dir.path());

        let git_tree = cache
            .resolve_to_tree(&repo_url, &None, Some(PathBuf::from("packages/pkg_a")))
            .await
            .unwrap();

        fs::create_dir_all(git_tree.path_to_tree());
        fs::write(
            git_tree.path_to_tree().join("garbage.txt"),
            "something to dirty the repo",
        );

        let result = git_tree.fetch_allow_dirty().await.unwrap();
    }

    /// Fetching should succeed if a clean checkout exists
    #[test(tokio::test)]
    async fn test_fetch_clean_exists() {
        let (repo_dir, repo_path, _, _) = setup_test_move_project().await;
        let cache_dir = tempdir().unwrap();
        let cache = GitCache::new_from_dir(cache_dir.path());

        let git_tree = cache
            .resolve_to_tree(&repo_path, &None, Some(PathBuf::from("packages/pkg_a")))
            .await
            .unwrap();

        git_tree.fetch().await.unwrap();

        // same as above
        let git_tree = cache
            .resolve_to_tree(&repo_path, &None, Some(PathBuf::from("packages/pkg_a")))
            .await
            .unwrap();

        git_tree.fetch().await.unwrap();
    }

    /// Fetching should succeed if the path is clean but other paths are not
    #[test(tokio::test)]
    async fn test_fetch_clean_parallel_dirty() {
        let (repo_dir, repo_url, _, _) = setup_test_move_project().await;
        let cache_dir = tempdir().unwrap();
        let cache = GitCache::new_from_dir(cache_dir.path());

        let git_tree = cache
            .resolve_to_tree(&repo_url, &None, Some(PathBuf::from("packages/pkg_a")))
            .await
            .unwrap();

        // fetch
        git_tree.fetch().await.unwrap();

        // create dirty file in dep's parent directory
        fs::create_dir_all(git_tree.path_to_tree().parent().unwrap());
        fs::write(
            git_tree.path_to_tree().join("garbage.txt"),
            "something to dirty the repo",
        );

        // fetch again - subtree should still be clean so it should succeed
        let result = git_tree.fetch().await.unwrap();
    }
}
