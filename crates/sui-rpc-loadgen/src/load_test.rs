// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::Sender;

use tokio::sync::mpsc;
use tracing::error;

use crate::payload::{Command, Payload, Processor, SignerInfo};

struct WorkerThread<R: Processor + Send + Sync + Clone> {
    processor: R,
    payload: Payload,
}

impl<R: Processor + Send + Sync + Clone> WorkerThread<R> {
    async fn run(&self) -> usize {
        let mut successful_commands = 0;
        match self.processor.apply(&self.payload).await {
            Ok(()) => successful_commands += 1,
            Err(e) => error!("Thread returns error: {e}"),
        }
        successful_commands
    }
}

pub struct LoadTestConfig {
    // TODO: support multiple commands
    pub command: Command,
    pub num_threads: usize,
    /// should divide tasks across multiple threads
    pub divide_tasks: bool,
    pub signer_info: Option<SignerInfo>,
    pub num_chunks_per_thread: usize,
    pub max_repeat: usize,
}

pub(crate) struct LoadTest<R: Processor + Send + Sync + Clone> {
    pub processor: R,
    pub config: LoadTestConfig,
}

impl<R: Processor + Send + Sync + Clone + 'static> LoadTest<R> {
    pub(crate) async fn run(&self) -> Result<(), Box<dyn Error>> {
        let start_time = Instant::now();
        let payloads = self.processor.prepare(&self.config).await?;
        let (tx, mut rx) = mpsc::channel(payloads.len());

        self.run_workers(tx, payloads).await;

        // Collect the results from the worker threads
        let mut num_successful_commands = 0;
        while let Some(num_successful) = rx.recv().await {
            num_successful_commands += num_successful;
        }

        let elapsed_time = start_time.elapsed();
        // TODO(chris): clean up this logic
        let total_commands = num_successful_commands
            * (self.config.max_repeat + 1)
            * self.config.num_chunks_per_thread;

        println!(
            "Total successful commands: {}, total time {:?}, commands per second {:.2}",
            total_commands,
            elapsed_time,
            get_tps(total_commands, elapsed_time),
        );

        self.processor.dump_cache_to_file(&self.config);

        Ok(())
    }

    async fn run_workers(&self, tx: Sender<usize>, payloads: Vec<Payload>) {
        println!("Running with {} threads...", payloads.len());
        for payload in payloads.iter() {
            let tx = tx.clone();
            let worker_thread = WorkerThread {
                processor: self.processor.clone(),
                payload: payload.clone(),
            };
            tokio::spawn(async move {
                let num_successful_commands = worker_thread.run().await;
                tx.send(num_successful_commands).await.unwrap();
            });
        }
    }
}

fn get_tps(num: usize, duration: Duration) -> f64 {
    num as f64 / duration.as_secs_f64()
}
