// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    balance::Balance,
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    collection_types::Bag,
    dynamic_field::{
        derive_dynamic_field_id, get_dynamic_field_from_store, get_dynamic_field_from_store_generic,
    },
    error::{SuiError, SuiResult},
    gas_coin::GasCoin,
    object::{Object, Owner},
    storage::ObjectStore,
    transaction::WithdrawTypeParam,
    MoveTypeTagTrait, MoveTypeTagTraitGeneric, SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_PACKAGE_ID,
};
use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};

pub const ACCUMULATOR_ROOT_MODULE: &IdentStr = ident_str!("accumulator");
pub const ACCUMULATOR_METADATA_MODULE: &IdentStr = ident_str!("accumulator_metadata");
pub const ACCUMULATOR_SETTLEMENT_MODULE: &IdentStr = ident_str!("accumulator_settlement");
pub const ACCUMULATOR_ROOT_CREATE_FUNC: &IdentStr = ident_str!("create");
pub const ACCUMULATOR_ROOT_SETTLE_U128_FUNC: &IdentStr = ident_str!("settle_u128");
pub const ACCUMULATOR_ROOT_SETTLEMENT_PROLOGUE_FUNC: &IdentStr = ident_str!("settlement_prologue");

const ACCUMULATOR_KEY_TYPE: &IdentStr = ident_str!("Key");
const ACCUMULATOR_OWNER_KEY_TYPE: &IdentStr = ident_str!("OwnerKey");
const ACCUMULATOR_OWNER_TYPE: &IdentStr = ident_str!("Owner");
const ACCUMULATOR_METADATA_KEY_TYPE: &IdentStr = ident_str!("MetadataKey");

const ACCUMULATOR_U128_TYPE: &IdentStr = ident_str!("U128");

pub fn get_accumulator_root_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<SequenceNumber>> {
    Ok(object_store
        .get_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("Accumulator root object must be shared"),
        }))
}

/// Rust type for the Move type AccumulatorKey used to derive the dynamic field id for the
/// balance account object.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct AccumulatorKey {
    owner: SuiAddress,
}

impl MoveTypeTagTraitGeneric for AccumulatorKey {
    fn get_type_tag(type_params: &[TypeTag]) -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: SUI_FRAMEWORK_PACKAGE_ID.into(),
            module: ACCUMULATOR_ROOT_MODULE.to_owned(),
            name: ACCUMULATOR_KEY_TYPE.to_owned(),
            type_params: type_params.to_vec(),
        }))
    }
}

#[derive(Serialize, Deserialize)]
pub struct AccumulatorOwner {
    balances: Bag,
    owner: SuiAddress,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct MetadataKey(u8);

impl MoveTypeTagTraitGeneric for MetadataKey {
    fn get_type_tag(type_params: &[TypeTag]) -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: SUI_FRAMEWORK_PACKAGE_ID.into(),
            module: ACCUMULATOR_METADATA_MODULE.to_owned(),
            name: ACCUMULATOR_METADATA_KEY_TYPE.to_owned(),
            type_params: type_params.to_vec(),
        }))
    }
}

#[derive(Serialize, Deserialize)]
pub struct AccumulatorMetadata {
    /// Any per-balance fields we wish to add in the future.
    fields: Bag,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OwnerKey {
    owner: SuiAddress,
}

impl MoveTypeTagTrait for OwnerKey {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ACCUMULATOR_METADATA_MODULE.to_owned(),
            name: ACCUMULATOR_OWNER_KEY_TYPE.to_owned(),
            type_params: vec![],
        }))
    }
}

