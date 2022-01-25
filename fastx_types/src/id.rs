// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{ident_str, identifier::IdentStr, language_storage::StructTag};
use serde::{Deserialize, Serialize};

use crate::{
    base_types::{ObjectID, SequenceNumber},
    FASTX_FRAMEWORK_ADDRESS,
};

pub const ID_MODULE_NAME: &IdentStr = ident_str!("ID");
pub const ID_STRUCT_NAME: &IdentStr = ID_MODULE_NAME;

/// Rust version of the Move FastX::ID::ID type
#[derive(Debug, Serialize, Deserialize)]
pub struct ID {
    id: IDBytes,
    version: u64,
}

/// Rust version of the Move FastX::ID::IDBytes type
#[derive(Debug, Serialize, Deserialize)]
struct IDBytes {
    bytes: ObjectID,
}

impl ID {
    pub fn new(bytes: ObjectID, version: SequenceNumber) -> Self {
        Self {
            id: IDBytes::new(bytes),
            version: version.value(),
        }
    }

    pub fn type_() -> StructTag {
        StructTag {
            address: FASTX_FRAMEWORK_ADDRESS,
            name: ID_STRUCT_NAME.to_owned(),
            module: ID_MODULE_NAME.to_owned(),
            type_params: Vec::new(),
        }
    }

    pub fn object_id(&self) -> &ObjectID {
        &self.id.bytes
    }

    pub fn version(&self) -> SequenceNumber {
        SequenceNumber::from(self.version)
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
