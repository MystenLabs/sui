// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Types and methods for external dependencies (of the form `{ git = "<repo>" }`)
//!
//! Git dependencies are cached in `~/.move`, which has the following structure:
//!
//! TODO: this doesn't match the implementation below:
//! ```ignore
//! .move/
//!   git/
//!     <remote 1>/ # a headless, sparse, and shallow git repository
//!       <sha 1>/ # a worktree checked out to the given sha
//!       <sha 2>/
//!       ...
//!     <remote 2>/
//!       ...
//!     ...
//! ```
use std::{
    fmt,
    marker::PhantomData,
    path::{Path, PathBuf},
    process::{ExitStatus, Output, Stdio},
};

use derive_where::derive_where;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, de};
use tokio::process::Command;
use tracing::debug;

use crate::errors::{GitError, GitErrorKind, Located, PackageError, PackageResult};

use super::{DependencySet, Pinned, Unpinned};

// TODO: (potential refactor): it might be good to separate out a separate module that is just git
//       stuff and another that uses that git stuff to implement the dependency operations (like
//       the jsonrpc / dependency::external split).

// TODO: curious about the benefit of using String instead of wrapping it. The advantage of
//       wrapping it is that we have invariants (with the type alias, nothing prevents us from
//       writing `let x : Sha = ""` (whereas `let x = Sha::new("")` can fail)
type Sha = String;

/// TODO keep same style around all types
///
/// A git dependency that is unpinned. The `rev` field can be either empty, a branch, or a sha. To
/// resolve this into a [`PinnedGitDependency`], call `pin_one` function.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UnpinnedGitDependency {
    /// The repository containing the dependency
    #[serde(rename = "git")]
    repo: String,

    /// The git commit or branch for the dependency.
    #[serde(default)]
    rev: Option<String>,

    /// The path within the repository
    #[serde(default)]
    path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PinnedGitDependency {
    /// The repository containing the dependency
    #[serde(rename = "git")]
    repo: String,

    /// The exact sha for the revision
    #[serde(deserialize_with = "deserialize_sha")]
    rev: Sha,

    /// The path within the repository
    #[serde(default)]
    path: PathBuf,
}

/// Helper struct that represents a Git repository, with extra information about which folder to
/// checkout.
// TODO: how is this different from [UnpinnedGitDependency]?
#[derive(Clone, Debug)]
pub struct GitRepo {
    /// Repository URL
    repo_url: String,
    /// Commit-ish (branch, tag, or SHA)
    rev: Option<String>,
    /// Path to package within the repo
    path: PathBuf,
}

/// Custom error type for SHA validation
// TODO: derive(Error)?
#[derive(Debug)]
pub enum ShaError {
    InvalidLength(usize),
    InvalidCharacters,
}

impl UnpinnedGitDependency {
    /// Replace all commit-ishes in [deps] with commits (i.e. SHAs). Requires fetching the git
    /// repositories
    pub async fn pin(
        deps: DependencySet<Self>,
    ) -> PackageResult<DependencySet<PinnedGitDependency>> {
        let mut res = DependencySet::new();
        for (env, package, dep) in deps.into_iter() {
            let dep = dep.pin_one().await?;
            res.insert(env, package, dep);
        }
        Ok(res)
    }

    /// Replace the commit-ish [self.rev] with a commit (i.e. a SHA). Requires fetching the git
    /// repository
    async fn pin_one(&self) -> PackageResult<PinnedGitDependency> {
        let git: GitRepo = self.into();
        let sha = git.find_sha().await?;

        Ok(PinnedGitDependency {
            repo: git.repo_url,
            rev: sha,
            path: git.path,
        })
    }
}

impl GitRepo {
    /// Create a new GitRepo instance
    pub fn new(repo: String, rev: Option<String>, path: PathBuf) -> Self {
        Self {
            repo_url: repo,
            rev,
            path,
        }
    }

    /// Get the repository URL
    pub fn repo_url(&self) -> &str {
        &self.repo_url
    }

