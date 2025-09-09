use std::collections::BTreeMap;

use crate::{
    flavor::MoveFlavor,
    package::EnvironmentName,
    schema::{Environment, Publication, PublishAddresses},
};
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LegacyEnvironment {
    pub chain_id: String,
    pub addresses: PublishAddresses,
    pub version: u64,
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

    /// The legacy publication information stored in a legacy `Move.lock` file.
    pub legacy_publications: BTreeMap<EnvironmentName, LegacyEnvironment>,

    /// The address information that originates from the manifest file.
    /// This is optional and is the first way of doing package version management,
    /// where `published-at="<latest_id>"`, and `[addresses] <xx> = "<original_id>"`
    ///
    /// When we're doing `try_get_published_at()` or `try_get_original_id` on `Package`, we fallback to these.
    pub manifest_address_info: Option<PublishAddresses>,
}

impl LegacyData {
    /// Return the published addresses of this package. It will first check the legacy addresses,
    /// info, and then use the manifest if the lockfile info is not available.
    pub fn publication<F: MoveFlavor>(&self, env: &Environment) -> Option<Publication<F>> {
        self.legacy_publications
            .get(env.name())
            .cloned()
            .map(|it| it.into())
            .or(self.manifest_address_info.as_ref().map(|it| Publication {
                chain_id: env.id().clone(),
                addresses: it.clone(),
                // For legacy packages that have the addresses in the manifest, we default to 0.
                version: 0,
                metadata: F::PublishedMetadata::default(),
            }))
    }
}

impl<F: MoveFlavor> From<LegacyEnvironment> for Publication<F> {
    fn from(value: LegacyEnvironment) -> Self {
        Self {
            chain_id: value.chain_id,
            addresses: value.addresses,
            version: value.version,
            metadata: F::PublishedMetadata::default(),
        }
    }
}
