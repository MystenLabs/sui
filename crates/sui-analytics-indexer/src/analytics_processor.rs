// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use anyhow::Result;
use object_store::path::Path;
use object_store::DynObjectStore;
use serde::Serialize;
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{error, info};

use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_data_ingestion_core::Worker;
use sui_rpc_api::CheckpointData;
use sui_storage::object_store::util::{copy_file, path_to_filesystem};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::analytics_metrics::AnalyticsMetrics;
use crate::handlers::AnalyticsHandler;
use crate::writers::AnalyticsWriter;
use crate::{
    join_paths, AnalyticsIndexerConfig, FileMetadata, MaxCheckpointReader, ParquetSchema,
    EPOCH_DIR_PREFIX,
};

struct State<S: Serialize + ParquetSchema> {
    current_epoch: u64,
    current_checkpoint_range: Range<u64>,
    last_commit_instant: Instant,
    num_checkpoint_iterations: u64,
    writer: Box<dyn AnalyticsWriter<S>>,
}

pub struct AnalyticsProcessor<S: Serialize + ParquetSchema> {
    handler: Box<dyn AnalyticsHandler<S>>,
    state: Mutex<State<S>>,
    metrics: AnalyticsMetrics,
    config: AnalyticsIndexerConfig,
    sender: mpsc::Sender<FileMetadata>,
    #[allow(dead_code)]
    kill_sender: oneshot::Sender<()>,
    #[allow(dead_code)]
    max_checkpoint_sender: oneshot::Sender<()>,
}

const CHECK_FILE_SIZE_ITERATION_CYCLE: u64 = 50;

#[async_trait::async_trait]
impl<S: Serialize + ParquetSchema + 'static> Worker for AnalyticsProcessor<S> {
    type Result = ();
    async fn process_checkpoint(&self, checkpoint_data: &CheckpointData) -> Result<()> {
        // get epoch id, checkpoint sequence number and timestamp, those are important
        // indexes when operating on data
        let epoch: u64 = checkpoint_data.checkpoint_summary.epoch();
        let checkpoint_num: u64 = *checkpoint_data.checkpoint_summary.sequence_number();
        let timestamp: u64 = checkpoint_data.checkpoint_summary.data().timestamp_ms;
        info!("Processing checkpoint {checkpoint_num}, epoch {epoch}, timestamp {timestamp}");
        let mut state = self.state.lock().await;
        if epoch > state.current_epoch {
            self.cut(&mut state).await?;
            self.update_to_next_epoch(epoch, &mut state);
            self.create_epoch_dirs(&state)?;
            self.reset(&mut state)?;
        }

        assert_eq!(epoch, state.current_epoch);

        assert_eq!(checkpoint_num, state.current_checkpoint_range.end);

        let num_checkpoints_processed =
            state.current_checkpoint_range.end - state.current_checkpoint_range.start;
        let cut_new_files = (num_checkpoints_processed >= self.config.checkpoint_interval)
            || (state.last_commit_instant.elapsed().as_secs() > self.config.time_interval_s)
            || (state.num_checkpoint_iterations % CHECK_FILE_SIZE_ITERATION_CYCLE == 0
                && state.writer.file_size()?.unwrap_or(0)
                    > self.config.max_file_size_mb * 1024 * 1024);
        if cut_new_files {
            self.cut(&mut state).await?;
            self.reset(&mut state)?;
        }
        self.metrics
            .total_received
            .with_label_values(&[self.name()])
            .inc();
        self.handler.process_checkpoint(checkpoint_data).await?;
        let rows = self.handler.read().await?;
        state.writer.write(&rows)?;
        state.current_checkpoint_range.end = state
            .current_checkpoint_range
            .end
            .checked_add(1)
            .context("Checkpoint sequence num overflow")?;
        state.num_checkpoint_iterations += 1;
        Ok(())
    }
}

