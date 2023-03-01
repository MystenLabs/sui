// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::SuiAddress;
use crate::common_move_layout::vec_field;
use crate::SUI_FRAMEWORK_ADDRESS;
use move_core_types::ident_str;
use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Rust version of the Move sui::vec_map::VecMap type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct VecMap<K, V> {
    pub contents: Vec<Entry<K, V>>,
}

impl<K, V> VecMap<K, V> {
    pub fn type_(k: TypeTag, v: TypeTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: ident_str!("VecMap").to_owned(),
            module: ident_str!("vec_map").to_owned(),
            type_params: vec![k, v],
        }
    }

    pub fn layout(k: (TypeTag, MoveTypeLayout), v: (TypeTag, MoveTypeLayout)) -> MoveStructLayout {
        MoveStructLayout::WithTypes {
            type_: Self::type_(k.0.clone(), v.0.clone()),
            fields: vec![vec_field(
                "contents",
                MoveTypeLayout::Struct(Entry::<K, V>::layout(k, v)),
            )],
        }
    }
}

/// Rust version of the Move sui::vec_map::Entry type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct Entry<K, V> {
    pub key: K,
    pub value: V,
}

impl<K, V> Entry<K, V> {
    pub fn type_(k: TypeTag, v: TypeTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: ident_str!("Entry").to_owned(),
            module: ident_str!("vec_map").to_owned(),
            type_params: vec![k, v],
        }
    }

    pub fn layout(k: (TypeTag, MoveTypeLayout), v: (TypeTag, MoveTypeLayout)) -> MoveStructLayout {
        MoveStructLayout::WithTypes {
            type_: Self::type_(k.0, v.0),
            fields: vec![
                MoveFieldLayout::new(ident_str!("key").to_owned(), k.1),
                MoveFieldLayout::new(ident_str!("value").to_owned(), v.1),
            ],
        }
    }
}

/// Rust version of the Move sui::vec_set::VecSet type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct VecSet<T> {
    contents: Vec<T>,
}

impl VecSet<SuiAddress> {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: ident_str!("VecSet").to_owned(),
            module: ident_str!("vec_set").to_owned(),
            type_params: vec![TypeTag::Address],
        }
    }

    pub fn layout() -> MoveStructLayout {
        MoveStructLayout::WithTypes {
            type_: Self::type_(),
            fields: vec![vec_field("contents", MoveTypeLayout::Address)],
        }
    }
}
