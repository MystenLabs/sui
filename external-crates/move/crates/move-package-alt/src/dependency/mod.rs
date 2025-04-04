// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod external;
mod git;
mod local;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{errors::PackageResult, flavor::MoveFlavor, package::PackageName};

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

/// Replace all dependencies with their pinned versions. The returned map is guaranteed to have the
/// same keys as [deps].
// TODO: this needs to change to support the fact that external resolvers return different results
// depending on the environment
fn pin<F: MoveFlavor>(
    deps: BTreeMap<PackageName, ManifestDependencyInfo<F>>,
) -> PackageResult<BTreeMap<PackageName, PinnedDependencyInfo<F>>> {
    todo!()
}

/// Ensure that all dependencies are stored locally and return the paths to their contents. The
/// returned map is guaranteed to have the same keys as [deps].
fn fetch<F: MoveFlavor>(
    deps: BTreeMap<PackageName, PinnedDependencyInfo<F>>,
) -> PackageResult<BTreeMap<PackageName, PinnedDependencyInfo<F>>> {
    todo!()
}
