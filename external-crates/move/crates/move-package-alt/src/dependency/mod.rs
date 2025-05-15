// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fmt::{self, Debug},
    marker::PhantomData,
    path::PathBuf,
    process::{Command, Stdio},
};

use derive_where::derive_where;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};

use tracing::debug;

use crate::{
    errors::{GitError, PackageError, PackageResult, ResolverError},
    flavor::MoveFlavor,
    package::{EnvironmentName, PackageName},
};

use external::ExternalDependency;
use git::{GitRepo, PinnedGitDependency, UnpinnedGitDependency};
use local::LocalDependency;

mod dependency_set;
// TODO: this shouldn't be pub; need to move resolver error into resolver module
pub mod external;
mod git;
mod local;

pub use dependency_set::DependencySet;

// TODO (potential refactor): consider using objects for manifest dependencies (i.e. `Box<dyn UnpinnedDependency>`).
//      part of the complexity here would be deserialization - probably need a flavor-specific
//      function that converts a toml value to a Box<dyn UnpinnedDependency>
//
//      resolution would also be interesting because of batch resolution. Would probably need a
//      trait method to return a resolver object, and then a method on the resolver object to
//      resolve a bunch of dependencies (resolvers could implement Eq)
//
// TODO: maybe rename ManifestDependencyInfo to UnpinnedDependency

/// Phantom type to represent pinned dependencies (see [PinnedDependency])
#[derive(Debug, PartialEq, Eq)]
pub struct Pinned;

/// Phantom type to represent unpinned dependencies (see [ManifestDependencyInfo])
#[derive(Debug, PartialEq)]
pub struct Unpinned;

/// [ManifestDependencyInfo]s contain the dependency-type-specific things that users write in their
/// Move.toml files in the `dependencies` section.
///
/// TODO: this paragraph will change with upcoming design changes:
/// There are additional general fields in the manifest format (like `override` or `rename-from`)
/// that are not part of the ManifestDependencyInfo. We separate these partly because these things
/// are not serialized to the Lock file. See [crate::package::manifest] for the full representation
/// of an entry in the `dependencies` table.
///
// Note: there is a custom Deserializer for this type; be sure to update it if you modify this
#[derive(Debug, Serialize)]
#[derive_where(Clone, PartialEq)]
#[serde(untagged)]
pub enum ManifestDependencyInfo<F: MoveFlavor + ?Sized> {
    Git(UnpinnedGitDependency),
    External(ExternalDependency),
    Local(LocalDependency),
    FlavorSpecific(F::FlavorDependency<Unpinned>),
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
#[derive(Debug, Serialize)]
#[derive_where(Clone)]
#[serde(untagged)]
pub enum PinnedDependencyInfo<F: MoveFlavor + ?Sized> {
    Git(PinnedGitDependency),
    Local(LocalDependency),
    FlavorSpecific(F::FlavorDependency<Pinned>),
}

// TODO: these should be moved down.

// UNPINNED
impl<'de, F> Deserialize<'de> for ManifestDependencyInfo<F>
where
    F: MoveFlavor + ?Sized,
    F::FlavorDependency<Unpinned>: Deserialize<'de>,
{
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
                Ok(ManifestDependencyInfo::Git(dep))
            } else if tbl.contains_key("r") {
                let dep = ExternalDependency::deserialize(data).map_err(de::Error::custom)?;
                Ok(ManifestDependencyInfo::External(dep))
            } else if tbl.contains_key("local") {
                let dep = LocalDependency::deserialize(data).map_err(de::Error::custom)?;
                Ok(ManifestDependencyInfo::Local(dep))
            } else {
                // TODO: maybe this could be prettier. The problem is that we don't know how to
                // tell if something is a flavor dependency. One option might be to add a method to
                // [MoveFlavor] that gives the list of flavor dependency tags. Another approach
                // worth considering is removing flavor dependencies entirely and just having
                // on-chain dependencies (with the flavor being used to resolve them).
                let dep = toml::Value::try_from(data)
                    .map_err(de::Error::custom)?
                    .try_into()
                    .map_err(|_| de::Error::custom("invalid dependency format"))?;
                Ok(ManifestDependencyInfo::FlavorSpecific(dep))
            }
        } else {
            Err(de::Error::custom("Manifest dependency must be a table"))
        }
    }
}

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

