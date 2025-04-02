// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod external;
mod git;
mod local;

use std::collections::BTreeMap;

pub use external::ExternalDependency;
pub use git::GitDependency;
pub use local::LocalDependency;
use serde::{Deserialize, Serialize};

use crate::{errors::PackageResult, flavor::MoveFlavor, package::PackageName};

/// Phantom type to represent pinned dependencies (see [PinnedDependency])
pub struct Pinned;

/// Phantom type to represent unpinned dependencies (see [ManifestDependency])
pub struct Unpinned;

/// Manifest dependencies are the things that users write in their Move.toml files
#[derive(Serialize, Deserialize)]
pub enum ManifestDependency<F: MoveFlavor> {
    Git(GitDependency<Unpinned>),
    External(ExternalDependency),
    Local(LocalDependency),
    FlavorSpecific(F::FlavorDependency<Unpinned>),
}

/// Pinned dependencies are guaranteed to always resolve to the same package source. For example,
/// a git dependendency with a branch or tag revision may change over time (and is thus not
/// pinned), whereas a git dependency with a sha revision is always guaranteed to produce the same
/// files.
#[derive(Serialize, Deserialize)]
pub enum PinnedDependency<F: MoveFlavor + ?Sized> {
    Git(GitDependency<Pinned>),
    Local(LocalDependency),
    FlavorSpecific(F::FlavorDependency<Pinned>),
}

/// Replace all dependencies with their pinned versions. The returned map is guaranteed to have the
/// same keys as [deps].
fn pin<F: MoveFlavor>(
    deps: BTreeMap<PackageName, ManifestDependency<F>>,
) -> PackageResult<BTreeMap<PackageName, PinnedDependency<F>>> {
    todo!()
}

/// Ensure that all dependencies are stored locally and return the paths to their contents. The
/// returned map is guaranteed to have the same keys as [deps].
fn fetch<F: MoveFlavor>(
    deps: BTreeMap<PackageName, PinnedDependency<F>>,
) -> PackageResult<BTreeMap<PackageName, PinnedDependency<F>>> {
    todo!()
}
