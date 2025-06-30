// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    balance::Balance,
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    dynamic_field::{derive_dynamic_field_id, DOFWrapper},
    error::SuiResult,
    gas_coin::GasCoin,
    object::{Object, Owner},
    storage::ObjectStore,
    transaction::WithdrawTypeParam,
    MoveTypeTagTrait, SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID,
};
use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};

pub const ACCUMULATOR_ROOT_MODULE: &IdentStr = ident_str!("accumulator");
pub const ACCUMULATOR_ROOT_CREATE_FUNC: &IdentStr = ident_str!("create");

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
    /// Raw bytes of the balance type name string.
    type_param: Vec<u8>,
}

impl AccumulatorKey {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_PACKAGE_ID.into(),
            module: ACCUMULATOR_ROOT_MODULE.to_owned(),
            name: ident_str!("AccumulatorKey").to_owned(),
            type_params: vec![],
        }
    }
}

impl MoveTypeTagTrait for AccumulatorKey {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(Self::type_()))
    }
}

pub fn derive_balance_account_object_id(
    owner: SuiAddress,
    type_param: WithdrawTypeParam,
) -> anyhow::Result<ObjectID> {
    let WithdrawTypeParam::Balance(type_param) = type_param;
    let full_type = TypeTag::Struct(Box::new(Balance::type_(type_param.to_type_tag()?)));
    let key = DOFWrapper {
        name: AccumulatorKey {
            owner,
            type_param: full_type.to_canonical_string(false).into_bytes(),
        },
    };
    derive_dynamic_field_id(
        SUI_ACCUMULATOR_ROOT_OBJECT_ID,
        &AccumulatorKey::get_type_tag(),
        &bcs::to_bytes(&key)?,
    )
    .map_err(|e| e.into())
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
    let account_object_id = derive_balance_account_object_id(owner, type_param).unwrap();
    Object::with_id_owner_gas_for_testing(account_object_id, owner, balance)
}
