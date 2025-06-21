// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod dependency_set;
pub mod external;
pub mod git;
pub mod local;
mod onchain;

pub use dependency_set::DependencySet;
use futures::future::join_all;
use tokio::join;
use toml_edit::TomlError;

use std::{collections::BTreeMap, path::PathBuf};

use derive_where::derive_where;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, DeserializeOwned},
};

use tracing::debug;

use crate::{
    errors::{FileHandle, PackageResult},
    flavor::MoveFlavor,
    git::GitTree,
    package::{EnvironmentName, manifest::ManifestResult, paths::PackagePath},
    schema::{
        Address, DefaultDependency, EnvironmentID, LocalDepInfo, LockfileDependencyInfo,
        LockfileGitDepInfo, ManifestDependencyInfo, ManifestGitDependency, OnChainDepInfo, Pin,
        ReplacementDependency, ResolverDependencyInfo,
    },
};

use git::PinnedGitDependency;

/// [Dependency<Combined>]s contain the dependency-type-specific things that users write in their
/// Move.toml files. They are formed by combining the entries from the `[dependencies]` and the
/// `[dep-replacements]` section of the manifest.
pub type Combined = ManifestDependencyInfo;

/// A [Dependency<Resolved>] is like a [Dependency<Combined>] except that it no longer has
/// externally resolved dependencies
pub type Resolved = ResolverDependencyInfo;

/// [Dependency<Pinned>]s are guaranteed to always resolve to the same package source. For example,
/// a git dependendency with a branch or tag revision may change over time (and is thus not
/// pinned), whereas a git dependency with a sha revision is always guaranteed to produce the same
/// files.
#[derive(Clone, Debug)]
pub enum Pinned {
    Local(LocalDepInfo),
    Git(PinnedGitDependency),
    OnChain(OnChainDepInfo),
}

pub type PinnedDependencyInfo = Dependency<Pinned>;

/// Once a dependency has been fetched, it is simply represented by a [PackagePath]
pub type Fetched = PackagePath;

/// [Dependency] wraps information about the location of a dependency (such as the `git` or `local`
/// fields) with additional metadata about how the dependency is used (such as the source file,
/// enviroment overrides, etc).
///
/// At different stages of the pipeline we have different information about the dependency location
/// (e.g. resolved dependencies have no `External` variant, pinned dependencies have a pinned git
/// dependency, etc). The `DepInfo` type encapsulates these invariants.
#[derive(Debug, Clone)]
pub struct Dependency<DepInfo> {
    dep_info: DepInfo,

    /// The environment in the dependency's namespace to use. For example, given
    /// ```toml
    /// dep-replacements.mainnet.foo = { ..., use-environment = "testnet" }
    /// ```
    /// `use_environment` variable would be `testnet`
    use_environment: EnvironmentName,

    /// Was this dependency written with `override = true` in its original manifest?
    is_override: bool,

    /// Does the original manifest override the published address?
    published_at: Option<Address>,

    /// What manifest or lockfile does this dependency come from?
    containing_file: FileHandle,
}

impl<T> Dependency<T> {
    /// Apply `f` to `self.dep_info`, keeping the remaining fields unchanged
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Dependency<U> {
        Dependency {
            dep_info: f(self.dep_info),
            use_environment: self.use_environment,
            is_override: self.is_override,
            published_at: self.published_at,
            containing_file: self.containing_file,
        }
    }

    pub fn use_environment(&self) -> &EnvironmentName {
        &self.use_environment
    }

    pub fn is_override(&self) -> bool {
        self.is_override
    }
}

impl Dependency<Combined> {
    /// Specialize an entry in the `[dependencies]` section, for the environment named
    /// `source_env_name`
    pub fn from_default(
        file: FileHandle,
        source_env_name: EnvironmentName,
        default: DefaultDependency,
    ) -> Self {
        Dependency {
            dep_info: default.dependency_info,
            use_environment: source_env_name,
            is_override: default.is_override,
            published_at: None,
            containing_file: file,
        }
    }

    /// Load from an entry in the `[dep-replacements]` section that has no corresponding entry in
    /// the `[dependencies]` section of the manifest. `source_env_name` refers
    /// to the environment name and ID in the original manifest; it is used as the default
    /// environment for the dependency, but will be overridden if `replacement` specifies
    /// `use-environment` field.
    pub fn from_replacement(
        file: FileHandle,
        source_env_name: EnvironmentName,
        replacement: ReplacementDependency,
    ) -> ManifestResult<Self> {
        let Some(dep) = replacement.dependency else {
            return Err(todo!());
        };

        Ok(Dependency {
            dep_info: dep.dependency_info,
            use_environment: replacement.use_environment.unwrap_or(source_env_name),
            is_override: dep.is_override,
            published_at: replacement.published_at,
            containing_file: file,
        })
    }

