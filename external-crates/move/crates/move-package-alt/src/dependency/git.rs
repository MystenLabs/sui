// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Types and methods for external dependencies (of the form `{ git = "<repo>" }`)
//!
//! Git dependencies are cached in `~/.move`. Each dependency has a sparse, shallow checkout
//! in the directory `~/.move/<remote>_<sha>` (see [crate::git::format_repo_to_fs_path])

use std::path::PathBuf;

use crate::{
    errors::PackageResult,
    git::{GitCache, GitSha, GitTree},
    schema::{LockfileGitDepInfo, ManifestGitDependency},
};

use super::DependencySet;

#[derive(Clone, Debug)]
pub struct PinnedGitDependency {
    inner: GitTree,
}

impl PinnedGitDependency {
    /// Fetch the given git dependency and return the path to the checked out repo
    pub async fn fetch(&self) -> PackageResult<PathBuf> {
        Ok(self.inner.fetch().await?)
    }

    /// Return the path that `fetch` would return without actually fetching the data
    pub fn unfetched_path(&self) -> PathBuf {
        self.inner.path_to_tree()
    }

    // TODO: remove
    pub fn inner(&self) -> &GitTree {
        &self.inner
    }
}

impl ManifestGitDependency {
    /// Replace the commit-ish [self.rev] with a commit (i.e. a SHA). Requires fetching the git
    /// repository
    pub async fn pin(&self) -> PackageResult<PinnedGitDependency> {
        let cache = GitCache::new();
        let ManifestGitDependency { repo, rev, path } = self.clone();
        let tree = cache.resolve_to_tree(&repo, &rev, Some(path)).await?;
        Ok(PinnedGitDependency { inner: tree })
    }
}
