// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! [`ReadStore`] adapter — checkpoints, committees, transactions,
//! effects, events.
//!
//! All point lookups delegate to the inherent helpers on
//! [`RpcStoreSchema`]. Trait methods that return [`Option`] suppress
//! storage errors and log them at `error` level; trait methods that
//! return [`Result`] surface them as
//! [`sui_types::storage::error::Error`] via `Error::custom`.

use std::sync::Arc;

use sui_consistent_store::reader::Reader;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::base_types::TransactionDigest;
use sui_types::committee::Committee;
use sui_types::committee::EpochId;
use sui_types::digests::CheckpointContentsDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::storage::ObjectKey;
use sui_types::storage::ReadStore;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::error::Result as StorageResult;
use sui_types::transaction::VerifiedTransaction;
use tracing::error;

use crate::reader::RpcStoreReader;
use crate::schema::primitives::U64Be;

impl<R: Reader + Send + Sync> ReadStore for RpcStoreReader<R> {
    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
        if !self.pipelines_available(&[crate::indexer::epochs::Epochs::NAME]) {
            return None;
        }
        match self.schema().get_committee(epoch) {
            Ok(Some(committee)) => Some(Arc::new(committee)),
            Ok(None) => None,
            Err(e) => {
                error!(epoch, "get_committee: {e:#}");
                None
            }
        }
    }

    fn get_latest_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        self.require_pipelines(&[crate::indexer::checkpoint_summary::CheckpointSummary::NAME])?;
        // The latest checkpoint header in `checkpoint_summary` is
        // the highest committed checkpoint. Read paths that require
        // every CF to be in sync at this checkpoint should be
        // routed through `at_snapshot` instead — there is no
        // ambient "min watermark across pipelines" guarantee here.
        let latest = self
            .schema()
            .checkpoint_summary
            .iter_rev(..)
            .map_err(StorageError::custom)?
            .next();
        let Some(entry) = latest else {
            return Err(StorageError::missing("no checkpoints in store"));
        };
        let (U64Be(seq), _) = entry.map_err(StorageError::custom)?;
        self.schema()
            .get_checkpoint_summary(seq)
            .map_err(StorageError::custom)?
            .ok_or_else(|| StorageError::missing(format!("checkpoint {seq} disappeared")))
    }

    fn get_highest_verified_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        // We only commit checkpoints that have been verified, so
        // "highest verified" coincides with "latest committed".
        self.get_latest_checkpoint()
    }

    fn get_highest_synced_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        // Likewise: we only commit checkpoints once their
        // contents/transactions/effects have been ingested, so
        // "highest synced" coincides with "latest committed". A
        // checkpoint header without its companion CFs is not a
        // state this store represents.
        self.get_latest_checkpoint()
    }

    fn get_lowest_available_checkpoint(&self) -> StorageResult<CheckpointSequenceNumber> {
        let watermarks = self
            .schema()
            .get_pruning_watermarks()
            .map_err(StorageError::custom)?
            .unwrap_or_default();
        Ok(watermarks.checkpoint_lo)
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        if !self.pipelines_available(&[
            crate::indexer::checkpoint_seq_by_digest::CheckpointSeqByDigest::NAME,
            crate::indexer::checkpoint_summary::CheckpointSummary::NAME,
        ]) {
            return None;
        }
        let seq = match self.schema().get_checkpoint_seq_by_digest(digest) {
            Ok(Some(seq)) => seq,
            Ok(None) => return None,
            Err(e) => {
                error!(?digest, "get_checkpoint_by_digest seq lookup: {e:#}");
                return None;
            }
        };
        match self.schema().get_checkpoint_summary(seq) {
            Ok(summary) => summary,
            Err(e) => {
                error!(seq, "get_checkpoint_by_digest summary lookup: {e:#}");
                None
            }
        }
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        if !self.pipelines_available(&[crate::indexer::checkpoint_summary::CheckpointSummary::NAME])
        {
            return None;
        }
        match self.schema().get_checkpoint_summary(sequence_number) {
            Ok(summary) => summary,
            Err(e) => {
                error!(sequence_number, "get_checkpoint_by_sequence_number: {e:#}");
                None
            }
        }
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        // Lookup by content digest would require a separate
        // `CheckpointContentsDigest → seq` index that this store
        // does not currently maintain. Callers that have a content
        // digest in hand are typically following a checkpoint
        // header they already located by sequence number; they
        // should use that sequence number directly.
        None
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        if !self
            .pipelines_available(&[crate::indexer::checkpoint_contents::CheckpointContents::NAME])
        {
            return None;
        }
        match self.schema().get_checkpoint_contents(sequence_number) {
            Ok(contents) => contents,
            Err(e) => {
                error!(
                    sequence_number,
                    "get_checkpoint_contents_by_sequence_number: {e:#}"
                );
                None
            }
        }
    }

    fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        if !self.pipelines_available(&[
            crate::indexer::tx_seq_by_digest::TxSeqByDigest::NAME,
            crate::indexer::transactions::Transactions::NAME,
        ]) {
            return None;
        }
        let tx_seq = match self.schema().get_tx_seq_by_digest(tx_digest) {
            Ok(Some(seq)) => seq,
            Ok(None) => return None,
            Err(e) => {
                error!(?tx_digest, "get_transaction seq lookup: {e:#}");
                return None;
            }
        };
        let (transaction, signatures) = match self.schema().get_transaction(tx_seq) {
            Ok(Some(pair)) => pair,
            Ok(None) => return None,
            Err(e) => {
                error!(tx_seq, "get_transaction data lookup: {e:#}");
                return None;
            }
        };
        let envelope =
            sui_types::transaction::Transaction::from_generic_sig_data(transaction, signatures);
        Some(Arc::new(VerifiedTransaction::new_unchecked(envelope)))
    }

    fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects> {
        if !self.pipelines_available(&[
            crate::indexer::tx_seq_by_digest::TxSeqByDigest::NAME,
            crate::indexer::effects::Effects::NAME,
        ]) {
            return None;
        }
        let tx_seq = match self.schema().get_tx_seq_by_digest(tx_digest) {
            Ok(Some(seq)) => seq,
            Ok(None) => return None,
            Err(e) => {
                error!(?tx_digest, "get_transaction_effects seq lookup: {e:#}");
                return None;
            }
        };
        match self.schema().get_effects(tx_seq) {
            Ok(Some((effects, _unchanged))) => Some(effects),
            Ok(None) => None,
            Err(e) => {
                error!(tx_seq, "get_transaction_effects: {e:#}");
                None
            }
        }
    }

    fn get_events(&self, event_digest: &TransactionDigest) -> Option<TransactionEvents> {
        if !self.pipelines_available(&[
            crate::indexer::tx_seq_by_digest::TxSeqByDigest::NAME,
            crate::indexer::events::Events::NAME,
        ]) {
            return None;
        }
        // `event_digest` is named for the trait but our index
        // resolves by transaction digest (events are keyed by
        // tx_seq in this store).
        let tx_seq = match self.schema().get_tx_seq_by_digest(event_digest) {
            Ok(Some(seq)) => seq,
            Ok(None) => return None,
            Err(e) => {
                error!(?event_digest, "get_events seq lookup: {e:#}");
                return None;
            }
        };
        match self.schema().get_events(tx_seq) {
            Ok(events) => events,
            Err(e) => {
                error!(tx_seq, "get_events: {e:#}");
                None
            }
        }
    }

    fn get_unchanged_loaded_runtime_objects(
        &self,
        digest: &TransactionDigest,
    ) -> Option<Vec<ObjectKey>> {
        if !self.pipelines_available(&[
            crate::indexer::tx_seq_by_digest::TxSeqByDigest::NAME,
            crate::indexer::effects::Effects::NAME,
        ]) {
            return None;
        }
        let tx_seq = match self.schema().get_tx_seq_by_digest(digest) {
            Ok(Some(seq)) => seq,
            Ok(None) => return None,
            Err(e) => {
                error!(
                    ?digest,
                    "get_unchanged_loaded_runtime_objects seq lookup: {e:#}"
                );
                return None;
            }
        };
        match self.schema().get_effects(tx_seq) {
            Ok(Some((_effects, unchanged))) => Some(unchanged),
            Ok(None) => None,
            Err(e) => {
                error!(tx_seq, "get_unchanged_loaded_runtime_objects: {e:#}");
                None
            }
        }
    }

    fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Option<CheckpointSequenceNumber> {
        if !self.pipelines_available(&[
            crate::indexer::tx_seq_by_digest::TxSeqByDigest::NAME,
            crate::indexer::tx_metadata_by_seq::TxMetadataBySeq::NAME,
        ]) {
            return None;
        }
        let tx_seq = match self.schema().get_tx_seq_by_digest(digest) {
            Ok(Some(seq)) => seq,
            Ok(None) => return None,
            Err(e) => {
                error!(?digest, "get_transaction_checkpoint seq lookup: {e:#}");
                return None;
            }
        };
        match self.schema().get_tx_metadata_by_seq(tx_seq) {
            Ok(Some(meta)) => Some(meta.checkpoint_seq),
            Ok(None) => None,
            Err(e) => {
                error!(tx_seq, "get_transaction_checkpoint: {e:#}");
                None
            }
        }
    }

    fn get_full_checkpoint_contents(
        &self,
        _sequence_number: Option<CheckpointSequenceNumber>,
        _digest: &CheckpointContentsDigest,
    ) -> Option<sui_types::messages_checkpoint::VersionedFullCheckpointContents> {
        // State-sync path. `VersionedFullCheckpointContents`
        // bundles transactions + signatures + effects for an entire
        // checkpoint; assembling it would require iterating the
        // checkpoint's contents and joining each row across
        // `transactions` + `effects`. Not on the rpc-api hot path,
        // so leave it as a follow-up.
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_types::crypto::AggregateAuthoritySignature;
    use sui_types::crypto::AuthorityStrongQuorumSignInfo;
    use sui_types::digests::CheckpointDigest;
    use sui_types::digests::TransactionDigest;
    use sui_types::gas::GasCostSummary;
    use sui_types::message_envelope::Message;
    use sui_types::messages_checkpoint::CheckpointSummary;
    use sui_types::storage::ReadStore;

    use crate::RpcStoreSchema;
    use crate::reader::RpcStoreReader;
    use crate::schema::checkpoint_contents;
    use crate::schema::checkpoint_seq_by_digest;
    use crate::schema::checkpoint_summary;
    use crate::schema::primitives::U64Be;
    use crate::schema::primitives::U64Varint;
    use crate::schema::pruning_watermark;

    fn dummy_summary(seq: u64) -> CheckpointSummary {
        CheckpointSummary {
            epoch: 0,
            sequence_number: seq,
            network_total_transactions: 0,
            content_digest: sui_types::digests::CheckpointContentsDigest::new([0; 32]),
            previous_digest: None,
            epoch_rolling_gas_cost_summary: GasCostSummary::default(),
            timestamp_ms: 0,
            checkpoint_commitments: vec![],
            end_of_epoch_data: None,
            version_specific_data: vec![],
        }
    }

    fn dummy_signature() -> AuthorityStrongQuorumSignInfo {
        AuthorityStrongQuorumSignInfo {
            epoch: 0,
            signature: AggregateAuthoritySignature::default(),
            signers_map: roaring::RoaringBitmap::new(),
        }
    }

    fn fresh_reader() -> (tempfile::TempDir, Db, RpcStoreReader) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        let reader = RpcStoreReader::new(db.clone(), Arc::new(schema));
        (dir, db, reader)
    }

    fn seed_checkpoint(db: &Db, reader: &RpcStoreReader, seq: u64) -> CheckpointSummary {
        let summary = dummy_summary(seq);
        let digest = summary.digest();
        let mut batch = db.batch();
        batch
            .put(
                &reader.schema().checkpoint_summary,
                &U64Be(seq),
                &checkpoint_summary::store(&summary, &dummy_signature()),
            )
            .unwrap();
        batch
            .put(
                &reader.schema().checkpoint_seq_by_digest,
                &checkpoint_seq_by_digest::Key(digest),
                &U64Varint(seq),
            )
            .unwrap();
        batch.commit().unwrap();
        summary
    }

    #[test]
    fn latest_checkpoint_errors_when_empty() {
        let (_dir, _db, reader) = fresh_reader();
        let err = reader.get_latest_checkpoint().unwrap_err();
        assert!(format!("{err:#}").contains("no checkpoints"));
    }

    #[test]
    fn latest_checkpoint_returns_highest_seq() {
        let (_dir, db, reader) = fresh_reader();
        seed_checkpoint(&db, &reader, 0);
        let s5 = seed_checkpoint(&db, &reader, 5);
        seed_checkpoint(&db, &reader, 3);

        let latest = reader.get_latest_checkpoint().unwrap();
        assert_eq!(latest.sequence_number(), s5.sequence_number());
    }

    #[test]
    fn lookup_by_digest_round_trips() {
        let (_dir, db, reader) = fresh_reader();
        let summary = seed_checkpoint(&db, &reader, 7);
        let digest: CheckpointDigest = summary.digest();
        let read = reader.get_checkpoint_by_digest(&digest).expect("present");
        assert_eq!(read.sequence_number(), summary.sequence_number());
    }

    #[test]
    fn lookup_by_digest_returns_none_for_unknown() {
        let (_dir, _db, reader) = fresh_reader();
        let digest = CheckpointDigest::new([9; 32]);
        assert!(reader.get_checkpoint_by_digest(&digest).is_none());
    }

    #[test]
    fn lowest_available_returns_zero_when_unset() {
        let (_dir, _db, reader) = fresh_reader();
        assert_eq!(reader.get_lowest_available_checkpoint().unwrap(), 0);
    }

    #[test]
    fn lowest_available_reflects_pruning_watermark() {
        let (_dir, db, reader) = fresh_reader();
        let mut batch = db.batch();
        batch
            .put(
                &reader.schema().pruning_watermark,
                &crate::schema::primitives::UnitKey,
                &pruning_watermark::store(&pruning_watermark::Watermarks {
                    tx_seq_lo: 100,
                    checkpoint_lo: 42,
                })
                .1,
            )
            .unwrap();
        batch.commit().unwrap();
        assert_eq!(reader.get_lowest_available_checkpoint().unwrap(), 42);
    }

    #[test]
    fn gated_checkpoint_reads_are_withheld() {
        use crate::config::PipelineAvailability;
        use crate::reader::availability;
        use sui_types::storage::error::Kind;

        let (_dir, db, reader) = fresh_reader();
        let summary = seed_checkpoint(&db, &reader, 7);
        let reader = reader.with_availability(availability(
            None,
            &[("checkpoint_summary", PipelineAvailability::Disabled)],
        ));

        // Result-returning read fails as unavailable...
        let err = reader.get_latest_checkpoint().unwrap_err();
        assert_eq!(err.kind(), Kind::Unavailable);

        // ...and Option-returning reads withhold the (present) row.
        assert!(reader.get_checkpoint_by_sequence_number(7).is_none());
        assert!(reader.get_checkpoint_by_digest(&summary.digest()).is_none());
    }

    #[test]
    fn contents_by_seq_round_trips() {
        let (_dir, db, reader) = fresh_reader();
        let contents =
            sui_types::messages_checkpoint::CheckpointContents::new_with_digests_only_for_tests(
                vec![sui_types::base_types::ExecutionDigests {
                    transaction: TransactionDigest::new([1; 32]),
                    effects: sui_types::digests::TransactionEffectsDigest::new([2; 32]),
                }],
            );
        let mut batch = db.batch();
        batch
            .put(
                &reader.schema().checkpoint_contents,
                &U64Be(11),
                &checkpoint_contents::store(&contents),
            )
            .unwrap();
        batch.commit().unwrap();

        let read = reader
            .get_checkpoint_contents_by_sequence_number(11)
            .expect("present");
        assert_eq!(read.digest(), contents.digest());
    }
}
