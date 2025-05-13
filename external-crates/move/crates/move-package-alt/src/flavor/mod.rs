// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod vanilla;

pub use vanilla::Vanilla;

use std::{
    collections::BTreeMap,
    fmt::Debug,
    path::{Path, PathBuf},
};

use serde::{Serialize, de::DeserializeOwned};

use crate::{
    dependency::{DependencySet, Pinned, PinnedDependencyInfo, Unpinned},
    errors::PackageResult,
    package::PackageName,
};

/// A [MoveFlavor] is used to parameterize the package management system. It defines the types and
/// methods for package management that are specific to a particular instantiation of the Move
/// language.
pub trait MoveFlavor: Debug {
    /// Return an identifier for the flavor, used to ensure that the correct compiler is being used
    /// to parse a manifest.
    fn name() -> String;

    /// Additional flavor-specific dependency types. Currently we only support flavor-specific
    /// dependencies that are already pinned (although in principle you could use an
    /// external resolved to do resolution and pinning for flavor-specific deps)
    type FlavorDependency<P: ?Sized>: Debug + Serialize + DeserializeOwned + Clone + PartialEq;

    /// Pin a batch of [Self::FlavorDependency]s (see TODO). The keys of the returned map should be
    /// the same as the keys of [dep].
    //
    // TODO: this interface means we can't batch dep-replacements together
    fn pin(
        &self,
        deps: DependencySet<Self::FlavorDependency<Unpinned>>,
    ) -> PackageResult<DependencySet<Self::FlavorDependency<Pinned>>>;

    /// Fetch a batch [Self::FlavorDependency] (see TODO)
    fn fetch(
        &self,
        deps: DependencySet<Self::FlavorDependency<Pinned>>,
    ) -> PackageResult<DependencySet<PathBuf>>;

    /// A [PublishedMetadata] should contain all of the information that is generated
    /// during publication.
    //
    // TODO: should this include object IDs, or is that generic for Move? What about build config?
    type PublishedMetadata: Debug + Serialize + DeserializeOwned + Clone;

    /// A [PackageMetadata] encapsulates the additional package information that can be stored in
    /// the `package` section of the manifest
    type PackageMetadata: Debug + Serialize + DeserializeOwned + Clone;

    /// An [AddressInfo] should give a unique identifier for a compiled package
    type AddressInfo: Debug + Serialize + DeserializeOwned + Clone;

    /// An [EnvironmentID] uniquely identifies a place that a package can be published. For
    /// example, an environment ID might be a chain identifier
    //
    // TODO: Given an [EnvironmentID] and an [ObjectID], ... should be uniquely determined
    type EnvironmentID: Serialize + DeserializeOwned + Clone + Eq + Ord + Debug + ToString;

    /// Return the implicit dependencies for the environments listed in [environments]
    fn implicit_deps(
        &self,
        environments: impl Iterator<Item = Self::EnvironmentID>,
    ) -> DependencySet<PinnedDependencyInfo<Self>>;
}
