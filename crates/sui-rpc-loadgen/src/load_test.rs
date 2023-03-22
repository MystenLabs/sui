// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::Sender;

use tokio::sync::mpsc;

use crate::payload::{Payload, Processor};

struct WorkerThread<R: Processor + Send + Sync + Clone> {
    processor: R,
    payload: Payload,
}

impl<R: Processor + Send + Sync + Clone> WorkerThread<R> {
    async fn run(&self) -> usize {
        let mut successful_commands = 0;
        if self.processor.apply(&self.payload).await.is_ok() {
            successful_commands += 1;
        }
        successful_commands
    }
}

pub(crate) struct LoadTest<R: Processor + Send + Sync + Clone> {
    pub processor: R,
    // one payload for each thread
    pub payloads: Vec<Payload>,
}

impl<R: Processor + Send + Sync + Clone + 'static> LoadTest<R> {
    pub(crate) async fn run(&self) -> Result<(), Box<dyn Error>> {
        let start_time = Instant::now();
        let (tx, mut rx) = mpsc::channel(self.payloads.len());
        self.run_workers(tx).await;

        // Collect the results from the worker threads
        let mut num_successful_commands = 0;
        while let Some(num_successful) = rx.recv().await {
            num_successful_commands += num_successful;
        }

        let elapsed_time = start_time.elapsed();

        println!(
            "Total successful commands: {}, total time {:?}, commands per second {:.2}",
            num_successful_commands,
            elapsed_time,
            get_tps(num_successful_commands, elapsed_time),
        );

        Ok(())
    }

    async fn run_workers(&self, tx: Sender<usize>) {
        println!("Running with {} threads...", self.payloads.len());
        for payload in self.payloads.iter() {
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
