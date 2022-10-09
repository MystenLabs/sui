// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
    value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
};
use serde::{Deserialize, Serialize};

use crate::{
    balance::{Balance, Supply},
    error::{ExecutionError, ExecutionErrorKind},
    object::{Data, Object},
};
use crate::{base_types::ObjectID, id::UID, SUI_FRAMEWORK_ADDRESS};
use schemars::JsonSchema;

pub const COIN_MODULE_NAME: &IdentStr = ident_str!("coin");
pub const COIN_STRUCT_NAME: &IdentStr = ident_str!("Coin");
pub const COIN_JOIN_FUNC_NAME: &IdentStr = ident_str!("join");
pub const COIN_SPLIT_N_FUNC_NAME: &IdentStr = ident_str!("split_n");
pub const COIN_SPLIT_VEC_FUNC_NAME: &IdentStr = ident_str!("split_vec");

// Rust version of the Move sui::coin::Coin type
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Eq, PartialEq)]
pub struct Coin {
    pub id: UID,
    pub balance: Balance,
}

impl Coin {
    pub fn new(id: UID, value: u64) -> Self {
        Self {
            id,
            balance: Balance::new(value),
        }
    }

    pub fn type_(type_param: StructTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: COIN_STRUCT_NAME.to_owned(),
            module: COIN_MODULE_NAME.to_owned(),
            type_params: vec![TypeTag::Struct(type_param)],
        }
    }

    /// Is this other StructTag representing a Coin?
    pub fn is_coin(other: &StructTag) -> bool {
        other.module.as_ident_str() == COIN_MODULE_NAME
            && other.name.as_ident_str() == COIN_STRUCT_NAME
    }

    /// Create a coin from BCS bytes
    pub fn from_bcs_bytes(content: &[u8]) -> Result<Self, ExecutionError> {
        bcs::from_bytes(content).map_err(|err| {
            ExecutionError::new_with_source(
                ExecutionErrorKind::InvalidCoinObject,
                format!("Unable to deserialize coin object: {:?}", err),
            )
        })
    }

    /// If the given object is a Coin, deserialize its contents and extract the balance Ok(Some(u64)).
    /// If it's not a Coin, return Ok(None).
    /// The cost is 2 comparisons if not a coin, and deserialization if its a Coin.
    pub fn extract_balance_if_coin(object: &Object) -> Result<Option<u64>, ExecutionError> {
        match &object.data {
            Data::Move(move_obj) => {
                if !Self::is_coin(&move_obj.type_) {
                    return Ok(None);
                }

                let coin = Self::from_bcs_bytes(move_obj.contents())?;
                Ok(Some(coin.value()))
            }
            _ => Ok(None), // package
        }
    }

    pub fn id(&self) -> &ObjectID {
        self.id.object_id()
    }

    pub fn value(&self) -> u64 {
        self.balance.value()
    }

    pub fn to_bcs_bytes(&self) -> Vec<u8> {
        bcs::to_bytes(&self).unwrap()
    }

    pub fn layout(type_param: StructTag) -> MoveStructLayout {
        MoveStructLayout::WithTypes {
            type_: Self::type_(type_param.clone()),
            fields: vec![
                MoveFieldLayout::new(
                    ident_str!("id").to_owned(),
                    MoveTypeLayout::Struct(UID::layout()),
                ),
                MoveFieldLayout::new(
                    ident_str!("balance").to_owned(),
                    MoveTypeLayout::Struct(Balance::layout(type_param)),
                ),
            ],
        }
    }
}

// Rust version of the Move sui::coin::TreasuryCap type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct TreasuryCap {
    pub id: UID,
    pub total_supply: Supply,
}
