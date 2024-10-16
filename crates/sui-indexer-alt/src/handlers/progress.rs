// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::db::{Connection, Db};
use crate::handlers::{Handler, IndexedMetadata};
use crate::models::watermarks::StoredWatermark;
use crate::schema::watermarks;
use diesel::upsert::excluded;
use diesel::ExpressionMethods;
use diesel::{OptionalExtension, QueryDsl};
use diesel_async::RunQueryDsl;
use std::collections::BTreeMap;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::watch;

pub struct ProgressTracker<H: Handler + 'static> {
    last_committed: IndexedMetadata,
    checkpoints_to_commit_from_batch: Vec<IndexedMetadata>,
    committed_checkpoints: BTreeMap<CheckpointSequenceNumber, IndexedMetadata>,
    committed_checkpoint_tx: watch::Sender<IndexedMetadata>,
    committed_checkpoint_rx: watch::Receiver<IndexedMetadata>,
    _marker: std::marker::PhantomData<fn() -> H>,
}

impl<H: Handler + 'static> ProgressTracker<H> {
    pub async fn new(db: &Db) -> anyhow::Result<Self> {
        let last_committed = Self::get_last_committed_checkpoint(db).await?;
        let (committed_checkpoint_tx, committed_checkpoint_rx) =
            watch::channel(last_committed.clone());
        tracing::info!(
            pipeline = H::NAME,
            "Initialized pipeline with last committed checkpoint: {:?}",
            last_committed,
        );
        Ok(Self {
            last_committed,
            checkpoints_to_commit_from_batch: vec![],
            committed_checkpoints: BTreeMap::new(),
            committed_checkpoint_tx,
            committed_checkpoint_rx,
            _marker: std::marker::PhantomData,
        })
    }

    pub fn subscribe_to_last_committed_checkpoint(&self) -> watch::Receiver<IndexedMetadata> {
        self.committed_checkpoint_rx.clone()
    }

    pub fn add_checkpoint_to_commit_from_batch(&mut self, indexed_metadata: IndexedMetadata) {
        self.checkpoints_to_commit_from_batch.push(indexed_metadata);
    }

    pub async fn commit_batch(&mut self, conn: &mut Connection<'_>) {
        self.committed_checkpoints.extend(
            self.checkpoints_to_commit_from_batch
                .drain(..)
                .map(|c| (c.sequence_number, c)),
        );
        let mut committed = false;
        while let Some(metadata) = self
            .committed_checkpoints
            .remove(&(self.last_committed.sequence_number + 1))
        {
            self.last_committed = metadata;
            committed = true;
        }
        if !committed {
            return;
        }
        if let Err(err) = self
            .committed_checkpoint_tx
            .send(self.last_committed.clone())
        {
            tracing::error!(
                pipeline = H::NAME,
                "Failed to send committed checkpoint to subscriber: {}",
                err
            );
        }
        // TODO: We could consider waiting for more checkpoints before updating watermark table.
        self.update_last_committed_checkpoint(conn).await;
        tracing::info!(
            pipeline = H::NAME,
            "Committed checkpoint: {:?}",
            self.last_committed
        )
    }

    async fn get_last_committed_checkpoint(db: &Db) -> anyhow::Result<IndexedMetadata> {
        let mut conn = db.connect().await?;
        let watermark = watermarks::table
            .filter(watermarks::entity.eq(H::NAME))
            .first::<StoredWatermark>(&mut conn)
            .await
            // Handle case where the watermark is not set yet
            .optional()?;
        if let Some(watermark) = watermark {
            Ok(IndexedMetadata {
                sequence_number: watermark.checkpoint_hi_inclusive as u64,
                epoch: watermark.epoch_hi_inclusive as u64,
                network_total_transactions: watermark.tx_hi_inclusive as u64,
            })
        } else {
            Ok(IndexedMetadata::default())
        }
    }

    async fn update_last_committed_checkpoint(&self, conn: &mut Connection<'_>) {
        diesel::insert_into(watermarks::table)
            .values((
                watermarks::entity.eq(H::NAME),
                watermarks::epoch_hi_inclusive.eq(self.last_committed.epoch as i64),
                watermarks::checkpoint_hi_inclusive.eq(self.last_committed.sequence_number as i64),
                watermarks::tx_hi_inclusive
                    .eq(self.last_committed.network_total_transactions as i64),
            ))
            .on_conflict(watermarks::entity)
            .do_update()
            .set((
                watermarks::epoch_hi_inclusive.eq(excluded(watermarks::epoch_hi_inclusive)),
                watermarks::checkpoint_hi_inclusive
                    .eq(excluded(watermarks::checkpoint_hi_inclusive)),
                watermarks::tx_hi_inclusive.eq(excluded(watermarks::tx_hi_inclusive)),
            ))
            .execute(conn)
            .await
            // Handle error properly.
            .unwrap();
    }
}
