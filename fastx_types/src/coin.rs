// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    account_address::AccountAddress,
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};

use crate::{
    base_types::{ObjectID, SequenceNumber},
    gas_coin::{GAS_ADDRESS, GAS_MODULE_NAME, GAS_STRUCT_NAME},
    id::ID,
};

/// 0x2CD564FF647DB701AFE7B1E8A3F1A31B
pub const COIN_ADDRESS: AccountAddress = AccountAddress::new([
    0x2C, 0xD5, 0x64, 0xFF, 0x64, 0x7D, 0xB7, 0x01, 0xAF, 0xE7, 0xB1, 0xE8, 0xA3, 0xF1, 0xA3, 0x1B,
]);
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
            address: GAS_ADDRESS,
            name: GAS_STRUCT_NAME.to_owned(),
            module: GAS_MODULE_NAME.to_owned(),
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
