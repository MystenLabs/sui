// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Serialize};

use crate::errors::PackageResult;

/// A [MoveFlavor] is used to parameterize the package management system. It defines the types and
/// methods for package management that are specific to a particular instantiation of the Move
/// language.
pub trait MoveFlavor {
    // TODO: this API is incomplete

    /// A [PublishedMetadata] should contain all of the information that is generated
    /// during publication.
    //
    // TODO: should this include object IDs, or is that generic for Move? What about build config?
    type PublishedMetadata: Serialize + for<'a> TryFrom<(&'a Path, toml_edit::Value)>;

    /// A [ManifestDendency] can appear in the [dependencies] section of the manifest
    type ManifestDependency: Serialize + for<'a> TryFrom<(&'a Path, toml_edit::Value)>;

    /// An [InternalDependency] has been resolved
    type InternalDependency;

    /// A [PinnedDependency] has been pinned - repeated fetching of the same pinned dependency
    /// should yield the same source tree.
    type PinnedDependency: Serialize + for<'a> TryFrom<(&'a Path, toml_edit::Value)>;

    /// An [EnvironmentID] uniquely identifies a place that a package can be published. For
    /// example, an environment ID might be a chain identifier
    //
    // TODO: Given an [EnvironmentID] and an [ObjectID], ... should be uniquely determined
    type EnvironmentID: Serialize + DeserializeOwned + Eq;

    /// Return the implicit dependencies for a given environment
    fn implicit_dependencies(&self, id: Self::EnvironmentID) -> Vec<Self::InternalDependency>;

    /// Execute an external resolver to replace a manifest dependency with a resolved dependency
    fn resolve(&self, dep: &Self::ManifestDependency) -> PackageResult<&Self::InternalDependency>;

    /// Replace a dependency with its pinned version
    fn pin(&self, dep: &Self::InternalDependency) -> PackageResult<&Self::PinnedDependency>;

    /// Ensure the dependency is stored locally and return the path to its contents
    fn fetch(&self, dep: &Self::PinnedDependency) -> PackageResult<PathBuf>;
}