    /// Get the revision
    pub fn rev(&self) -> Option<&str> {
        self.rev.as_deref()
    }

    /// Try to fetch the repository at the given sha ([`rev`]).
    pub async fn fetch(&self) -> PackageResult<PathBuf> {
        self.fetch_impl(None).await
    }

    /// Internal implementation of fetch. It uses the default folder to fetch the repo to, defined
    /// by the MOVE_HOME env variable.
    async fn fetch_impl(&self, fetch_to_folder: Option<PathBuf>) -> PackageResult<PathBuf> {
        let sha = self.find_sha().await?;

        if !check_is_commit_sha(&sha) {
            return Err(PackageError::Git(GitError::invalid_sha(
                &self.repo_url,
                &sha,
            )));
        }
        debug!("git repo: {:?}", self);

        let repo_fs_path = format_repo_to_fs_path(&self.repo_url, &sha, fetch_to_folder);
        debug!("Repo path on disk: {:?}", repo_fs_path.display());

        // Check out the repo at the given sha
        self.checkout_repo(&repo_fs_path, &sha).await?;

        Ok(repo_fs_path)
    }

    /// Used for testing to be able to specify which folder to fetch to. Use `fetch` for all other needs.
    // TODO: should be non-pub
    pub async fn fetch_to_folder(&self, fetch_to_folder: PathBuf) -> PackageResult<PathBuf> {
        self.fetch_impl(Some(fetch_to_folder)).await
    }

    /// Checkout the repository using a sparse checkout. It will try to clone without checkout, set
    /// sparse checkout directory, and then checkout the folder specified by `self.path` at the
    /// given sha.
    ///
    // TODO think more about debug statements and what information to log
    async fn checkout_repo(&self, repo_fs_path: &PathBuf, sha: &str) -> PackageResult<()> {
        // Checkout repo if it does not exist already
        if !repo_fs_path.exists() {
            // Sparse checkout repo
            self.try_clone_sparse_checkout(repo_fs_path).await?;
            debug!("Sparse checkout successful");

            debug!("Path to checkout: {:?}", self.path);
            // Set the sparse checkout path
            self.try_sparse_checkout_init(repo_fs_path).await?;
            debug!("Sparse checkout init successful");

            self.try_set_sparse_dir(repo_fs_path, &self.path).await?;
            debug!("Sparse checkout set successful");

            self.try_checkout_at_sha(repo_fs_path, sha).await?;
            debug!("Checkout at sha {sha} successful");
        } else if self.is_dirty(repo_fs_path).await? {
            debug!("Repo is dirty");
            return Err(PackageError::Git(GitError::dirty(&self.repo_url)));
        }

        Ok(())
    }

    /// Check out the given SHA in the given repo
    async fn try_checkout_at_sha(&self, repo_fs_path: &PathBuf, sha: &str) -> PackageResult<()> {
        debug!("Checking out with SHA: {sha}");
        let cmd = self
            .run_git_cmd_with_args(&["checkout", sha], Some(repo_fs_path))
            .await?;
        Ok(())
    }

    /// Set the sparse checkout directory to the given path
    async fn try_set_sparse_dir(&self, repo_fs_path: &PathBuf, path: &Path) -> PackageResult<()> {
        debug!("Setting sparse checkout path to: {:?}", path);
        let cmd = self
            .run_git_cmd_with_args(
                &["sparse-checkout", "set", &path.to_string_lossy()],
                Some(repo_fs_path),
            )
            .await?;
        Ok(())
    }

    /// Try to initialize the repository in sparse-checkout mode.
    async fn try_sparse_checkout_init(&self, repo_fs_path: &PathBuf) -> PackageResult<()> {
        // git sparse-checkout init --cone
        debug!("Calling sparse checkout init");
        let cmd = self
            .run_git_cmd_with_args(&["sparse-checkout", "init", "--cone"], Some(repo_fs_path))
            .await?;

        if !cmd.status.success() {
            return Err(PackageError::Git(GitError::generic(format!(
                "git sparse-checkout init failed for {}, with error: {}",
                repo_fs_path.display(),
                cmd.status
            ))));
        }

        Ok(())
    }

