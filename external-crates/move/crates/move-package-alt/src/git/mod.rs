// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod errors;
pub mod sha;

use std::{
    io::BufRead,
    path::{Path, PathBuf},
    process::{Output, Stdio},
};

use errors::{GitError, GitResult};
use sha::GitSha;
use tokio::process::Command;
use tracing::{debug, info};

/// Helper struct that represents a Git repository, with extra information about which folder to
/// checkout.
#[derive(Clone, Debug)]
pub struct GitRepo {
    /// Repository URL
    pub repo_url: String,
    /// Commit-ish (branch, tag, or SHA)
    pub rev: Option<String>,
    /// Folder to checkout during sparse checkout
    pub path: PathBuf,
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
    pub async fn fetch(&self) -> GitResult<PathBuf> {
        self.fetch_impl(None).await
    }

    /// Internal implementation of fetch. It uses the default folder to fetch the repo to, defined
    /// by the MOVE_HOME env variable.
    async fn fetch_impl(&self, fetch_to_folder: Option<PathBuf>) -> GitResult<PathBuf> {
        let sha = self.find_sha().await?;

        debug!("git repo: {:?}", self);

        let repo_fs_path = format_repo_to_fs_path(&self.repo_url, &sha, fetch_to_folder);
        debug!("Repo path on disk: {:?}", repo_fs_path.display());

        // Check out the repo at the given sha
        self.checkout_repo(&repo_fs_path, &sha).await?;

        Ok(repo_fs_path)
    }

    /// Used for testing to be able to specify which folder to fetch to. Use `fetch` for all other needs.
    async fn fetch_to_folder(&self, fetch_to_folder: PathBuf) -> GitResult<PathBuf> {
        self.fetch_impl(Some(fetch_to_folder)).await
    }

    /// Checkout the repository using a sparse checkout. It will try to clone without checkout, set
    /// sparse checkout directory, and then checkout the folder specified by `self.path` at the
    /// given sha.
    ///
    // TODO think more about debug statements and what information to log
    async fn checkout_repo(&self, repo_fs_path: &PathBuf, sha: &GitSha) -> GitResult<()> {
        // Checkout repo if it does not exist already
        if !repo_fs_path.exists() {
            // Sparse checkout repo
            self.try_clone_sparse_checkout(repo_fs_path).await?;
            debug!("Sparse checkout successful");

            debug!("Path to checkout: {:?}", repo_fs_path);
            // Set the sparse checkout path
            self.try_sparse_checkout_init(repo_fs_path).await?;
            debug!("Sparse checkout init successful");

            self.try_set_sparse_dir(repo_fs_path, &self.path).await?;
            debug!("Sparse checkout set successful");

            self.try_checkout_at_sha(repo_fs_path, sha).await?;
            debug!("Checkout at sha {sha:?} successful");
        } else if self.is_dirty(repo_fs_path).await? {
            debug!("Repo is dirty");
            return Err(GitError::dirty(&self.repo_url));
        }

        Ok(())
    }

    /// Check out the given SHA in the given repo
    async fn try_checkout_at_sha(&self, repo_fs_path: &PathBuf, sha: &GitSha) -> GitResult<()> {
        debug!("Checking out with SHA: {sha:?}");
        run_git_cmd_with_args(&["checkout", sha.as_ref()], Some(repo_fs_path)).await?;
        Ok(())
    }

    /// Set the sparse checkout directory to the given path
    async fn try_set_sparse_dir(&self, repo_fs_path: &PathBuf, path: &Path) -> GitResult<()> {
        // git sparse-checkout set <path>
        debug!("Setting sparse checkout path to: {:?}", path);
        run_git_cmd_with_args(
            &["sparse-checkout", "set", &path.to_string_lossy()],
            Some(repo_fs_path),
        )
        .await?;
        Ok(())
    }

    /// Try to initialize the repository in sparse-checkout mode.
    async fn try_sparse_checkout_init(&self, repo_fs_path: &PathBuf) -> GitResult<()> {
        // git sparse-checkout init --cone
        debug!("Calling sparse checkout init");
        run_git_cmd_with_args(&["sparse-checkout", "init", "--cone"], Some(repo_fs_path)).await?;
        Ok(())
    }

