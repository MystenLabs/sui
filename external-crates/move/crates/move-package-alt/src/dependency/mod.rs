// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod dependency_set;
pub mod external;
pub mod git;
pub mod local;
mod onchain;

pub use dependency_set::DependencySet;
use onchain::OnChainDependency;
use toml_edit::TomlError;

use std::{collections::BTreeMap, path::PathBuf};

use derive_where::derive_where;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, DeserializeOwned},
};

use tracing::debug;

use crate::{
    errors::PackageResult,
    flavor::MoveFlavor,
    package::{EnvironmentName, paths::PackagePath},
};

use external::ExternalDependency;
use git::{PinnedGitDependency, UnpinnedGitDependency};
use local::LocalDependency;

// TODO (potential refactor): consider using objects for manifest dependencies (i.e. `Box<dyn UnpinnedDependency>`).
//      part of the complexity here would be deserialization - probably need a flavor-specific
//      function that converts a toml value to a Box<dyn UnpinnedDependency>
//
//      resolution would also be interesting because of batch resolution. Would probably need a
//      trait method to return a resolver object, and then a method on the resolver object to
//      resolve a bunch of dependencies (resolvers could implement Eq)
//

/// Phantom type to represent pinned dependencies (see [PinnedDependency])
#[derive(Debug, PartialEq, Eq)]
pub struct Pinned;

/// Phantom type to represent unpinned dependencies (see [UnpinnedDependencyInfo])
#[derive(Debug, PartialEq)]
pub struct Unpinned;

/// [UnpinnedDependencyInfo]s contain the dependency-type-specific things that users write in their
/// Move.toml files in the `dependencies` section.
///
/// TODO: this paragraph will change with upcoming design changes:
/// There are additional general fields in the manifest format (like `override` or `rename-from`)
/// that are not part of the UnpinnedDependencyInfo. We separate these partly because these things
/// are not serialized to the Lock file. See [crate::package::manifest] for the full representation
/// of an entry in the `dependencies` table.
///
// Note: there is a custom Deserializer for this type; be sure to update it if you modify this
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum UnpinnedDependencyInfo {
    Git(UnpinnedGitDependency),
    External(ExternalDependency),
    Local(LocalDependency),
    OnChain(OnChainDependency),
}

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
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(untagged)]
#[serde(bound = "")]
pub enum PinnedDependencyInfo {
    Git(PinnedGitDependency),
    Local(LocalDependency),
    OnChain(OnChainDependency),
}

impl PinnedDependencyInfo {
    /// Return a dependency representing the root package
    pub fn root_dependency(path: &PackagePath) -> Self {
        Self::Local(LocalDependency::root_dependency(path))
    }

    pub async fn fetch(&self) -> PackageResult<PathBuf> {
        match self {
            PinnedDependencyInfo::Git(dep) => Ok(dep.fetch().await?),
            PinnedDependencyInfo::Local(dep) => Ok(dep.unfetched_path().clone()),
            PinnedDependencyInfo::OnChain(dep) => todo!(),
        }
    }

    /// Return the absolute path to the directory that this package would be fetched into, without
    /// actually fetching it
    pub fn unfetched_path(&self) -> PathBuf {
        match self {
            PinnedDependencyInfo::Git(dep) => dep.unfetched_path(),
            PinnedDependencyInfo::Local(dep) => dep.unfetched_path(),
            PinnedDependencyInfo::OnChain(dep) => todo!(),
        }
    }

    pub fn as_git_dep(&self) -> Option<PinnedGitDependency> {
        if let PinnedDependencyInfo::Git(dep) = self {
            Some(dep.clone())
        } else {
            None
        }
    }
}

// TODO: these should be moved down.
// UNPINNED
impl<'de> Deserialize<'de> for UnpinnedDependencyInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = toml::value::Value::deserialize(deserializer)?;

        if let Some(tbl) = data.as_table() {
            if tbl.is_empty() {
                return Err(de::Error::custom("dependency has no fields"));
            }
            if tbl.contains_key("git") {
                let dep = UnpinnedGitDependency::deserialize(data).map_err(de::Error::custom)?;
                Ok(UnpinnedDependencyInfo::Git(dep))
            } else if tbl.contains_key("r") {
                let dep = ExternalDependency::deserialize(data).map_err(de::Error::custom)?;
                Ok(UnpinnedDependencyInfo::External(dep))
            } else if tbl.contains_key("local") {
                let dep = LocalDependency::deserialize(data).map_err(de::Error::custom)?;
                Ok(UnpinnedDependencyInfo::Local(dep))
            } else if tbl.contains_key("on-chain") {
                let dep = OnChainDependency::deserialize(data).map_err(de::Error::custom)?;
                Ok(UnpinnedDependencyInfo::OnChain(dep))
            } else {
                Err(de::Error::custom(
                    "Invalid dependency; dependencies must have exactly one of the following fields: `git`, `r`, `local`, or `on-chain`",
                ))
            }
        } else {
            Err(de::Error::custom("Manifest dependency must be a table"))
        }
    }
}