    /// Try to clone git repository with sparse mode and no checkout.
    async fn try_clone_sparse_checkout(&self, repo_fs_path: &Path) -> PackageResult<()> {
        debug!(
            "Cloning repo with no checkout in sparse mode: {:?}, to folder: {:?}",
            self.repo_url,
            repo_fs_path.display()
        );
        let cmd = self
            .run_git_cmd_with_args(
                &[
                    "clone",
                    "--sparse",
                    "--filter=blob:none",
                    "--no-checkout",
                    &self.repo_url,
                    &repo_fs_path.to_string_lossy(),
                ],
                None,
            )
            .await;

        if cmd.is_err() {
            return Err(PackageError::Git(GitError::generic(format!(
                "git clone failed for {}, with error: {}",
                self.repo_url,
                cmd.unwrap_err()
            ))));
        }

        Ok(())
    }

    /// Runs a git command from the provided arguments.
    // TODO: check for error codes here
    pub async fn run_git_cmd_with_args(
        &self,
        args: &[&str],
        cwd: Option<&PathBuf>,
    ) -> PackageResult<Output> {
        // Run the git command
        debug!(
            "Running git command with args {:?} in cwd: {}",
            args,
            cwd.unwrap_or(&PathBuf::from(".")).display()
        );

        let mut cmd = Command::new("git");

        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }

        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PackageError::Git(GitError::command_error(e.to_string())))
    }

    /// Check if the git repository is dirty
    pub async fn is_dirty(&self, repo_fs_path: &PathBuf) -> PackageResult<bool> {
        debug!("Checking if repo is dirty");
        let cmd = self
            .run_git_cmd_with_args(
                &["status", "--porcelain", "--untracked-files=no"],
                Some(repo_fs_path),
            )
            .await?;

        if !cmd.stdout.is_empty() {
            debug!("Repo {} is dirty", self.repo_url);
            return Ok(true);
        }

        Ok(false)
    }

    /// Find the SHA of the given commit/branch in the given repo. This will make a remote call so
    /// network is required.
    async fn find_sha(&self) -> PackageResult<String> {
        if let Some(r) = self.rev.as_ref() {
            if check_is_commit_sha(r) {
                return Ok(r.to_string());
            }

            // if there is some revision which is likely a branch, a tag, or a wrong SHA (e.g., capital
            // letter), then we have a different set of arguments than if there is no revision. In no
            // revision case, we need to find the default branch of that remote.

            // we have a branch or tag
            // git ls-remote https://github.com/user/repo.git refs/heads/main
            let cmd = self
                .run_git_cmd_with_args(
                    &["ls-remote", &self.repo_url, &format!("refs/heads/{r}")],
                    None,
                )
                .await?;

            if !cmd.status.success() {
                debug!(
                    "Could not run git ls-remote command, return non-zero exit code: {:?}",
                    cmd
                );
                return Err(PackageError::Git(GitError::generic(format!(
                    "git ls-remote failed for {}, with error: {}",
                    self.repo_url, cmd.status
                ))));
            }

            let stdout = String::from_utf8(cmd.stdout)?;

            Ok(stdout
                .split_whitespace()
                .next()
                .ok_or(PackageError::Git(GitError::no_sha(&self.repo_url, r)))?
                .to_string())
        } else {
            // nothing specified, so we need to find the default branch
            self.find_default_branch_and_get_sha().await
        }
    }

    /// Find the default branch and return the SHA
    async fn find_default_branch_and_get_sha(&self) -> PackageResult<String> {
        let cmd = self
            .run_git_cmd_with_args(&["ls-remote", "--symref", &self.repo_url, "HEAD"], None)
            .await?;

        if !cmd.status.success() {
            debug!(
                "Could not run git ls-remote --symref command, return non-zero exit code: {:?}",
                cmd
            );
            return Err(PackageError::Git(GitError::generic(format!(
                "git ls-remote failed for {}, with error: {}",
                self.repo_url, cmd.status
            ))));
        }

        let stdout = String::from_utf8(cmd.stdout)?;
        let lines: Vec<_> = stdout.lines().collect();
        let default_branch = lines[0].split_whitespace().nth(1).ok_or_else(|| {
            debug!(
                "Could not find default branch.\nlines{:?}\nself{:?}",
                lines, self
            );
            PackageError::Git(GitError::no_sha(&self.repo_url, "HEAD"))
        })?;
        let sha = lines[1].split_whitespace().next().ok_or_else(|| {
            debug!(
                "Could not find sha for default branch.\nlines: {:?}\nself: {:?}\n",
                lines, self
            );
            PackageError::Git(GitError::no_sha(&self.repo_url, "HEAD"))
        })?;

        Ok(sha.to_string())
    }
}

