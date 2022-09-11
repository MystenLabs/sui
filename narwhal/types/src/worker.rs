// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{Batch, BatchDigest};
use blake2::digest::Update;
use crypto::PublicKey;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(test)]
#[path = "tests/batch_serde.rs"]
mod batch_serde;

/// The message exchanged between workers.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkerMessage {
    /// Used by workers to send a new batch or to reply to a batch request.
    Batch(Batch),
    /// Used by workers to request batches.
    BatchRequest(Vec<BatchDigest>, /* origin */ PublicKey),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientBatchRequest(pub Vec<BatchDigest>);

/// Hashes a serialized batch message without deserializing it into a batch.
///
/// See the test `test_batch_and_serialized`, which guarantees that the output of this
/// function remains the same as the [`Hash::digest`] result you would get from [`Batch`].
/// See also the micro-benchmark `batch_digest`, which checks the performance of this is
/// identical to hashing a serialized batch.
///
/// TODO: remove the expects in the below, making this return a `Result` and correspondingly
/// doing error management at the callers. See #268
/// TODO: update batch hashing to reflect hashing fixed sequences of transactions, see #87.
pub fn serialized_batch_digest<K: AsRef<[u8]>>(sbm: K) -> Result<BatchDigest, DigestError> {
    let sbm = sbm.as_ref();
    let mut offset = 4; // skip the enum variant selector
    let num_transactions = u64::from_le_bytes(
        sbm[offset..offset + 8]
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
    Ok(BatchDigest::new(fastcrypto::blake2b_256(|hasher| {
        transactions.iter().for_each(|tx| hasher.update(tx))
    })))
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DigestError {
    #[error("Invalid argument: invalid byte at {0}")]
    InvalidArgumentError(usize),
}

fn read_one_transaction(sbm: &[u8], offset: usize) -> Result<(&[u8], usize), DigestError> {
    let length = u64::from_le_bytes(
        sbm[offset..offset + 8]
            .try_into()
            .map_err(|_| DigestError::InvalidArgumentError(offset))?,
    );
    let length = usize::try_from(length).map_err(|_| DigestError::InvalidArgumentError(offset))?;
    let end = offset + 8 + length;
    Ok((&sbm[offset + 8..end], end))
}
