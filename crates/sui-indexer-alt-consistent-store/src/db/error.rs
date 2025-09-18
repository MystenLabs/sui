// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("BCS error: {0}")]
    Bcs(#[from] bcs::Error),

    #[error("Key decode error: {0}")]
    KeyDecode(#[from] bincode::error::DecodeError),

    #[error("Key encode error: {0}")]
    KeyEncode(#[from] bincode::error::EncodeError),

    #[error("Internal error: {0:?}")]
    Internal(#[from] anyhow::Error),

    #[error("No such column family: {0:?}")]
    NoColumnFamily(String),

    #[error("Checkpoint {checkpoint} not in consistent range")]
    NotInRange { checkpoint: u64 },

    #[error("Storage error: {0}")]
    Storage(#[from] rocksdb::Error),
}
