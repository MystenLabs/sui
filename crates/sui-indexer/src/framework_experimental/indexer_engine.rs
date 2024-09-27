// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::framework_experimental::indexer_pipeline::IndexerPipelineTrait;
use crate::framework_experimental::progress::ProgressTracker;
use mysten_metrics::spawn_monitored_task;
use std::path::PathBuf;
use std::sync::Arc;
use sui_data_ingestion_core::reader::CheckpointReader;
use sui_data_ingestion_core::ReaderOptions;
use sui_types::full_checkpoint_content::CheckpointData;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, info};

#[allow(dead_code)]
pub struct IndexerEngine {
    reader_exit_sender: tokio::sync::oneshot::Sender<()>,
    exit_receiver: tokio::sync::oneshot::Receiver<()>,
    checkpoint_recv: mpsc::Receiver<Arc<CheckpointData>>,
    reader_handle: JoinHandle<anyhow::Result<()>>,
    pipelines: Vec<Arc<dyn IndexerPipelineTrait>>,
}

#[allow(dead_code)]
impl IndexerEngine {
    pub async fn initialize(
        path: PathBuf,
        remote_store_url: Option<String>,
        remote_store_options: Vec<(String, String)>,
        reader_options: ReaderOptions,
        pipelines: Vec<Arc<dyn IndexerPipelineTrait>>,
        progress_tracker: ProgressTracker,
    ) -> (Self, tokio::sync::oneshot::Sender<()>) {
        let min_start_checkpoint = progress_tracker.get_min_progress();
        let (checkpoint_reader, checkpoint_recv, gc_sender, reader_exit_sender) =
            CheckpointReader::initialize(
                path,
                min_start_checkpoint,
                remote_store_url,
                remote_store_options,
                reader_options,
            );
        progress_tracker.start(gc_sender);
        let (exit_sender, exit_receiver) = tokio::sync::oneshot::channel();
        let reader_handle = spawn_monitored_task!(checkpoint_reader.run());
        let engine = Self {
            exit_receiver,
            reader_exit_sender,
            checkpoint_recv,
            reader_handle,
            pipelines,
        };
        (engine, exit_sender)
    }

    pub async fn run(mut self) {
        info!(
            "Starting the indexer engine with the following pipelines: {:?}",
            self.pipelines
                .iter()
                .map(|p| p.get_pipeline_name())
                .collect::<Vec<_>>()
        );
        loop {
            tokio::select! {
                Some(checkpoint) = self.checkpoint_recv.recv() => {
                    debug!("Received checkpoint: {:?}", checkpoint);
                    for pipeline in self.pipelines.clone() {
                        let checkpoint_clone = checkpoint.clone();
                        spawn_monitored_task!(pipeline.process_checkpoint(checkpoint_clone));
                    }
                }
                _ = &mut self.exit_receiver => {
                    break;
                }
            }
        }
        let _ = self.reader_exit_sender.send(());
        let _ = self.reader_handle.await;
    }
}
