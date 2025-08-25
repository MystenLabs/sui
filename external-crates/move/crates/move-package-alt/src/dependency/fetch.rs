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

use super::{Dependency, PinnedDependencyInfo, pin::Pinned};

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
            Pinned::Git(dep) => dep.inner.fetch().await?,
            _ => pinned.unfetched_path(),
        };
        let path = PackagePath::new(path)?;
        Ok(Self(pinned.0.clone().map(|_| path)))
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
