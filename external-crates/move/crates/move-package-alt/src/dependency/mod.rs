// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod dependency_set;

mod fetch;
mod parse;
mod resolve;

pub use dependency_set::DependencySet;

use std::{collections::BTreeMap, path::PathBuf};

use derive_where::derive_where;
use serde::{Deserialize, Deserializer, Serialize, de};

use tracing::debug;

use crate::{
    errors::{FileHandle, PackageResult},
    flavor::MoveFlavor,
    git::GitTree,
    package::paths::PackagePath,
    schema::{
        Address, EnvironmentID, EnvironmentName, LocalDependency, LockfileDependencyInfo,
        ManifestDependencyInfo, OnChainDependency, ResolverDependencyInfo,
    },
};

pub type Parsed = ManifestDependencyInfo;

pub type Resolved = ResolverDependencyInfo;

pub enum Pinned {
    Local(LocalDependency),
    Git(GitTree),
    OnChain(OnChainDependency),
}

pub type Fetched = PackagePath;

/// [Dependency] wraps information about the location of a dependency (such as the `git` or `local`
/// fields) with additional metadata about how the dependency is used (such as the source file,
/// enviroment overrides, etc).
#[derive(Debug)]
pub struct Dependency<DepInfo> {
    dep_info: DepInfo,

    /// The environment in the dependency's namespace to use. For example, given
    /// ```toml
    /// dep-replacements.mainnet.foo = { ..., use-environment = "testnet" }
    /// ```
    /// `use_environment` variable would be `testnet`
    use_environment: EnvironmentName,

    /// The local environment ID for this dependency. For example, given
    /// ```toml
    /// environments.mainnet = "0x1234"
    /// dep-replacements.mainnet.foo = { ..., use-environment = "testnet" }
    /// ```
    /// `source_environment` would be "0x1234"
    source_environment: EnvironmentID,

    /// Was this dependency written with `override = true` in its original manifest?
    is_override: bool,

    /// Does the original manifest override the published address?
    published_at: Option<Address>,

    /// What manifest or lockfile does this dependency come from?
    containing_file: FileHandle,
}

impl<T> Dependency<T> {
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Dependency<U> {
        Dependency {
            dep_info: f(self.dep_info),
            use_environment: self.use_environment,
            source_environment: self.source_environment,
            is_override: self.is_override,
            published_at: self.published_at,
            containing_file: self.containing_file,
        }
    }
}

