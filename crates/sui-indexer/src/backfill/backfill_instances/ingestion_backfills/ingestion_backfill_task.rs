// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::backfill::backfill_instances::ingestion_backfills::IngestionBackfillTrait;
use crate::backfill::backfill_task::BackfillTask;
use crate::database::ConnectionPool;
use dashmap::DashMap;
use std::ops::RangeInclusive;
use std::sync::Arc;
use sui_data_ingestion_core::{setup_single_workflow, ReaderOptions, Worker};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::Notify;

pub struct IngestionBackfillTask<T: IngestionBackfillTrait> {
    ready_checkpoints: Arc<DashMap<CheckpointSequenceNumber, T::ProcessedType>>,
    notify: Arc<Notify>,
    _exit_sender: tokio::sync::oneshot::Sender<()>,
}

impl<T: IngestionBackfillTrait + 'static> IngestionBackfillTask<T> {
    pub async fn new(remote_store_url: String, start_checkpoint: CheckpointSequenceNumber) -> Self {
        let ready_checkpoints = Arc::new(DashMap::new());
        let notify = Arc::new(Notify::new());
        let adapter: Adapter<T> = Adapter {
            ready_checkpoints: ready_checkpoints.clone(),
            notify: notify.clone(),
        };
        let reader_options = ReaderOptions {
            batch_size: 200,
            ..Default::default()
        };
        let (executor, _exit_sender) = setup_single_workflow(
            adapter,
            remote_store_url,
            start_checkpoint,
            200,
            Some(reader_options),
        )
        .await
        .unwrap();
        tokio::task::spawn(async move {
            executor.await.unwrap();
        });
        Self {
            ready_checkpoints,
            notify,
            _exit_sender,
        }
    }
}

pub struct Adapter<T: IngestionBackfillTrait> {
    ready_checkpoints: Arc<DashMap<CheckpointSequenceNumber, T::ProcessedType>>,
    notify: Arc<Notify>,
}

#[async_trait::async_trait]
impl<T: IngestionBackfillTrait> Worker for Adapter<T> {
    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> anyhow::Result<()> {
        let processed = T::process_checkpoint(checkpoint);
        self.ready_checkpoints
            .insert(checkpoint.checkpoint_summary.sequence_number, processed);
        self.notify.notify_waiters();
        Ok(())
    }
}

#[async_trait::async_trait]
impl<T: IngestionBackfillTrait> BackfillTask for IngestionBackfillTask<T> {
    async fn backfill_range(&self, pool: ConnectionPool, range: &RangeInclusive<usize>) {
        let mut checkpoints = vec![];
        let mut start = *range.start();
        let end = *range.end();
        loop {
            while start <= end {
                if let Some(checkpoint) = self
                    .ready_checkpoints
                    .remove(&(start as CheckpointSequenceNumber))
                {
                    checkpoints.push(checkpoint.1);
                    start += 1;
                } else {
                    break;
                }
            }
            if start <= end {
                self.notify.notified().await;
            } else {
                break;
            }
        }
        T::commit_chunk(pool.clone(), checkpoints).await;
    }
}
