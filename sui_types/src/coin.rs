// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
    value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
};
use serde::{Deserialize, Serialize};

use crate::{
    base_types::{ObjectID, SequenceNumber},
    id::VersionedID,
    SUI_FRAMEWORK_ADDRESS,
};

pub const COIN_MODULE_NAME: &IdentStr = ident_str!("Coin");
pub const COIN_STRUCT_NAME: &IdentStr = COIN_MODULE_NAME;
pub const COIN_JOIN_FUNC_NAME: &IdentStr = ident_str!("join_");
pub const COIN_SPLIT_VEC_FUNC_NAME: &IdentStr = ident_str!("split_vec");

// Rust version of the Move Sui::Coin::Coin type
#[derive(Debug, Serialize, Deserialize)]
pub struct Coin {
    id: VersionedID,
    value: u64,
}

impl Coin {
    pub fn new(id: VersionedID, value: u64) -> Self {
        Self { id, value }
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
        self.value
    }

    pub fn to_bcs_bytes(&self) -> Vec<u8> {
        bcs::to_bytes(&self).unwrap()
    }

    pub fn layout(type_param: StructTag) -> MoveStructLayout {
        MoveStructLayout::WithTypes {
            type_: Self::type_(type_param),
            fields: vec![
                MoveFieldLayout::new(
                    ident_str!("id").to_owned(),
                    MoveTypeLayout::Struct(VersionedID::layout()),
                ),
                MoveFieldLayout::new(ident_str!("value").to_owned(), MoveTypeLayout::U64),
            ],
        }
    }
}