impl AccumulatorOwner {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ACCUMULATOR_METADATA_MODULE.to_owned(),
            name: ACCUMULATOR_OWNER_TYPE.to_owned(),
            type_params: vec![],
        }
    }

    pub fn derive_owner_address(owner: SuiAddress) -> ObjectID {
        derive_dynamic_field_id(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            &TypeTag::Address,
            &bcs::to_bytes(&owner).expect("to_bytes should not fail"),
        )
        .expect("derive_dynamic_field_id should not fail")
    }

    pub fn exists(object_store: &dyn ObjectStore, owner: SuiAddress) -> SuiResult<bool> {
        let key = OwnerKey { owner };
        let key_bytes = bcs::to_bytes(&key).expect("to_bytes should not fail");
        let id = derive_dynamic_field_id(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            &OwnerKey::get_type_tag(),
            &key_bytes,
        )
        .map_err(|e| SuiError::TypeError {
            error: format!("Failed to derive dynamic field id: {}", e),
        })?;
        Ok(object_store.get_object(&id).is_some())
    }

    pub fn load(object_store: &dyn ObjectStore, owner: SuiAddress) -> SuiResult<Self> {
        let key = OwnerKey { owner };
        get_dynamic_field_from_store(object_store, SUI_ACCUMULATOR_ROOT_OBJECT_ID, &key)
    }

    pub fn metadata_exists(
        &self,
        object_store: &dyn ObjectStore,
        type_: &TypeTag,
    ) -> SuiResult<bool> {
        let key = MetadataKey::default();
        let key_bytes = bcs::to_bytes(&key).unwrap();
        let id = derive_dynamic_field_id(
            *self.balances.id.object_id(),
            &MetadataKey::get_type_tag(&[type_.clone()]),
            &key_bytes,
        )
        .map_err(|e| SuiError::TypeError {
            error: format!("Failed to derive dynamic field id: {}", e),
        })?;
        Ok(object_store.get_object(&id).is_some())
    }

    pub fn load_metadata(
        &self,
        object_store: &dyn ObjectStore,
        type_: &TypeTag,
    ) -> SuiResult<AccumulatorMetadata> {
        let key = MetadataKey::default();
        get_dynamic_field_from_store_generic(
            object_store,
            *self.balances.id.object_id(),
            &key,
            &[type_.clone()],
        )
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum AccumulatorValue {
    U128(U128),
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct U128 {
    pub value: u128,
}

impl MoveTypeTagTrait for U128 {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ACCUMULATOR_ROOT_MODULE.to_owned(),
            name: ACCUMULATOR_U128_TYPE.to_owned(),
            type_params: vec![],
        }))
    }
}

impl AccumulatorValue {
    pub fn get_field_id(owner: SuiAddress, type_: &TypeTag) -> Result<ObjectID, bcs::Error> {
        let key = AccumulatorKey { owner };
        let key_bytes = bcs::to_bytes(&key).unwrap();
        derive_dynamic_field_id(SUI_ACCUMULATOR_ROOT_OBJECT_ID, type_, &key_bytes)
    }

    pub fn exists(
        object_store: &dyn ObjectStore,
        owner: SuiAddress,
        type_: &TypeTag,
    ) -> SuiResult<bool> {
        let key = AccumulatorKey { owner };
        let key_bytes = bcs::to_bytes(&key).unwrap();

        let id = derive_dynamic_field_id(
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            &AccumulatorKey::get_type_tag(&[type_.clone()]),
            &key_bytes,
        )
        .map_err(|e| SuiError::TypeError {
            error: format!("Failed to derive dynamic field id: {}", e),
        })?;

        Ok(object_store.get_object(&id).is_some())
    }

    pub fn load(
        object_store: &dyn ObjectStore,
        owner: SuiAddress,
        type_: &TypeTag,
    ) -> SuiResult<Self> {
        if !Balance::is_balance_type(type_) {
            return Err(SuiError::TypeError {
                error: "only Balance<T> is supported".to_string(),
            });
        }

        let key = AccumulatorKey { owner };
        let value: U128 = get_dynamic_field_from_store_generic(
            object_store,
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            &key,
            &[type_.clone()],
        )?;

        Ok(Self::U128(value))
    }
}

/// Given an account object, return the balance of the account.
/// This is a temporary function for testing.
pub fn get_balance_from_account_for_testing(account_object: &Object) -> u64 {
    // TODO(address-balances): Implement this properly.
    GasCoin::try_from(account_object).unwrap().value()
}

pub fn update_account_balance_for_testing(account_object: &mut Object, balance_change: i128) {
    let new_balance = get_balance_from_account_for_testing(account_object) as i128 + balance_change;
    account_object
        .data
        .try_as_move_mut()
        .unwrap()
        .set_coin_value_unsafe(new_balance as u64);
}

/// Create an account object for testing.
/// This is a temporary function for testing.
pub fn create_account_for_testing(
    owner: SuiAddress,
    type_param: WithdrawTypeParam,
    balance: u64,
) -> Object {
    let type_tag = type_param.get_type_tag().unwrap();
    let account_object_id = AccumulatorValue::get_field_id(owner, &type_tag).unwrap();
    Object::with_id_owner_gas_for_testing(account_object_id, owner, balance)
}
