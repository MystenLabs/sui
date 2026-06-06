// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! [`IngestionClientTrait`] backed by a fullnode's local checkpoint and
//! perpetual stores.
//!
//! The embedded `sui-rpc-store` indexer fetches historical checkpoints by
//! sequence number -- to backfill the history cohort and to fill gaps the live
//! broadcast stream drops. On a fullnode every executed checkpoint's data
//! already lives in the local stores, so this client assembles a [`Checkpoint`]
//! from them via [`ReadStore::get_checkpoint_data`] rather than fetching from a
//! remote object store or gRPC endpoint.
//!
//! [`Checkpoint`]: sui_types::full_checkpoint_content::Checkpoint

use async_trait::async_trait;
use sui_indexer_alt_framework::ingestion::ingestion_client::CheckpointError;
use sui_indexer_alt_framework::ingestion::ingestion_client::CheckpointResult;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientTrait;
use sui_types::digests::ChainIdentifier;
use sui_types::storage::ReadStore;

/// An [`IngestionClientTrait`] over any [`ReadStore`]. In production `R` is the
/// fullnode's [`RocksDbStore`], which reads the local checkpoint and perpetual
/// stores; the generic bound keeps it unit-testable against an in-memory store.
///
/// `chain_id` is captured at construction rather than derived from the genesis
/// checkpoint, because genesis may have been pruned away on a long-running
/// fullnode.
///
/// [`RocksDbStore`]: crate::storage::RocksDbStore
pub struct PerpetualStoreIngestionClient<R> {
    store: R,
    chain_id: ChainIdentifier,
}

impl<R> PerpetualStoreIngestionClient<R> {
    pub fn new(store: R, chain_id: ChainIdentifier) -> Self {
        Self { store, chain_id }
    }
}

