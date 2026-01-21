// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_indexer_alt_framework::store::Store;
use sui_types::base_types::EpochId;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::schema::RowSchema;
use crate::store::AnalyticsStore;

/// Row types implement this to provide epoch and checkpoint information for batching.
/// Batches are committed at epoch boundaries to ensure files don't span epochs.
///
/// This trait requires `RowSchema + Send + Sync`, making `dyn Row` object-safe
/// and usable for dynamic dispatch in the analytics store.
pub trait Row: RowSchema + Send + Sync {
    fn get_epoch(&self) -> EpochId;
    fn get_checkpoint(&self) -> u64;
}

/// Private trait for type-erased row storage.
///
/// Allows storing `Vec<T>` without knowing `T` at compile time.
/// The conversion to `&dyn Row` happens lazily during iteration.
trait TypeErasedRows: Send + Sync {
    fn len(&self) -> usize;
    fn iter(&self) -> Box<dyn Iterator<Item = &dyn Row> + '_>;
}

impl<T: Row + 'static> TypeErasedRows for Vec<T> {
    fn len(&self) -> usize {
        Vec::len(self)
    }

    fn iter(&self) -> Box<dyn Iterator<Item = &dyn Row> + '_> {
        Box::new(self.as_slice().iter().map(|item| item as &dyn Row))
    }
}

/// Rows from a single checkpoint with metadata.
///
/// Stores rows in a type-erased container, allowing storage of `Vec<T>`
/// for any concrete row type T. The conversion to `&dyn Row` happens lazily
/// during iteration.
#[derive(Clone)]
pub struct CheckpointRows {
    pub checkpoint: u64,
    pub epoch: EpochId,
    rows: Arc<dyn TypeErasedRows>,
}

impl CheckpointRows {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.len() == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn Row> + '_ {
        self.rows.iter()
    }
}

/// Batch of rows grouped by checkpoint, ready for commit.
/// Non-generic: conversion to trait objects happens in batch(), not commit().
#[derive(Default)]
pub struct Batch {
    checkpoints: Vec<CheckpointRows>,
}

impl Batch {
    fn push(&mut self, checkpoint_rows: CheckpointRows) {
        self.checkpoints.push(checkpoint_rows);
    }

    fn as_slice(&self) -> &[CheckpointRows] {
        &self.checkpoints
    }
}

/// Generic wrapper that implements Handler for any Processor with analytics batching.
///
/// This adapter wraps a `Processor` and provides the common batching and commit
/// logic for writing analytics data to object stores. Uses a copy-on-write pattern
/// for atomic commits: accumulated rows are only swapped after successful upload.
///
/// Note: Accumulated rows and pipeline config are stored on the `AnalyticsStore`,
/// not on this handler. This makes handlers stateless and allows state to be shared.
pub struct AnalyticsHandler<P>
where
    P: Processor,
{
    processor: P,
}

impl<P: Processor> AnalyticsHandler<P>
where
    P::Value: Row,
{
    /// Create a new analytics handler and register the pipeline with the store.
    pub fn new(processor: P) -> Self {
        Self { processor }
    }
}

#[async_trait]
impl<P> Processor for AnalyticsHandler<P>
where
    P: Processor + Send + Sync,
    P::Value: Send + Sync,
{
    const NAME: &'static str = P::NAME;
    const FANOUT: usize = P::FANOUT;
    type Value = P::Value;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        self.processor.process(checkpoint).await
    }
}

#[async_trait]
impl<P, T> sequential::Handler for AnalyticsHandler<P>
where
    P: Processor<Value = T> + Send + Sync,
    T: Row + 'static,
{
    type Store = AnalyticsStore;
    type Batch = Batch;

    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Self::Value>) {
        let Some(first) = values.as_slice().first() else {
            return;
        };
        let checkpoint = first.get_checkpoint();
        let epoch = first.get_epoch();

        let rows: Vec<T> = values.collect();

        let checkpoint_rows = CheckpointRows {
            checkpoint,
            epoch,
            rows: Arc::new(rows) as Arc<dyn TypeErasedRows>,
        };

        batch.push(checkpoint_rows);
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> Result<usize> {
        conn.commit_batch::<P>(batch.as_slice()).await
    }
}
