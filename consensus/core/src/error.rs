// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;
use typed_store::TypedStoreError;

/// Errors that can occur when processing blocks, reading from storage, or encountering shutdown.
#[allow(unused)]
#[derive(Clone, Debug, Error)]
pub enum ConsensusError {
    #[error("Error deserializing block: {0}")]
    MalformedBlock(#[from] bcs::Error),

    #[error("RocksDB failure: {0}")]
    RocksDBFailure(#[from] TypedStoreError),
}

#[allow(unused)]
pub type ConsensusResult<T> = Result<T, ConsensusError>;