#[async_trait]
impl<R> IngestionClientTrait for PerpetualStoreIngestionClient<R>
where
    R: ReadStore + Send + Sync + 'static,
{
    async fn chain_id(&self) -> anyhow::Result<ChainIdentifier> {
        Ok(self.chain_id)
    }

    async fn checkpoint(&self, checkpoint: u64) -> CheckpointResult {
        // A checkpoint below the perpetual store's pruning watermark, or above
        // the highest executed one, is simply absent. Report it as NotFound so
        // the framework's `wait_for` can retry checkpoints that have not been
        // executed yet, while pruned checkpoints stay permanently unavailable.
        let summary = self
            .store
            .get_checkpoint_by_sequence_number(checkpoint)
            .ok_or(CheckpointError::NotFound)?;
        let contents = self
            .store
            .get_checkpoint_contents_by_digest(&summary.content_digest)
            .ok_or(CheckpointError::NotFound)?;
        self.store
            .get_checkpoint_data(summary, contents)
            .map_err(|e| CheckpointError::Fetch(anyhow::Error::from(e)))
    }

    async fn latest_checkpoint_number(&self) -> anyhow::Result<u64> {
        Ok(self.store.get_latest_checkpoint_sequence_number()?)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use sui_types::base_types::ObjectID;
    use sui_types::base_types::TransactionDigest;
    use sui_types::base_types::VersionNumber;
    use sui_types::committee::Committee;
    use sui_types::committee::EpochId;
    use sui_types::digests::CheckpointDigest;
    use sui_types::effects::TransactionEffects;
    use sui_types::effects::TransactionEvents;
    use sui_types::full_checkpoint_content::Checkpoint;
    use sui_types::messages_checkpoint::CheckpointContents;
    use sui_types::messages_checkpoint::CheckpointContentsDigest;
    use sui_types::messages_checkpoint::CheckpointSequenceNumber;
    use sui_types::messages_checkpoint::VerifiedCheckpoint;
    use sui_types::messages_checkpoint::VersionedFullCheckpointContents;
    use sui_types::object::Object;
    use sui_types::storage::ObjectKey;
    use sui_types::storage::ObjectStore;
    use sui_types::storage::error::Error as StorageError;
    use sui_types::storage::error::Result as StorageResult;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
    use sui_types::transaction::VerifiedTransaction;

    use super::*;

    const TEST_CHAIN_ID_BYTES: [u8; 32] = [7u8; 32];

    fn test_chain_id() -> ChainIdentifier {
        CheckpointDigest::new(TEST_CHAIN_ID_BYTES).into()
    }

    /// In-memory [`ReadStore`] holding a handful of pre-built full checkpoints.
    /// Only the methods [`PerpetualStoreIngestionClient`] exercises are
    /// implemented; the rest panic so a future change that starts relying on
    /// them is caught loudly.
    struct MockReadStore {
        checkpoints: BTreeMap<CheckpointSequenceNumber, Checkpoint>,
        /// When set, `get_checkpoint_contents_by_digest` returns `None` for this
        /// checkpoint even though its summary is present, exercising the
        /// summary-without-contents NotFound path.
        drop_contents_for: Option<CheckpointSequenceNumber>,
    }

    fn store_with(seqs: impl IntoIterator<Item = u64>) -> MockReadStore {
        let checkpoints = seqs
            .into_iter()
            .map(|seq| (seq, TestCheckpointBuilder::new(seq).build_checkpoint()))
            .collect();
        MockReadStore {
            checkpoints,
            drop_contents_for: None,
        }
    }

    impl ObjectStore for MockReadStore {
        fn get_object(&self, _: &ObjectID) -> Option<Object> {
            unimplemented!("ingestion client never loads objects directly")
        }

        fn get_object_by_key(&self, _: &ObjectID, _: VersionNumber) -> Option<Object> {
            unimplemented!("ingestion client never loads objects directly")
        }
    }

    impl ReadStore for MockReadStore {
        fn get_checkpoint_by_sequence_number(
            &self,
            sequence_number: CheckpointSequenceNumber,
        ) -> Option<VerifiedCheckpoint> {
            self.checkpoints
                .get(&sequence_number)
                .map(|cp| VerifiedCheckpoint::new_unchecked(cp.summary.clone()))
        }

        fn get_checkpoint_contents_by_digest(
            &self,
            digest: &CheckpointContentsDigest,
        ) -> Option<CheckpointContents> {
            self.checkpoints.values().find_map(|cp| {
                (cp.summary.content_digest == *digest
                    && self.drop_contents_for != Some(*cp.summary.sequence_number()))
                .then(|| cp.contents.clone())
            })
        }

        fn get_checkpoint_data(
            &self,
            checkpoint: VerifiedCheckpoint,
            _contents: CheckpointContents,
        ) -> StorageResult<Checkpoint> {
            Ok(self.checkpoints[checkpoint.sequence_number()].clone())
        }

        fn get_latest_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
            self.checkpoints
                .values()
                .next_back()
                .map(|cp| VerifiedCheckpoint::new_unchecked(cp.summary.clone()))
                .ok_or_else(|| StorageError::missing("no checkpoints"))
        }

        fn get_committee(&self, _: EpochId) -> Option<Arc<Committee>> {
            unimplemented!()
        }

        fn get_highest_verified_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
            unimplemented!()
        }

        fn get_highest_synced_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
            unimplemented!()
        }

        fn get_lowest_available_checkpoint(&self) -> StorageResult<CheckpointSequenceNumber> {
            unimplemented!()
        }

        fn get_checkpoint_by_digest(&self, _: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
            unimplemented!()
        }

        fn get_checkpoint_contents_by_sequence_number(
            &self,
            _: CheckpointSequenceNumber,
        ) -> Option<CheckpointContents> {
            unimplemented!()
        }

        fn get_transaction(&self, _: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
            unimplemented!()
        }

        fn get_transaction_effects(&self, _: &TransactionDigest) -> Option<TransactionEffects> {
            unimplemented!()
        }

        fn get_events(&self, _: &TransactionDigest) -> Option<TransactionEvents> {
            unimplemented!()
        }

        fn get_unchanged_loaded_runtime_objects(
            &self,
            _: &TransactionDigest,
        ) -> Option<Vec<ObjectKey>> {
            unimplemented!()
        }

        fn get_transaction_checkpoint(
            &self,
            _: &TransactionDigest,
        ) -> Option<CheckpointSequenceNumber> {
            unimplemented!()
        }

        fn get_full_checkpoint_contents(
            &self,
            _: Option<CheckpointSequenceNumber>,
            _: &CheckpointContentsDigest,
        ) -> Option<VersionedFullCheckpointContents> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn chain_id_returns_configured_value() {
        let client = PerpetualStoreIngestionClient::new(store_with([]), test_chain_id());
        assert_eq!(client.chain_id().await.unwrap(), test_chain_id());
    }

    #[tokio::test]
    async fn checkpoint_missing_is_not_found() {
        let client = PerpetualStoreIngestionClient::new(store_with([1, 2]), test_chain_id());
        assert!(matches!(
            client.checkpoint(5).await,
            Err(CheckpointError::NotFound)
        ));
    }

    #[tokio::test]
    async fn checkpoint_present_round_trips() {
        let client = PerpetualStoreIngestionClient::new(store_with([0, 1, 2]), test_chain_id());
        let cp = client.checkpoint(1).await.unwrap();
        assert_eq!(*cp.summary.sequence_number(), 1);
    }

    #[tokio::test]
    async fn checkpoint_summary_without_contents_is_not_found() {
        let mut store = store_with([3]);
        store.drop_contents_for = Some(3);
        let client = PerpetualStoreIngestionClient::new(store, test_chain_id());
        assert!(matches!(
            client.checkpoint(3).await,
            Err(CheckpointError::NotFound)
        ));
    }

    #[tokio::test]
    async fn latest_checkpoint_number_is_highest_executed() {
        let client = PerpetualStoreIngestionClient::new(store_with([0, 4, 9]), test_chain_id());
        assert_eq!(client.latest_checkpoint_number().await.unwrap(), 9);
    }
}
