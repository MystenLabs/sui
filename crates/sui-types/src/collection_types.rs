// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::base_types::ObjectID;
use crate::id::UID;

/// Rust version of the Move sui::vec_map::VecMap type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct VecMap<K, V> {
    pub contents: Vec<Entry<K, V>>,
}

impl<K: PartialEq, V> VecMap<K, V> {
    pub fn get(&self, key: &K) -> Option<&V> {
        self.contents
            .iter()
            .find_map(|entry| (&entry.key == key).then_some(&entry.value))
    }
}

/// Rust version of the Move sui::vec_map::Entry type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Entry<K, V> {
    pub key: K,
    pub value: V,
}

/// Rust version of the Move sui::vec_set::VecSet type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct VecSet<T> {
    pub contents: Vec<T>,
}

/// Rust version of the Move sui::table::Table type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct TableVec {
    pub contents: Table,
}

impl Default for TableVec {
    fn default() -> Self {
        TableVec {
            contents: Table {
                id: ObjectID::ZERO,
                size: 0,
            },
        }
    }
}

/// Rust version of the Move sui::table::Table type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Table {
    pub id: ObjectID,
    pub size: u64,
}

impl Default for Table {
    fn default() -> Self {
        Table {
            id: ObjectID::ZERO,
            size: 0,
        }
    }
}

/// Rust version of the Move sui::linked_table::LinkedTable type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct LinkedTable<K> {
    pub id: ObjectID,
    pub size: u64,
    pub head: Option<K>,
    pub tail: Option<K>,
}

impl<K> Default for LinkedTable<K> {
    fn default() -> Self {
        LinkedTable {
            id: ObjectID::ZERO,
            size: 0,
            head: None,
            tail: None,
        }
    }
}

/// Rust version of the Move sui::linked_table::Node type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct LinkedTableNode<K, V> {
    pub prev: Option<K>,
    pub next: Option<K>,
    pub value: V,
}

/// Rust version of the Move sui::bag::Bag type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Bag {
    pub id: UID,
    pub size: u64,
}

impl Default for Bag {
    fn default() -> Self {
        Self {
            id: UID::new(ObjectID::ZERO),
            size: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::VecMap;

    #[test]
    fn test_vec_map() {
        let vec_map = VecMap {
            contents: vec![
                ("key1".to_string(), "value1".to_string()),
                ("key2".to_string(), "value2".to_string()),
            ]
            .into_iter()
            .map(|(key, value)| super::Entry { key, value })
            .collect(),
        };

        assert_eq!(
            vec_map.get(&"key1".to_string()),
            Some(&"value1".to_string())
        );
        assert_eq!(
            vec_map.get(&"key2".to_string()),
            Some(&"value2".to_string())
        );
        assert_eq!(vec_map.get(&"key3".to_string()), None);
    }
}
