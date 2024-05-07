// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[non_exhaustive]
#[derive(Error, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Ord, PartialOrd)]
pub enum TypedStoreError {
    #[error("rocksdb error: {0}")]
    RocksDBError(String),
    #[error("(de)serialization error: {0}")]
    SerializationError(String),
    #[error("the column family {0} was not registered with the database")]
    UnregisteredColumn(String),
    #[error("a batch operation can't operate across databases")]
    CrossDBBatch,
    #[error("Metric reporting thread failed with error")]
    MetricsReporting,
    #[error("Transaction should be retried")]
    RetryableTransactionError,
}
