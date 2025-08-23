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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LegacyData {
    /// The deprecated incompatible name of this package.
    /// Can be used to fix graph edges (lookup for `hash(incompatible_name) -> new_name`)
    /// This is optional, in case the old `name` was not matching the `addresses` declaration.
    pub incompatible_name: Option<String>,

    /// These addresses should store all addresses that were part of the package
    pub addresses: BTreeMap<Identifier, AccountAddress>,

    /// The address information that originates from a legacy manifest or lockfile. There are a few
    /// places these addresses could come from:
    ///  - they could come from a legacy lockfile
    ///  - they could come from the `published-at` field and the `addresses` table
    ///  - they could both be taken from the `addresses` field
    pub publication: Option<PublishAddresses>,
}
