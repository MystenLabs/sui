// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{de::DeserializeOwned, Serialize};

use crate::{
    dependency::{Pinned, PinnedDependency, Unpinned},
    errors::PackageResult,
    package::PackageName,
};

/// A [MoveFlavor] is used to parameterize the package management system. It defines the types and
/// methods for package management that are specific to a particular instantiation of the Move
/// language.
pub trait MoveFlavor {
    // TODO: this API is incomplete

    /// Additional flavor-specific dependency types. Currently we only support flavor-specific
    /// dependencies that are already pinned (although in principle you could use an
    /// external resolved to do resolution and pinning for flavor-specific deps)
    type FlavorDependency<P: ?Sized>: Serialize + DeserializeOwned + Clone;

    /// Pin a batch of [Self::FlavorDependency]s (see TODO). The keys of the returned map should be
    /// the same as the keys of [dep].
    //
    // TODO: this interface means we can't batch dep-overrides together
    fn pin(
        &self,
        deps: BTreeMap<PackageName, Self::FlavorDependency<Unpinned>>,
    ) -> PackageResult<BTreeMap<PackageName, Self::FlavorDependency<Pinned>>>;

    /// Fetch a batch [Self::FlavorDependency] (see TODO)
    fn fetch(
        &self,
        deps: BTreeMap<PackageName, Self::FlavorDependency<Pinned>>,
    ) -> PackageResult<BTreeMap<PackageName, PathBuf>>;

    /// A [PublishedMetadata] should contain all of the information that is generated
    /// during publication.
    //
    // TODO: should this include object IDs, or is that generic for Move? What about build config?
    type PublishedMetadata: Serialize + DeserializeOwned + Clone;

    /// An [EnvironmentID] uniquely identifies a place that a package can be published. For
    /// example, an environment ID might be a chain identifier
    //
    // TODO: Given an [EnvironmentID] and an [ObjectID], ... should be uniquely determined
    type EnvironmentID: Serialize + DeserializeOwned + Clone + Eq;

    /// Return the implicit dependencies for a given environment
    fn implicit_deps(&self, environment: Self::EnvironmentID) -> Vec<PinnedDependency<Self>>;
}
