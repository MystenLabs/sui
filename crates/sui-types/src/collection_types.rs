// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

/// Rust version of the Move sui::vec_map::VecMap type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct VecMap<K, V> {
    pub contents: Vec<Entry<K, V>>,
}

/// Rust version of the Move sui::vec_map::Entry type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Entry<K, V> {
    key: K,
    value: V,
}

/// Rust version of the Move sui::vec_set::VecSet type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct VecSet<T> {
    contents: Vec<T>,
}
