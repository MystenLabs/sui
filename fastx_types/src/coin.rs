// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};

use crate::{
    base_types::{ObjectID, SequenceNumber},
    id::ID,
    FASTX_FRAMEWORK_ADDRESS,
};

pub const COIN_MODULE_NAME: &IdentStr = ident_str!("Coin");
pub const COIN_STRUCT_NAME: &IdentStr = COIN_MODULE_NAME;

// Rust version of the Move FastX::Coin::Coin type
#[derive(Debug, Serialize, Deserialize)]
pub struct Coin {
    id: ID,
    value: u64,
}

impl Coin {
    pub fn new(id: ID, value: u64) -> Self {
        Self { id, value }
    }

    pub fn type_(type_param: StructTag) -> StructTag {
        StructTag {
            address: FASTX_FRAMEWORK_ADDRESS,
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
}
