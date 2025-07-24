// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

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
    pub(crate) inner: GitTree,
}

#[derive(Debug, Clone)]
pub struct PinnedDependencyInfo(pub(super) Dependency<Pinned>);

impl PinnedDependencyInfo {
    /// Return a dependency representing the root package
    pub fn root_dependency(containing_file: FileHandle, use_environment: EnvironmentName) -> Self {
        PinnedDependencyInfo(Dependency {
            dep_info: Pinned::Local(LocalDepInfo { local: ".".into() }),
            use_environment,
            is_override: true,
            addresses: None,
            containing_file,
            rename_from: None,
        })
    }

    /// Replace all dependencies in `deps` with their pinned versions. This requires first
    /// resolving external dependencies and then pinning all dependencies.
    ///
    /// External dependencies are resolved in the environment with id `environment_id`
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

    /// Create a pinned dependency from a pin in a lockfile. This involves attaching the context of
    /// the file it is contained in (`containing_file`) and the environment it is defined in
    /// (`env`).
    ///
    /// The returned dependency has the `override` field set, since we assume dependencies are
    /// only pinned to the lockfile after the linkage checks have been performed.
    ///
    /// We do not set the `rename-from` field, since when we are creating the pinned dependency we
    /// don't yet know what the rename-from field  should be. The caller is responsible for calling
    /// [Self::with_rename_from] if they need to establish the rename-from check invariant.
    pub fn from_pin(containing_file: FileHandle, env: &EnvironmentName, pin: &Pin) -> Self {
        let dep_info = match &pin.source {
            LockfileDependencyInfo::Local(loc) => Pinned::Local(loc.clone()),
            LockfileDependencyInfo::OnChain(chain) => Pinned::OnChain(chain.clone()),
            LockfileDependencyInfo::Git(git) => todo!(),
        };

        PinnedDependencyInfo(Dependency {
            dep_info,
            use_environment: pin.use_environment.clone().unwrap_or(env.clone()),
            is_override: true,
            addresses: pin.address_override.clone(),
            rename_from: None,
            containing_file,
        })
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

    pub fn with_rename_from(mut self, name: PackageName) -> Self {
        self.0.rename_from = Some(name);
        self
    }

    pub fn with_override(mut self, is_override: bool) -> Self {
        self.0.is_override = is_override;
        self
    }
}

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
