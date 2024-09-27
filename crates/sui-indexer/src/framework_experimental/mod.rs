// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::database::ConnectionPool;
use crate::framework_experimental::indexer_engine::IndexerEngine;
use crate::framework_experimental::indexer_handler_trait::IndexerHandlerTrait;
use crate::framework_experimental::indexer_pipeline::{IndexerPipeline, IndexerPipelineTrait};
use crate::framework_experimental::progress::ProgressTracker;
use mysten_metrics::spawn_monitored_task;
use std::path::PathBuf;
use std::sync::Arc;
use sui_data_ingestion_core::ReaderOptions;
use tokio::task::JoinHandle;

mod indexer_engine;
mod indexer_handler_trait;
mod indexer_pipeline;
mod progress;

/// This is the primary interface for using the indexer framework.
/// It allows registering multiple pipelines and starting the engine.
/// Each pipeline needs to implement a type for `IndexerHandlerTrait`.
/// NOTE: This is still experimental, please do not use it yet in production pipeline.
#[allow(dead_code)]
pub struct IndexerPipelines {
    pipelines: Vec<Arc<dyn IndexerPipelineTrait>>,
    pool: ConnectionPool,
    progress_tracker: ProgressTracker,
    commit_batch_size: usize,
}

#[allow(dead_code)]
impl IndexerPipelines {
    pub fn new(pool: ConnectionPool, commit_batch_size: usize) -> Self {
        let progress_tracker = ProgressTracker::new();
        Self {
            pipelines: vec![],
            pool,
            progress_tracker,
            commit_batch_size,
        }
    }

    pub async fn register_pipeline<H: IndexerHandlerTrait + 'static>(mut self) -> Self {
        let task_name = H::get_name();
        let last_checkpoint = H::get_progress().await;
        let progress_sender = self.progress_tracker.register(task_name, last_checkpoint);
        let pool = self.pool.clone();
        self.pipelines.push(Arc::new(IndexerPipeline::<H>::new(
            pool,
            progress_sender,
            last_checkpoint,
            self.commit_batch_size,
        )));
        self
    }

    /// Returns a handle to the indexer engine and a sender to signal the engine to stop.
    pub async fn start(
        self,
        path: PathBuf,
        remote_store_url: Option<String>,
        remote_store_options: Vec<(String, String)>,
        reader_options: ReaderOptions,
    ) -> (JoinHandle<()>, tokio::sync::oneshot::Sender<()>) {
        let (engine, exit_sender) = IndexerEngine::initialize(
            path,
            remote_store_url,
            remote_store_options,
            reader_options,
            self.pipelines,
            self.progress_tracker,
        )
        .await;
        let handle = spawn_monitored_task!(engine.run());
        (handle, exit_sender)
    }
}
