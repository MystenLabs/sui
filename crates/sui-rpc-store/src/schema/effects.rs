// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `tx_seq` → `StoredEffects`.

use sui_consistent_store::Protobuf;

use crate::proto::StoredEffects;
use crate::schema::keys::U64Be;

pub const NAME: &str = "effects";

pub type Key = U64Be;
pub type Value = Protobuf<StoredEffects>;

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
