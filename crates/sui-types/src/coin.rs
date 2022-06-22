// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
    value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
};
use serde::{Deserialize, Serialize};

use crate::balance::{Balance, Supply};
use crate::{
    base_types::{ObjectID, SequenceNumber},
    id::VersionedID,
    SUI_FRAMEWORK_ADDRESS,
};
use schemars::JsonSchema;

pub const COIN_MODULE_NAME: &IdentStr = ident_str!("coin");
pub const COIN_STRUCT_NAME: &IdentStr = ident_str!("Coin");
pub const COIN_JOIN_FUNC_NAME: &IdentStr = ident_str!("join");
pub const COIN_SPLIT_VEC_FUNC_NAME: &IdentStr = ident_str!("split_vec");

// Rust version of the Move sui::coin::Coin type
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Eq, PartialEq)]
pub struct Coin {
    pub id: VersionedID,
    pub balance: Balance,
}

impl Coin {
    pub fn new(id: VersionedID, value: u64) -> Self {
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

    pub fn id(&self) -> &ObjectID {
        self.id.object_id()
    }

    pub fn version(&self) -> SequenceNumber {
        self.id.version()
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
                    MoveTypeLayout::Struct(VersionedID::layout()),
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
    pub id: VersionedID,
    pub total_supply: Supply,
}
