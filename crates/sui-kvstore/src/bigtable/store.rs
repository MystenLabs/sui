// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! BigTable Store implementation for sui-indexer-alt-framework.
//!
//! This implements the `Store` and `Connection` traits to allow the new framework
//! to use BigTable for watermark storage. Per-pipeline watermarks are stored in
//! the `watermark_alt` table.

use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use sui_indexer_alt_framework_store_traits::ConcurrentConnection;
use sui_indexer_alt_framework_store_traits::Connection;
use sui_indexer_alt_framework_store_traits::InitWatermark;
use sui_indexer_alt_framework_store_traits::PrunerWatermark;
use sui_indexer_alt_framework_store_traits::ReaderWatermark;
use sui_indexer_alt_framework_store_traits::Store;

use crate::Watermark;
use crate::bigtable::client::BigTableClient;

/// A Store implementation backed by BigTable.
#[derive(Clone)]
pub struct BigTableStore {
    client: BigTableClient,
}

/// A connection to BigTable for watermark operations and data writes.
pub struct BigTableConnection<'a> {
    client: BigTableClient,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl BigTableStore {
    pub fn new(client: BigTableClient) -> Self {
        Self { client }
    }
}

impl BigTableConnection<'_> {
    /// Returns a mutable reference to the underlying BigTable client.
    pub fn client(&mut self) -> &mut BigTableClient {
        &mut self.client
    }
}

#[async_trait]
impl sui_indexer_alt_framework_store_traits::ConcurrentStore for BigTableStore {
    type ConcurrentConnection<'c> = BigTableConnection<'c>;
}

#[async_trait]
impl Store for BigTableStore {
    type Connection<'c> = BigTableConnection<'c>;

    async fn connect<'c>(&'c self) -> Result<Self::Connection<'c>> {
        Ok(BigTableConnection {
            client: self.client.clone(),
            _marker: std::marker::PhantomData,
        })
    }
}

#[async_trait]
impl Connection for BigTableConnection<'_> {
    async fn init_watermark(
        &mut self,
        pipeline_task: &str,
        _checkpoint_hi_inclusive: Option<u64>,
    ) -> Result<Option<InitWatermark>> {
        Ok(self
            .committer_watermark(pipeline_task)
            .await?
            .map(|w| InitWatermark {
                checkpoint_hi_inclusive: Some(w.checkpoint_hi_inclusive),
                reader_lo: None,
            }))
    }

    async fn accepts_chain_id(
        &mut self,
        _pipeline_task: &str,
        _chain_id: [u8; 32],
    ) -> Result<bool> {
        // TODO: Implement storing chain_id
        Ok(true)
    }

    async fn committer_watermark(
        &mut self,
        pipeline_task: &str,
    ) -> Result<Option<CommitterWatermark>> {
        Ok(self
            .client
            .get_pipeline_watermark(pipeline_task)
            .await?
            .map(Into::into))
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline_task: &str,
        watermark: CommitterWatermark,
    ) -> Result<bool> {
        let pipeline_watermark: Watermark = watermark.into();
        self.client
            .set_pipeline_watermark(pipeline_task, &pipeline_watermark)
            .await?;
        Ok(true)
    }
}

#[async_trait]
impl ConcurrentConnection for BigTableConnection<'_> {
    async fn reader_watermark(&mut self, _pipeline: &str) -> Result<Option<ReaderWatermark>> {
        Ok(None)
    }

    async fn pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        _delay: Duration,
    ) -> Result<Option<PrunerWatermark>> {
        Ok(None)
    }

    async fn set_reader_watermark(
        &mut self,
        _pipeline: &'static str,
        _reader_lo: u64,
    ) -> Result<bool> {
        Ok(false)
    }

    async fn set_pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        _pruner_hi: u64,
    ) -> Result<bool> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::testing::BigTableEmulator;
    use crate::testing::INSTANCE_ID;
    use crate::testing::create_tables;
    use crate::testing::require_bigtable_emulator;

    const PIPELINE: &str = "pipeline";
    const EPOCH_HI: u64 = 7;
    const CHECKPOINT_HI: u64 = 200;
    const TX_HI: u64 = 42;
    const TIMESTAMP_MS_HI: u64 = 99;

    /// Spawn a BigTable emulator and return a connected store.
    async fn store_conn() -> (BigTableEmulator, BigTableStore) {
        require_bigtable_emulator();
        let emulator = tokio::task::spawn_blocking(BigTableEmulator::start)
            .await
            .unwrap()
            .unwrap();
        create_tables(emulator.host(), INSTANCE_ID).await.unwrap();
        let client = BigTableClient::new_local(emulator.host().to_string(), INSTANCE_ID.into())
            .await
            .unwrap();
        (emulator, BigTableStore::new(client))
    }

    #[tokio::test]
    async fn test_init_watermark_returns_existing_on_conflict() {
        let (_emulator, store) = store_conn().await;
        let mut conn = store.connect().await.unwrap();

        let watermark = CommitterWatermark {
            epoch_hi_inclusive: EPOCH_HI,
            checkpoint_hi_inclusive: CHECKPOINT_HI,
            tx_hi: TX_HI,
            timestamp_ms_hi_inclusive: TIMESTAMP_MS_HI,
        };
        conn.set_committer_watermark(PIPELINE, watermark)
            .await
            .unwrap();

        // init must surface the existing committer watermark regardless of the input.
        let init = conn
            .init_watermark(PIPELINE, Some(0))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(init.checkpoint_hi_inclusive, Some(CHECKPOINT_HI));
        // BigTable has no trailing-edge / reader watermark concept.
        assert_eq!(init.reader_lo, None);
    }
}
