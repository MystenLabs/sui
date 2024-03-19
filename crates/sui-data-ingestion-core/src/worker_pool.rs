// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::executor::MAX_CHECKPOINTS_IN_PROGRESS;
use crate::Worker;
use mysten_metrics::spawn_monitored_task;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;
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
    pub fn new(worker: W, task_name: String, concurrency: usize) -> Self {
        Self {
            task_name,
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
        let mut updates = HashMap::new();

        let (progress_sender, mut progress_receiver) = mpsc::channel(MAX_CHECKPOINTS_IN_PROGRESS);
        let mut workers = vec![];
        let mut idle: BTreeSet<_> = (0..self.concurrency).collect();
        let mut checkpoints = VecDeque::new();

        // spawn child workers
        for worker_id in 0..self.concurrency {
            let (worker_sender, mut worker_recv) =
                mpsc::channel::<CheckpointData>(MAX_CHECKPOINTS_IN_PROGRESS);
            let (term_sender, mut term_receiver) = oneshot::channel::<()>();
            let cloned_progress_sender = progress_sender.clone();
            let task_name = self.task_name.clone();
            workers.push((worker_sender, term_sender));

            let worker = self.worker.clone();
            spawn_monitored_task!(async move {
                loop {
                    tokio::select! {
                        _ = &mut term_receiver => break,
                        Some(checkpoint) = worker_recv.recv() => {
                            let sequence_number = checkpoint.checkpoint_summary.sequence_number;
                            info!("received checkpoint for processing {} for workflow {}", sequence_number, task_name);
                            let start_time = Instant::now();
                            let backoff = backoff::ExponentialBackoff::default();
                            backoff::future::retry(backoff, || async {
                                worker
                                    .clone()
                                    .process_checkpoint(checkpoint.clone())
                                    .await
                                    .map_err(|err| {
                                        info!("transient worker execution error {:?} for checkpoint {}", err, sequence_number);
                                        backoff::Error::transient(err)
                                    })
                            })
                            .await
                            .expect("checkpoint processing failed for checkpoint");
                            info!("finished checkpoint processing {} for workflow {} in {:?}", sequence_number, task_name, start_time.elapsed());
                            cloned_progress_sender.send((worker_id, sequence_number, worker.save_progress(sequence_number).await)).await.expect("failed to update progress");
                        }
                    }
                }
            });
        }
        // main worker pool loop
        loop {
            tokio::select! {
                Some((worker_id, status_update, progress_watermark)) = progress_receiver.recv() => {
                    idle.insert(worker_id);
                    updates.insert(status_update, progress_watermark);
                    if status_update == current_checkpoint_number {
                        let mut executor_status_update = None;
                        while let Some(progress_watermark) = updates.remove(&current_checkpoint_number) {
                            if let Some(watermark) =  progress_watermark {
                                executor_status_update = Some(watermark + 1);
                            }
                            current_checkpoint_number += 1;
                        }
                        if let Some(update) = executor_status_update {
                            executor_progress_sender
                                .send((self.task_name.clone(), update))
                                .await
                                .expect("Failed to send progress update to the executor");
                        }
                    }
                    while !checkpoints.is_empty() && !idle.is_empty() {
                        let checkpoint = checkpoints.pop_front().unwrap();
                        let worker_id = idle.pop_first().unwrap();
                        workers[worker_id].0.send(checkpoint).await.expect("failed to dispatch a task");
                    }
                }
                Some(checkpoint) = checkpoint_receiver.recv() => {
                    let sequence_number = checkpoint.checkpoint_summary.sequence_number;
                    if sequence_number < current_checkpoint_number {
                        continue;
                    }
                    if idle.is_empty() {
                        checkpoints.push_back(checkpoint);
                    } else {
                        let worker_id = idle.pop_first().unwrap();
                        workers[worker_id].0.send(checkpoint).await.expect("failed to dispatch a task");
                    }
                }
            }
        }
    }
}
