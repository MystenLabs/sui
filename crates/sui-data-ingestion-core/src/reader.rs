// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::create_remote_store_client;
use crate::executor::MAX_CHECKPOINTS_IN_PROGRESS;
use anyhow::Result;
use backoff::backoff::Backoff;
use futures::StreamExt;
use mysten_metrics::spawn_monitored_task;
use notify::RecursiveMode;
use notify::Watcher;
use object_store::path::Path;
use object_store::ObjectStore;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use sui_rest_api::Client;
use sui_storage::blob::Blob;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tap::pipe::Pipe;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::oneshot;
use tokio::time::timeout;
use tracing::{debug, error, info};

/// Implements a checkpoint reader that monitors a local directory.
/// Designed for setups where the indexer daemon is colocated with FN.
/// This implementation is push-based and utilizes the inotify API.
pub struct CheckpointReader {
    path: PathBuf,
    remote_store_url: Option<String>,
    remote_store_options: Vec<(String, String)>,
    current_checkpoint_number: CheckpointSequenceNumber,
    last_pruned_watermark: CheckpointSequenceNumber,
    checkpoint_sender: mpsc::Sender<CheckpointData>,
    processed_receiver: mpsc::Receiver<CheckpointSequenceNumber>,
    remote_fetcher_receiver: Option<mpsc::Receiver<Result<(CheckpointData, usize)>>>,
    exit_receiver: oneshot::Receiver<()>,
    options: ReaderOptions,
    data_limiter: DataLimiter,
}

#[derive(Clone)]
pub struct ReaderOptions {
    pub tick_interal_ms: u64,
    pub timeout_secs: u64,
    pub batch_size: usize,
    pub data_limit: usize,
}

impl Default for ReaderOptions {
    fn default() -> Self {
        Self {
            tick_interal_ms: 100,
            timeout_secs: 5,
            batch_size: 100,
            data_limit: 0,
        }
    }
}

enum RemoteStore {
    ObjectStore(Box<dyn ObjectStore>),
    Rest(sui_rest_api::Client),
    Hybrid(Box<dyn ObjectStore>, sui_rest_api::Client),
}

impl CheckpointReader {
    /// Represents a single iteration of the reader.
    /// Reads files in a local directory, validates them, and forwards `CheckpointData` to the executor.
    async fn read_local_files(&self) -> Result<Vec<CheckpointData>> {
        let mut files = vec![];
        for entry in fs::read_dir(self.path.clone())? {
            let entry = entry?;
            let filename = entry.file_name();
            if let Some(sequence_number) = Self::checkpoint_number_from_file_path(&filename) {
                if sequence_number >= self.current_checkpoint_number {
                    files.push((sequence_number, entry.path()));
                }
            }
        }
        files.sort();
        debug!("unprocessed local files {:?}", files);
        let mut checkpoints = vec![];
        for (_, filename) in files.iter().take(MAX_CHECKPOINTS_IN_PROGRESS) {
            let checkpoint = Blob::from_bytes::<CheckpointData>(&fs::read(filename)?)?;
            if self.exceeds_capacity(checkpoint.checkpoint_summary.sequence_number) {
                break;
            }
            checkpoints.push(checkpoint);
        }
        Ok(checkpoints)
    }

    fn exceeds_capacity(&self, checkpoint_number: CheckpointSequenceNumber) -> bool {
        ((MAX_CHECKPOINTS_IN_PROGRESS as u64 + self.last_pruned_watermark) <= checkpoint_number)
            || self.data_limiter.exceeds()
    }

