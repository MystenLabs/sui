// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! BigTable Store implementation for sui-indexer-alt-framework.
//!
//! This implements the `Store` and `Connection` traits to allow the new framework
//! to use BigTable for watermark storage. Per-pipeline watermarks are stored in
//! the `watermarks` table, with fallback to the legacy `watermark_alt` table for
//! migration support.

use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use sui_indexer_alt_framework_store_traits::Connection;
use sui_indexer_alt_framework_store_traits::PrunerWatermark;
use sui_indexer_alt_framework_store_traits::ReaderWatermark;
use sui_indexer_alt_framework_store_traits::Store;

use crate::KeyValueStoreReader as _;
use crate::PipelineWatermark;
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
        _default_next_checkpoint: u64,
    ) -> Result<Option<u64>> {
        // First check if we have a pipeline-specific watermark
        if let Some(watermark) = self.client.get_pipeline_watermark(pipeline_task).await? {
            return Ok(Some(watermark.checkpoint_hi_inclusive));
        }

        // If no pipeline watermark exists, check the legacy watermark_alt table
        // and use it as the initial watermark for migration.
        let next_checkpoint = self.client.get_latest_checkpoint().await?;
        if next_checkpoint == 0 {
            Ok(None)
        } else {
            // Return the last processed checkpoint (next - 1)
            Ok(Some(next_checkpoint - 1))
        }
    }

    async fn committer_watermark(
        &mut self,
        pipeline_task: &str,
    ) -> Result<Option<CommitterWatermark>> {
        // First check for pipeline-specific watermark
        if let Some(watermark) = self.client.get_pipeline_watermark(pipeline_task).await? {
            return Ok(Some(watermark.into()));
        }

        // Fall back to legacy watermark
        let next_checkpoint = self.client.get_latest_checkpoint().await?;
        if next_checkpoint == 0 {
            Ok(None)
        } else {
            Ok(Some(CommitterWatermark {
                epoch_hi_inclusive: 0,
                checkpoint_hi_inclusive: next_checkpoint - 1,
                tx_hi: 0,
                timestamp_ms_hi_inclusive: 0,
            }))
        }
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline_task: &str,
        watermark: CommitterWatermark,
    ) -> Result<bool> {
        let pipeline_watermark: PipelineWatermark = watermark.into();
        self.client
            .set_pipeline_watermark(pipeline_task, &pipeline_watermark)
            .await?;
        Ok(true)
    }

    // Phase 1: Return Ok(None) - reader/pruner watermarks not needed for concurrent
    // pipelines without pruning.

    async fn reader_watermark(
        &mut self,
        _pipeline: &'static str,
    ) -> Result<Option<ReaderWatermark>> {
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
