// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{AuthorityName, TransactionDigest};
use crate::crypto::{sha3_hash, AuthoritySignature, BcsSignable};
use serde::{Deserialize, Serialize};

pub type TxSequenceNumber = u64;

/// Either a freshly sequenced transaction hash or a batch
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum UpdateItem {
    Transaction((TxSequenceNumber, TransactionDigest)),
    Batch(SignedBatch),
}

pub type BatchDigest = [u8; 32];

#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Hash, Default, Debug, Serialize, Deserialize)]
pub struct TransactionBatch(Vec<(TxSequenceNumber, TransactionDigest)>);
impl BcsSignable for TransactionBatch {}

#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Default, Debug, Serialize, Deserialize)]
pub struct AuthorityBatch {
    // TODO: Add epoch
    /// The next sequence number after the end of this batch
    pub next_sequence_number: u64,

    /// The first sequence number of this batch
    pub initial_sequence_number: u64,

    // The number of items in the batch
    pub size: u64,

    /// The digest of the previous block, if there is one
    pub previous_digest: Option<BatchDigest>,

    // The digest of all transactions digests in this batch
    pub transactions_digest: [u8; 32],
}

impl BcsSignable for AuthorityBatch {}

impl AuthorityBatch {
    pub fn digest(&self) -> BatchDigest {
        sha3_hash(self)
    }

    /// The first batch for any authority indexes at zero
    /// and has zero length.
    pub fn initial() -> AuthorityBatch {
        let to_hash = TransactionBatch(Vec::new());
        let transactions_digest = sha3_hash(&to_hash);

        AuthorityBatch {
            next_sequence_number: 0,
            initial_sequence_number: 0,
            size: 0,
            previous_digest: None,
            transactions_digest,
        }
    }

    /// Make a batch, containing some transactions, and following the previous
    /// batch.
    pub fn make_next(
        previous_batch: &AuthorityBatch,
        transactions: &[(TxSequenceNumber, TransactionDigest)],
    ) -> AuthorityBatch {
        let transaction_vec = transactions.to_vec();
        debug_assert!(!transaction_vec.is_empty());

        let initial_sequence_number = transaction_vec[0].0 as u64;
        let next_sequence_number = (transaction_vec[transaction_vec.len() - 1].0 + 1) as u64;

        let to_hash = TransactionBatch(transaction_vec);
        let transactions_digest = sha3_hash(&to_hash);

        AuthorityBatch {
            next_sequence_number,
            initial_sequence_number,
            size: transactions.len() as u64,
            previous_digest: Some(previous_batch.digest()),
            transactions_digest,
        }
    }
}

/// An transaction signed by a single authority
#[derive(Eq, Clone, Debug, Serialize, Deserialize)]
pub struct SignedBatch {
    pub batch: AuthorityBatch,
    pub authority: AuthorityName,
    pub signature: AuthoritySignature,
}

impl SignedBatch {
    pub fn new(
        batch: AuthorityBatch,
        secret: &dyn signature::Signer<AuthoritySignature>,
        authority: AuthorityName,
    ) -> SignedBatch {
        SignedBatch {
            signature: AuthoritySignature::new(&batch, secret),
            batch,
            authority,
        }
    }
}

impl PartialEq for SignedBatch {
    fn eq(&self, other: &Self) -> bool {
        self.batch == other.batch && self.authority == other.authority
    }
}
