use std::path::PathBuf;

use thiserror::Error;

use crate::{
    git::{GitError, GitTree},
    package::paths::{PackagePath, PackagePathError, PackagePathResult},
    schema::{
        LocalDependency, LockfileDependencyInfo, OnChainDependency, PinnedGitDependency,
        UnpinnedGitDependency,
    },
};

use super::{Dependency, Pinned};

#[derive(Error, Debug)]
pub enum FetchError {
    #[error(transparent)]
    Git(#[from] GitError),

    #[error(transparent)]
    NoManifest(#[from] PackagePathError),
}

pub type FetchResult<T> = Result<T, FetchError>;

impl Dependency<Pinned> {
    /// Ensure that the package has been downloaded and has a manifest
    pub async fn fetch(&self) -> FetchResult<PackagePath> {
        match &self.dep_info {
            Pinned::Local(local) => Ok(PackagePath::new(local_dir(&self, local))?),
            Pinned::OnChain(onchain) => todo!(),
            Pinned::Git(git) => Ok(PackagePath::new(git.fetch().await?)?),
        }
    }

    /// Return the path that would be returned by `fetch`, without actually fetching
    pub fn unfetched_path(&self) -> PathBuf {
        match &self.dep_info {
            Pinned::Local(local) => local_dir(&self, local),
            Pinned::OnChain(onchain) => todo!(),
            Pinned::Git(git) => git.fs_path(),
        }
    }
}

/// Returns the absolute path to `local`, assuming that it is contained in `context`
fn local_dir(context: &Dependency<Pinned>, local: &LocalDependency) -> PathBuf {
    context
        .containing_file
        .path()
        .parent()
        .expect("non-directory files have parents")
        .to_path_buf()
}
