// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::framework_experimental::indexer_handler_trait::IndexerHandlerTrait;
use mysten_metrics::spawn_monitored_task;
use std::collections::HashMap;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

#[allow(dead_code)]
pub struct ProgressTracker {
    progress_sender: mpsc::Sender<ProgressUpdate>,
    progress_recv: mpsc::Receiver<ProgressUpdate>,
    progress: HashMap<&'static str, CheckpointSequenceNumber>,
}

#[allow(dead_code)]
impl ProgressTracker {
    pub fn new() -> Self {
        let (progress_sender, progress_recv) = mpsc::channel(10000);
        Self {
            progress_sender,
            progress_recv,
            progress: HashMap::new(),
        }
    }

    pub fn register(
        &mut self,
        task_name: &'static str,
        last_checkpoint: CheckpointSequenceNumber,
    ) -> mpsc::Sender<ProgressUpdate> {
        let exists = self.progress.insert(task_name, last_checkpoint);
        assert!(exists.is_none(), "Duplicate task name: {}", task_name);
        info!(
            "Registered task {} with last committed checkpoint {}",
            task_name, last_checkpoint
        );
        self.progress_sender.clone()
    }

    pub fn get_min_progress(&self) -> CheckpointSequenceNumber {
        self.progress.values().cloned().min().unwrap()
    }

    pub fn start(self, gc_sender: mpsc::Sender<CheckpointSequenceNumber>) {
        spawn_monitored_task!(self.track_progress(gc_sender));
    }

    async fn track_progress(mut self, gc_sender: mpsc::Sender<CheckpointSequenceNumber>) {
        let mut last_gc = self.get_min_progress();
        while let Some(update) = self.progress_recv.recv().await {
            debug!(
                "Received progress update for task {}: {}",
                update.task_name, update.last_committed_checkpoint
            );
            let last_checkpoint = update.last_committed_checkpoint;
            let task_name = update.task_name;
            self.progress.insert(task_name, last_checkpoint);
            let new_gc = self.get_min_progress();
            if new_gc > last_gc {
                info!("Notifying CheckpointReader GC: {}", new_gc);
                last_gc = new_gc;
                if let Err(err) = gc_sender.send(last_gc).await {
                    error!("Failed to notify CheckpointReader GC: {:?}", err);
                    break;
                }
            }
        }
        error!(
            "Progress tracker stopped. Latest progress map: {:?}",
            self.progress
        );
    }
}

#[allow(dead_code)]
pub struct ProgressUpdate {
    task_name: &'static str,
    last_committed_checkpoint: CheckpointSequenceNumber,
}

impl ProgressUpdate {
    pub fn new<H: IndexerHandlerTrait>(
        last_committed_checkpoint: CheckpointSequenceNumber,
    ) -> Self {
        Self {
            task_name: H::get_name(),
            last_committed_checkpoint,
        }
    }
}
