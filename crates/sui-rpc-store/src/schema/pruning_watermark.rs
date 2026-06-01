// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `()` → `PruningWatermarks`.
//!
//! Singleton row that holds the lowest still-available `tx_seq`,
//! `checkpoint_seq`, and `object_version`. Drives compaction filters
//! and serves `available_range` requests.

use sui_consistent_store::Protobuf;

use crate::proto::PruningWatermarks;
use crate::schema::keys::UnitKey;

pub const NAME: &str = "pruning_watermark";

pub type Key = UnitKey;
pub type Value = Protobuf<PruningWatermarks>;

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
