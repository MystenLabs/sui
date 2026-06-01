// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `checkpoint_seq` → `StoredCheckpointSummary`.
//!
//! Holds the lightweight, signed checkpoint header. Contents — the
//! list of executed tx digests — live in
//! [`super::checkpoint_contents`](super::checkpoint_contents) so
//! summary-only lookups skip the larger payload.

use sui_consistent_store::Protobuf;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
use sui_types::crypto::AuthorityStrongQuorumSignInfo;
use sui_types::messages_checkpoint::CertifiedCheckpointSummary;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::CheckpointSummary;
use sui_types::messages_checkpoint::VerifiedCheckpoint;

use crate::proto::StoredCheckpointSummary;
use crate::schema::keys::U64Be;

pub const NAME: &str = "checkpoint_summary";

pub type Key = U64Be;
pub type Value = Protobuf<StoredCheckpointSummary>;

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}

/// Build a `StoredCheckpointSummary` row from a checkpoint
/// summary and its quorum signature.
///
/// BCS-encode failures here would indicate either OOM or a bug in
/// the types' `Serialize` impls; we panic rather than thread a
/// `Result` through every call site.
pub fn store(
    summary: &CheckpointSummary,
    signature: &AuthorityStrongQuorumSignInfo,
) -> Value {
    let summary_bcs = bcs::to_bytes(summary).expect("bcs encode CheckpointSummary");
    let signature_bcs = bcs::to_bytes(signature).expect("bcs encode AuthorityStrongQuorumSignInfo");
    Protobuf(StoredCheckpointSummary {
        summary_bcs: summary_bcs.into(),
        signature_bcs: signature_bcs.into(),
    })
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Look up the signed summary of a checkpoint by sequence
    /// number.
    ///
    /// Decodes the stored BCS payloads and rewraps them as a
    /// [`VerifiedCheckpoint`]. The "verified" tag is asserted via
    /// `new_unchecked`: we trust what we put in our own store.
    pub fn get_checkpoint_summary(
        &self,
        seq: CheckpointSequenceNumber,
    ) -> Result<Option<VerifiedCheckpoint>, Error> {
        let Some(stored) = self.checkpoint_summary.get(&U64Be(seq))? else {
            return Ok(None);
        };
        let stored = stored.into_inner();
        let summary: CheckpointSummary = bcs::from_bytes(&stored.summary_bcs)
            .map_err(|e| DecodeError::with_source("bcs decode CheckpointSummary", e))?;
        let signature: AuthorityStrongQuorumSignInfo = bcs::from_bytes(&stored.signature_bcs)
            .map_err(|e| {
                DecodeError::with_source("bcs decode AuthorityStrongQuorumSignInfo", e)
            })?;
        let certified = CertifiedCheckpointSummary::new_from_data_and_sig(summary, signature);
        Ok(Some(VerifiedCheckpoint::new_unchecked(certified)))
    }
}

#[cfg(test)]
mod tests {
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_types::crypto::AggregateAuthoritySignature;
    use sui_types::digests::CheckpointContentsDigest;
    use sui_types::gas::GasCostSummary;

    use super::*;
    use crate::RpcStoreSchema;

    fn fresh_db() -> (tempfile::TempDir, sui_consistent_store::Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    fn dummy_summary(seq: CheckpointSequenceNumber) -> CheckpointSummary {
        CheckpointSummary {
            epoch: 0,
            sequence_number: seq,
            network_total_transactions: 0,
            content_digest: CheckpointContentsDigest::random(),
            previous_digest: None,
            epoch_rolling_gas_cost_summary: GasCostSummary::default(),
            timestamp_ms: 0,
            checkpoint_commitments: Vec::new(),
            end_of_epoch_data: None,
            version_specific_data: Vec::new(),
        }
    }

    fn dummy_sig() -> AuthorityStrongQuorumSignInfo {
        // Synthetic placeholder values — we're verifying the
        // storage round-trip, not the signature's validity.
        AuthorityStrongQuorumSignInfo {
            epoch: 0,
            signature: AggregateAuthoritySignature::default(),
            signers_map: Default::default(),
        }
    }

    #[test]
    fn get_returns_none_for_unknown_seq() {
        let (_dir, _db, schema) = fresh_db();
        assert!(schema.get_checkpoint_summary(7).unwrap().is_none());
    }

    #[test]
    fn store_then_get_round_trips() {
        let (_dir, db, schema) = fresh_db();
        let summary = dummy_summary(42);
        let sig = dummy_sig();

        let mut batch = db.batch();
        batch
            .put(
                &schema.checkpoint_summary,
                &U64Be(42),
                &store(&summary, &sig),
            )
            .unwrap();
        batch.commit().unwrap();

        let read = schema
            .get_checkpoint_summary(42)
            .unwrap()
            .expect("checkpoint present");
        assert_eq!(read.data(), &summary);
        assert_eq!(read.auth_sig().epoch, sig.epoch);
        // RoaringBitmap doesn't impl PartialEq, but bcs bytes do —
        // round-trip both signatures through bcs and compare.
        let read_sig_bcs = bcs::to_bytes(read.auth_sig()).unwrap();
        let expected_sig_bcs = bcs::to_bytes(&sig).unwrap();
        assert_eq!(read_sig_bcs, expected_sig_bcs);
    }

    #[test]
    fn overwrite_replaces_previous() {
        let (_dir, db, schema) = fresh_db();
        let first = dummy_summary(42);
        let later = dummy_summary(42);
        let later_digest = later.content_digest;
        let sig = dummy_sig();

        let mut batch = db.batch();
        batch
            .put(
                &schema.checkpoint_summary,
                &U64Be(42),
                &store(&first, &sig),
            )
            .unwrap();
        batch
            .put(
                &schema.checkpoint_summary,
                &U64Be(42),
                &store(&later, &sig),
            )
            .unwrap();
        batch.commit().unwrap();

        let read = schema
            .get_checkpoint_summary(42)
            .unwrap()
            .expect("checkpoint present");
        assert_eq!(read.data().content_digest, later_digest);
    }
}
