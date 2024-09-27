// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::database::ConnectionPool;
use crate::framework_experimental::indexer_handler_trait::IndexerHandlerTrait;
use crate::framework_experimental::progress::ProgressUpdate;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::mpsc;
use tokio::time;
use tracing::debug;

#[allow(dead_code)]
#[async_trait::async_trait]
pub trait IndexerPipelineTrait: Send + Sync {
    async fn process_checkpoint(&self, checkpoint: Arc<CheckpointData>);
    fn get_pipeline_name(&self) -> &'static str;
}

#[allow(dead_code)]
pub struct IndexerPipeline<H>
where
    H: IndexerHandlerTrait + 'static,
{
    item_sender: mpsc::Sender<(CheckpointSequenceNumber, Vec<H::ProcessedType>)>,
    start_checkpoint: CheckpointSequenceNumber,
}

#[allow(dead_code)]
impl<H> IndexerPipeline<H>
where
    H: IndexerHandlerTrait + 'static,
{
    pub fn new(
        pool: ConnectionPool,
        progress_sender: mpsc::Sender<ProgressUpdate>,
        last_checkpoint: CheckpointSequenceNumber,
        max_commit_batch_size: usize,
    ) -> Self {
        let (item_sender, item_receiver) = mpsc::channel(1000);
        let start_checkpoint = last_checkpoint + 1;

        // Start the commit task
        tokio::spawn(Self::commit_task(
            item_receiver,
            progress_sender,
            start_checkpoint,
            pool,
            max_commit_batch_size,
        ));

        Self {
            item_sender,
            start_checkpoint,
        }
    }

    async fn commit_task(
        mut item_receiver: mpsc::Receiver<(CheckpointSequenceNumber, Vec<H::ProcessedType>)>,
        progress_sender: mpsc::Sender<ProgressUpdate>,
        start_checkpoint: CheckpointSequenceNumber,
        mut pool: ConnectionPool,
        max_commit_batch_size: usize,
    ) {
        let batch_timeout = Duration::from_secs(1); // Adjust as needed

        let mut pending_items = PendingItems::<H>::new(max_commit_batch_size, start_checkpoint);

        // Create an interval for batch timeout
        let mut interval = time::interval(batch_timeout);
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                Some((seq_num, items)) = item_receiver.recv() => {
                    debug!("[{:?}] Received processed items for seq_num: {}", H::get_name(), seq_num);
                    if let Some((last_commit_seq_num, items)) = pending_items.add_ready_to_commit(seq_num, items) {
                        Self::commit_batch(&mut pool, last_commit_seq_num, items, progress_sender.clone()).await;
                    }
                }
                _ = interval.tick() => {
                    if let Some((last_commit_seq_num, items)) = pending_items.take_ready_to_commit() {
                        Self::commit_batch(&mut pool, last_commit_seq_num, items, progress_sender.clone()).await;
                    }
                }
                else => {
                    break; // Exit the loop
                }
            }
        }
    }

    // TODO: Make this function async so that we could commit multiple batches in parallel.
    async fn commit_batch(
        pool: &mut ConnectionPool,
        last_checkpoint_number: CheckpointSequenceNumber,
        ready_items: Vec<H::ProcessedType>,
        progress_sender: mpsc::Sender<ProgressUpdate>,
    ) {
        debug!(
            "[{}] Committing batch of {} items with last_checkpoint_number {}",
            H::get_name(),
            ready_items.len(),
            last_checkpoint_number
        );
        H::commit_chunk(pool, ready_items).await;
        H::update_progress(last_checkpoint_number).await;

        progress_sender
            .send(ProgressUpdate::new::<H>(last_checkpoint_number))
            .await
            .unwrap();
    }
}

#[async_trait::async_trait]
impl<H> IndexerPipelineTrait for IndexerPipeline<H>
where
    H: IndexerHandlerTrait + 'static,
{
    async fn process_checkpoint(&self, checkpoint: Arc<CheckpointData>) {
        let seq_num = checkpoint.checkpoint_summary.sequence_number;
        let item_sender = self.item_sender.clone();

        tokio::spawn(async move {
            let processed = H::process_checkpoint(checkpoint).await;
            item_sender.send((seq_num, processed)).await.unwrap();
        });
    }

    fn get_pipeline_name(&self) -> &'static str {
        H::get_name()
    }
}

struct PendingItems<H: IndexerHandlerTrait> {
    buffer: BTreeMap<CheckpointSequenceNumber, Vec<H::ProcessedType>>,
    next_sequence_number: CheckpointSequenceNumber,
    ready_to_commit: Vec<H::ProcessedType>,
    ready_to_commit_batch_size: usize,
    max_chunk_size: usize,
}

impl<H: IndexerHandlerTrait> PendingItems<H> {
    pub fn new(max_chunk_size: usize, next_sequence_number: CheckpointSequenceNumber) -> Self {
        Self {
            buffer: BTreeMap::new(),
            next_sequence_number,
            ready_to_commit: Vec::new(),
            ready_to_commit_batch_size: 0,
            max_chunk_size,
        }
    }

    pub fn add_ready_to_commit(
        &mut self,
        seq_num: CheckpointSequenceNumber,
        processed_items: Vec<H::ProcessedType>,
    ) -> Option<(CheckpointSequenceNumber, Vec<H::ProcessedType>)> {
        self.buffer.insert(seq_num, processed_items);
        while let Some(items) = self.buffer.remove(&self.next_sequence_number) {
            self.ready_to_commit_batch_size += items.len();
            self.ready_to_commit.extend(items);
            self.next_sequence_number += 1;
            if self.ready_to_commit_batch_size >= self.max_chunk_size {
                return self.take_ready_to_commit();
            }
        }
        None
    }

    pub fn take_ready_to_commit(
        &mut self,
    ) -> Option<(CheckpointSequenceNumber, Vec<H::ProcessedType>)> {
        if self.ready_to_commit.is_empty() {
            return None;
        }
        let items = std::mem::take(&mut self.ready_to_commit);
        self.ready_to_commit_batch_size = 0;
        Some((self.next_sequence_number - 1, items))
    }
}
