// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::executor::MAX_CHECKPOINTS_IN_PROGRESS;
use crate::workers::Worker;
use mysten_metrics::spawn_monitored_task;
use std::collections::HashSet;
use std::sync::Arc;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::info;

pub struct WorkerPool<W: Worker> {
    pub task_name: String,
    concurrency: usize,
    worker: Arc<W>,
}

impl<W: Worker + 'static> WorkerPool<W> {
    pub fn new(worker: W, concurrency: usize) -> Self {
        Self {
            task_name: worker.name().to_string(),
            concurrency,
            worker: Arc::new(worker),
        }
    }
    pub async fn run(
        self,
        mut current_checkpoint_number: CheckpointSequenceNumber,
        mut checkpoint_receiver: mpsc::Receiver<CheckpointData>,
        executor_progress_sender: mpsc::Sender<(String, CheckpointSequenceNumber)>,
    ) {
        info!(
            "Starting indexing pipeline {} with concurrency {}. Current watermark is {}.",
            self.task_name, self.concurrency, current_checkpoint_number
        );
        let mut updates: HashSet<u64> = HashSet::new();
        let mut in_progress = 0;

        let (progress_sender, mut progress_receiver) = mpsc::channel(MAX_CHECKPOINTS_IN_PROGRESS);
        let mut workers = vec![];

        // spawn child workers
        for _ in 0..self.concurrency {
            let (worker_sender, mut worker_recv) =
                mpsc::channel::<CheckpointData>(MAX_CHECKPOINTS_IN_PROGRESS);
            let (term_sender, mut term_receiver) = oneshot::channel::<()>();
            let cloned_progress_sender = progress_sender.clone();
            workers.push((worker_sender, term_sender));

            let worker = self.worker.clone();
            spawn_monitored_task!(async move {
                loop {
                    tokio::select! {
                        _ = &mut term_receiver => break,
                        Some(checkpoint) = worker_recv.recv() => {
                            let backoff = backoff::ExponentialBackoff::default();
                            backoff::future::retry(backoff, || async {
                                worker
                                    .clone()
                                    .process_checkpoint(checkpoint.clone())
                                    .await
                                    .map_err(backoff::Error::transient)
                            })
                            .await
                            .expect("checkpoint processing failed for checkpoint");
                            cloned_progress_sender
                                .send(checkpoint.checkpoint_summary.sequence_number)
                                .await
                                .expect("failed to update progress");
                        }
                    }
                }
            });
        }
        // main worker pool loop
        loop {
            tokio::select! {
                Some(checkpoint) = checkpoint_receiver.recv(), if in_progress < MAX_CHECKPOINTS_IN_PROGRESS => {
                    let sequence_number = checkpoint.checkpoint_summary.sequence_number;
                    if sequence_number < current_checkpoint_number {
                        continue;
                    }
                    let worker_id = (sequence_number % self.concurrency as u64) as usize;
                    info!("received checkpoint for processing {} for workflow {}", sequence_number, self.task_name);
                    in_progress += 1;
                    workers[worker_id].0.send(checkpoint).await.expect("failed to dispatch a task");
                }
                Some(status_update) = progress_receiver.recv() => {
                    info!("finished checkpoint processing {} for workflow {}", status_update, self.task_name);
                    in_progress -= 1;
                    updates.insert(status_update);
                    if status_update == current_checkpoint_number {
                        while updates.remove(&current_checkpoint_number) {
                            current_checkpoint_number += 1;
                        }
                        executor_progress_sender
                            .send((self.task_name.clone(), current_checkpoint_number))
                            .await
                            .expect("Failed to send progress update to the executor");
                    }
                }
            }
        }
    }
}
