// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::anyhow;

/// Error type for DataLoader implementations that wraps Arc<anyhow::Error>
/// for efficient cloning while preserving ergonomic error handling.
#[derive(Debug, Clone, thiserror::Error)]
#[error(transparent)]
pub struct Error(#[from] Arc<anyhow::Error>);

impl From<anyhow::Error> for Error {
    fn from(error: anyhow::Error) -> Self {
        Error(Arc::new(error))
    }
}

impl From<tonic::Status> for Error {
    fn from(error: tonic::Status) -> Self {
        Error(Arc::new(anyhow!(error).context("gRPC error")))
    }
}
