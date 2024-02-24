// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::executor::MAX_CHECKPOINTS_IN_PROGRESS;
use anyhow::anyhow;
use anyhow::Result;
use futures::future::try_join_all;
use notify::RecursiveMode;
use notify::Watcher;
use object_store::path::Path;
use object_store::{parse_url_opts, ObjectStore};
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use sui_storage::blob::Blob;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::timeout;
use tracing::{debug, info};
use url::Url;

pub(crate) const ENV_VAR_LOCAL_READ_TIMEOUT_MS: &str = "LOCAL_READ_TIMEOUT_MS";

/// Implements a checkpoint reader that monitors a local directory.
/// Designed for setups where the indexer daemon is colocated with FN.
/// This implementation is push-based and utilizes the inotify API.
pub struct CheckpointReader {
    path: PathBuf,
    remote_store: Option<Box<dyn ObjectStore>>,
    remote_read_batch_size: usize,
    current_checkpoint_number: CheckpointSequenceNumber,
    last_pruned_watermark: CheckpointSequenceNumber,
    checkpoint_sender: mpsc::Sender<CheckpointData>,
    processed_receiver: mpsc::Receiver<CheckpointSequenceNumber>,
    exit_receiver: oneshot::Receiver<()>,
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
        for (idx, (sequence_number, filename)) in files.iter().enumerate() {
            if self.current_checkpoint_number + idx as u64 != *sequence_number {
                return Err(anyhow!("checkpoint sequence should not have any gaps"));
            }
            let checkpoint = Blob::from_bytes::<CheckpointData>(&fs::read(filename)?)?;
            checkpoints.push(checkpoint);
        }
        Ok(checkpoints)
    }

    async fn remote_fetch(&self) -> Result<Vec<CheckpointData>> {
        let mut checkpoints = vec![];
        if let Some(ref store) = self.remote_store {
            let limit = std::cmp::min(
                self.current_checkpoint_number + self.remote_read_batch_size as u64,
                self.last_pruned_watermark + MAX_CHECKPOINTS_IN_PROGRESS as u64,
            );
            let futures =
                (self.current_checkpoint_number..limit).map(|checkpoint_number| async move {
                    let path = Path::from(format!("{}.chk", checkpoint_number));
                    match store.get(&path).await {
                        Ok(resp) => resp.bytes().await.map(Some),
                        Err(err) if err.to_string().contains("404") => Ok(None),
                        Err(err) => Err(err),
                    }
                });
            for bytes in try_join_all(futures).await? {
                if bytes.is_none() {
                    break;
                }
                let checkpoint = Blob::from_bytes::<CheckpointData>(&bytes.unwrap())?;
                checkpoints.push(checkpoint);
            }
        }
        Ok(checkpoints)
    }

    async fn sync(&mut self) -> Result<()> {
        let backoff = backoff::ExponentialBackoff::default();
        let mut checkpoints = backoff::future::retry(backoff, || async {
            self.read_local_files()
                .await
                .map_err(backoff::Error::transient)
        })
        .await?;

        if checkpoints.is_empty() {
            checkpoints = self.remote_fetch().await?;
        }

        info!(
            "Local reader. Current checkpoint number: {}, pruning watermark: {}, unprocessed checkpoints: {:?}",
            self.current_checkpoint_number, self.last_pruned_watermark, checkpoints.len(),
        );
        for checkpoint in checkpoints {
            assert_eq!(
                checkpoint.checkpoint_summary.sequence_number,
                self.current_checkpoint_number
            );
            if (MAX_CHECKPOINTS_IN_PROGRESS as u64 + self.last_pruned_watermark)
                <= checkpoint.checkpoint_summary.sequence_number
            {
                break;
            }
            self.checkpoint_sender.send(checkpoint).await?;
            self.current_checkpoint_number += 1;
        }
        Ok(())
    }

    /// Cleans the local directory by removing all processed checkpoint files.
    fn gc_processed_files(&mut self, watermark: CheckpointSequenceNumber) -> Result<()> {
        info!("cleaning processed files, watermark is {}", watermark);
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
        remote_read_batch_size: usize,
    ) -> (
        Self,
        mpsc::Receiver<CheckpointData>,
        mpsc::Sender<CheckpointSequenceNumber>,
        oneshot::Sender<()>,
    ) {
        let (checkpoint_sender, checkpoint_recv) = mpsc::channel(MAX_CHECKPOINTS_IN_PROGRESS);
        let (processed_sender, processed_receiver) = mpsc::channel(MAX_CHECKPOINTS_IN_PROGRESS);
        let (exit_sender, exit_receiver) = oneshot::channel();
        let remote_store = remote_store_url.map(|url| {
            if remote_store_options.is_empty() {
                let builder = object_store::http::HttpBuilder::new().with_url(url);
                Box::new(
                    builder
                        .build()
                        .expect("failed to parse remote store config"),
                )
            } else {
                parse_url_opts(
                    &Url::parse(&url).expect("failed to parse remote store url"),
                    remote_store_options,
                )
                .expect("failed to parse remote store config")
                .0
            }
        });
        let reader = Self {
            path,
            remote_store,
            current_checkpoint_number: starting_checkpoint_number,
            last_pruned_watermark: starting_checkpoint_number,
            checkpoint_sender,
            processed_receiver,
            remote_read_batch_size,
            exit_receiver,
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

        let timeout_ms = std::env::var(ENV_VAR_LOCAL_READ_TIMEOUT_MS)
            .unwrap_or("1000".to_string())
            .parse::<u64>()?;

        loop {
            tokio::select! {
                _ = &mut self.exit_receiver => break,
                Some(gc_checkpoint_number) = self.processed_receiver.recv() => {
                    self.gc_processed_files(gc_checkpoint_number).expect("Failed to clean the directory");
                }
                Ok(Some(_)) | Err(_) = timeout(Duration::from_millis(timeout_ms), inotify_recv.recv())  => {
                    self.sync().await.expect("Failed to read checkpoint files");
                }
            }
        }
        Ok(())
    }
}
