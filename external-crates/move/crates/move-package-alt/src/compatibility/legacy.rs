use std::collections::BTreeMap;

use crate::{package::EnvironmentName, schema::PublishAddresses};
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LegacyEnvironment {
    pub chain_id: String,
    pub addresses: PublishAddresses,
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LegacyData {
    /// The old-style name of this package, taken from the `package.name` field of the manifest.
    /// This differs from the modern name because it is not used in the source to refer to the
    /// package (in fact it has no semantic content). However, we need to keep it around for a few
    /// things after parsing
    pub legacy_name: String,

    /// These addresses should store all addresses that were part of the package.
    pub named_addresses: BTreeMap<Identifier, AccountAddress>,

    /// The address information that originates from the manifest file.
    /// This is optional and is the first way of doing package version management,
    /// where `published-at="<latest_id>"`, and `[addresses] <xx> = "<original_id>"`
    ///
    /// When we're doing `try_get_published_at()` or `try_get_original_id` on `Package`, we fallback to these.
    pub manifest_address_info: Option<PublishAddresses>,

    /// The legacy environments that were part of the package's legacy lockfile
    pub legacy_environments: BTreeMap<String, LegacyEnvironment>,
}

impl LegacyData {
    /// Return the published addresses of this package. It will first check the manifest address
    /// info, and then use legacy environments if the manifest info is not available.
    // TODO: we probably want to promote this to return [Publication]
    pub fn publication(&self, env: &EnvironmentName) -> Option<&PublishAddresses> {
        self.legacy_environments
            .get(env)
            .map(|env| &env.addresses)
            .or(self.manifest_address_info.as_ref())
    }
}