/// Split up deps into kinds. The union of the output sets is the same as [deps]
#[allow(clippy::type_complexity)]
fn split<F: MoveFlavor>(
    deps: &DependencySet<ManifestDependencyInfo<F>>,
) -> (
    DependencySet<UnpinnedGitDependency>,
    DependencySet<ExternalDependency>,
    DependencySet<LocalDependency>,
    DependencySet<F::FlavorDependency<Unpinned>>,
) {
    let mut gits = DependencySet::new();
    let mut exts = DependencySet::new();
    let mut locs = DependencySet::new();
    let mut flav = DependencySet::new();

    for (env, package_name, dep) in deps.clone().into_iter() {
        match dep {
            ManifestDependencyInfo::Git(info) => gits.insert(env, package_name, info),
            ManifestDependencyInfo::External(info) => exts.insert(env, package_name, info),
            ManifestDependencyInfo::Local(info) => locs.insert(env, package_name, info),
            ManifestDependencyInfo::FlavorSpecific(info) => flav.insert(env, package_name, info),
        }
    }

    (gits, exts, locs, flav)
}

// TODO: this will change with upcoming design changes:
/// Replace all dependencies with their pinned versions. The returned set may have a different set
/// of keys than the input, for example if new implicit dependencies are added or if external
/// resolvers resolve default deps to dep-replacements, or if dep-replacements are identical to the
/// default deps.
pub async fn pin<F: MoveFlavor>(
    flavor: &F,
    mut deps: DependencySet<ManifestDependencyInfo<F>>,
    envs: &BTreeMap<EnvironmentName, F::EnvironmentID>,
) -> PackageResult<DependencySet<PinnedDependencyInfo<F>>> {
    // resolution
    ExternalDependency::resolve(&mut deps, envs).await?;
    debug!("done resolving");

    // pinning
    let (mut gits, exts, mut locs, mut flav) = split(&deps);
    assert!(exts.is_empty(), "resolve must remove external dependencies");

    let pinned_gits: DependencySet<PinnedDependencyInfo<F>> = UnpinnedGitDependency::pin(gits)
        .await?
        .into_iter()
        .map(|(env, package, dep)| (env, package, PinnedDependencyInfo::Git::<F>(dep)))
        .collect();

    let pinned_locs = locs
        .into_iter()
        .map(|(env, package, dep)| (env, package, PinnedDependencyInfo::Local::<F>(dep)))
        .collect();

    let pinned_flav = flavor
        .pin(flav)?
        .into_iter()
        .map(|(env, package, dep)| {
            (
                env,
                package,
                PinnedDependencyInfo::FlavorSpecific::<F>(dep.clone()),
            )
        })
        .collect();

    Ok(DependencySet::merge([
        pinned_gits,
        pinned_locs,
        pinned_flav,
    ]))
}

// TODO: this will change with the upcoming design changes:
/// For each environment, if none of the implicit dependencies are present in [deps] (or the
/// default environment), then they are all added.
// TODO: what's the notion of identity used here?
fn add_implicit_deps<F: MoveFlavor>(
    flavor: &F,
    deps: &mut DependencySet<PinnedDependencyInfo<F>>,
) -> PackageResult<()> {
    todo!()
}

/// Ensure that all dependencies are stored locally and return the paths to their contents. The
/// returned map is guaranteed to have the same keys as [deps].
fn fetch<F: MoveFlavor>(
    deps: DependencySet<PinnedDependencyInfo<F>>,
) -> PackageResult<DependencySet<PathBuf>> {
    // TODO: check if dependency is a Move project.

    todo!()
}

// TODO: unit tests
#[cfg(test)]
mod tests {}