    pub fn from_default_with_replacement(
        file: FileHandle,
        source_env_name: EnvironmentName,
        default: DefaultDependency,
        replacement: ReplacementDependency,
    ) -> ManifestResult<Self> {
        let dep = replacement.dependency.unwrap_or(default);

        // TODO: possibly additional compatibility checks here?

        Ok(Dependency {
            dep_info: dep.dependency_info,
            use_environment: replacement.use_environment.unwrap_or(source_env_name),
            is_override: dep.is_override,
            published_at: replacement.published_at,
            containing_file: file,
        })
    }
}

impl Dependency<Pinned> {
    /// Return a dependency representing the root package
    pub fn root_dependency(containing_file: FileHandle, use_environment: EnvironmentName) -> Self {
        Self {
            dep_info: Pinned::Local(todo!()),
            use_environment,
            is_override: true,
            published_at: None,
            containing_file,
        }
    }

    pub fn from_pin(containing_file: FileHandle, env: &EnvironmentName, pin: &Pin) -> Self {
        let dep_info = match &pin.source {
            LockfileDependencyInfo::Local(loc) => Pinned::Local(loc.clone()),
            LockfileDependencyInfo::OnChain(chain) => Pinned::OnChain(chain.clone()),
            LockfileDependencyInfo::Git(git) => todo!(),
        };

        Self {
            dep_info,
            use_environment: pin.use_environment.clone().unwrap_or(env.clone()),
            is_override: false, // TODO
            published_at: None, // TODO
            containing_file,
        }
    }

    pub async fn fetch(&self) -> PackageResult<PathBuf> {
        // TODO: need to actually fetch local dep
        match &self.dep_info {
            Pinned::Git(dep) => Ok(dep.fetch().await?),
            Pinned::Local(dep) => Ok(dep.absolute_path(self.containing_file.path())),
            Pinned::OnChain(dep) => todo!(),
        }
    }

    /// Return the absolute path to the directory that this package would be fetched into, without
    /// actually fetching it
    pub fn unfetched_path(&self) -> PathBuf {
        match &self.dep_info {
            Pinned::Git(dep) => dep.unfetched_path(),
            Pinned::Local(dep) => dep.absolute_path(self.containing_file.path()),
            Pinned::OnChain(dep) => todo!(),
        }
    }
}

impl From<Dependency<Pinned>> for LockfileDependencyInfo {
    fn from(value: Dependency<Pinned>) -> Self {
        match value.dep_info {
            Pinned::Local(loc) => Self::Local(loc),
            Pinned::Git(git) => Self::Git(LockfileGitDepInfo {
                repo: git.inner().repo_url().to_string(),
                rev: git.inner().sha().clone(),
                path: git.inner().path_in_repo().to_path_buf(),
            }),
            Pinned::OnChain(on_chain) => Self::OnChain(on_chain),
        }
    }
}

/// Replace all dependencies with their pinned versions.
pub async fn pin<F: MoveFlavor>(
    mut deps: DependencySet<Dependency<Combined>>,
    envs: &BTreeMap<EnvironmentName, EnvironmentID>,
) -> PackageResult<DependencySet<Dependency<Pinned>>> {
    use Pinned as P;

    // resolution
    let deps = Dependency::resolve(deps, envs).await?;
    debug!("done resolving");

    // pinning
    let mut result: DependencySet<Dependency<P>> = DependencySet::new();
    for (env, pkg, dep) in deps.into_iter() {
        let transformed = match &dep.dep_info {
            ResolverDependencyInfo::Local(loc) => P::Local(loc.clone()),
            ResolverDependencyInfo::Git(git) => P::Git(git.pin().await?),
            ResolverDependencyInfo::OnChain(chain) => P::OnChain(todo!()),
        };
        result.insert(env, pkg, dep.map(|_| transformed));
    }

    Ok(result)
}

/// For each environment, if none of the implicit dependencies are present in [deps] (or the
/// default environment), then they are all added.
// TODO: what's the notion of identity used here? I think it has to be by name
fn add_implicit_deps<F: MoveFlavor>(
    flavor: &F,
    deps: &mut DependencySet<Dependency<Pinned>>,
) -> PackageResult<()> {
    todo!()
}

/// Fetch and ensure that all dependencies are stored locally and return the paths to their
/// contents. The returned map is guaranteed to have the same keys as [deps].
pub async fn fetch<F: MoveFlavor>(
    flavor: &F,
    deps: DependencySet<Dependency<Pinned>>,
) -> PackageResult<DependencySet<Dependency<Fetched>>> {
    use DependencySet as DS;
    use Pinned as P;

    let mut result = DependencySet::new();
    for (env, pkg, dep) in deps.into_iter() {
        let fetched = PackagePath::new(dep.fetch().await?)?;
        result.insert(env, pkg, dep.map(|_| fetched))
    }
    Ok(result)
}

// TODO: unit tests
#[cfg(test)]
mod tests {}
