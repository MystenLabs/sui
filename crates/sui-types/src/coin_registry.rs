// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::{ObjectID, SequenceNumber},
    collection_types::VecMap,
    error::SuiResult,
    id::UID,
    object::Owner,
    storage::ObjectStore,
    SUI_COIN_REGISTRY_OBJECT_ID,
};
use move_core_types::{ident_str, identifier::IdentStr};
use serde::{Deserialize, Serialize};

pub const COIN_REGISTRY_MODULE_NAME: &IdentStr = ident_str!("coin_registry");
pub const CURRENCY_KEY_STRUCT_NAME: &IdentStr = ident_str!("CurrencyKey");

pub fn get_coin_registry_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<SequenceNumber>> {
    Ok(object_store
        .get_object(&SUI_COIN_REGISTRY_OBJECT_ID)
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("CoinRegistry object must be shared"),
        }))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Supply {
    pub value: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum SupplyState {
    Fixed(Supply) = 0,
    BurnOnly(Supply) = 1,
    Unknown = 2,
}

/// Empty struct used as a key for deriving Currency addresses
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CurrencyKey {
    /// Move serializes empty structs as [0x00] while Rust serde serializes them as []. This field
    /// is a workaround to bridge the difference.
    dummy: bool,
}

impl CurrencyKey {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { dummy: false }
    }
}

/// Currency stores metadata such as name, symbol, decimals, icon_url and description,
/// as well as supply states (optional) and regulatory status.
#[derive(Debug, Serialize, Deserialize)]
pub struct Currency {
    pub id: UID,
    pub decimals: u8,
    pub name: String,
    pub symbol: String,
    pub description: String,
    pub icon_url: String,
    pub supply: Option<SupplyState>,
    pub regulated: CurrencyRegulatedState,
    pub treasury_cap_id: Option<ObjectID>,
    pub metadata_cap_id: MetadataCapState,
    pub extra_fields: VecMap<String, ExtraField>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CurrencyRegulatedState {
    Regulated {
        cap: ObjectID,
        allow_global_pause: Option<bool>,
        variant: u8,
    },
    Unregulated,
    Unknown,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum MetadataCapState {
    Claimed(ObjectID),
    Unclaimed,
    Deleted,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TypeName {
    pub name: String,
}

pub type ExtraField = (TypeName, Vec<u8>);
