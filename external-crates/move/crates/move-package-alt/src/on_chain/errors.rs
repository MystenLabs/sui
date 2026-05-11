// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Error types for on-chain package fetching.

use std::path::PathBuf;

use thiserror::Error;

use crate::{package::package_lock::LockError, schema::PublishedID};

/// Errors that can occur while fetching an on-chain package.
#[derive(Error, Debug)]
pub enum OnChainError {
    #[error("failed to fetch on-chain package at {address}: {source}")]
    FetchFailed {
        address: PublishedID,
        #[source]
        source: anyhow::Error,
    },

    #[error("failed to write on-chain package cache at {path}: {source}")]
    CacheWriteFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse cached manifest at {path}: {source}")]
    ManifestParseFailed {
        path: PathBuf,
        #[source]
        source: toml_edit::de::Error,
    },

    #[error("failed to serialize manifest for on-chain package {address}: {source}")]
    ManifestSerializeFailed {
        address: PublishedID,
        #[source]
        source: toml_edit::ser::Error,
    },

    #[error(transparent)]
    LockFailed(#[from] LockError),
}

pub type OnChainResult<T> = Result<T, OnChainError>;
