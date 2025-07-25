// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::{ObjectID, SequenceNumber},
    dynamic_field::{
        self, DYNAMIC_OBJECT_FIELD_MODULE_NAME, DYNAMIC_OBJECT_FIELD_WRAPPER_STRUCT_NAME,
    },
    error::SuiResult,
    id::UID,
    object::Owner,
    storage::ObjectStore,
    TypeTag, SUI_COIN_REGISTRY_OBJECT_ID,
};
use move_core_types::{ident_str, identifier::IdentStr, language_storage::StructTag};
use serde::{Deserialize, Serialize};

pub const COIN_REGISTRY_MODULE_NAME: &IdentStr = ident_str!("coin_registry");
pub const COIN_REGISTRY_CREATE_FUNCTION_NAME: &IdentStr = ident_str!("create");
pub const COIN_DATA_STRUCT_NAME: &IdentStr = ident_str!("CoinData");
pub const COIN_DATA_KEY_STRUCT_NAME: &IdentStr = ident_str!("CoinDataKey");

/// The empty struct used as a key to access coin metadata hung off the CoinRegistry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoinDataKey {
    /// Move serializes empty structs as [0x00] while Rust serde serializes them as []. This field
    /// is a workaround to bridge the difference.
    dummy: bool,
}

impl CoinDataKey {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { dummy: false }
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
            _ => unreachable!("Coin Registry object must be shared"),
        }))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CoinData {
    pub id: UID,
    pub decimals: u8,
    pub name: String,
    pub symbol: String,
    pub description: String,
    pub icon_url: String,
    pub supply: Option<SupplyState>,
    pub regulated: RegulatedState,
    pub treasury_cap_id: Option<ObjectID>,
    pub metadata_cap_id: Option<ObjectID>,
    pub extra_fields: Vec<(String, ExtraField)>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Supply {
    pub value: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum SupplyState {
    Fixed(Supply) = 0,
    Unknown = 1,
}

#[derive(Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum RegulatedState {
    Regulated { cap: ObjectID, variant: u8 } = 0,
    Unknown = 1,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExtraField {
    pub type_name: String,
    pub value: Vec<u8>,
}

pub fn derive_dynamic_field_object_id(coin_type: &StructTag) -> Result<ObjectID, bcs::Error> {
    // For dynamic object fields, the key type is Wrapper<CoinDataKey<T>>
    let coin_data_key_type = StructTag {
        address: move_core_types::account_address::AccountAddress::from_hex_literal("0x2").unwrap(),
        module: move_core_types::identifier::Identifier::new(COIN_REGISTRY_MODULE_NAME.as_str())
            .unwrap(),
        name: move_core_types::identifier::Identifier::new(COIN_DATA_KEY_STRUCT_NAME.as_str())
            .unwrap(),
        type_params: vec![TypeTag::Struct(Box::new(coin_type.clone()))],
    };
    let wrapper_type_tag = TypeTag::Struct(Box::new(StructTag {
        address: move_core_types::account_address::AccountAddress::from_hex_literal("0x2").unwrap(),
        module: move_core_types::identifier::Identifier::new(
            DYNAMIC_OBJECT_FIELD_MODULE_NAME.as_str(),
        )
        .unwrap(),
        name: move_core_types::identifier::Identifier::new(
            DYNAMIC_OBJECT_FIELD_WRAPPER_STRUCT_NAME.as_str(),
        )
        .unwrap(),
        type_params: vec![TypeTag::Struct(Box::new(coin_data_key_type))],
    }));
    let coin_data_key_bytes = bcs::to_bytes(&CoinDataKey::new())
        .expect("BCS serialization of CoinDataKey (empty struct) cannot fail");
    let field_id = dynamic_field::derive_dynamic_field_id(
        SUI_COIN_REGISTRY_OBJECT_ID,
        &wrapper_type_tag,
        &coin_data_key_bytes,
    )?;
    Ok(field_id)
}
