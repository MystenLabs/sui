// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{Batch, BatchDigest};

use fastcrypto::hash::HashFunction;
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

pub type TxResponse = tokio::sync::oneshot::Sender<BatchDigest>;
pub type PrimaryResponse = Option<tokio::sync::oneshot::Sender<()>>;

/// Hashes a serialized batch message without deserializing it into a batch.
///
/// See the test `test_batch_and_serialized`, which guarantees that the output of this
/// function remains the same as the [`fastcrypto::hash::Hash::digest`] result you would get from [`Batch`].
/// See also the micro-benchmark `batch_digest`, which checks the performance of this is
/// identical to hashing a serialized batch.
///
/// TODO: remove the expects in the below, making this return a `Result` and correspondingly
/// doing error management at the callers. See #268
/// TODO: update batch hashing to reflect hashing fixed sequences of transactions, see #87.
pub fn serialized_batch_digest<K: AsRef<[u8]>>(sbm: K) -> Result<BatchDigest, DigestError> {
    let sbm = sbm.as_ref();
    let mut offset = 0;
    let num_transactions = u64::from_le_bytes(
        sbm.get(offset..offset + 8)
            .ok_or(DigestError::InvalidLengthError)?
            .try_into()
            .map_err(|_| DigestError::InvalidArgumentError(offset))?,
    );
    offset += 8;
    let mut transactions = Vec::new();
    for _i in 0..num_transactions {
        let (tx_ref, new_offset) = read_one_transaction(sbm, offset)?;
        transactions.push(tx_ref);
        offset = new_offset;
    }
    Ok(BatchDigest::new(
        crypto::DefaultHashFunction::digest_iterator(transactions.iter()).into(),
    ))
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DigestError {
    #[error("Invalid argument: invalid byte at {0}")]
    InvalidArgumentError(usize),
    #[error("Invalid length")]
    InvalidLengthError,
}

fn read_one_transaction(sbm: &[u8], offset: usize) -> Result<(&[u8], usize), DigestError> {
    let length = u64::from_le_bytes(
        sbm.get(offset..offset + 8)
            .ok_or(DigestError::InvalidLengthError)?
            .try_into()
            .map_err(|_| DigestError::InvalidArgumentError(offset))?,
    );
    let length = usize::try_from(length).map_err(|_| DigestError::InvalidArgumentError(offset))?;
    let end = offset + 8 + length;
    Ok((&sbm[offset + 8..end], end))
}
