// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `EpochId` → `StoredEpoch`.

use sui_consistent_store::Protobuf;

use crate::proto::StoredEpoch;
use crate::schema::keys::U64Be;

pub const NAME: &str = "epochs";

pub type Key = U64Be;
pub type Value = Protobuf<StoredEpoch>;

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
