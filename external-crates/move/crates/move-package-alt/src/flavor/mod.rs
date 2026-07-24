// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod vanilla;

pub use vanilla::Vanilla;

use std::{collections::BTreeMap, fmt::Debug};

use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

use crate::schema::{
    EnvironmentID, EnvironmentName, LockfileDependencyInfo, OriginalID, PackageName,
    ParsedManifest, PublishedID, ReplacementDependency, SystemDepName,
};
use indexmap::IndexMap;

/// Data returned by [`MoveFlavor::fetch_onchain_package`] representing a package fetched from the
/// network.
#[derive(Clone, Debug)]
pub struct OnChainPackageData {
    /// Module name → serialized `CompiledModule` bytecode (the `.mv` format)
    pub modules: BTreeMap<String, Vec<u8>>,
    /// Original dependency ID → current linked address
    pub dependencies: BTreeMap<OriginalID, PublishedID>,
    /// The original (runtime) ID of this package
    pub original_id: OriginalID,
    /// The on-chain version of this package
    pub version: u64,
}

/// A [MoveFlavor] is used to parameterize the package management system. It defines the types and
/// methods for package management that are specific to a particular instantiation of the Move
/// language.
///
/// Note: this is distinct from [`move_compiler::editions::Flavor`], which selects compiler syntax
/// and semantics (e.g. `Core` vs `Sui`). `MoveFlavor` controls package-level concerns like system
/// dependencies, default environments, and on-chain fetching.
#[async_trait]
pub trait MoveFlavor: Debug + Send + Sync {
    /// Return an identifier for the flavor, used to ensure that the correct compiler is being used
    /// to parse a manifest.
    fn name(&self) -> String;

    /// A [PublishedMetadata] should contain all of the information that is generated
    /// during publication.
    //
    // TODO: should this include object IDs, or is that generic for Move? What about build config?
    type PublishedMetadata: Debug + Serialize + DeserializeOwned + Clone + Default + Send + Sync;

    /// A [PackageMetadata] encapsulates the additional package information that can be stored in
    /// the `package` section of the manifest
    type PackageMetadata: Debug + Serialize + DeserializeOwned + Clone + Send;

    /// An [AddressInfo] should give a unique identifier for a compiled package
    type AddressInfo: Debug + Serialize + DeserializeOwned + Clone + Send;

    /// Return the default environments for the flavor.
    /// Used for populating new manifests & migration purposes.
    fn default_environments(&self) -> IndexMap<EnvironmentName, EnvironmentID>;

    /// Whether two environment IDs identify the same environment. Defaults to string equality;
    /// flavors that admit multiple encodings of the same ID (e.g. full and truncated chain
    /// identifiers) can override this.
    fn environment_ids_match(&self, a: &EnvironmentID, b: &EnvironmentID) -> bool {
        a == b
    }

    /// Return ALL the system dependencies for the requested `environment`.
    async fn system_deps(
        &self,
        environment: &EnvironmentID,
    ) -> BTreeMap<SystemDepName, LockfileDependencyInfo>;

    /// Return the default system dependencies for the requested `environment`.
    async fn implicit_dependencies(
        &self,
        environment: &EnvironmentID,
    ) -> BTreeMap<PackageName, ReplacementDependency>;

    /// Fail if an edition is not allowed
    fn validate_manifest(&self, manifest: &ParsedManifest) -> Result<(), String>;

    /// Should this address be considered published in all environments? Publications with system
    /// addresses are not dropped when substituting ephemeral addresses (they can still be
    /// overridden)
    fn is_system_address(&self, address: &OriginalID) -> bool;

    /// Fetch the on-chain package at `address` from the network, returning an
    /// [`OnChainPackageData`].
    async fn fetch_onchain_package(
        &self,
        address: &PublishedID,
    ) -> anyhow::Result<OnChainPackageData>;
}
