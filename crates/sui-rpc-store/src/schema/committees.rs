// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `EpochId` → `StoredCommittee`.
//!
//! The validator committee active for each epoch.

use sui_consistent_store::Protobuf;

use crate::proto::StoredCommittee;
use crate::schema::keys::U64Be;

pub const NAME: &str = "committees";

pub type Key = U64Be;
pub type Value = Protobuf<StoredCommittee>;

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
