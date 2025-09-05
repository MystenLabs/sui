// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod vanilla;

pub use vanilla::Vanilla;

use std::{collections::BTreeMap, fmt::Debug};

use serde::{Serialize, de::DeserializeOwned};

use crate::schema::{EnvironmentID, EnvironmentName, PackageName, ReplacementDependency};

/// A [MoveFlavor] is used to parameterize the package management system. It defines the types and
/// methods for package management that are specific to a particular instantiation of the Move
/// language.
pub trait MoveFlavor: Debug {
    /// Return an identifier for the flavor, used to ensure that the correct compiler is being used
    /// to parse a manifest.
    fn name() -> String;

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

    /// Return the default environments for the flavor.
    /// Used for populating new manifests & migration purposes.
    fn default_environments() -> BTreeMap<EnvironmentName, EnvironmentID>;

    /// Return ALL the system dependencies for the requested environment.
    fn system_dependencies(
        environment: EnvironmentID,
    ) -> BTreeMap<PackageName, ReplacementDependency>;

    /// Return the default system dependencies for the requested environment.
    fn default_system_dependencies(
        environment: EnvironmentID,
    ) -> BTreeMap<PackageName, ReplacementDependency>;
}
