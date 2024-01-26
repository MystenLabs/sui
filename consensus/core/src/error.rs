// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::error::FastCryptoError;
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

    #[error("Failed to verify the block's signature with error: {0}")]
    SignatureVerificationFailure(#[from] FastCryptoError),

    #[error("Unknown authority provided: {0}")]
    UnknownAuthority(String),
}

#[allow(unused)]
pub type ConsensusResult<T> = Result<T, ConsensusError>;

#[macro_export]
macro_rules! bail {
    ($e:expr) => {
        return Err($e);
    };
}

#[macro_export(local_inner_macros)]
macro_rules! ensure {
    ($cond:expr, $e:expr) => {
        if !($cond) {
            bail!($e);
        }
    };
}
