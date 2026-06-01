// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `checkpoint_seq` → `StoredCheckpointSummary`.
//!
//! Holds the lightweight, signed checkpoint header. Contents — the
//! list of executed tx digests — live in
//! [`super::checkpoint_contents`](super::checkpoint_contents) so
//! summary-only lookups skip the larger payload.

use sui_consistent_store::Protobuf;

use crate::proto::StoredCheckpointSummary;
use crate::schema::keys::U64Be;

pub const NAME: &str = "checkpoint_summary";

pub type Key = U64Be;
pub type Value = Protobuf<StoredCheckpointSummary>;

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
