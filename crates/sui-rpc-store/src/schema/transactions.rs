// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `tx_seq` → `StoredTransaction`.

use sui_consistent_store::Protobuf;

use crate::proto::StoredTransaction;
use crate::schema::keys::U64Be;

pub const NAME: &str = "transactions";

pub type Key = U64Be;
pub type Value = Protobuf<StoredTransaction>;

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
