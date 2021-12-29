// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    account_address::AccountAddress, ident_str, identifier::IdentStr, language_storage::StructTag,
};
use serde::{Deserialize, Serialize};

use crate::base_types::ObjectID;

/// 0x26580A83058312EAAD705393D6AE6B23
pub const ID_ADDRESS: AccountAddress = AccountAddress::new([
    0x26, 0x58, 0x0A, 0x83, 0x05, 0x83, 0x12, 0xEA, 0xAD, 0x70, 0x53, 0x93, 0xD6, 0xAE, 0x6B, 0x23,
]);
pub const ID_MODULE_NAME: &IdentStr = ident_str!("ID");
pub const ID_STRUCT_NAME: &IdentStr = ID_MODULE_NAME;

/// Rust version of the Move FastX::ID::ID type
#[derive(Debug, Serialize, Deserialize)]
pub struct ID {
    id: IDBytes,
}

/// Rust version of the Move FastX::ID::IDBytes type
#[derive(Debug, Serialize, Deserialize)]
struct IDBytes {
    bytes: ObjectID,
}

impl ID {
    pub fn new(bytes: ObjectID) -> Self {
        Self {
            id: IDBytes::new(bytes),
        }
    }

    pub fn type_() -> StructTag {
        StructTag {
            address: ID_ADDRESS,
            name: ID_STRUCT_NAME.to_owned(),
            module: ID_MODULE_NAME.to_owned(),
            type_params: Vec::new(),
        }
    }

    pub fn object_id(&self) -> &ObjectID {
        &self.id.bytes
    }

    pub fn to_bcs_bytes(&self) -> Vec<u8> {
        bcs::to_bytes(&self).unwrap()
    }
}

impl IDBytes {
    pub fn new(bytes: ObjectID) -> Self {
        Self { bytes }
    }
}
