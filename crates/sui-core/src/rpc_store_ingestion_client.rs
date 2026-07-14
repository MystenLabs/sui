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
        let lowest = self
            .store
            .get_lowest_available_checkpoint()
            .map_err(|e| CheckpointError::Fetch(e.into()))?;
        if checkpoint < lowest {
            // The pruning gate holds the perpetual floor at or below every
            // embedded pipeline's committed watermark, so this firing means a
            // checkpoint was pruned out from under a pipeline that still
            // needs it -- a permanently unserveable request the framework
            // will retry forever, stalling that pipeline. This log line is
            // the stall's primary signal; keep it specific.
            tracing::error!(
                checkpoint,
                lowest_available = lowest,
                "the embedded rpc-store indexer requested a checkpoint below \
                 the perpetual store's pruning floor; the requesting pipeline \
                 cannot make progress",
            );
        }

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
    use super::*;
    use crate::rpc_store_test_utils::store_with;
    use crate::rpc_store_test_utils::test_chain_id;

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
