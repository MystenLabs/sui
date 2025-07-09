use crate::{
    compatibility::{LegacyAddressDeclarations, LegacyDevAddressDeclarations},
    schema::{OriginalID, PublishedID},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ManifestPublishInformation {
    pub published_at: PublishedID,
    pub original_id: OriginalID,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LegacyData {
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
    /// When we're doing `try_get_published_at()` or `try_get_original_id` on `Package`, we fallback to these.
    pub manifest_address_info: Option<ManifestPublishInformation>,
}
