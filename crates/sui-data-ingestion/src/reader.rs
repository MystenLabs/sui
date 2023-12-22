// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::executor::MAX_CHECKPOINTS_IN_PROGRESS;
use anyhow::anyhow;
use anyhow::Result;
use notify::RecursiveMode;
use notify::Watcher;
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
use tracing::info;

pub(crate) const ENV_VAR_LOCAL_READ_TIMEOUT_MS: &str = "LOCAL_READ_TIMEOUT_MS";

/// Implements a checkpoint reader that monitors a local directory.
/// Designed for setups where the indexer daemon is colocated with FN.
/// This implementation is push-based and utilizes the inotify API.
pub struct LocalReader {
    path: PathBuf,
    checkpoint_sender: mpsc::Sender<CheckpointData>,
    processed_receiver: mpsc::Receiver<CheckpointSequenceNumber>,
    exit_receiver: oneshot::Receiver<()>,
}

impl LocalReader {
    /// Represents a single iteration of the reader.
    /// Reads files in a local directory, validates them, and forwards `CheckpointData` to the executor.
    async fn read_files(
        &self,
        current_checkpoint_number: CheckpointSequenceNumber,
    ) -> Result<CheckpointSequenceNumber> {
        let mut files = vec![];
        for entry in fs::read_dir(self.path.clone())? {
            let entry = entry?;
            let filename = entry.file_name();
            if let Some(sequence_number) = Self::checkpoint_number_from_file_path(&filename) {
                if sequence_number >= current_checkpoint_number {
                    files.push((sequence_number, entry.path()));
                }
            }
        }
        files.sort();
        info!(
            "local reader: current checkpoint number is {}. Unprocessed local files are {:?}",
            current_checkpoint_number, files
        );
        for (idx, (sequence_number, filename)) in files.iter().enumerate() {
            if current_checkpoint_number + idx as u64 != *sequence_number {
                return Err(anyhow!("checkpoint sequence should not have any gaps"));
            }
            let checkpoint = Blob::from_bytes::<CheckpointData>(&fs::read(filename)?)?;
            self.checkpoint_sender.send(checkpoint).await?;
        }
        Ok(current_checkpoint_number + files.len() as u64)
    }

    /// Cleans the local directory by removing all processed checkpoint files.
    fn gc_processed_files(&self, watermark: CheckpointSequenceNumber) -> Result<()> {
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
            checkpoint_sender,
            processed_receiver,
            exit_receiver,
        };
        (reader, checkpoint_recv, processed_sender, exit_sender)
    }

    pub async fn run(mut self, mut checkpoint_number: CheckpointSequenceNumber) -> Result<()> {
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
            .unwrap_or("60000".to_string())
            .parse::<u64>()?;

        loop {
            tokio::select! {
                Ok(Some(_)) | Err(_) = timeout(Duration::from_millis(timeout_ms), inotify_recv.recv())  => {
                    let backoff = backoff::ExponentialBackoff::default();
                    checkpoint_number = backoff::future::retry(backoff, || async {
                        self.read_files(checkpoint_number).await.map_err(backoff::Error::transient)
                    })
                    .await
                    .expect("Failed to read checkpoint files");
                }
                Some(gc_checkpoint_number) = self.processed_receiver.recv() => {
                    self.gc_processed_files(gc_checkpoint_number).expect("Failed to clean the directory");
                }
                _ = &mut self.exit_receiver => break,
            }
        }
        Ok(())
    }
}
