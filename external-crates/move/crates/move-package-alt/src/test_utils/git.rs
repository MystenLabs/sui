// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0
//
// Copied and adapted from
// <https://github.com/rust-lang/cargo/tree/master/crates/cargo-test-support/src> at SHA
// 4ac865d3d7b62281ad4dcb92406c816b6f1aeceb

//! # Git Testing Support
//!
//! ## Creating a git dependency
//! [`new()`] is an easy way to create a new git repository containing a
//! project that you can then use as a dependency. To create a project in a [RepoProject],
//! you can call `commit` on it. This returns a [Commit] object that you can then tag, branch, or
//! retrieve the sha for.
//!
//! ### Example:
//!
//! TODO: out of date
//! ```no_run
//! let git_project = git::new();
//! let commit = git_project.commit(|project| {
//!     project.add_packages(["a", "b", "c"]).add_deps([("a", "b"), ("b", "c")])
//! });
//!
//! // Use the `root()` or `root_path()` method to get the FS path to the new repository.
//! let p = project()
//!     .file("Move.toml", &format!(r#"
//!         [package]
//!         name = "a"
//!         version = "1.0.0"
//!         edition = "2024"
//!
//! let scenario = TestPackageGraph::new(["root"])
//!     .add_git_dep("root", commit.branch("main"), "a", |dep| dep)
//!     .build();
//! ```
//!
//! ## Manually creating repositories
//!
//! [`repo()`] can be used to create a [`RepoBuilder`] which provides a way of
//! adding files to a blank repository and committing them.

use tempfile::TempDir;

use crate::git::run_git_cmd_with_args;
use crate::test_utils::*;

use super::graph_builder::TestPackageGraph;

/// A [RepoProject] represents a bare repository in a temporary directory. You can add new commits
/// to the repository using [RepoProject::commit].
pub struct RepoProject {
    /// The root contains a repository `root/repo` which always has a detached checkout of main.
    /// While creating a commit, we also create a temporary worktree in `root/worktree`.
    ///
    /// There is also an `empty_commit` branch that always refers to the initial commit
    root: TempDir,
}

/// A [Commit] represents a single commit in a [RepoProject]
pub struct Commit<'repo> {
    repo: &'repo RepoProject,
    sha: String,
}

pub async fn new() -> RepoProject {
    RepoProject::new().await
}

impl Commit<'_> {
    pub fn sha(&self) -> String {
        self.sha.to_string()
    }

    pub fn short_sha(&self) -> String {
        self.sha[0..8].to_string()
    }

    pub async fn branch(&self, name: impl AsRef<str>) -> String {
        self.repo.branch(&self.sha, name.as_ref()).await;
        name.as_ref().to_string()
    }

    pub async fn tag(&self, name: impl AsRef<str>) -> String {
        self.repo.tag(&self.sha, name.as_ref()).await;
        name.as_ref().to_string()
    }
}

impl RepoProject {
    pub async fn new() -> Self {
        let result = Self {
            root: TempDir::new().unwrap(),
        };
        result.init().await;
        result
    }

    pub fn repo_path(&self) -> PathBuf {
        self.root.as_ref().join("repo")
    }

    pub fn repo_path_str(&self) -> String {
        self.repo_path().to_string_lossy().to_string()
    }

    /// Builds a new project using `build` (starting from an empty directory), then commits it to
    /// the repository and updates the `main` branch. Returns the created commit
    pub async fn commit<F>(&self, build: F) -> Commit
    where
        F: FnOnce(TestPackageGraph) -> TestPackageGraph,
    {
        self.add_worktree().await;
        let mut builder = TestPackageGraph::new(Vec::<&str>::new()).at(self.worktree_path());
        builder = build(builder);
        builder.build();

        self.add_all().await;
        let sha = self.commit_worktree().await;
        let result = Commit { repo: self, sha };
        self.delete_worktree().await;
        result
    }

    /// commit the contents of the worktree, updates the `main` branch, and returns the commit hash
    async fn commit_worktree(&self) -> String {
        run_git_cmd_with_args(
            &["commit", "--allow-empty", "-m", "test commit message"],
            Some(&self.worktree_path()),
        )
        .await
        .unwrap();
        let mut result = run_git_cmd_with_args(&["rev-parse", "HEAD"], Some(&self.worktree_path()))
            .await
            .unwrap();

        // remove trailing newline
        result.pop();

        run_git_cmd_with_args(&["branch", "-f", "main", &result], Some(&self.repo_path()))
            .await
            .unwrap();

        result
    }

    /// add all files in the worktree
    async fn add_all(&self) {
        run_git_cmd_with_args(&["add", "."], Some(&self.worktree_path()))
            .await
            .unwrap();
        run_git_cmd_with_args(&["status"], Some(&self.worktree_path()))
            .await
            .unwrap();
    }

    /// update `branch_name` to refer to `sha`
    async fn branch(&self, sha: &str, branch_name: &str) {
        run_git_cmd_with_args(
            &["branch", "--force", branch_name, sha],
            Some(&self.repo_path()),
        )
        .await
        .unwrap();
    }

    /// update `tag` to refer to `sha`
    async fn tag(&self, sha: &str, tag_name: &str) {
        run_git_cmd_with_args(&["tag", "-f", tag_name, sha], Some(&self.repo_path()))
            .await
            .unwrap();
    }

    /// create an empty repository with an initial empty commit inside of [Self::repo_path]
    async fn init(&self) {
        fs::create_dir_all(self.repo_path()).unwrap();
        run_git_cmd_with_args(
            &["init", "--initial-branch", "main"],
            Some(&self.repo_path()),
        )
        .await
        .unwrap();
        run_git_cmd_with_args(
            &["config", "user.email", "foo@bar.com"],
            Some(&self.repo_path()),
        )
        .await
        .unwrap();
        run_git_cmd_with_args(&["config", "user.name", "Foo Bar"], Some(&self.repo_path()))
            .await
            .unwrap();

        run_git_cmd_with_args(
            &["commit", "-m", "initial commit", "--allow-empty"],
            Some(&self.repo_path()),
        )
        .await
        .unwrap();

        run_git_cmd_with_args(&["branch", "empty_commit"], Some(&self.repo_path()))
            .await
            .unwrap();

        run_git_cmd_with_args(&["checkout", "--detach"], Some(&self.repo_path()))
            .await
            .unwrap();
    }

    /// creates the worktree containing the empty initial commit
    async fn add_worktree(&self) {
        run_git_cmd_with_args(
            &[
                "worktree",
                "add",
                "--detach",
                self.worktree_path().to_string_lossy().as_ref(),
                "empty_commit",
            ],
            Some(&self.repo_path()),
        )
        .await
        .unwrap();
    }

    /// removes the worktree
    async fn delete_worktree(&self) {
        run_git_cmd_with_args(
            &[
                "worktree",
                "remove",
                self.worktree_path().to_string_lossy().as_ref(),
            ],
            Some(&self.repo_path()),
        )
        .await
        .unwrap();

        run_git_cmd_with_args(&["checkout", "--detach", "main"], Some(&self.repo_path()))
            .await
            .unwrap();
    }

    fn worktree_path(&self) -> PathBuf {
        self.root.as_ref().join("worktree")
    }
}

impl std::fmt::Debug for Commit<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.sha().fmt(f)
    }
}