    async fn fetch_from_object_store(
        store: &dyn ObjectStore,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> Result<(CheckpointData, usize)> {
        let path = Path::from(format!("{}.chk", checkpoint_number));
        let response = store.get(&path).await?;
        let bytes = response.bytes().await?;
        Ok((Blob::from_bytes::<CheckpointData>(&bytes)?, bytes.len()))
    }

    async fn fetch_from_full_node(
        client: &Client,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> Result<(CheckpointData, usize)> {
        let checkpoint = client.get_full_checkpoint(checkpoint_number).await?;
        let size = bcs::serialized_size(&checkpoint)?;
        Ok((checkpoint, size))
    }

    async fn remote_fetch_checkpoint_internal(
        store: &RemoteStore,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> Result<(CheckpointData, usize)> {
        match store {
            RemoteStore::ObjectStore(store) => {
                Self::fetch_from_object_store(store, checkpoint_number).await
            }
            RemoteStore::Rest(client) => {
                Self::fetch_from_full_node(client, checkpoint_number).await
            }
            RemoteStore::Hybrid(store, client) => {
                match Self::fetch_from_full_node(client, checkpoint_number).await {
                    Ok(result) => Ok(result),
                    Err(_) => Self::fetch_from_object_store(store, checkpoint_number).await,
                }
            }
        }
    }

    async fn remote_fetch_checkpoint(
        store: &RemoteStore,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> Result<(CheckpointData, usize)> {
        let mut backoff = backoff::ExponentialBackoff::default();
        backoff.max_elapsed_time = Some(Duration::from_secs(60));
        backoff.initial_interval = Duration::from_millis(100);
        backoff.current_interval = backoff.initial_interval;
        backoff.multiplier = 1.0;
        loop {
            match Self::remote_fetch_checkpoint_internal(store, checkpoint_number).await {
                Ok(data) => return Ok(data),
                Err(err) => match backoff.next_backoff() {
                    Some(duration) => {
                        if !err.to_string().contains("404") {
                            debug!(
                                "remote reader retry in {} ms. Error is {:?}",
                                duration.as_millis(),
                                err
                            );
                        }
                        tokio::time::sleep(duration).await
                    }
                    None => return Err(err),
                },
            }
        }
    }

    fn start_remote_fetcher(&mut self) -> mpsc::Receiver<Result<(CheckpointData, usize)>> {
        let batch_size = self.options.batch_size;
        let start_checkpoint = self.current_checkpoint_number;
        let (sender, receiver) = mpsc::channel(batch_size);
        let url = self
            .remote_store_url
            .clone()
            .expect("remote store url must be set");
        let store = if let Some((fn_url, remote_url)) = url.split_once('|') {
            let object_store = create_remote_store_client(
                remote_url.to_string(),
                self.remote_store_options.clone(),
                self.options.timeout_secs,
            )
            .expect("failed to create remote store client");
            RemoteStore::Hybrid(object_store, sui_rest_api::Client::new(fn_url))
        } else if url.ends_with("/rest") {
            RemoteStore::Rest(sui_rest_api::Client::new(url))
        } else {
            let object_store = create_remote_store_client(
                url,
                self.remote_store_options.clone(),
                self.options.timeout_secs,
            )
            .expect("failed to create remote store client");
            RemoteStore::ObjectStore(object_store)
        };

        spawn_monitored_task!(async move {
            let mut checkpoint_stream = (start_checkpoint..u64::MAX)
                .map(|checkpoint_number| Self::remote_fetch_checkpoint(&store, checkpoint_number))
                .pipe(futures::stream::iter)
                .buffered(batch_size);

            while let Some(checkpoint) = checkpoint_stream.next().await {
                if sender.send(checkpoint).await.is_err() {
                    info!("remote reader dropped");
                    break;
                }
            }
        });
        receiver
    }

    fn remote_fetch(&mut self) -> Vec<CheckpointData> {
        let mut checkpoints = vec![];
        if self.remote_fetcher_receiver.is_none() {
            self.remote_fetcher_receiver = Some(self.start_remote_fetcher());
        }
        while !self.exceeds_capacity(self.current_checkpoint_number + checkpoints.len() as u64) {
            match self.remote_fetcher_receiver.as_mut().unwrap().try_recv() {
                Ok(Ok((checkpoint, size))) => {
                    self.data_limiter.add(&checkpoint, size);
                    checkpoints.push(checkpoint);
                }
                Ok(Err(err)) => {
                    error!("remote reader transient error {:?}", err);
                    self.remote_fetcher_receiver = None;
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    error!("remote reader channel disconnect error");
                    self.remote_fetcher_receiver = None;
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        checkpoints
    }

    async fn sync(&mut self) -> Result<()> {
        let backoff = backoff::ExponentialBackoff::default();
        let mut checkpoints = backoff::future::retry(backoff, || async {
            self.read_local_files().await.map_err(|err| {
                info!("transient local read error {:?}", err);
                backoff::Error::transient(err)
            })
        })
        .await?;

        let mut read_source: &str = "local";
        if self.remote_store_url.is_some()
            && (checkpoints.is_empty()
                || checkpoints[0].checkpoint_summary.sequence_number
                    > self.current_checkpoint_number)
        {
            checkpoints = self.remote_fetch();
            read_source = "remote";
        } else {
            // cancel remote fetcher execution because local reader has made progress
            self.remote_fetcher_receiver = None;
        }

        info!(
            "Read from {}. Current checkpoint number: {}, pruning watermark: {}, new updates: {:?}",
            read_source,
            self.current_checkpoint_number,
            self.last_pruned_watermark,
            checkpoints.len(),
        );
        for checkpoint in checkpoints {
            if read_source == "local"
                && checkpoint.checkpoint_summary.sequence_number > self.current_checkpoint_number
            {
                break;
            }
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

    pub fn initialize(
        path: PathBuf,
        starting_checkpoint_number: CheckpointSequenceNumber,
        remote_store_url: Option<String>,
        remote_store_options: Vec<(String, String)>,
        options: ReaderOptions,
    ) -> (
        Self,
        mpsc::Receiver<CheckpointData>,
        mpsc::Sender<CheckpointSequenceNumber>,
        oneshot::Sender<()>,
    ) {
        let (checkpoint_sender, checkpoint_recv) = mpsc::channel(MAX_CHECKPOINTS_IN_PROGRESS);
        let (processed_sender, processed_receiver) = mpsc::channel(MAX_CHECKPOINTS_IN_PROGRESS);
        let (exit_sender, exit_receiver) = oneshot::channel();
        let reader = Self {
            path,
            remote_store_url,
            remote_store_options,
            current_checkpoint_number: starting_checkpoint_number,
            last_pruned_watermark: starting_checkpoint_number,
            checkpoint_sender,
            processed_receiver,
            remote_fetcher_receiver: None,
            exit_receiver,
            data_limiter: DataLimiter::new(options.data_limit),
            options,
        };
        (reader, checkpoint_recv, processed_sender, exit_sender)
    }

    pub async fn run(mut self) -> Result<()> {
        let (inotify_sender, mut inotify_recv) = mpsc::channel(1);
        std::fs::create_dir_all(self.path.clone()).expect("failed to create a directory");
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
            .watch(&self.path, RecursiveMode::NonRecursive)
            .expect("Inotify watcher failed");
        self.gc_processed_files(self.last_pruned_watermark)
            .expect("Failed to clean the directory");

        loop {
            tokio::select! {
                _ = &mut self.exit_receiver => break,
                Some(gc_checkpoint_number) = self.processed_receiver.recv() => {
                    self.gc_processed_files(gc_checkpoint_number).expect("Failed to clean the directory");
                }
                Ok(Some(_)) | Err(_) = timeout(Duration::from_millis(self.options.tick_interal_ms), inotify_recv.recv())  => {
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
