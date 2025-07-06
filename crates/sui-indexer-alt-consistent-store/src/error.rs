// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("BCS error: {0}")]
    Bcs(#[from] bcs::Error),

    #[error("Bincode error: {0}")]
    Bincode(#[from] bincode::Error),

    #[error("Internal error: {0:?}")]
    Internal(#[from] anyhow::Error),

    #[error("Checkpoint {checkpoint} not in consistent range")]
    NotInRange { checkpoint: u64 },

    #[error("Storage error: {0}")]
    Storage(#[from] rocksdb::Error),
}