// PINNED

impl<'de> Deserialize<'de> for PinnedDependencyInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = toml::value::Value::deserialize(deserializer)?;

        // TODO: check for more than one of these
        // TODO: can this be done with a macro or higher order function?
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
            } else if tbl.contains_key("on-chain") {
                let dep = OnChainDependency::deserialize(data).map_err(de::Error::custom)?;
                Ok(PinnedDependencyInfo::OnChain(dep))
            } else {
                Err(de::Error::custom(
                    "Invalid dependency; dependencies must have exactly one of the following fields: `git`, `local`, or `on-chain`",
                ))
            }
        } else {
            Err(de::Error::custom("Manifest dependency must be a table"))
        }
    }
}

/// Split up deps into kinds. The union of the output sets is the same as [deps]
#[allow(clippy::type_complexity)]
fn split(
    deps: &DependencySet<UnpinnedDependencyInfo>,
) -> (
    DependencySet<UnpinnedGitDependency>,
    DependencySet<ExternalDependency>,
    DependencySet<LocalDependency>,
    DependencySet<OnChainDependency>,
) {
    use DependencySet as DS;
    use UnpinnedDependencyInfo as U;

    let mut gits = DS::new();
    let mut exts = DS::new();
    let mut locs = DS::new();
    let mut chain = DS::new();

    for (env, package_name, dep) in deps.clone().into_iter() {
        match dep {
            U::Git(info) => gits.insert(env, package_name, info),
            U::External(info) => exts.insert(env, package_name, info),
            U::Local(info) => locs.insert(env, package_name, info),
            U::OnChain(info) => chain.insert(env, package_name, info),
        }
    }

    (gits, exts, locs, chain)
}

/// Replace all dependencies with their pinned versions. The returned set may have a different set
/// of keys than the input, for example if new implicit dependencies are added or if external
/// resolvers resolve default deps to dep-replacements, or if dep-replacements are identical to the
/// default deps.
pub async fn pin<F: MoveFlavor>(
    mut deps: DependencySet<UnpinnedDependencyInfo>,
    envs: &BTreeMap<EnvironmentName, F::EnvironmentID>,
) -> PackageResult<DependencySet<PinnedDependencyInfo>> {
    use PinnedDependencyInfo as P;

    // resolution
    ExternalDependency::resolve(&mut deps, envs).await?;
    debug!("done resolving");

    // pinning
    let (mut gits, exts, mut locs, mut chain) = split(&deps);
    assert!(exts.is_empty(), "resolve must remove external dependencies");

    let pinned_gits: DependencySet<P> = UnpinnedGitDependency::pin(gits)
        .await?
        .into_iter()
        .map(|(env, package, dep)| (env, package, P::Git(dep)))
        .collect();

    let pinned_locs = locs
        .into_iter()
        .map(|(env, package, dep)| (env, package, P::Local(dep)))
        .collect();

    let pinned_flav = chain
        .into_iter()
        .map(|(env, package, dep)| (env, package, P::OnChain(dep)))
        .collect();

    Ok(DependencySet::merge([
        pinned_gits,
        pinned_locs,
        pinned_flav,
    ]))
}

/// For each environment, if none of the implicit dependencies are present in [deps] (or the
/// default environment), then they are all added.
// TODO: what's the notion of identity used here?
fn add_implicit_deps<F: MoveFlavor>(
    flavor: &F,
    deps: &mut DependencySet<PinnedDependencyInfo>,
) -> PackageResult<()> {
    todo!()
}

/// Fetch and ensure that all dependencies are stored locally and return the paths to their
/// contents. The returned map is guaranteed to have the same keys as [deps].
pub async fn fetch<F: MoveFlavor>(
    flavor: &F,
    deps: DependencySet<PinnedDependencyInfo>,
) -> PackageResult<DependencySet<PathBuf>> {
    use DependencySet as DS;
    use PinnedDependencyInfo as P;

    let mut gits = DS::new();
    let mut locs = DS::new();
    let mut chain = DS::new();

    for (env, package_name, dep) in deps.into_iter() {
        match dep {
            P::Git(info) => gits.insert(env, package_name, info),
            P::Local(info) => locs.insert(env, package_name, info),
            P::OnChain(info) => chain.insert(env, package_name, info),
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

    let mut chain_paths = DS::new();
    for (env, package, dep) in chain {
        chain_paths.insert(env, package, dep.unfetched_path().clone());
    }

    Ok(DS::merge([git_paths, loc_paths, chain_paths]))
}

// TODO: unit tests
#[cfg(test)]
mod tests {}
