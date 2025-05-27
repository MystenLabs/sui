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

use crate::git::GitRepo;
use crate::{
    errors::{Located, PackageError, PackageResult},
    git::sha::GitSha,
};

use super::{DependencySet, Pinned, Unpinned};

/// TODO keep same style around all types
///
/// A git dependency that is unpinned. The `rev` field can be either empty, a branch, or a sha. To
/// resolve this into a [`PinnedGitDependency`], call `pin_one` function.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UnpinnedGitDependency {
    /// The repository containing the dependency
    #[serde(rename = "git")]
    pub repo: String,

    /// The git commit or branch for the dependency.
    #[serde(default)]
    pub rev: Option<String>,

    /// The path within the repository
    #[serde(default)]
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PinnedGitDependency {
    /// The repository containing the dependency
    #[serde(rename = "git")]
    pub repo: String,

    /// The exact sha for the revision
    pub rev: GitSha,

    /// The path within the repository
    #[serde(default)]
    pub path: PathBuf,
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

impl From<&UnpinnedGitDependency> for GitRepo {
    fn from(dep: &UnpinnedGitDependency) -> Self {
        GitRepo::new(dep.repo.clone(), dep.rev.clone(), dep.path.clone())
    }
}

impl From<&PinnedGitDependency> for GitRepo {
    fn from(dep: &PinnedGitDependency) -> Self {
        GitRepo::new(
            dep.repo.clone(),
            Some(dep.rev.clone().into()),
            dep.path.clone(),
        )
    }
}

impl From<UnpinnedGitDependency> for GitRepo {
    fn from(dep: UnpinnedGitDependency) -> Self {
        GitRepo::new(dep.repo, dep.rev, dep.path)
    }
}

impl From<PinnedGitDependency> for GitRepo {
    fn from(dep: PinnedGitDependency) -> Self {
        GitRepo::new(dep.repo, Some(dep.rev.into()), dep.path)
    }
}

/// Fetch the given git dependency and return the path to the checked out repo
pub async fn fetch_dep(dep: PinnedGitDependency) -> PackageResult<PathBuf> {
    let git_repo = GitRepo::from(&dep);
    Ok(git_repo.fetch().await?)
}
