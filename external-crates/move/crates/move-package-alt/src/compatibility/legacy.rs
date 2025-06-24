use std::collections::BTreeMap;

use crate::{
    compatibility::{LegacyAddressDeclarations, LegacyDevAddressDeclarations},
    package::{PackageName, PublishInformation, PublishInformationMap, PublishedIds},
};
use move_core_types::account_address::AccountAddress;
use serde::{Deserialize, Serialize};

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

    /// The address information that originates from the manifest file.
    /// This is optional and is the first way of doing package version management,
    /// where `published-at="<latest_id>"`, and `[addresses] <xx> = "<original_id>"`
    ///
    /// When we're doing `get_package_ids()` on `Package`, we return this.
    pub manifest_address_info: Option<PublishedIds>,

    /// This is the old environments, we could potentially merge this directly on the
    /// `Package<F>` constructor, instead of keeping a separate point of info!
    pub environments: Option<PublishInformationMap>,
}
