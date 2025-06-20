use std::collections::BTreeMap;

use crate::{
    compatibility::legacy_manifest::{LegacyAddressDeclarations, LegacyDevAddressDeclarations},
    package::PackageName,
};
use move_core_types::account_address::AccountAddress;
use serde::{Deserialize, Serialize};

/// In old `lockfiles`, we had environments specified as `[env.mainnet]`, `[env.testnet]` etc.
/// It is not far away from the current system, but we keep it as part of the deprecated information.
pub type LegacyEnvironments = BTreeMap<String, ManagedPackage>;

#[derive(Serialize, Deserialize, Debug)]
pub struct ManagedPackage {
    #[serde(rename = "chain-id")]
    pub chain_id: String,
    #[serde(rename = "original-published-id")]
    pub original_published_id: String,
    #[serde(rename = "latest-published-id")]
    pub latest_published_id: String,
    #[serde(rename = "published-version")]
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LegacyPackageInformation {
    /// The deprecated incompatible name of this package.
    /// Can be used to fix graph edges (lookup for `hash(incompatible_name) -> new_name`)
    /// This is optional, in case the old `name` was not matching the `addresses` declaration.
    pub incompatible_name: Option<String>,

    /// These addresses should store all addresses that were part of the package.
    pub addresses: LegacyAddressDeclarations,

    /// These addresses should store all DEV addresses that were part of the package.
    pub dev_addresses: Option<LegacyDevAddressDeclarations>,

    pub environments: LegacyEnvironments,
}
