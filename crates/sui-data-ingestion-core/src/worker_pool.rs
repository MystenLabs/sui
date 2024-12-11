// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::executor::MAX_CHECKPOINTS_IN_PROGRESS;
use crate::reducer::reduce;
use crate::{Reducer, Worker};
use mysten_metrics::spawn_monitored_task;
use std::collections::{BTreeSet, VecDeque};
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
    reducer: Option<Box<dyn Reducer<W::Result>>>,
}

impl<W: Worker + 'static> WorkerPool<W> {
    pub fn new(worker: W, task_name: String, concurrency: usize) -> Self {
        Self {
            task_name,
            concurrency,
            worker: Arc::new(worker),
            reducer: None,
        }
    }
    pub fn new_with_reducer(
        worker: W,
        task_name: String,
        concurrency: usize,
        reducer: Box<dyn Reducer<W::Result>>,
    ) -> Self {
        Self {
            task_name,
            concurrency,
            worker: Arc::new(worker),
            reducer: Some(reducer),
        }
    }

    pub async fn run(
        mut self,
        watermark: CheckpointSequenceNumber,
        mut checkpoint_receiver: mpsc::Receiver<Arc<CheckpointData>>,
        executor_progress_sender: mpsc::Sender<(String, CheckpointSequenceNumber)>,
    ) {
        info!(
            "Starting indexing pipeline {} with concurrency {}. Current watermark is {}.",
            self.task_name, self.concurrency, watermark
        );
        let (progress_sender, mut progress_receiver) = mpsc::channel(MAX_CHECKPOINTS_IN_PROGRESS);
        let (reducer_sender, reducer_receiver) = mpsc::channel(MAX_CHECKPOINTS_IN_PROGRESS);
        let mut workers = vec![];
        let mut idle: BTreeSet<_> = (0..self.concurrency).collect();
        let mut checkpoints = VecDeque::new();

        let mut join_handles = vec![];

        // spawn child workers
        for worker_id in 0..self.concurrency {
            let (worker_sender, mut worker_recv) =
                mpsc::channel::<Arc<CheckpointData>>(MAX_CHECKPOINTS_IN_PROGRESS);
            let (term_sender, mut term_receiver) = oneshot::channel::<()>();
            let cloned_progress_sender = progress_sender.clone();
            let task_name = self.task_name.clone();
            workers.push((worker_sender, term_sender));

            let worker = self.worker.clone();
            let join_handle = spawn_monitored_task!(async move {
                loop {
                    tokio::select! {
                        _ = &mut term_receiver => break,
                        Some(checkpoint) = worker_recv.recv() => {
                            let sequence_number = checkpoint.checkpoint_summary.sequence_number;
                            info!("received checkpoint for processing {} for workflow {}", sequence_number, task_name);
                            let start_time = Instant::now();
                            let backoff = backoff::ExponentialBackoff::default();
                            let result = backoff::future::retry(backoff, || async {
                                worker
                                    .clone()
                                    .process_checkpoint(&checkpoint)
                                    .await
                                    .map_err(|err| {
                                        info!("transient worker execution error {:?} for checkpoint {}", err, sequence_number);
                                        backoff::Error::transient(err)
                                    })
                            })
                            .await
                            .expect("checkpoint processing failed for checkpoint");
                            info!("finished checkpoint processing {} for workflow {} in {:?}", sequence_number, task_name, start_time.elapsed());
                            if cloned_progress_sender.send((worker_id, sequence_number, result)).await.is_err() {
                                // The progress channel closing is a sign we need to exit this loop.
                                break;
                            }
                        }
                    }
                }
            });

            // Keep all join handles to ensure all workers are terminated before exiting
            join_handles.push(join_handle);
        }
        spawn_monitored_task!(reduce::<W>(
            self.task_name.clone(),
            watermark,
            reducer_receiver,
            executor_progress_sender,
            std::mem::take(&mut self.reducer),
        ));
        // main worker pool loop
        loop {
            tokio::select! {
                Some((worker_id, checkpoint_number, message)) = progress_receiver.recv() => {
                    idle.insert(worker_id);
                    if reducer_sender.send((checkpoint_number, message)).await.is_err() {
                        break;
                    }
                    while !checkpoints.is_empty() && !idle.is_empty() {
                        let checkpoint = checkpoints.pop_front().unwrap();
                        let worker_id = idle.pop_first().unwrap();
                        if workers[worker_id].0.send(checkpoint).await.is_err() {
                            // The worker channel closing is a sign we need to exit this loop.
                            break;
                        }
                    }
                }
                maybe_checkpoint = checkpoint_receiver.recv() => {
                    if maybe_checkpoint.is_none() {
                        break;
                    }
                    let checkpoint = maybe_checkpoint.expect("invariant's checked");
                    let sequence_number = checkpoint.checkpoint_summary.sequence_number;
                    if sequence_number < watermark {
                        continue;
                    }
                    self.worker.preprocess_hook(&checkpoint).expect("failed to preprocess task");
                    if idle.is_empty() {
                        checkpoints.push_back(checkpoint);
                    } else {
                        let worker_id = idle.pop_first().unwrap();
                        if workers[worker_id].0.send(checkpoint).await.is_err() {
                            // The worker channel closing is a sign we need to exit this loop.
                            break;
                        };
                    }
                }
            }
        }

        // Clean up code for graceful termination

        // Notify the exit handles of all workers to terminate
        drop(workers);

        // Wait for all workers to finish
        for join_handle in join_handles {
            join_handle.await.expect("worker thread panicked");
        }
    }
}
