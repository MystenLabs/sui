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
use sui_indexer_alt_framework_store_traits::Connection;
use sui_indexer_alt_framework_store_traits::InitWatermark;
use sui_indexer_alt_framework_store_traits::PrunerWatermark;
use sui_indexer_alt_framework_store_traits::ReaderWatermark;
use sui_indexer_alt_framework_store_traits::Store;
use sui_indexer_alt_framework_store_traits::init_with_committer_watermark;

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
        init_watermark: InitWatermark,
    ) -> Result<InitWatermark> {
        init_with_committer_watermark(self, pipeline_task, init_watermark).await
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
