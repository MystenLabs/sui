// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::base_types::{ObjectID, SuiAddress};

/// Rust version of the Move sui::vec_map::VecMap type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct VecMap<K, V> {
    pub contents: Vec<Entry<K, V>>,
}

/// Rust version of the Move sui::vec_map::Entry type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct Entry<K, V> {
    pub key: K,
    pub value: V,
}

/// Rust version of the Move sui::vec_set::VecSet type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct VecSet<T> {
    contents: Vec<T>,
}

/// Rust version of the Move std::option::Option type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct MoveOption<T> {
    pub vec: Vec<T>,
}

impl<T> MoveOption<T> {
    pub fn empty() -> Self {
        Self { vec: vec![] }
    }

    pub fn into_option(self) -> Option<T> {
        let Self { mut vec } = self;
        vec.pop()
    }
}

/// Rust version of the Move sui::table::Table type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct TableVec {
    pub contents: Table,
}

impl Default for TableVec {
    fn default() -> Self {
        TableVec {
            contents: Table {
                id: ObjectID::from(SuiAddress::ZERO),
                size: 0,
            },
        }
    }
}

/// Rust version of the Move sui::table::Table type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct Table {
    pub id: ObjectID,
    pub size: u64,
}

impl Default for Table {
    fn default() -> Self {
        Table {
            id: ObjectID::from(SuiAddress::ZERO),
            size: 0,
        }
    }
}

/// Rust version of the Move sui::linked_table::LinkedTable type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct LinkedTable<K> {
    pub id: ObjectID,
    pub size: u64,
    pub head: MoveOption<K>,
    pub tail: MoveOption<K>,
}

impl<K> Default for LinkedTable<K> {
    fn default() -> Self {
        LinkedTable {
            id: ObjectID::from(SuiAddress::ZERO),
            size: 0,
            head: MoveOption { vec: vec![] },
            tail: MoveOption { vec: vec![] },
        }
    }
}
