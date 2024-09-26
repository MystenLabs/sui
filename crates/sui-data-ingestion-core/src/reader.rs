// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoint_fetcher::CheckpointFetcher;
use crate::executor::MAX_CHECKPOINTS_IN_PROGRESS;
use anyhow::Result;
#[cfg(not(target_os = "macos"))]
use notify::{RecommendedWatcher, RecursiveMode};
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use std::{collections::BTreeMap, sync::Arc};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::oneshot;
use tokio::time::timeout;
use tracing::{error, info};

pub struct CheckpointReader {
    /// Used to read from a local directory when running with a colocated FN.
    /// When fetching from a remote store, a temp dir can be passed in and it will be an no-op
    path: PathBuf,
    current_checkpoint_number: CheckpointSequenceNumber,
    last_pruned_watermark: CheckpointSequenceNumber,
    checkpoint_sender: mpsc::Sender<Arc<CheckpointData>>,
    processed_receiver: mpsc::Receiver<CheckpointSequenceNumber>,
    fetcher_receiver: mpsc::Receiver<(Arc<CheckpointData>, usize)>,
    exit_receiver: oneshot::Receiver<()>,
    options: ReaderOptions,
    data_limiter: DataLimiter,
}

#[derive(Clone)]
pub struct ReaderOptions {
    pub tick_interval_ms: u64,
    pub timeout_secs: u64,
    /// number of maximum concurrent requests to the remote store. Increase it for backfills
    pub batch_size: usize,
    pub data_limit: usize,
    pub upper_limit: Option<CheckpointSequenceNumber>,
}

impl Default for ReaderOptions {
    fn default() -> Self {
        Self {
            tick_interval_ms: 100,
            timeout_secs: 5,
            batch_size: 10,
            data_limit: 0,
            upper_limit: None,
        }
    }
}

impl CheckpointReader {
    fn exceeds_capacity(&self, checkpoint_number: CheckpointSequenceNumber) -> bool {
        ((MAX_CHECKPOINTS_IN_PROGRESS as u64 + self.last_pruned_watermark) <= checkpoint_number)
            || self.data_limiter.exceeds()
    }

