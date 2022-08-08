// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{AuthorityName, ExecutionDigests};
use crate::crypto::{sha3_hash, AuthoritySignInfo, AuthoritySignature, VerificationObligation};
use crate::error::{SuiError, SuiResult};
use crate::message_envelope::{Envelope, Message};
use serde::{Deserialize, Serialize};

pub type TxSequenceNumber = u64;

/// Either a freshly sequenced transaction/effects tuple of hashes or a batch
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum UpdateItem {
    Transaction((TxSequenceNumber, ExecutionDigests)),
    Batch(SignedBatch),
}

pub type BatchDigest = [u8; 32];

#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Hash, Default, Debug, Serialize, Deserialize)]
pub struct TransactionBatch(pub Vec<(TxSequenceNumber, ExecutionDigests)>);

#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Default, Debug, Serialize, Deserialize)]
pub struct AuthorityBatch {
    // TODO: Add epoch, and update follower_store.rs to store the epoch of the last seen batch
    /// The next sequence number after the end of this batch
    pub next_sequence_number: u64,

    /// The first sequence number of this batch
    pub initial_sequence_number: u64,

    // The number of items in the batch
    pub size: u64,

    /// The digest of the previous block, if there is one
    pub previous_digest: Option<BatchDigest>,

    /// The digest of all transactions digests in this batch
    pub transactions_digest: [u8; 32],
}

impl AuthorityBatch {
    /// The first batch for any authority indexes at zero
    /// and has zero length.
    pub fn initial() -> AuthorityBatch {
        let transaction_batch = TransactionBatch(Vec::new());
        let transactions_digest = sha3_hash(&transaction_batch);

        AuthorityBatch {
            next_sequence_number: 0,
            initial_sequence_number: 0,
            size: 0,
            previous_digest: None,
            transactions_digest,
        }
    }

    /// Make a batch, containing some transaction/effects, and following the previous
    /// batch.
    pub fn make_next(
        previous_batch: &AuthorityBatch,
        transactions: &[(TxSequenceNumber, ExecutionDigests)],
    ) -> Result<AuthorityBatch, SuiError> {
        let len = transactions.len();
        if len == 0 {
            return Err(SuiError::GenericAuthorityError {
                error: "Transaction number must be positive.".to_string(),
            });
        };
        // TODO: ensure batches are always contiguous with previous
        // if transaction_vec[0].0 != previous_batch.next_sequence_number {
        //     return Err(SuiError::GenericAuthorityError {
        //         error: "Transactions sequence not contiguous with last batch.".to_string(),
        //     });
        // }

        let initial_sequence_number = transactions[0].0 as u64;
        let next_sequence_number = (transactions[len - 1].0 + 1) as u64;

        let transaction_batch = TransactionBatch(transactions.to_vec());
        let transactions_digest = sha3_hash(&transaction_batch);

        Ok(AuthorityBatch {
            next_sequence_number,
            initial_sequence_number,
            size: len as u64,
            previous_digest: Some(previous_batch.digest()),
            transactions_digest,
        })
    }
}

impl Message for AuthorityBatch {
    type DigestType = BatchDigest;

    fn digest(&self) -> Self::DigestType {
        sha3_hash(self)
    }

    fn verify(&self) -> SuiResult {
        fp_ensure!(
            self.initial_sequence_number <= self.next_sequence_number,
            SuiError::from("Invalid AuthorityBatch sequence number")
        );
        fp_ensure!(
            self.next_sequence_number - self.initial_sequence_number >= self.size,
            SuiError::from("Invalid AuthorityBatch size")
        );
        Ok(())
    }

    fn add_to_verification_obligation(&self, _: &mut VerificationObligation) -> SuiResult<()> {
        Ok(())
    }
}

pub type SignedBatch = Envelope<AuthorityBatch, AuthoritySignInfo>;

impl SignedBatch {
    pub fn new_with_zero_epoch(
        batch: AuthorityBatch,
        secret: &dyn signature::Signer<AuthoritySignature>,
        authority: AuthorityName,
    ) -> SignedBatch {
        Self::new(0, batch, secret, authority)
    }
}
