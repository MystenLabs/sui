// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `tx_seq` → `StoredEvents`.

use sui_consistent_store::Protobuf;

use crate::proto::StoredEvents;
use crate::schema::keys::U64Be;

pub const NAME: &str = "events";

pub type Key = U64Be;
pub type Value = Protobuf<StoredEvents>;

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
