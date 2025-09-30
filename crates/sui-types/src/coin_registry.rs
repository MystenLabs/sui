// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};

use crate::{
    base_types::{ObjectID, SequenceNumber},
    collection_types::VecMap,
    derived_object,
    error::SuiResult,
    object::Owner,
    storage::ObjectStore,
    SUI_COIN_REGISTRY_OBJECT_ID, SUI_FRAMEWORK_ADDRESS,
};

pub const COIN_REGISTRY_MODULE_NAME: &IdentStr = ident_str!("coin_registry");
pub const CURRENCY_KEY_STRUCT_NAME: &IdentStr = ident_str!("CurrencyKey");

/// Rust representation of `sui::coin_registry::CurrencyKey<T>`.
#[derive(Serialize, Deserialize, Copy, Clone, Default, PartialEq, Eq)]
pub struct CurrencyKey(bool);

/// Rust representation of `sui::coin_registry::Currency<phantom T>`.
#[derive(Serialize, Deserialize)]
pub struct Currency {
    pub id: ObjectID,
    pub decimals: u8,
    pub name: String,
    pub symbol: String,
    pub description: String,
    pub icon_url: String,
    pub supply: Option<SupplyState>,
    pub regulated: RegulatedState,
    pub treasury_cap_id: Option<ObjectID>,
    pub metadata_cap_id: MetadataCapState,
    pub extra_fields: VecMap<String, ExtraField>,
}

/// Rust representation of `sui::coin_registry::SupplyState<phantom T>`.
#[derive(Serialize, Deserialize)]
pub enum SupplyState {
    Fixed(u64),
    BurnOnly(u64),
    Unknown,
}

/// Rust representation of `sui::coin_registry::RegulatedState`.
#[derive(Serialize, Deserialize)]
pub enum RegulatedState {
    Regulated {
        cap: ObjectID,
        allow_global_pause: Option<bool>,
        variant: u8,
    },
    Unregulated,
    Unknown,
}

/// Rust representation of `sui::coin_registry::MetadataCapState`.
#[derive(Serialize, Deserialize)]
pub enum MetadataCapState {
    Claimed(ObjectID),
    Unclaimed,
    Deleted,
}

/// Rust representation of `sui::coin_registry::ExtraField`.
#[derive(Serialize, Deserialize)]
pub struct ExtraField {
    pub type_: String,
    pub value: Vec<u8>,
}

impl Currency {
    /// Derive the ObjectID for `sui::coin_registry::Currency<$coin_type>`.
    pub fn derive_object_id(coin_type: TypeTag) -> Result<ObjectID, bcs::Error> {
        let key = TypeTag::Struct(Box::new(StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: COIN_REGISTRY_MODULE_NAME.to_owned(),
            name: CURRENCY_KEY_STRUCT_NAME.to_owned(),
            type_params: vec![coin_type],
        }));

        derived_object::derive_object_id(
            SUI_COIN_REGISTRY_OBJECT_ID,
            &key,
            &bcs::to_bytes(&CurrencyKey::default())?,
        )
    }

    /// Is this `StructTag` a `sui::coin_registry::Currency<...>`?
    pub fn is_currency(tag: &StructTag) -> bool {
        tag.address == SUI_FRAMEWORK_ADDRESS
            && tag.module.as_str() == "coin_registry"
            && tag.name.as_str() == "Currency"
    }
}

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
