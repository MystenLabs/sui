// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::id::UID;
use crate::MOVE_STDLIB_ADDRESS;
use move_core_types::ident_str;
use move_core_types::language_storage::StructTag;
use move_core_types::value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout};

pub fn u64_field(field: &'static str) -> MoveFieldLayout {
    MoveFieldLayout::new(ident_str!(field).to_owned(), MoveTypeLayout::U64)
}

pub fn bool_field(field: &'static str) -> MoveFieldLayout {
    MoveFieldLayout::new(ident_str!(field).to_owned(), MoveTypeLayout::Bool)
}

pub fn vec_field(field: &'static str, element_type: MoveTypeLayout) -> MoveFieldLayout {
    MoveFieldLayout::new(
        ident_str!(field).to_owned(),
        MoveTypeLayout::Vector(Box::new(element_type)),
    )
}

pub fn vec_u8_field(field: &'static str) -> MoveFieldLayout {
    vec_field(field, MoveTypeLayout::U8)
}

pub fn string_field(field: &'static str) -> MoveFieldLayout {
    MoveFieldLayout::new(
        ident_str!(field).to_owned(),
        MoveTypeLayout::Struct(string_layout()),
    )
}

pub fn address_field(field: &'static str) -> MoveFieldLayout {
    MoveFieldLayout::new(ident_str!(field).to_owned(), MoveTypeLayout::Address)
}

pub fn uid_field(field: &'static str) -> MoveFieldLayout {
    struct_field(field, UID::layout())
}

pub fn struct_field(field: &'static str, struct_: MoveStructLayout) -> MoveFieldLayout {
    MoveFieldLayout::new(
        ident_str!(field).to_owned(),
        MoveTypeLayout::Struct(struct_),
    )
}

fn string_type() -> StructTag {
    StructTag {
        address: MOVE_STDLIB_ADDRESS,
        name: ident_str!("String").to_owned(),
        module: ident_str!("string").to_owned(),
        type_params: vec![],
    }
}

pub fn string_layout() -> MoveStructLayout {
    MoveStructLayout::WithTypes {
        type_: string_type(),
        fields: vec![vec_u8_field("bytes")],
    }
}
