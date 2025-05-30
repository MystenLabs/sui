// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Types and methods for external dependencies (of the form `{ git = "<repo>" }`)
//!
//! Git dependencies are cached in `~/.move`. Each dependency has a sparse, shallow checkout
//! in the directory `~/.move/<remote>_<sha>` (see [crate::git::format_repo_to_fs_path])

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    errors::PackageResult,
    git::{GitRepo, sha::GitSha},
};

use super::DependencySet;

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

impl PinnedGitDependency {
    /// Fetch the given git dependency and return the path to the checked out repo
    pub async fn fetch(&self) -> PackageResult<PathBuf> {
        let git_repo = GitRepo::from(self);
        Ok(git_repo.fetch().await?)
    }
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