// Implement std::error::Error for custom error
impl std::error::Error for ShaError {}

impl fmt::Display for ShaError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ShaError::InvalidLength(len) => {
                write!(f, "SHA should be 40 characters long, got {}", len)
            }
            ShaError::InvalidCharacters => write!(
                f,
                "SHA should only contain hexadecimal lowercase characters (0-9, a-f)"
            ),
        }
    }
}

impl From<&UnpinnedGitDependency> for GitRepo {
    fn from(dep: &UnpinnedGitDependency) -> Self {
        GitRepo::new(dep.repo.clone(), dep.rev.clone(), dep.path.clone())
    }
}

impl From<&PinnedGitDependency> for GitRepo {
    fn from(dep: &PinnedGitDependency) -> Self {
        GitRepo::new(dep.repo.clone(), Some(dep.rev.clone()), dep.path.clone())
    }
}

impl From<UnpinnedGitDependency> for GitRepo {
    fn from(dep: UnpinnedGitDependency) -> Self {
        GitRepo::new(dep.repo, dep.rev, dep.path)
    }
}

impl From<PinnedGitDependency> for GitRepo {
    fn from(dep: PinnedGitDependency) -> Self {
        GitRepo::new(dep.repo, Some(dep.rev), dep.path)
    }
}

/// Fetch the given git dependency and return the path to the checked out repo
pub async fn fetch_dep(dep: PinnedGitDependency) -> PackageResult<PathBuf> {
    let git_repo = GitRepo::from(&dep);
    git_repo.fetch().await
}

/// Deserialize a SHA string to ensure it is well formed.
pub fn deserialize_sha<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let sha = String::deserialize(deserializer)?;
    if sha.len() != 40 {
        Err(de::Error::custom(ShaError::InvalidLength(sha.len())))
    } else if !check_is_commit_sha(&sha) {
        Err(de::Error::custom(ShaError::InvalidCharacters))
    } else {
        Ok(sha)
    }
}

/// Format the repository URL to a filesystem name based on the SHA
pub fn format_repo_to_fs_path(repo: &str, sha: &str, root_path: Option<PathBuf>) -> PathBuf {
    let root_path = root_path
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| (*move_command_line_common::env::MOVE_HOME).to_string());
    PathBuf::from(format!("{}/{}_{}", root_path, url_to_file_name(repo), sha))
}

