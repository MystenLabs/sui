// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use path_clean::PathClean;
use tracing::debug;

use crate::{
    dependency::ResolvedDependency,
    errors::{FileHandle, PackageResult},
    flavor::MoveFlavor,
    git::{GitCache, GitTree},
    schema::{
        EnvironmentID, EnvironmentName, LocalDepInfo, LockfileDependencyInfo, LockfileGitDepInfo,
        ManifestGitDependency, OnChainDepInfo, PackageName, Pin, ResolverDependencyInfo,
    },
};

use super::{CombinedDependency, Dependency};

/// [Dependency<Pinned>]s are guaranteed to always resolve to the same package source. For example,
/// a git dependendency with a branch or tag revision may change over time (and is thus not
/// pinned), whereas a git dependency with a sha revision is always guaranteed to produce the same
/// files.
#[derive(Clone, Debug)]
pub(super) enum Pinned {
    Local(LocalDepInfo),
    Git(PinnedGitDependency),
    OnChain(OnChainDepInfo),
}

#[derive(Clone, Debug)]
pub struct PinnedGitDependency {
    inner: GitTree,
}

#[derive(Debug, Clone)]
pub struct PinnedDependencyInfo(pub(super) Dependency<Pinned>);

impl PinnedDependencyInfo {
    /// Return a dependency representing the root package
    pub fn root_dependency(containing_file: FileHandle, use_environment: EnvironmentName) -> Self {
        PinnedDependencyInfo(Dependency {
            dep_info: Pinned::Local(todo!()),
            use_environment,
            is_override: true,
            addresses: None,
            containing_file,
            rename_from: None,
        })
    }

    pub fn from_pin(containing_file: FileHandle, env: &EnvironmentName, pin: &Pin) -> Self {
        let dep_info = match &pin.source {
            LockfileDependencyInfo::Local(loc) => Pinned::Local(loc.clone()),
            LockfileDependencyInfo::OnChain(chain) => Pinned::OnChain(chain.clone()),
            LockfileDependencyInfo::Git(git) => todo!(),
        };

        PinnedDependencyInfo(Dependency {
            dep_info,
            use_environment: pin.use_environment.clone().unwrap_or(env.clone()),
            is_override: false, // TODO
            addresses: None,    // TODO
            rename_from: None,  // TODO
            containing_file,
        })
    }

    // TODO: replace PackageResult here
    // TODO: move this to fetch.rs
    pub async fn fetch(&self) -> PackageResult<PathBuf> {
        // TODO: need to actually fetch local dep
        match &self.0.dep_info {
            Pinned::Git(dep) => Ok(dep.fetch().await?),
            Pinned::Local(dep) => Ok(dep.absolute_path(self.0.containing_file.path())),
            Pinned::OnChain(_) => todo!(),
        }
    }

    /// Return the absolute path to the directory that this package would be fetched into, without
    /// actually fetching it
    pub fn unfetched_path(&self) -> PathBuf {
        match &self.0.dep_info {
            Pinned::Git(dep) => dep.unfetched_path(),
            Pinned::Local(dep) => dep.absolute_path(self.0.containing_file.path()),
            Pinned::OnChain(dep) => todo!(),
        }
    }

    pub fn use_environment(&self) -> &EnvironmentName {
        self.0.use_environment()
    }

    pub fn is_override(&self) -> bool {
        self.0.is_override
    }

    pub fn rename_from(&self) -> &Option<PackageName> {
        self.0.rename_from()
    }
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
}

// TODO: what is this here for?
impl ManifestGitDependency {
    /// Replace the commit-ish [self.rev] with a commit (i.e. a SHA). Requires fetching the git
    /// repository
    pub async fn pin(&self) -> PackageResult<PinnedGitDependency> {
        let cache = GitCache::new();
        let ManifestGitDependency { repo, rev, subdir } = self.clone();
        let tree = cache.resolve_to_tree(&repo, &rev, Some(subdir)).await?;
        Ok(PinnedGitDependency { inner: tree })
    }
}

impl From<PinnedDependencyInfo> for LockfileDependencyInfo {
    fn from(value: PinnedDependencyInfo) -> Self {
        match value.0.dep_info {
            Pinned::Local(loc) => Self::Local(loc),
            Pinned::Git(git) => Self::Git(LockfileGitDepInfo {
                repo: git.inner.repo_url().to_string(),
                rev: git.inner.sha().clone(),
                path: git.inner.path_in_repo().to_path_buf(),
            }),
            Pinned::OnChain(on_chain) => Self::OnChain(on_chain),
        }
    }
}

/// Replace all dependencies with their pinned versions.
pub async fn pin<F: MoveFlavor>(
    deps: BTreeMap<PackageName, CombinedDependency>,
    environment_id: &EnvironmentID,
) -> PackageResult<BTreeMap<PackageName, PinnedDependencyInfo>> {
    use Pinned as P;

    // resolution
    let deps = ResolvedDependency::resolve(deps, environment_id).await?;
    debug!("done resolving");

    // pinning
    let mut result: BTreeMap<PackageName, PinnedDependencyInfo> = BTreeMap::new();
    for (pkg, dep) in deps.into_iter() {
        let transformed = match &dep.0.dep_info {
            ResolverDependencyInfo::Local(loc) => P::Local(loc.clone()),
            ResolverDependencyInfo::Git(git) => P::Git(git.pin().await?),
            ResolverDependencyInfo::OnChain(_) => P::OnChain(todo!()),
        };
        result.insert(pkg, PinnedDependencyInfo(dep.0.map(|_| transformed)));
    }

    Ok(result)
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
