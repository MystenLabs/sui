// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `tx_seq` → `TxMeta`.
//!
//! Carries digest, containing checkpoint, position-within-checkpoint,
//! event count, and timestamp. The `tx_seq → digest` direction of the
//! bijection lives here; the inverse is
//! [`super::tx_seq_by_digest`](super::tx_seq_by_digest).

use sui_consistent_store::Protobuf;

use crate::proto::TxMeta;
use crate::schema::keys::U64Be;

pub const NAME: &str = "tx_meta_by_seq";

pub type Key = U64Be;
pub type Value = Protobuf<TxMeta>;

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
