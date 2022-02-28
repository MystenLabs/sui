// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::StructTag,
    value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
};
use serde::{Deserialize, Serialize};

use crate::{
    base_types::{ObjectID, SequenceNumber},
    SUI_FRAMEWORK_ADDRESS,
};

pub const ID_MODULE_NAME: &IdentStr = ident_str!("ID");
pub const VERSIONED_ID_STRUCT_NAME: &IdentStr = ident_str!("VersionedID");
pub const UNIQUE_ID_STRUCT_NAME: &IdentStr = ident_str!("UniqueID");
pub const ID_STRUCT_NAME: &IdentStr = ID_MODULE_NAME;

/// Rust version of the Move Sui::ID::VersionedID type
#[derive(Debug, Serialize, Deserialize)]
pub struct VersionedID {
    id: UniqueID,
    version: u64,
}

/// Rust version of the Move Sui::ID::UniqueID type
#[derive(Debug, Serialize, Deserialize)]
struct UniqueID {
    id: ID,
}

/// Rust version of the Move Sui::ID::ID type
#[derive(Debug, Serialize, Deserialize)]
struct ID {
    bytes: ObjectID,
}

impl VersionedID {
    pub fn new(bytes: ObjectID, version: SequenceNumber) -> Self {
        Self {
            id: UniqueID {
                id: { ID { bytes } },
            },
            version: version.value(),
        }
    }

    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: VERSIONED_ID_STRUCT_NAME.to_owned(),
            module: ID_MODULE_NAME.to_owned(),
            type_params: Vec::new(),
        }
    }

    pub fn object_id(&self) -> &ObjectID {
        &self.id.id.bytes
    }

    pub fn version(&self) -> SequenceNumber {
        SequenceNumber::from(self.version)
    }

    pub fn to_bcs_bytes(&self) -> Vec<u8> {
        bcs::to_bytes(&self).unwrap()
    }

    pub fn layout() -> MoveStructLayout {
        MoveStructLayout::WithTypes {
            type_: Self::type_(),
            fields: vec![
                MoveFieldLayout::new(
                    ident_str!("id").to_owned(),
                    MoveTypeLayout::Struct(UniqueID::layout()),
                ),
                MoveFieldLayout::new(ident_str!("version").to_owned(), MoveTypeLayout::U64),
            ],
        }
    }
}

impl UniqueID {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: UNIQUE_ID_STRUCT_NAME.to_owned(),
            module: ID_MODULE_NAME.to_owned(),
            type_params: Vec::new(),
        }
    }

    pub fn layout() -> MoveStructLayout {
        MoveStructLayout::WithTypes {
            type_: Self::type_(),
            fields: vec![MoveFieldLayout::new(
                ident_str!("id").to_owned(),
                MoveTypeLayout::Struct(ID::layout()),
            )],
        }
    }
}

impl ID {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: ID_STRUCT_NAME.to_owned(),
            module: ID_MODULE_NAME.to_owned(),
            type_params: Vec::new(),
        }
    }

    pub fn layout() -> MoveStructLayout {
        MoveStructLayout::WithTypes {
            type_: Self::type_(),
            fields: vec![MoveFieldLayout::new(
                ident_str!("bytes").to_owned(),
                MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
            )],
        }
    }
}
