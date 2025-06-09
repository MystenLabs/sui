use std::path::PathBuf;

use thiserror::Error;

use crate::{
    git::{GitRepo, errors::GitError},
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
            LockfileDependencyInfo::Local(local) => Ok(PackagePath::new(local_dir(&self, local))?),
            LockfileDependencyInfo::OnChain(onchain) => todo!(),
            LockfileDependencyInfo::Git(git) => git.fetch().await,
        }
    }

    /// Return the path that would be returned by `fetch`, without actually fetching
    pub fn unfetched_path(&self) -> PathBuf {
        match &self.dep_info {
            LockfileDependencyInfo::Local(local) => local_dir(&self, local),
            LockfileDependencyInfo::OnChain(onchain) => todo!(),
            LockfileDependencyInfo::Git(git) => git_dir(git),
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

/// Returns the absolute path to `dep`
fn git_dir(dep: &PinnedGitDependency) -> PathBuf {
    todo!()
}

/// Returns the absolute path to `dep`
fn onchain_dir(dep: &OnChainDependency) -> PathBuf {
    todo!()
}

/// Fetch the given git dependency and return the path to the checked out repo
pub async fn git_fetch(dep: PinnedGitDependency) -> FetchResult<PackagePath> {
    let git_repo = GitRepo::from(dep);
    Ok(PackagePath::new(git_repo.fetch().await?)?)
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

impl From<PinnedGitDependency> for GitRepo {
    fn from(dep: PinnedGitDependency) -> Self {
        GitRepo::new(dep.repo, Some(dep.rev.into()), dep.path)
    }
}
