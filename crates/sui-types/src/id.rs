// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::OnceLock;

use crate::MoveTypeTagTrait;
use crate::{SUI_FRAMEWORK_ADDRESS, base_types::ObjectID};
use move_core_types::account_address::AccountAddress;
use move_core_types::compressed::annotated::{self as CA, BackendBuilder as _, LayoutHandle};
use move_core_types::language_storage::TypeTag;
use move_core_types::{
    annotated_value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
    ident_str,
    identifier::IdentStr,
    language_storage::StructTag,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const OBJECT_MODULE_NAME_STR: &str = "object";
pub const OBJECT_MODULE_NAME: &IdentStr = ident_str!(OBJECT_MODULE_NAME_STR);
pub const UID_STRUCT_NAME: &IdentStr = ident_str!("UID");
pub const ID_STRUCT_NAME: &IdentStr = ident_str!("ID");
pub const RESOLVED_SUI_ID: (&AccountAddress, &IdentStr, &IdentStr) =
    (&SUI_FRAMEWORK_ADDRESS, OBJECT_MODULE_NAME, ID_STRUCT_NAME);

/// Rust version of the Move sui::object::Info type
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone, Eq, PartialEq)]
pub struct UID {
    pub id: ID,
}

/// Rust version of the Move sui::object::ID type
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone, Eq, PartialEq)]
#[serde(transparent)]
pub struct ID {
    pub bytes: ObjectID,
}

impl UID {
    pub fn new(bytes: ObjectID) -> Self {
        Self {
            id: { ID::new(bytes) },
        }
    }

    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: OBJECT_MODULE_NAME.to_owned(),
            name: UID_STRUCT_NAME.to_owned(),
            type_params: Vec::new(),
        }
    }

    pub fn object_id(&self) -> &ObjectID {
        &self.id.bytes
    }

    pub fn to_bcs_bytes(&self) -> Vec<u8> {
        bcs::to_bytes(&self).unwrap()
    }

    pub fn layout() -> MoveStructLayout {
        MoveStructLayout {
            type_: Self::type_(),
            fields: vec![MoveFieldLayout::new(
                ident_str!("id").to_owned(),
                MoveTypeLayout::Struct(Box::new(ID::layout())),
            )],
        }
    }

    pub fn compressed_layout() -> CA::MoveStructLayout<'static> {
        static CELL: OnceLock<CA::MoveTypeLayout> = OnceLock::new();
        let owned = CELL.get_or_init(|| {
            let tree = MoveTypeLayout::Struct(Box::new(Self::layout()));
            CA::MoveTypeLayout::try_from(&tree).unwrap()
        });
        owned.as_struct().expect("struct layout")
    }

    pub fn layout_for_builder(
        builder: &mut CA::MoveTypeLayoutBuilder,
    ) -> anyhow::Result<LayoutHandle> {
        let id_layout = ID::layout_for_builder(builder)?;
        let type_ = Self::type_();
        let id = ident_str!("id").to_owned();
        builder.struct_layout(&type_, &[(&id, id_layout)])
    }
}

impl ID {
    pub fn new(object_id: ObjectID) -> Self {
        Self { bytes: object_id }
    }

    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: OBJECT_MODULE_NAME.to_owned(),
            name: ID_STRUCT_NAME.to_owned(),
            type_params: Vec::new(),
        }
    }

    pub fn layout() -> MoveStructLayout {
        MoveStructLayout {
            type_: Self::type_(),
            fields: vec![MoveFieldLayout::new(
                ident_str!("bytes").to_owned(),
                MoveTypeLayout::Address,
            )],
        }
    }

    pub fn compressed_layout() -> CA::MoveStructLayout<'static> {
        static CELL: OnceLock<CA::MoveTypeLayout> = OnceLock::new();
        let owned = CELL.get_or_init(|| {
            let tree = MoveTypeLayout::Struct(Box::new(Self::layout()));
            CA::MoveTypeLayout::try_from(&tree).unwrap()
        });
        owned.as_struct().expect("struct layout")
    }

    pub fn layout_for_builder(
        builder: &mut CA::MoveTypeLayoutBuilder,
    ) -> anyhow::Result<LayoutHandle> {
        let address_layout = builder.address();
        let type_ = Self::type_();
        let bytes = ident_str!("bytes").to_owned();
        builder.struct_layout(&type_, &[(&bytes, address_layout)])
    }
}

impl MoveTypeTagTrait for ID {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(Self::type_()))
    }
}
