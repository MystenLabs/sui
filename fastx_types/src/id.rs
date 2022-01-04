// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    account_address::AccountAddress, ident_str, identifier::IdentStr, language_storage::StructTag,
};
use serde::{Deserialize, Serialize};

use crate::base_types::ObjectID;

/// 0x3C2B307C3239F61643AF5E9A09D7D0C9
pub const ID_ADDRESS: AccountAddress = AccountAddress::new([
    0x3C, 0x2B, 0x30, 0x7C, 0x32, 0x39, 0xF6, 0x16, 0x43, 0xAF, 0x5E, 0x9A, 0x09, 0xD7, 0xD0, 0xC9,
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
