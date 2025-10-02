// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

/// Error type for DataLoader implementations that wraps Arc<anyhow::Error>
/// for efficient cloning while preserving ergonomic error handling.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Anyhow(Arc<anyhow::Error>),
    #[error("gRPC error: {0}")]
    Tonic(#[from] Arc<tonic::Status>),
}

impl From<anyhow::Error> for Error {
    fn from(error: anyhow::Error) -> Self {
        Error::Anyhow(Arc::new(error))
    }
}

impl From<tonic::Status> for Error {
    fn from(error: tonic::Status) -> Self {
        Error::Tonic(Arc::new(error))
    }
}
