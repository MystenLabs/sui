// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Error types for on-chain package fetching.

use thiserror::Error;

use crate::{package::package_lock::LockError, schema::PublishedID};

/// Errors that can occur while fetching an on-chain package.
#[derive(Error, Debug)]
pub enum OnChainError {
    #[error("{source}")]
    Fetch {
        address: PublishedID,
        #[source]
        source: anyhow::Error,
    },

    #[error(transparent)]
    Lock(#[from] LockError),
}

pub type OnChainResult<T> = Result<T, OnChainError>;