/*

// TODO (potential refactor): consider using objects for manifest dependencies (i.e. `Box<dyn UnpinnedDependency>`).
//      part of the complexity here would be deserialization - probably need a flavor-specific
//      function that converts a toml value to a Box<dyn UnpinnedDependency>
//
//      resolution would also be interesting because of batch resolution. Would probably need a
//      trait method to return a resolver object, and then a method on the resolver object to
//      resolve a bunch of dependencies (resolvers could implement Eq)
//

/// Pinned dependencies are guaranteed to always resolve to the same package source. For example,
/// a git dependendency with a branch or tag revision may change over time (and is thus not
/// pinned), whereas a git dependency with a sha revision is always guaranteed to produce the same
/// files.
///
/// Local dependencies are a somewhat special case here - we want to pin them as local deps during
/// development, because the developer would expect to use the latest code without having to
/// explicitly repin, but we need to convert them to persistent dependencies when we publish since
/// we want to retain that information for source verification.
// Note: there is a custom Deserializer for this type; be sure to update it if you modify this
#[derive(Debug, Serialize)]
#[derive_where(Clone, PartialEq)]
#[serde(untagged)]
#[serde(bound = "")]
pub enum PinnedDependencyInfo<F: MoveFlavor + ?Sized> {
    Git(PinnedGitDependency),
    Local(LocalDependency),
    FlavorSpecific(F::FlavorDependency<Pinned>),
}

impl<F: MoveFlavor> PinnedDependencyInfo<F> {
    /// Return a dependency representing the root package
    pub fn root_dependency(path: &PackagePath) -> Self {
        Self::Local(LocalDependency::root_dependency(path))
    }

    pub async fn fetch(&self) -> PackageResult<PathBuf> {
        match self {
            PinnedDependencyInfo::Git(dep) => dep.fetch().await,
            PinnedDependencyInfo::Local(dep) => Ok(dep.unfetched_path().clone()),
            PinnedDependencyInfo::FlavorSpecific(dep) => todo!(),
        }
    }

    /// Return the absolute path to the directory that this package would be fetched into, without
    /// actually fetching it
    pub fn unfetched_path(&self) -> PathBuf {
        match self {
            PinnedDependencyInfo::Git(dep) => {
                format_repo_to_fs_path(&dep.repo, &dep.rev, Some(dep.path.clone()))
            }
            PinnedDependencyInfo::Local(dep) => dep.unfetched_path(),
            PinnedDependencyInfo::FlavorSpecific(dep) => todo!(),
        }
    }
}

// TODO: these should be moved down.
// UNPINNED
// PINNED

impl<'de, F> Deserialize<'de> for PinnedDependencyInfo<F>
where
    F: MoveFlavor + ?Sized,
    F::FlavorDependency<Pinned>: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = toml::value::Value::deserialize(deserializer)?;

        if let Some(tbl) = data.as_table() {
            if tbl.is_empty() {
                return Err(de::Error::custom("Dependency has no fields"));
            }
            if tbl.contains_key("git") {
                let dep = PinnedGitDependency::deserialize(data).map_err(de::Error::custom)?;
                Ok(PinnedDependencyInfo::Git(dep))
            } else if tbl.contains_key("local") {
                let dep = LocalDependency::deserialize(data).map_err(de::Error::custom)?;
                Ok(PinnedDependencyInfo::Local(dep))
            } else {
                let dep = toml::Value::try_from(data)
                    .map_err(de::Error::custom)?
                    .try_into()
                    .map_err(|_| de::Error::custom("invalid dependency format"))?;

                Ok(PinnedDependencyInfo::FlavorSpecific(dep))
            }
        } else {
            Err(de::Error::custom("Manifest dependency must be a table"))
        }
    }
}

/// For each environment, if none of the implicit dependencies are present in [deps] (or the
/// default environment), then they are all added.
// TODO: what's the notion of identity used here?
fn add_implicit_deps<F: MoveFlavor>(
    flavor: &F,
    deps: &mut DependencySet<PinnedDependencyInfo<F>>,
) -> PackageResult<()> {
    todo!()
}

/// Fetch and ensure that all dependencies are stored locally and return the paths to their
/// contents. The returned map is guaranteed to have the same keys as [deps].
pub async fn fetch<F: MoveFlavor>(
    flavor: &F,
    deps: DependencySet<PinnedDependencyInfo<F>>,
) -> PackageResult<DependencySet<PathBuf>> {
    use DependencySet as DS;
    use PinnedDependencyInfo as P;

    let mut gits = DS::new();
    let mut locs = DS::new();
    let mut flav = DS::new();

    for (env, package_name, dep) in deps.into_iter() {
        match dep {
            P::Git(info) => gits.insert(env, package_name, info),
            P::Local(info) => locs.insert(env, package_name, info),
            P::FlavorSpecific(info) => flav.insert(env, package_name, info),
        }
    }

    let mut git_paths = DS::new();
    for (env, package, dep) in gits {
        let path = dep.fetch().await?;
        git_paths.insert(env, package, path);
    }

    let mut loc_paths = DS::new();
    for (env, package, dep) in locs {
        loc_paths.insert(env, package, dep.unfetched_path().clone());
    }

    let flav_deps_path = flavor.fetch(flav)?;

    Ok(DS::merge([git_paths, loc_paths, flav_deps_path]))
}

// TODO: unit tests
#[cfg(test)]
mod tests {}
*/
