// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `EpochId` → `EpochInfo`.

use sui_consistent_store::Protobuf;

use crate::proto::EpochInfo;
use crate::schema::keys::U64Be;

pub const NAME: &str = "epoch_info";

pub type Key = U64Be;
pub type Value = Protobuf<EpochInfo>;

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