/// Check if the given string is a valid commit SHA, i.e., 40 character long with only
/// lowercase letters and digits
///
// TODO: rename this function to is_sha
fn check_is_commit_sha(input: &str) -> bool {
    input.len() == 40
        && input
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// Transform a repository URL into a directory name
fn url_to_file_name(url: &str) -> String {
    regex::Regex::new(r"/|:|\.|@")
        .unwrap()
        .replace_all(url, "_")
        .to_string()
}

// TODO: add more tests
#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::Path;
    use tempfile::{TempDir, tempdir};

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
    /// It returns the temporary directory, the root path of the project, the first commit sha, and
    /// and the second commit sha.
    pub async fn setup_test_move_project() -> (TempDir, PathBuf, String, String) {
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
        eprintln!("{output:?}");
        let commits = run_git_cmd(&["log", "--pretty=format:%H"], &root_path).await;
        let commits = String::from_utf8_lossy(&commits.stdout);
        let commits: Vec<_> = commits.lines().collect();

        let branch = run_git_cmd(&["rev-parse", "--abbrev-ref", "HEAD"], &root_path).await;

        (
            temp_dir,
            root_path,
            commits[1].to_string(),
            commits[0].to_string(),
        )
    }

    #[tokio::test]
    async fn test_sparse_checkout_folder() {
        let (temp_folder, fs_repo, first_sha, second_sha) = setup_test_move_project().await;
        let fs_repo = fs_repo.to_str().unwrap();

        // Pass in a branch name
        let git_repo = GitRepo {
            repo_url: fs_repo.to_string(),
            rev: Some("main".to_string()),
            path: PathBuf::from("packages/pkg_a"),
        };

        // Fetch the dependency
        let checkout_path = git_repo
            .fetch_to_folder(temp_folder.into_path())
            .await
            .unwrap();
        // Verify only packages/pkg_a was checked out
        assert!(checkout_path.join("packages/pkg_a").exists());
        assert!(!checkout_path.join("packages/pkg_b").exists());

        let (temp_folder, fs_repo, first_sha, second_sha) = setup_test_move_project().await;
        let fs_repo = fs_repo.to_str().unwrap();
        // Pass in a commit SHA
        let git_dep = GitRepo {
            repo_url: fs_repo.to_string(),
            rev: Some(second_sha.to_string()),
            path: PathBuf::from("packages/pkg_b"),
        };

        // Fetch the dependency
        let checkout_path = git_dep
            .fetch_to_folder(temp_folder.into_path())
            .await
            .unwrap();

        // Verify only packages/pkg_b was checked out
        assert!(checkout_path.join("packages/pkg_b").exists());
        assert!(!checkout_path.join("packages/pkg_a").exists());
    }

    #[tokio::test]
    async fn test_wrong_sha() {
        let (temp_folder, fs_repo, first_sha, second_sha) = setup_test_move_project().await;
        let fs_repo = fs_repo.to_str().unwrap();

        let git_dep = GitRepo {
            repo_url: fs_repo.to_string(),
            rev: Some("912saTsvc".to_string()),
            path: PathBuf::from("packages/pkg_a"),
        };

        // Fetch the dependency
        let result = git_dep.fetch_to_folder(temp_folder.into_path()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_wrong_branch_name() {
        let (temp_folder, fs_repo, first_sha, second_sha) = setup_test_move_project().await;
        let fs_repo = fs_repo.to_str().unwrap();

        let git_dep = GitRepo {
            repo_url: fs_repo.to_string(),
            rev: Some("test".to_string()),
            path: PathBuf::from("packages/pkg_a"),
        };

        // Fetch the dependency
        let result = git_dep.fetch_to_folder(temp_folder.into_path()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_repo_is_dirty() {
        let (temp_folder, fs_repo, first_sha, second_sha) = setup_test_move_project().await;
        let fs_repo = fs_repo.to_str().unwrap();

        let git_dep = GitRepo {
            repo_url: fs_repo.to_string(),
            rev: Some("main".to_string()),
            path: PathBuf::from("packages/pkg_a"),
        };

        let checkout_path = git_dep
            .fetch_to_folder(temp_folder.into_path())
            .await
            .unwrap();
        // Delete a file in the repo to make it dirty
        let move_toml_path = checkout_path.join("packages/pkg_a").join("Move.toml");
        fs::remove_file(move_toml_path).unwrap();
        // Check if the repo is dirty
        let is_dirty = git_dep.is_dirty(&checkout_path).await.unwrap();
        assert!(is_dirty);
    }
}
