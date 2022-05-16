// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::StructTag;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, Bytes};

/// User-defined event emitted by executing Move code.
/// Executing a transaction produces an ordered log of these
#[serde_as]
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct Event {
    pub type_: StructTag,
    #[serde_as(as = "Bytes")]
    pub contents: Vec<u8>,
}

impl Event {
    pub fn new(type_: StructTag, contents: Vec<u8>) -> Self {
        Event { type_, contents }
    }
}