    /// Try to clone git repository with sparse mode and no checkout.
    async fn try_clone_sparse_checkout(&self, repo_fs_path: &Path) -> GitResult<()> {
        debug!(
            "Cloning repo with no checkout in sparse mode: {:?}, to folder: {:?}",
            self.repo_url,
            repo_fs_path.display()
        );
        run_git_cmd_with_args(
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
        .await?;
        Ok(())
    }

    /// Check if the git repository is dirty
    pub async fn is_dirty(&self, repo_fs_path: &PathBuf) -> GitResult<bool> {
        debug!("Checking if repo is dirty");
        let output = run_git_cmd_with_args(
            &["status", "--porcelain", "--untracked-files=no"],
            Some(repo_fs_path),
        )
        .await?;

        if !output.is_empty() {
            debug!("Repo {} is dirty", self.repo_url);
            return Ok(true);
        }

        Ok(false)
    }

    /// Find the SHA of the given commit/branch in the given repo. This will make a remote call so
    /// network is required.
    pub(crate) async fn find_sha(&self) -> GitResult<GitSha> {
        if let Some(r) = self.rev.as_ref() {
            if let Ok(sha) = GitSha::try_from(r.to_string()) {
                return Ok(sha);
            }

            // if there is some revision which is likely a branch, a tag, or a wrong SHA (e.g., capital
            // letter), then we have a different set of arguments than if there is no revision. In no
            // revision case, we need to find the default branch of that remote.

            // we have a branch or tag
            // git ls-remote https://github.com/user/repo.git refs/heads/main
            let stdout = run_git_cmd_with_args(
                &["ls-remote", &self.repo_url, &format!("refs/heads/{r}")],
                None,
            )
            .await?;

            Ok(stdout
                .split_whitespace()
                .next()
                .ok_or(GitError::no_sha(&self.repo_url, r))?
                .to_string()
                .try_into()
                .expect("git returns valid shas"))
        } else {
            // nothing specified, so we need to find the default branch
            self.find_default_branch_and_get_sha().await
        }
    }

    /// Find the default branch and return the SHA
    pub(crate) async fn find_default_branch_and_get_sha(&self) -> GitResult<GitSha> {
        let stdout =
            run_git_cmd_with_args(&["ls-remote", "--symref", &self.repo_url, "HEAD"], None).await?;

        let lines: Vec<_> = stdout.lines().collect();
        // TODO: default_branch is ignored here; are we guaranteed that the sha is always the
        // second line?
        let default_branch = lines[0].split_whitespace().nth(1).ok_or_else(|| {
            debug!("Could not find default branch.\nlines {lines:?}\nself{self:?}");
            GitError::no_sha(&self.repo_url, "HEAD")
        })?;
        let sha_str = lines[1].split_whitespace().next().ok_or_else(|| {
            debug!("Could not find sha for default branch.\nlines: {lines:?}\nself: {self:?}\n");
            GitError::no_sha(&self.repo_url, "HEAD")
        })?;

        let sha = GitSha::try_from(sha_str.to_string())
            .expect("Git should return correctly formatted shas");

        Ok(sha)
    }
}

/// Format the repository URL to a filesystem name based on the SHA
pub fn format_repo_to_fs_path(repo: &str, sha: &GitSha, root_path: Option<PathBuf>) -> PathBuf {
    let root_path = root_path
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| (*move_command_line_common::env::MOVE_HOME).to_string());
    PathBuf::from(format!(
        "{}/{}_{:?}",
        root_path,
        url_to_file_name(repo),
        sha
    ))
}

/// Runs `git <args>` in `cwd`. Fails if there is an io failure or if `git` returns a non-zero
/// exit status; returns the standard output.
async fn run_git_cmd_with_args(args: &[&str], cwd: Option<&PathBuf>) -> GitResult<String> {
    // Run the git command

    let mut cmd = Command::new("git");
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }

    debug!(
        "Running {}{cmd:?}",
        match cwd {
            Some(p) => format!("(in dir {p:?}) "),
            None => String::new(),
        }
    );

    let output = cmd
        .output()
        .await
        .map_err(|e| GitError::io_error(&cmd, &cwd, e))?;

    if !output.status.success() {
        return Err(GitError::nonzero_exit_status(&cmd, &cwd, output.status));
    }

    if !output.stderr.is_empty() {
        info!("output from {cmd:?}:");
        for line in output.stderr.lines() {
            info!("  │ {}", line.expect("vector read can't fail"));
        }
    }

    if !output.stdout.is_empty() {
        debug!("stdout from {cmd:?}:");
        for line in output.stdout.lines() {
            debug!("  │ {}", line.expect("vector read can't fail"));
        }
    }

    String::from_utf8(output.stdout).map_err(|e| GitError::non_utf_output(&cmd, &cwd))
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
