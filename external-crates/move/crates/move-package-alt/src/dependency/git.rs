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
    errors::{GitError, GitErrorKind, Located, PackageError, PackageResult},
    git::is_sha,
};

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
    #[serde(deserialize_with = "deserialize_sha")]
    pub rev: Sha,

    /// The path within the repository
    #[serde(default)]
    pub path: PathBuf,
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
    } else if !is_sha(&sha) {
        Err(de::Error::custom(ShaError::InvalidCharacters))
    } else {
        Ok(sha)
    }
}
