// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Git dependencies are cached in `~/.move`. Each dependency has a sparse, shallow checkout
//! in the directory `~/.move/<remote>_<sha>` (see [crate::git::format_repo_to_fs_path])

use std::path::{Path, PathBuf};

use path_clean::PathClean;
use thiserror::Error;

use crate::{
    git::GitError,
    package::paths::{PackagePath, PackagePathError},
    schema::LocalDepInfo,
};

use super::{
    Dependency, PinnedDependencyInfo,
    pin::{Pinned, PinnedGitDependency},
};

/// Once a dependency has been fetched, it is simply represented by a [PackagePath]
type Fetched = PackagePath;

pub struct FetchedDependency(pub(super) Dependency<Fetched>);

#[derive(Error, Debug)]
pub enum FetchError {
    #[error(transparent)]
    BadPackage(#[from] PackagePathError),

    #[error(transparent)]
    GitFailure(#[from] GitError),
}

pub type FetchResult<T> = Result<T, FetchError>;

impl FetchedDependency {
    /// Ensure that the dependency's files are present on the disk and return a path to them
    pub async fn fetch(pinned: &PinnedDependencyInfo) -> FetchResult<Self> {
        // TODO: need to actually fetch local dep
        let path = match &pinned.0.dep_info {
            Pinned::Git(dep) => dep.fetch().await?,
            Pinned::Local(dep) => dep.absolute_path(pinned.0.containing_file.path()),
            Pinned::OnChain(_) => todo!(),
        };
        let path = PackagePath::new(path)?;
        Ok(Self(pinned.0.clone().map(|_| path)))
    }

    /// Return the absolute path to the directory that this package would be fetched into, without
    /// actually fetching it
    pub fn unfetched_path(pinned: &PinnedDependencyInfo) -> PathBuf {
        match &pinned.0.dep_info {
            Pinned::Git(dep) => dep.unfetched_path(),
            Pinned::Local(dep) => dep.absolute_path(pinned.0.containing_file.path()),
            Pinned::OnChain(dep) => todo!(),
        }
    }

    pub fn path(&self) -> &PackagePath {
        &self.0.dep_info
    }
}

impl PinnedGitDependency {
    /// Fetch the given git dependency and return the path to the checked out repo
    pub async fn fetch(&self) -> FetchResult<PathBuf> {
        Ok(self.inner.fetch().await?)
    }

    /// Return the path that `fetch` would return without actually fetching the data
    pub fn unfetched_path(&self) -> PathBuf {
        self.inner.path_to_tree()
    }
}

impl From<FetchedDependency> for PackagePath {
    fn from(value: FetchedDependency) -> Self {
        value.0.dep_info
    }
}

impl LocalDepInfo {
    /// Retrieve the absolute path to [`LocalDependency`] without actually fetching it.
    pub fn absolute_path(&self, containing_file: impl AsRef<Path>) -> PathBuf {
        // TODO: handle panic with a proper error.
        containing_file
            .as_ref()
            .parent()
            .expect("non-directory files have parents")
            .join(&self.local)
            .clean()
    }
}
