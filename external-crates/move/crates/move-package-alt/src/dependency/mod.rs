// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod external;
mod git;
mod local;

mod dependency_set;

pub use dependency_set::DependencySet;

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    errors::PackageResult,
    flavor::MoveFlavor,
    package::{EnvironmentName, PackageName},
};

use derive_where::derive_where;
use external::ExternalDependency;
use git::GitDependency;
use local::LocalDependency;

/// Phantom type to represent pinned dependencies (see [PinnedDependency])
pub struct Pinned;

/// Phantom type to represent unpinned dependencies (see [ManifestDependencyInfo])
pub struct Unpinned;

/// [ManifestDependencyInfo]s contain the dependency-type-specific things that users write in their
/// Move.toml files in the `dependencies` section.
///
/// There are additional general fields in the manifest format (like `override` or `rename-from`)
/// that are not part of the ManifestDependencyInfo. We separate these partly because these things
/// are not serialized to the Lock file. See [crate::package::manifest] for the full representation
/// of an entry in the `dependencies` table.
#[derive(Serialize, Deserialize)]
#[derive_where(Clone)]
pub enum ManifestDependencyInfo<F: MoveFlavor> {
    Git(GitDependency<Unpinned>),
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
#[derive(Serialize, Deserialize)]
#[derive_where(Clone)]
pub enum PinnedDependencyInfo<F: MoveFlavor + ?Sized> {
    Git(GitDependency<Pinned>),
    Local(LocalDependency),
    FlavorSpecific(F::FlavorDependency<Pinned>),
}

/// Split up deps into kinds. The union of the output sets is the same as [deps]
fn split<F: MoveFlavor>(
    deps: &DependencySet<ManifestDependencyInfo<F>>,
) -> (
    DependencySet<GitDependency<Unpinned>>,
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

/// Replace all dependencies with their pinned versions. The returned map is guaranteed to have the
/// same keys as [deps].
fn pin<F: MoveFlavor>(
    flavor: &F,
    deps: &DependencySet<ManifestDependencyInfo<F>>, // TODO: maybe take by value?
) -> PackageResult<DependencySet<PinnedDependencyInfo<F>>> {
    let (gits, exts, locs, flav) = split(deps);

    let pinned_gits = GitDependency::pin(&gits)
        .unwrap() // TODO: error collection!
        .map(|dep| PinnedDependencyInfo::Git::<F>(dep.clone()));

    let pinned_exts = ExternalDependency::resolve::<F>(&exts).unwrap(); // TODO: errors!

    let pinned_locs = locs.map(|dep| PinnedDependencyInfo::Local::<F>(dep.clone()));

    let pinned_flav = flavor
        .pin(flav)
        .unwrap() // TODO: Errors!
        .map(|dep| PinnedDependencyInfo::FlavorSpecific::<F>(dep.clone()));

    Ok(DependencySet::merge([
        pinned_gits,
        pinned_exts,
        pinned_locs,
        pinned_flav,
    ]))
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

/// Ensure that all dependencies are stored locally and return the paths to their contents. The
/// returned map is guaranteed to have the same keys as [deps].
fn fetch<F: MoveFlavor>(
    deps: DependencySet<PinnedDependencyInfo<F>>,
) -> PackageResult<DependencySet<PathBuf>> {
    todo!()
}