    fn receive_checkpoints(&mut self) -> Vec<Arc<CheckpointData>> {
        let mut checkpoints = vec![];
        while !self.exceeds_capacity(self.current_checkpoint_number + checkpoints.len() as u64) {
            match self.fetcher_receiver.try_recv() {
                Ok((checkpoint, size)) => {
                    self.data_limiter.add(&checkpoint, size);
                    checkpoints.push(checkpoint);
                }
                Err(TryRecvError::Disconnected) => {
                    error!("remote reader channel disconnect error");
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        checkpoints
    }

    async fn sync(&mut self) -> Result<()> {
        let checkpoints = self.receive_checkpoints();

        info!(
            "Current checkpoint number: {}, pruning watermark: {}, new updates: {:?}",
            self.current_checkpoint_number,
            self.last_pruned_watermark,
            checkpoints.len(),
        );
        for checkpoint in checkpoints {
            assert_eq!(
                checkpoint.checkpoint_summary.sequence_number,
                self.current_checkpoint_number
            );
            self.checkpoint_sender.send(checkpoint).await?;
            self.current_checkpoint_number += 1;
        }
        Ok(())
    }

    /// Cleans the local directory by removing all processed checkpoint files.
    fn gc_processed_files(&mut self, watermark: CheckpointSequenceNumber) -> Result<()> {
        info!("cleaning processed files, watermark is {}", watermark);
        self.data_limiter.gc(watermark);
        self.last_pruned_watermark = watermark;
        for entry in fs::read_dir(self.path.clone())? {
            let entry = entry?;
            let filename = entry.file_name();
            if let Some(sequence_number) = Self::checkpoint_number_from_file_path(&filename) {
                if sequence_number < watermark {
                    fs::remove_file(entry.path())?;
                }
            }
        }
        Ok(())
    }

    fn checkpoint_number_from_file_path(file_name: &OsString) -> Option<CheckpointSequenceNumber> {
        file_name
            .to_str()
            .and_then(|s| s.rfind('.').map(|pos| &s[..pos]))
            .and_then(|s| s.parse().ok())
    }

    pub async fn initialize(
        path: PathBuf,
        starting_checkpoint_number: CheckpointSequenceNumber,
        remote_store_url: Option<String>,
        remote_store_options: Vec<(String, String)>,
        options: ReaderOptions,
    ) -> (
        Self,
        mpsc::Receiver<Arc<CheckpointData>>,
        mpsc::Sender<CheckpointSequenceNumber>,
        oneshot::Sender<()>,
    ) {
        let (checkpoint_sender, checkpoint_recv) = mpsc::channel(MAX_CHECKPOINTS_IN_PROGRESS);
        let (processed_sender, processed_receiver) = mpsc::channel(MAX_CHECKPOINTS_IN_PROGRESS);
        let (exit_sender, exit_receiver) = oneshot::channel();
        let checkpoint_fetcher = CheckpointFetcher::new(
            path.clone(),
            remote_store_url,
            remote_store_options,
            options.timeout_secs,
        );
        let fetcher_receiver = checkpoint_fetcher
            .start_fetching_checkpoints(options.batch_size, starting_checkpoint_number)
            .await;
        let reader = Self {
            path,
            current_checkpoint_number: starting_checkpoint_number,
            last_pruned_watermark: starting_checkpoint_number,
            checkpoint_sender,
            processed_receiver,
            fetcher_receiver,
            exit_receiver,
            data_limiter: DataLimiter::new(options.data_limit),
            options,
        };
        (reader, checkpoint_recv, processed_sender, exit_sender)
    }

    #[cfg(not(target_os = "macos"))]
    fn init_watcher(
        inotify_sender: mpsc::Sender<()>,
        path: &std::path::Path,
    ) -> RecommendedWatcher {
        use notify::Watcher;
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Err(err) = res {
                eprintln!("watch error: {:?}", err);
            }
            inotify_sender
                .blocking_send(())
                .expect("Failed to send inotify update");
        })
        .expect("Failed to init inotify");
        watcher
            .watch(path, RecursiveMode::NonRecursive)
            .expect("Inotify watcher failed");
        watcher
    }

    pub async fn run(mut self) -> Result<()> {
        let (_inotify_sender, mut inotify_recv) = mpsc::channel::<()>(1);
        std::fs::create_dir_all(self.path.clone()).expect("failed to create a directory");

        #[cfg(not(target_os = "macos"))]
        let _watcher = Self::init_watcher(_inotify_sender, &self.path);

        self.gc_processed_files(self.last_pruned_watermark)
            .expect("Failed to clean the directory");

        loop {
            tokio::select! {
                _ = &mut self.exit_receiver => break,
                Some(gc_checkpoint_number) = self.processed_receiver.recv() => {
                    self.gc_processed_files(gc_checkpoint_number).expect("Failed to clean the directory");
                }
                Ok(Some(_)) | Err(_) = timeout(Duration::from_millis(self.options.tick_interval_ms), inotify_recv.recv())  => {
                    self.sync().await.expect("Failed to read checkpoint files");
                }
            }
        }
        Ok(())
    }
}

pub struct DataLimiter {
    limit: usize,
    queue: BTreeMap<CheckpointSequenceNumber, usize>,
    in_progress: usize,
}

impl DataLimiter {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            queue: BTreeMap::new(),
            in_progress: 0,
        }
    }

    fn exceeds(&self) -> bool {
        self.limit > 0 && self.in_progress >= self.limit
    }

    fn add(&mut self, checkpoint: &CheckpointData, size: usize) {
        if self.limit == 0 {
            return;
        }
        self.in_progress += size;
        self.queue
            .insert(checkpoint.checkpoint_summary.sequence_number, size);
    }

    fn gc(&mut self, watermark: CheckpointSequenceNumber) {
        if self.limit == 0 {
            return;
        }
        self.queue = self.queue.split_off(&watermark);
        self.in_progress = self.queue.values().sum();
    }
}
