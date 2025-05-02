// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use anyhow::Result;
use object_store::path::Path;
use object_store::DynObjectStore;
use serde::Serialize;
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex};
use tracing::{error, info};

use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_data_ingestion_core::Worker;
use sui_storage::object_store::util::{copy_file, path_to_filesystem};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::analytics_metrics::AnalyticsMetrics;
use crate::handlers::AnalyticsHandler;
use crate::writers::AnalyticsWriter;
use crate::{
    join_paths, FileMetadata, MaxCheckpointReader, ParquetSchema, TaskContext, EPOCH_DIR_PREFIX,
};

struct State<S: Serialize + ParquetSchema + Send + Sync> {
    current_epoch: u64,
    current_checkpoint_range: Range<u64>,
    last_commit_instant: Instant,
    num_checkpoint_iterations: u64,
    writer: Arc<Mutex<Box<dyn AnalyticsWriter<S>>>>,
}

pub struct AnalyticsProcessor<S: Serialize + ParquetSchema + Send + Sync> {
    handler: Box<dyn AnalyticsHandler<S>>,
    state: TokioMutex<State<S>>,
    task_context: TaskContext,
    sender: mpsc::Sender<FileMetadata>,
    #[allow(dead_code)]
    kill_sender: oneshot::Sender<()>,
    #[allow(dead_code)]
    max_checkpoint_sender: oneshot::Sender<()>,
}

const CHECK_FILE_SIZE_ITERATION_CYCLE: u64 = 50;

#[async_trait::async_trait]
impl<S: Serialize + ParquetSchema + Send + Sync + 'static> Worker for AnalyticsProcessor<S> {
    type Result = ();
    async fn process_checkpoint_arc(&self, checkpoint_data: &Arc<CheckpointData>) -> Result<()> {
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

        let (cur_size, cur_rows) = {
            let writer = state.writer.lock().unwrap();
            (writer.file_size()?.unwrap_or(0), writer.rows()?)
        };

        let cut_new_files = (num_checkpoints_processed
            >= self.task_context.config.checkpoint_interval)
            || (state.last_commit_instant.elapsed().as_secs()
                > self.task_context.config.time_interval_s)
            || (state.num_checkpoint_iterations % CHECK_FILE_SIZE_ITERATION_CYCLE == 0
                && cur_size > self.task_context.config.max_file_size_mb * 1024 * 1024)
            || (cur_rows >= self.task_context.config.max_row_count);

        if cut_new_files {
            self.cut(&mut state).await?;
            self.reset(&mut state)?;
        }

        self.task_context
            .metrics
            .total_received
            .with_label_values(&[self.name()])
            .inc();

        let iter = self.handler.process_checkpoint(&checkpoint_data).await?;
        {
            let mut writer = state.writer.lock().unwrap();
            writer.write(iter)?;
        }

        state.current_checkpoint_range.end = state
            .current_checkpoint_range
            .end
            .checked_add(1)
            .context("Checkpoint sequence num overflow")?;
        state.num_checkpoint_iterations += 1;
        Ok(())
    }
}

impl<S: Serialize + ParquetSchema + Send + Sync + 'static> AnalyticsProcessor<S> {
    pub async fn new(
        handler: Box<dyn AnalyticsHandler<S>>,
        writer: Box<dyn AnalyticsWriter<S>>,
        max_checkpoint_reader: Box<dyn MaxCheckpointReader>,
        next_checkpoint_seq_num: CheckpointSequenceNumber,
        task_context: TaskContext,
    ) -> Result<Self> {
        let local_store_config = ObjectStoreConfig {
            directory: Some(task_context.checkpoint_dir_path().to_path_buf()),
            object_store: Some(ObjectStoreType::File),
            ..Default::default()
        };
        let local_object_store = local_store_config.make()?;
        let remote_object_store = task_context.job_config.remote_store_config.make()?;
        let (kill_sender, kill_receiver) = oneshot::channel();
        let (sender, receiver) = mpsc::channel::<FileMetadata>(100);
        let name = handler.name().to_string();
        let checkpoint_dir = task_context.checkpoint_dir_path();
        let cloned_metrics = task_context.metrics.clone();
        tokio::spawn(Self::start_syncing_with_remote(
            remote_object_store,
            local_object_store.clone(),
            checkpoint_dir.to_path_buf(),
            task_context.config.remote_store_path_prefix()?,
            receiver,
            kill_receiver,
            cloned_metrics,
            name.clone(),
        ));
        let (max_checkpoint_sender, max_checkpoint_receiver) = oneshot::channel::<()>();
        tokio::spawn(Self::setup_max_checkpoint_metrics_updates(
            max_checkpoint_reader,
            task_context.metrics.clone(),
            max_checkpoint_receiver,
            name,
        ));

        let state = State {
            current_epoch: 0,
            current_checkpoint_range: next_checkpoint_seq_num..next_checkpoint_seq_num,
            last_commit_instant: Instant::now(),
            num_checkpoint_iterations: 0,
            writer: Arc::new(Mutex::new(writer)),
        };

        Ok(Self {
            handler,
            state: TokioMutex::new(state),
            task_context,
            sender,
            kill_sender,
            max_checkpoint_sender,
        })
    }

    #[inline]
    fn name(&self) -> &str {
        self.handler.name()
    }

    async fn cut(&self, state: &mut State<S>) -> Result<()> {
        if state.current_checkpoint_range.is_empty() {
            return Ok(());
        }

        let writer = state.writer.clone();
        let end_seq = state.current_checkpoint_range.end;

        // flush in blocking pool. These files can be huge and we don't want to block the tokio
        // threads
        let flushed = tokio::task::spawn_blocking(move || {
            let mut w = writer.lock().unwrap();
            w.flush(end_seq)
        })
        .await??;

        if flushed {
            let file_metadata = FileMetadata::new(
                self.task_context.config.file_type,
                self.task_context.config.file_format,
                state.current_epoch,
                state.current_checkpoint_range.clone(),
            );
            self.emit_file_size_metric(&file_metadata)?;

            self.sender.send(file_metadata).await?;
            tokio::task::yield_now().await;
        }
        Ok(())
    }

    fn emit_file_size_metric(&self, file_metadata: &FileMetadata) -> Result<()> {
        let object_path = file_metadata.file_path();
        let file_path = path_to_filesystem(
            self.task_context.checkpoint_dir_path().to_path_buf(),
            &object_path,
        )?;
        if file_path.exists() {
            if let Ok(metadata) = fs::metadata(&file_path) {
                let file_size = metadata.len();
                self.task_context
                    .metrics
                    .file_size_bytes
                    .with_label_values(&[self.name()])
                    .observe(file_size as f64);
            }
        };
        Ok(())
    }

    fn update_to_next_epoch(&self, epoch: u64, state: &mut State<S>) {
        state.current_epoch = epoch;
    }

    fn epoch_dir(&self, state: &State<S>) -> Result<PathBuf> {
        let path = path_to_filesystem(
            self.task_context.checkpoint_dir_path().to_path_buf(),
            &self.task_context.config.file_type.dir_prefix(),
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
        {
            let mut writer = state.writer.lock().unwrap();
            writer.reset(state.current_epoch, state.current_checkpoint_range.start)?;
        }
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
        let remote_dest = join_paths(prefix.as_ref(), &path);
        info!("Syncing file to remote: {:?}", &remote_dest);
        copy_file(&path, &remote_dest, &from, &to).await?;
        fs::remove_file(path_to_filesystem(dir, &path)?)?;
        Ok(())
    }
}
