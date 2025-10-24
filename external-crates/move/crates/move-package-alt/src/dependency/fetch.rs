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

use super::{Dependency, Pinned};

/// Once a dependency has been fetched, it is simply represented by a [PackagePath]
type Fetched = PackagePath;

pub struct FetchedDependency(pub(super) Dependency<Fetched>);

#[derive(Error, Debug)]
pub enum FetchError {
    #[error("Failed to load dependency `{1}`: {0}")]
    BadPackage(PackagePathError, String),

    #[error("Error while fetching `{1}`: {0}")]
    GitFailure(GitError, String),
}

pub type FetchResult<T> = Result<T, FetchError>;

impl FetchedDependency {
    /// Ensure that the dependency's files are present on the disk and return a path to them
    /// Assumes that `pinned` is already normalized - paths of any local dependencies are relative
    /// to the current working directory, and local dependencies of git dependencies have been
    /// transformed into git dependencies
    pub async fn fetch(pinned: &Pinned) -> FetchResult<PackagePath> {
        let path = match &pinned {
            Pinned::Git(dep) => dep
                .inner
                .fetch()
                .await
                .map_err(FetchError::from_git(pinned))?,
            _ => pinned.unfetched_path(),
        };

        PackagePath::new(path).map_err(FetchError::from_package(pinned))
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

impl FetchError {
    fn from_package(pinned: &Pinned) -> impl FnOnce(PackagePathError) -> Self {
        let pin = format!("{pinned}");
        |e| Self::BadPackage(e, pin)
    }

    fn from_git(pinned: &Pinned) -> impl FnOnce(GitError) -> Self {
        let pin = format!("{pinned}");
        |e| Self::GitFailure(e, pin)
    }
}