impl<S: Serialize + ParquetSchema + 'static> AnalyticsProcessor<S> {
    pub async fn new(
        handler: Box<dyn AnalyticsHandler<S>>,
        writer: Box<dyn AnalyticsWriter<S>>,
        max_checkpoint_reader: Box<dyn MaxCheckpointReader>,
        next_checkpoint_seq_num: CheckpointSequenceNumber,
        metrics: AnalyticsMetrics,
        config: AnalyticsIndexerConfig,
    ) -> Result<Self> {
        let local_store_config = ObjectStoreConfig {
            directory: Some(config.checkpoint_dir.clone()),
            object_store: Some(ObjectStoreType::File),
            ..Default::default()
        };
        let local_object_store = local_store_config.make()?;
        let remote_object_store = config.remote_store_config.make()?;
        let (kill_sender, kill_receiver) = oneshot::channel::<()>();
        let (sender, receiver) = mpsc::channel::<FileMetadata>(100);
        let name: String = handler.name().parse()?;
        let checkpoint_dir = config.checkpoint_dir.clone();
        let cloned_metrics = metrics.clone();
        tokio::task::spawn(Self::start_syncing_with_remote(
            remote_object_store,
            local_object_store.clone(),
            checkpoint_dir,
            config.remote_store_path_prefix.clone(),
            receiver,
            kill_receiver,
            cloned_metrics,
            name.clone(),
        ));
        let (max_checkpoint_sender, max_checkpoint_receiver) = oneshot::channel::<()>();
        tokio::task::spawn(Self::setup_max_checkpoint_metrics_updates(
            max_checkpoint_reader,
            metrics.clone(),
            max_checkpoint_receiver,
            name,
        ));
        let state = State {
            current_epoch: 0,
            current_checkpoint_range: next_checkpoint_seq_num..next_checkpoint_seq_num,
            last_commit_instant: Instant::now(),
            num_checkpoint_iterations: 0,
            writer,
        };
        Ok(Self {
            handler,
            state: Mutex::new(state),
            kill_sender,
            sender,
            max_checkpoint_sender,
            metrics,
            config,
        })
    }

    fn name(&self) -> &str {
        self.handler.name()
    }

    async fn cut(&self, state: &mut State<S>) -> anyhow::Result<()> {
        if !state.current_checkpoint_range.is_empty()
            && state.writer.flush(state.current_checkpoint_range.end)?
        {
            let file_metadata = FileMetadata::new(
                self.config.file_type,
                self.config.file_format,
                state.current_epoch,
                state.current_checkpoint_range.clone(),
            );
            self.sender.send(file_metadata).await?;
            tokio::task::yield_now().await;
        }
        Ok(())
    }

    fn update_to_next_epoch(&self, epoch: u64, state: &mut State<S>) {
        state.current_epoch = epoch;
    }

    fn epoch_dir(&self, state: &State<S>) -> Result<PathBuf> {
        let path = path_to_filesystem(
            self.config.checkpoint_dir.to_path_buf(),
            &self.config.file_type.dir_prefix(),
        )?
        .join(format!("{}{}", EPOCH_DIR_PREFIX, state.current_epoch));
        Ok(path)
    }

    fn create_epoch_dirs(&self, state: &State<S>) -> Result<()> {
        let epoch_dir = self.epoch_dir(state)?;
        if epoch_dir.exists() {
            fs::remove_dir_all(&epoch_dir)?;
        }
        fs::create_dir_all(&epoch_dir)?;
        Ok(())
    }

    fn reset(&self, state: &mut State<S>) -> Result<()> {
        self.reset_checkpoint_range(state);
        state
            .writer
            .reset(state.current_epoch, state.current_checkpoint_range.start)?;
        self.reset_last_commit_ts(state);
        Ok(())
    }

    fn reset_checkpoint_range(&self, state: &mut State<S>) {
        state.current_checkpoint_range =
            state.current_checkpoint_range.end..state.current_checkpoint_range.end
    }

    fn reset_last_commit_ts(&self, state: &mut State<S>) {
        state.last_commit_instant = Instant::now();
    }

    async fn start_syncing_with_remote(
        remote_object_store: Arc<DynObjectStore>,
        local_object_store: Arc<DynObjectStore>,
        local_staging_root_dir: PathBuf,
        remote_store_path_prefix: Option<Path>,
        mut file_recv: mpsc::Receiver<FileMetadata>,
        mut recv: oneshot::Receiver<()>,
        metrics: AnalyticsMetrics,
        name: String,
    ) -> Result<()> {
        loop {
            tokio::select! {
                _ = &mut recv => break,
                file = file_recv.recv() => {
                    if let Some(file_metadata) = file {
                        info!("Received {name} file with checkpoints: {:?}", &file_metadata.checkpoint_seq_range);
                        let checkpoint_seq_num = file_metadata.checkpoint_seq_range.end;
                        Self::sync_file_to_remote(
                                local_staging_root_dir.clone(),
                                file_metadata.file_path(),
                                remote_store_path_prefix.clone(),
                                local_object_store.clone(),
                                remote_object_store.clone()
                            )
                            .await
                            .expect("Syncing checkpoint should not fail");
                        metrics.last_uploaded_checkpoint.with_label_values(&[&name]).set(checkpoint_seq_num as i64);
                    } else {
                        info!("Terminating upload sync loop");
                        break;
                    }
                },
            }
        }
        Ok(())
    }

    async fn setup_max_checkpoint_metrics_updates(
        max_checkpoint_reader: Box<dyn MaxCheckpointReader>,
        analytics_metrics: AnalyticsMetrics,
        mut recv: oneshot::Receiver<()>,
        handler_name: String,
    ) -> Result<()> {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            tokio::select! {
                _ = &mut recv => break,
                 _ = interval.tick() => {
                    let max_checkpoint = max_checkpoint_reader.max_checkpoint().await;
                    if let Ok(max_checkpoint) = max_checkpoint {
                        analytics_metrics
                            .max_checkpoint_on_store
                            .with_label_values(&[&handler_name])
                            .set(max_checkpoint);
                    } else {
                        error!("Failed to read max checkpoint for {} with err: {}", handler_name, max_checkpoint.unwrap_err());
                    }

                 }
            }
        }
        Ok(())
    }

    async fn sync_file_to_remote(
        dir: PathBuf,
        path: Path,
        prefix: Option<Path>,
        from: Arc<DynObjectStore>,
        to: Arc<DynObjectStore>,
    ) -> Result<()> {
        let remote_dest = join_paths(prefix, &path);
        info!("Syncing file to remote: {:?}", &remote_dest);
        copy_file(&path, &remote_dest, &from, &to).await?;
        fs::remove_file(path_to_filesystem(dir, &path)?)?;
        Ok(())
    }
}
