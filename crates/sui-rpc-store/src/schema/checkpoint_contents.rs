// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `checkpoint_seq` → `StoredCheckpointContents`.
//!
//! Holds the ordered list of executed transaction digests for each
//! checkpoint.

use sui_consistent_store::Protobuf;

use crate::proto::StoredCheckpointContents;
use crate::schema::keys::U64Be;

pub const NAME: &str = "checkpoint_contents";

pub type Key = U64Be;
pub type Value = Protobuf<StoredCheckpointContents>;

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
