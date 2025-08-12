use std::collections::BTreeMap;

use crate::{package::EnvironmentName, schema::PublishAddresses};
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct LegacyEnvironment {
    pub chain_id: String,
    pub addresses: PublishAddresses,
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LegacyData {
    /// The deprecated incompatible name of this package.
    /// Can be used to fix graph edges (lookup for `hash(incompatible_name) -> new_name`)
    /// This is optional, in case the old `name` was not matching the `addresses` declaration.
    pub incompatible_name: Option<String>,

    /// These addresses should store all addresses that were part of the package.
    pub addresses: BTreeMap<Identifier, AccountAddress>,

    /// The address information that originates from the manifest file.
    /// This is optional and is the first way of doing package version management,
    /// where `published-at="<latest_id>"`, and `[addresses] <xx> = "<original_id>"`
    ///
    /// When we're doing `try_get_published_at()` or `try_get_original_id` on `Package`, we fallback to these.
    pub manifest_address_info: Option<PublishAddresses>,

    /// The legacy environments that were part of the package (goes from env name -> )
    pub legacy_environments: BTreeMap<String, LegacyEnvironment>,
}

impl LegacyData {
    /// Return the published addresses of this package. It will first check the manifest address
    /// info, and then use legacy environments if the manifest info is not available.
    pub fn publication(&self, env: &EnvironmentName) -> Option<&PublishAddresses> {
        self.legacy_environments
            .get(env)
            .map(|env| &env.addresses)
            .or(self.manifest_address_info.as_ref())
    }
}
