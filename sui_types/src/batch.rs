// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{AuthorityName, TransactionDigest};
use crate::crypto::{sha3_hash, AuthoritySignature, BcsSignable};
use roaring::RoaringBitmap;
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
pub struct TransactionBatch(pub Vec<(TxSequenceNumber, TransactionDigest)>);
impl BcsSignable for TransactionBatch {}

#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Default, Debug, Serialize, Deserialize)]
pub struct AuthorityBatch {
    // TODO: Add epoch
    /// The highest sequence number in this batch
    pub highest_sequence_number: u64,

    /// The lowest sequence number in this batch
    pub lowest_sequence_number: u64,

    // A compressed representation of the sequence numbers in this batch
    // We use the X-platform standard roaring bitmap format, see https://github.com/RoaringBitmap/RoaringFormatSpec
    pub seqnum_bitmap: Vec<u8>,

    /// The digest of the previous batch, if there is one
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
            highest_sequence_number: 0,
            lowest_sequence_number: 0,
            seqnum_bitmap: Vec::default(),
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
        let mut transaction_vec = transactions.to_vec();
        transaction_vec.sort_by(|(s_a, _d_a), (s_b, _d_b)| s_a.cmp(s_b));
        debug_assert!(!transaction_vec.is_empty());

        let lowest_sequence_number = transaction_vec[0].0 as u64;
        let highest_sequence_number = (transaction_vec[transaction_vec.len() - 1].0) as u64;

        let mut rb = RoaringBitmap::new();
        // insert the seq nums into the bitmap
        transaction_vec
            .iter()
            .map(|(seqnum, _)| seqnum - lowest_sequence_number)
            .for_each(|s| {
                let insertee =
                    u32::try_from(s).expect("batch contains more than u32::MAX elements!");
                rb.insert(insertee);
            });
        let mut bitmap_bytes = Vec::default();
        rb.serialize_into(&mut bitmap_bytes)
            .expect("Bitmap serialization failed!"); // the Vec is growable

        let to_hash = TransactionBatch(transaction_vec);
        let transactions_digest = sha3_hash(&to_hash);

        AuthorityBatch {
            highest_sequence_number,
            lowest_sequence_number,
            seqnum_bitmap: bitmap_bytes,
            previous_digest: Some(previous_batch.digest()),
            transactions_digest,
        }
    }

    pub fn seqnums(&self) -> Vec<u64> {
        let rb = RoaringBitmap::deserialize_from(&self.seqnum_bitmap[..])
            .expect("bitmap deserialization failed!");
        rb.iter()
            .map(|value| self.lowest_sequence_number + value as u64)
            .collect()
    }

    pub fn size(&self) -> u64 {
        let rb = RoaringBitmap::deserialize_from(&self.seqnum_bitmap[..])
            .expect("bitmap deserialization failed!");
        rb.len()
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
