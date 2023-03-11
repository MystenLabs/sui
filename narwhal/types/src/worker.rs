// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{Batch, BatchDigest};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(test)]
#[path = "tests/batch_serde.rs"]
mod batch_serde;

/// Used by workers to send a new batch.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerBatchMessage {
    pub batch: Batch,
}

/// Used by primary to ask worker for the request.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestBatchRequest {
    pub batch: BatchDigest,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestBatchResponse {
    pub batch: Option<Batch>,
}

/// Used by primary to bulk request batches.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestBatchesRequest {
    pub batches: Vec<BatchDigest>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestBatchesResponse {
    pub batches: Vec<Option<Batch>>,
}

pub type TxResponse = tokio::sync::oneshot::Sender<BatchDigest>;
pub type PrimaryResponse = Option<tokio::sync::oneshot::Sender<()>>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DigestError {
    #[error("Invalid argument: invalid byte at {0}")]
    InvalidArgumentError(usize),
    #[error("Invalid length")]
    InvalidLengthError,
}
