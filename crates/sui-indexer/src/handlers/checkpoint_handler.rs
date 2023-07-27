// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use fastcrypto::traits::ToFromBytes;
use futures::future::join_all;
use futures::stream::FuturesOrdered;
use futures::FutureExt;
use futures::StreamExt;
use jsonrpsee::http_client::HttpClient;
use move_core_types::ident_str;
use std::collections::BTreeMap;
use std::env;
use std::sync::Arc;
use tap::tap::TapFallible;
use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use mysten_metrics::spawn_monitored_task;
use sui_core::subscription_handler::SubscriptionHandler;
use sui_json_rpc::api::ReadApiClient;
use sui_json_rpc_types::{
    OwnedObjectRef, SuiGetPastObjectRequest, SuiObjectData, SuiObjectDataOptions, SuiRawData,
    SuiTransactionBlockDataAPI, SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI,
};
use sui_sdk::error::Error;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::committee::EpochId;
use sui_types::messages_checkpoint::{CheckpointCommitment, CheckpointSequenceNumber};
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use sui_types::sui_system_state::{get_sui_system_state, SuiSystemStateTrait};
use sui_types::SUI_SYSTEM_ADDRESS;

use crate::errors::DataDownloadError;
use crate::errors::IndexerError;
use crate::metrics::IndexerMetrics;
use crate::models::checkpoints::Checkpoint;
use crate::models::epoch::{DBEpochInfo, SystemEpochInfoEvent};
use crate::models::objects::{DeletedObject, Object, ObjectStatus};
use crate::models::packages::Package;
use crate::models::transactions::Transaction;
use crate::store::{
    CheckpointData, IndexerStore, TemporaryCheckpointStore, TemporaryEpochStore,
    TransactionObjectChanges,
};
use crate::types::{CheckpointTransactionBlockResponse, TemporaryTransactionBlockResponseStore};
use crate::utils::multi_get_full_transactions;
use crate::IndexerConfig;

const MAX_PARALLEL_DOWNLOADS: usize = 24;
const DOWNLOAD_RETRY_INTERVAL_IN_SECS: u64 = 10;
const CHECKPOINT_INDEX_RETRY_INTERVAL_IN_SECS: u64 = 10;
const DB_COMMIT_RETRY_INTERVAL_IN_MILLIS: u64 = 100;
const MULTI_GET_CHUNK_SIZE: usize = 50;
const CHECKPOINT_QUEUE_LIMIT: usize = 24;
const DOWNLOAD_QUEUE_SIZE: usize = 10000;
const EPOCH_QUEUE_LIMIT: usize = 2;

#[allow(clippy::type_complexity)]
#[derive(Clone)]
pub struct CheckpointHandler<S> {
    state: S,
    http_client: HttpClient,
    metrics: IndexerMetrics,
    config: IndexerConfig,
    // FIXME: these do not need to be Arc<Mutex<>>
    tx_checkpoint_sender: Arc<Mutex<Sender<TemporaryCheckpointStore>>>,
    tx_checkpoint_receiver: Arc<Mutex<Receiver<TemporaryCheckpointStore>>>,
    object_changes_sender:
        Arc<Mutex<Sender<(CheckpointSequenceNumber, Vec<TransactionObjectChanges>)>>>,
    object_changes_receiver:
        Arc<Mutex<Receiver<(CheckpointSequenceNumber, Vec<TransactionObjectChanges>)>>>,
    epoch_sender: Arc<Mutex<Sender<TemporaryEpochStore>>>,
    epoch_receiver: Arc<Mutex<Receiver<TemporaryEpochStore>>>,
}

impl<S> CheckpointHandler<S>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    pub fn new(
        state: S,
        http_client: HttpClient,
        _subscription_handler: Arc<SubscriptionHandler>,
        metrics: IndexerMetrics,
        config: &IndexerConfig,
    ) -> Self {
        let (tx_checkpoint_sender, tx_checkpoint_receiver) = mpsc::channel(CHECKPOINT_QUEUE_LIMIT);
        let (object_changes_sender, object_changes_receiver) =
            mpsc::channel(CHECKPOINT_QUEUE_LIMIT);
        let (epoch_sender, epoch_receiver) = mpsc::channel(EPOCH_QUEUE_LIMIT);
        Self {
            state,
            http_client,
            metrics,
            config: config.clone(),
            tx_checkpoint_sender: Arc::new(Mutex::new(tx_checkpoint_sender)),
            tx_checkpoint_receiver: Arc::new(Mutex::new(tx_checkpoint_receiver)),
            object_changes_sender: Arc::new(Mutex::new(object_changes_sender)),
            object_changes_receiver: Arc::new(Mutex::new(object_changes_receiver)),
            epoch_sender: Arc::new(Mutex::new(epoch_sender)),
            epoch_receiver: Arc::new(Mutex::new(epoch_receiver)),
        }
    }

    pub fn spawn(self) -> JoinHandle<()> {
        info!("Indexer checkpoint handler started...");
        let (downloaded_checkpoint_data_sender, downloaded_checkpoint_data_receiver) =
            tokio::sync::mpsc::channel(DOWNLOAD_QUEUE_SIZE);
        let self_clone = self.clone();
        // Start Checkpoint/Tx Downloader
        spawn_monitored_task!(async move {
            // -1 will be returned when checkpoints table is empty.
            let last_seq_from_db = self_clone
                .state
                .get_latest_tx_checkpoint_sequence_number()
                .await
                .expect("Failed to get latest tx checkpoint sequence number from DB");
            Self::run_checkpoint_downloader(
                self_clone,
                last_seq_from_db + 1,
                downloaded_checkpoint_data_sender,
            )
            .await;
        });
        // Start Checkpoint/Tx Indexing Processor
        let mut checkpoint_processor = CheckpointProcessor {
            state: self.state.clone(),
            metrics: self.metrics.clone(),
            epoch_sender: self.epoch_sender.clone(),
            checkpoint_sender: self.tx_checkpoint_sender.clone(),
            downloaded_checkpoint_data_receiver,
        };
        spawn_monitored_task!(async move {
            let mut res = checkpoint_processor.run().await;
            while let Err(e) = &res {
                warn!(
                    "Indexer checkpoint data processing failed with error: {:?}, retrying after {:?} secs...",
                    e, CHECKPOINT_INDEX_RETRY_INTERVAL_IN_SECS
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    CHECKPOINT_INDEX_RETRY_INTERVAL_IN_SECS,
                ))
                .await;
                res = checkpoint_processor.run().await;
            }
        });
        // Start Checkpoint/Tx Commit Handler
        let tx_checkpoint_commit_handler = self.clone();
        spawn_monitored_task!(async move {
            let mut checkpoint_commit_res = tx_checkpoint_commit_handler
                .start_tx_checkpoint_commit()
                .await;
            while let Err(e) = &checkpoint_commit_res {
                warn!(
                    "Indexer checkpoint commit failed with error: {:?}, retrying after {:?} secs...",
                    e, DOWNLOAD_RETRY_INTERVAL_IN_SECS
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    DOWNLOAD_RETRY_INTERVAL_IN_SECS,
                ))
                .await;
                checkpoint_commit_res = tx_checkpoint_commit_handler
                    .start_tx_checkpoint_commit()
                    .await;
            }
        });

        // Start Checkpoint Objects Downloader
        let (downloaded_object_data_sender, downloaded_object_data_receiver) =
            tokio::sync::mpsc::channel(DOWNLOAD_QUEUE_SIZE);
        let self_clone = self.clone();
        spawn_monitored_task!(async move {
            // -1 will be returned when checkpoints table is empty.
            let last_seq_from_db = self_clone
                .state
                .get_latest_object_checkpoint_sequence_number()
                .await
                .expect("Failed to get latest object checkpoint sequence number from DB");
            Self::run_checkpoint_downloader(
                self_clone,
                last_seq_from_db + 1,
                downloaded_object_data_sender,
            )
            .await;
        });

        // Start Checkpoint Objects Indexing Processor
        let mut checkpoint_objects_processor = CheckpointObjectsProcessor {
            metrics: self.metrics.clone(),
            object_changes_sender: self.object_changes_sender.clone(),
            downloaded_object_data_receiver,
        };
        spawn_monitored_task!(async move {
            let mut res = checkpoint_objects_processor.run().await;
            while let Err(e) = &res {
                warn!(
                    "Indexer checkpoint object data processing failed with error: {:?}, retrying after {:?} secs...",
                    e, CHECKPOINT_INDEX_RETRY_INTERVAL_IN_SECS
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    CHECKPOINT_INDEX_RETRY_INTERVAL_IN_SECS,
                ))
                .await;
                res = checkpoint_objects_processor.run().await;
            }
        });

        // Start Checkpoint Objects Commit Handler
        let object_checkpoint_commit_handler = self.clone();
        spawn_monitored_task!(async move {
            let mut checkpoint_commit_res = object_checkpoint_commit_handler
                .start_object_checkpoint_commit()
                .await;
            while let Err(e) = &checkpoint_commit_res {
                warn!(
                    "Indexer object checkpoint commit failed with error: {:?}, retrying after {:?} secs...",
                    e, DOWNLOAD_RETRY_INTERVAL_IN_SECS
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    DOWNLOAD_RETRY_INTERVAL_IN_SECS,
                ))
                .await;
                checkpoint_commit_res = object_checkpoint_commit_handler
                    .start_object_checkpoint_commit()
                    .await;
            }
        });

        // Start Epoch Commit Handler
        spawn_monitored_task!(async move {
            let mut epoch_commit_res = self.start_epoch_commit().await;
            while let Err(e) = &epoch_commit_res {
                warn!(
                    "Indexer epoch commit failed with error: {:?}, retrying after {:?} secs...",
                    e, DOWNLOAD_RETRY_INTERVAL_IN_SECS
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    DOWNLOAD_RETRY_INTERVAL_IN_SECS,
                ))
                .await;
                epoch_commit_res = self.start_epoch_commit().await;
            }
        })
    }

    pub async fn run_checkpoint_downloader(
        checkpoint_download_handler: Self,
        start_seq: i64,
        tx: Sender<CheckpointData>,
    ) {
        let mut checkpoint_download_index_res = checkpoint_download_handler
            .start_download_checkpoint_data(start_seq, tx.clone())
            .await;
        while let Err(e) = &checkpoint_download_index_res {
            error!(
                "Indexer checkpoint download & index failed with error: {:?}, retrying after {:?} secs...",
                e, DOWNLOAD_RETRY_INTERVAL_IN_SECS
            );
            tokio::time::sleep(std::time::Duration::from_secs(
                DOWNLOAD_RETRY_INTERVAL_IN_SECS,
            ))
            .await;
            checkpoint_download_index_res = checkpoint_download_handler
                .start_download_checkpoint_data(
                    e.next_checkpoint_sequence_number as i64,
                    tx.clone(),
                )
                .await;
        }
    }

    async fn start_download_checkpoint_data(
        &self,
        // FIXME: this does not need to be i64,
        starting_checkpoint_seq: i64,
        tx: Sender<CheckpointData>,
    ) -> Result<(), DataDownloadError> {
        info!("Indexer checkpoint download & index task resumed from {starting_checkpoint_seq}...");
        let mut next_cursor_sequence_number = starting_checkpoint_seq;
        // NOTE: we will download checkpoints in parallel, but we will commit them sequentially.
        // We will start with MAX_PARALLEL_DOWNLOADS, and adjust if no more checkpoints are available.
        let current_parallel_downloads = MAX_PARALLEL_DOWNLOADS;
        loop {
            let mut download_futures = FuturesOrdered::new();
            for seq_num in next_cursor_sequence_number
                ..next_cursor_sequence_number + current_parallel_downloads as i64
            {
                download_futures.push_back(self.download_checkpoint_data(seq_num as u64));
            }
            // NOTE: Push sequentially and if one of the downloads failed,
            // we will discard all following checkpoints and retry, to avoid messing up the DB commit order.
            while let Some(res) = download_futures.next().await {
                match res {
                    Ok(checkpoint) => {
                        tx.send(checkpoint)
                            .await
                            .expect("Send to checkpoint channel should not fail");
                        next_cursor_sequence_number += 1;
                    }
                    Err(e) => {
                        return Err(DataDownloadError {
                            error: e,
                            next_checkpoint_sequence_number: next_cursor_sequence_number as u64,
                        })
                    }
                }
            }
        }
    }

    async fn start_tx_checkpoint_commit(&self) -> Result<(), IndexerError> {
        info!("Indexer checkpoint commit task started...");
        let checkpoint_commit_batch_size = env::var("CHECKPOINT_COMMIT_BATCH_SIZE")
            .unwrap_or(5.to_string())
            .parse::<u64>()
            .unwrap();
        loop {
            let mut checkpoint_receiver_guard = self.tx_checkpoint_receiver.lock().await;

            let mut indexed_checkpoint_batch = vec![];
            loop {
                if let Ok(ckp) = checkpoint_receiver_guard.try_recv() {
                    indexed_checkpoint_batch.push(ckp);
                    if indexed_checkpoint_batch.len() >= checkpoint_commit_batch_size as usize {
                        break;
                    }
                } else if indexed_checkpoint_batch.is_empty() {
                    if let Some(ckp) = checkpoint_receiver_guard.recv().await {
                        indexed_checkpoint_batch.push(ckp);
                        break;
                    }
                } else {
                    break;
                }
            }
            drop(checkpoint_receiver_guard);

            let mut checkpoint_batch = vec![];
            let mut tx_batch = vec![];

            if indexed_checkpoint_batch.is_empty() {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }

            if self.config.skip_db_commit {
                info!(
                    "[Checkpoint/Tx] Downloaded and indexed checkpoint {:?} - {:?} successfully, skipping DB commit...",
                    indexed_checkpoint_batch.first().map(|c| c.checkpoint.sequence_number),
                    indexed_checkpoint_batch.last().map(|c| c.checkpoint.sequence_number),
                );
                continue;
            }

            for indexed_checkpoint in indexed_checkpoint_batch {
                // Write checkpoint to DB
                let TemporaryCheckpointStore {
                    checkpoint,
                    transactions,
                    events,
                    packages,
                    input_objects,
                    changed_objects,
                    move_calls,
                    recipients,
                } = indexed_checkpoint;
                checkpoint_batch.push(checkpoint);
                tx_batch.push(transactions);

                // NOTE: retrials are necessary here, otherwise results can be popped and discarded.
                let events_handler = self.clone();
                spawn_monitored_task!(async move {
                    let mut event_commit_res = events_handler.state.persist_events(&events).await;
                    while let Err(e) = event_commit_res {
                        warn!(
                            "Indexer event commit failed with error: {:?}, retrying after {:?} milli-secs...",
                            e, DB_COMMIT_RETRY_INTERVAL_IN_MILLIS
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(
                            DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
                        ))
                        .await;
                        event_commit_res = events_handler.state.persist_events(&events).await;
                    }
                });

                let packages_handler = self.clone();
                spawn_monitored_task!(async move {
                    let mut package_commit_res =
                        packages_handler.state.persist_packages(&packages).await;
                    while let Err(e) = package_commit_res {
                        warn!(
                            "Indexer package commit failed with error: {:?}, retrying after {:?} milli-secs...",
                            e, DB_COMMIT_RETRY_INTERVAL_IN_MILLIS
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(
                            DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
                        ))
                        .await;
                        package_commit_res =
                            packages_handler.state.persist_packages(&packages).await;
                    }
                });

                let tx_index_table_handler = self.clone();
                spawn_monitored_task!(async move {
                    let mut transaction_index_tables_commit_res = tx_index_table_handler
                        .state
                        .persist_transaction_index_tables(
                            &input_objects,
                            &changed_objects,
                            &move_calls,
                            &recipients,
                        )
                        .await;
                    while let Err(e) = transaction_index_tables_commit_res {
                        warn!(
                            "Indexer transaction index tables commit failed with error: {:?}, retrying after {:?} milli-secs...",
                            e, DB_COMMIT_RETRY_INTERVAL_IN_MILLIS
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(
                            DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
                        ))
                        .await;
                        transaction_index_tables_commit_res = tx_index_table_handler
                            .state
                            .persist_transaction_index_tables(
                                &input_objects,
                                &changed_objects,
                                &move_calls,
                                &recipients,
                            )
                            .await;
                    }
                });
            }

            // now commit batched data
            let tx_batch = tx_batch.into_iter().flatten().collect::<Vec<_>>();
            let checkpoint_tx_db_guard = self.metrics.checkpoint_db_commit_latency.start_timer();
            let mut checkpoint_tx_commit_res = self
                .state
                .persist_checkpoint_transactions(
                    &checkpoint_batch,
                    &tx_batch,
                    self.metrics.total_transaction_chunk_committed.clone(),
                )
                .await;
            while let Err(e) = checkpoint_tx_commit_res {
                warn!(
                    "Indexer checkpoint & transaction commit failed with error: {:?}, retrying after {:?} milli-secs...",
                    e, DB_COMMIT_RETRY_INTERVAL_IN_MILLIS
                );
                tokio::time::sleep(std::time::Duration::from_millis(
                    DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
                ))
                .await;
                checkpoint_tx_commit_res = self
                    .state
                    .persist_checkpoint_transactions(
                        &checkpoint_batch,
                        &tx_batch,
                        self.metrics.total_transaction_chunk_committed.clone(),
                    )
                    .await;
            }
            let elapsed = checkpoint_tx_db_guard.stop_and_record();
            // unwrap: batch must not be empty at this point
            let first_checkpoint_seq = checkpoint_batch.first().as_ref().unwrap().sequence_number;
            let last_checkpoint_seq = checkpoint_batch.last().as_ref().unwrap().sequence_number;
            self.metrics
                .latest_tx_checkpoint_sequence_number
                .set(last_checkpoint_seq);

            self.metrics.total_tx_checkpoint_committed.inc();
            let tx_count = tx_batch.len();
            self.metrics
                .total_transaction_committed
                .inc_by(tx_count as u64);
            info!(
                elapsed,
                "Tx Checkpoint {}-{} committed with {} transactions.",
                first_checkpoint_seq,
                last_checkpoint_seq,
                tx_count,
            );
            self.metrics
                .transaction_per_checkpoint
                .observe(tx_count as f64 / (last_checkpoint_seq - first_checkpoint_seq + 1) as f64);
        }
    }

    async fn start_object_checkpoint_commit(&self) -> Result<(), IndexerError> {
        info!("Indexer object checkpoint commit task started...");
        let checkpoint_commit_batch_size = env::var("CHECKPOINT_COMMIT_BATCH_SIZE")
            .unwrap_or(5.to_string())
            .parse::<u64>()
            .unwrap();
        loop {
            let mut checkpoint_receiver_guard = self.object_changes_receiver.lock().await;

            let mut object_changes_batch = vec![];
            let mut seqs = vec![];
            loop {
                if let Ok((seq, object_changes)) = checkpoint_receiver_guard.try_recv() {
                    object_changes_batch.push(object_changes);
                    seqs.push(seq);
                    if object_changes_batch.len() >= checkpoint_commit_batch_size as usize {
                        break;
                    }
                } else if object_changes_batch.is_empty() {
                    if let Some((seq, object_changes)) = checkpoint_receiver_guard.recv().await {
                        object_changes_batch.push(object_changes);
                        seqs.push(seq);
                        break;
                    }
                } else {
                    break;
                }
            }
            drop(checkpoint_receiver_guard);

            let mut object_change_batch = vec![];

            if object_changes_batch.is_empty() {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }
            // unwrap: seqs gets updated along with indexed_checkpoint_batch, hence must not be empty at this point
            let last_checkpoint_seq = seqs.last().unwrap();
            let first_checkpoint_seq = seqs.first().unwrap();

            if self.config.skip_db_commit {
                info!(
                    "[Object] Downloaded and indexed checkpoint {} - {} successfully, skipping DB commit...",
                    last_checkpoint_seq,
                    first_checkpoint_seq,
                );
                continue;
            }
            for object_changes in object_changes_batch {
                object_change_batch.push(object_changes);
            }

            // NOTE: commit object changes in the current task to stick to the original order,
            // spawned tasks are possible to be executed in a different order.
            let object_changes = object_change_batch
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            let object_commit_timer = self.metrics.object_db_commit_latency.start_timer();
            let mut object_changes_commit_res = self
                .state
                .persist_object_changes(
                    &object_changes,
                    self.metrics.object_mutation_db_commit_latency.clone(),
                    self.metrics.object_deletion_db_commit_latency.clone(),
                    self.metrics.total_object_change_chunk_committed.clone(),
                )
                .await;
            while let Err(e) = object_changes_commit_res {
                warn!(
                    "Indexer object changes commit failed with error: {:?}, retrying after {:?} milli-secs...",
                    e, DB_COMMIT_RETRY_INTERVAL_IN_MILLIS
                );
                tokio::time::sleep(std::time::Duration::from_millis(
                    DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
                ))
                .await;
                object_changes_commit_res = self
                    .state
                    .persist_object_changes(
                        &object_changes,
                        self.metrics.object_mutation_db_commit_latency.clone(),
                        self.metrics.object_deletion_db_commit_latency.clone(),
                        self.metrics.total_object_change_chunk_committed.clone(),
                    )
                    .await;
            }
            let elapsed = object_commit_timer.stop_and_record();
            self.metrics.total_object_checkpoint_committed.inc();
            self.metrics
                .total_object_change_committed
                .inc_by(object_changes.len() as u64);
            self.metrics
                .latest_indexer_object_checkpoint_sequence_number
                .set(*last_checkpoint_seq as i64);
            info!(
                elapsed,
                "Object Checkpoint {}-{} committed with {} object changes",
                first_checkpoint_seq,
                last_checkpoint_seq,
                object_changes.len(),
            );
        }
    }

    async fn start_epoch_commit(&self) -> Result<(), IndexerError> {
        info!("Indexer epoch commit task started...");
        loop {
            let mut epoch_receiver_guard = self.epoch_receiver.lock().await;
            let indexed_epoch = epoch_receiver_guard.recv().await;
            drop(epoch_receiver_guard);

            // Write epoch to DB if needed
            if let Some(indexed_epoch) = indexed_epoch {
                if indexed_epoch.last_epoch.is_some() {
                    let epoch_db_guard = self.metrics.epoch_db_commit_latency.start_timer();
                    let mut epoch_commit_res = self.state.persist_epoch(&indexed_epoch).await;
                    // NOTE: retrials are necessary here, otherwise indexed_epoch can be popped and discarded.
                    // FIXME: use macro to replace this pattern in this file.
                    while let Err(e) = epoch_commit_res {
                        warn!(
                            "Indexer epoch commit failed with error: {:?}, retrying after {:?} milli-secs...",
                            e, DB_COMMIT_RETRY_INTERVAL_IN_MILLIS
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(
                            DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
                        ))
                        .await;
                        epoch_commit_res = self.state.persist_epoch(&indexed_epoch).await;
                    }
                    epoch_db_guard.stop_and_record();
                    self.metrics.total_epoch_committed.inc();
                }
            } else {
                // sleep for 1 sec to avoid occupying the mutex, as this happens once per epoch / day
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }

    /// TODO: rewrite this thing.
    /// Download all the data we need for one checkpoint.
    async fn download_checkpoint_data(
        &self,
        seq: CheckpointSequenceNumber,
    ) -> Result<CheckpointData, IndexerError> {
        let latest_fn_checkpoint_seq = self
            .http_client
            .get_latest_checkpoint_sequence_number()
            .await
            .map_err(|e| {
                IndexerError::FullNodeReadingError(format!(
                    "Failed to get latest checkpoint sequence number and error {:?}",
                    e
                ))
            })?;
        self.metrics
            .latest_fullnode_checkpoint_sequence_number
            .set((*latest_fn_checkpoint_seq) as i64);

        let mut checkpoint = self
            .http_client
            .get_checkpoint(seq.into())
            .await
            .map_err(|e| {
                IndexerError::FullNodeReadingError(format!(
                    "Failed to get checkpoint with sequence number {} and error {:?}",
                    seq, e
                ))
            });
        let fn_checkpoint_guard = self
            .metrics
            .fullnode_checkpoint_wait_and_download_latency
            .start_timer();
        while checkpoint.is_err() {
            // sleep for 0.1 second and retry if latest checkpoint is not available yet
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            // TODO(gegaowp): figure how to only measure successful checkpoint download time
            checkpoint = self
                .http_client
                .get_checkpoint(seq.into())
                .await
                .map_err(|e| {
                    IndexerError::FullNodeReadingError(format!(
                        "Failed to get checkpoint with sequence number {} and error {:?}",
                        seq, e
                    ))
                })
        }
        fn_checkpoint_guard.stop_and_record();
        // unwrap here is safe because we checked for error above
        let checkpoint = checkpoint.unwrap();

        let fn_transaction_guard = self
            .metrics
            .fullnode_transaction_download_latency
            .start_timer();
        let transactions = join_all(checkpoint.transactions.chunks(MULTI_GET_CHUNK_SIZE).map(
            |digests| multi_get_full_transactions(self.http_client.clone(), digests.to_vec()),
        ))
        .await
        .into_iter()
        .try_fold(vec![], |mut acc, chunk| {
            acc.extend(chunk?);
            Ok::<_, IndexerError>(acc)
        })?;
        fn_transaction_guard.stop_and_record();

        let fn_object_guard = self.metrics.fullnode_object_download_latency.start_timer();
        let object_changes = transactions
            .iter()
            .flat_map(|tx| get_object_changes(&tx.effects))
            .collect::<Vec<_>>();
        let changed_objects =
            fetch_changed_objects(self.http_client.clone(), object_changes).await?;
        fn_object_guard.stop_and_record();

        Ok(CheckpointData {
            checkpoint,
            transactions,
            changed_objects,
        })
    }

    fn index_packages(
        transactions: &[CheckpointTransactionBlockResponse],
        changed_objects: &[(ObjectStatus, SuiObjectData)],
    ) -> Result<Vec<Package>, IndexerError> {
        let object_map = changed_objects
            .iter()
            .filter_map(|(_, o)| {
                if let SuiRawData::Package(p) = &o
                    .bcs
                    .as_ref()
                    .expect("Expect the content field to be non-empty from data fetching")
                {
                    Some((o.object_id, p))
                } else {
                    None
                }
            })
            .collect::<BTreeMap<_, _>>();

        transactions
            .iter()
            .flat_map(|tx| {
                tx.effects.created().iter().map(|oref| {
                    object_map
                        .get(&oref.reference.object_id)
                        .map(|o| Package::try_from(*tx.transaction.data.sender(), o))
                })
            })
            .flatten()
            .collect()
    }
}

struct CheckpointProcessor<S>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    state: S,
    metrics: IndexerMetrics,
    epoch_sender: Arc<Mutex<Sender<TemporaryEpochStore>>>,
    checkpoint_sender: Arc<Mutex<Sender<TemporaryCheckpointStore>>>,
    downloaded_checkpoint_data_receiver: Receiver<CheckpointData>,
}

impl<S> CheckpointProcessor<S>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    async fn run(&mut self) -> Result<(), IndexerError> {
        loop {
            let checkpoint_data = self
                .downloaded_checkpoint_data_receiver
                .recv()
                .await
                .expect("Sender of Checkpoint Processor's rx should not be closed.");

            // Index checkpoint data
            let index_timer = self.metrics.checkpoint_index_latency.start_timer();

            let (checkpoint, epoch) =
                Self::index_checkpoint_and_epoch(&self.state, &checkpoint_data)
                    .await
                    .tap_err(|e| {
                        error!(
                            "Failed to index checkpoints {:?} with error: {}",
                            checkpoint_data,
                            e.to_string()
                        );
                    })?;
            index_timer.stop_and_record();

            // commit first epoch immediately, send other epochs to channel to be committed later.
            if let Some(epoch) = epoch {
                if epoch.last_epoch.is_none() {
                    let epoch_db_guard = self.metrics.epoch_db_commit_latency.start_timer();
                    info!("Persisting first epoch...");
                    let mut persist_first_epoch_res = self.state.persist_epoch(&epoch).await;
                    while persist_first_epoch_res.is_err() {
                        warn!("Failed to persist first epoch, retrying...");
                        persist_first_epoch_res = self.state.persist_epoch(&epoch).await;
                    }
                    epoch_db_guard.stop_and_record();
                    self.metrics.total_epoch_committed.inc();
                    info!("Persisted first epoch");
                } else {
                    let epoch_sender_guard = self.epoch_sender.lock().await;
                    // NOTE: when the channel is full, epoch_sender_guard will wait until the channel has space.
                    epoch_sender_guard.send(epoch).await.map_err(|e| {
                        error!(
                            "Failed to send indexed epoch to epoch commit handler with error {}",
                            e.to_string()
                        );
                        IndexerError::MpscChannelError(e.to_string())
                    })?;
                    drop(epoch_sender_guard);
                }
            }

            let checkpoint_sender_guard = self.checkpoint_sender.lock().await;
            // NOTE: when the channel is full, checkpoint_sender_guard will wait until the channel has space.
            // Checkpoints are sent sequentially to stick to the order of checkpoint sequence numbers.
            checkpoint_sender_guard
                .send(checkpoint)
                .await
                .unwrap_or_else(|e| {
                    panic!(
                        "checkpoint channel send should not fail, but got error: {:?}",
                        e
                    )
                });
            drop(checkpoint_sender_guard);
        }
    }

    async fn index_checkpoint_and_epoch(
        state: &S,
        data: &CheckpointData,
    ) -> Result<(TemporaryCheckpointStore, Option<TemporaryEpochStore>), IndexerError> {
        let CheckpointData {
            checkpoint,
            transactions,
            changed_objects,
        } = data;

        // Index transaction
        let temp_tx_store_iter = transactions
            .iter()
            .map(|tx| TemporaryTransactionBlockResponseStore::from(tx.clone()));
        let db_transactions: Vec<Transaction> = temp_tx_store_iter
            .map(|tx| tx.try_into())
            .collect::<Result<Vec<Transaction>, _>>()?;

        // Index events
        let events = transactions
            .iter()
            .flat_map(|tx| tx.events.data.iter().map(move |event| event.clone().into()))
            .collect::<Vec<_>>();

        // Index packages
        let packages = CheckpointHandler::<S>::index_packages(transactions, changed_objects)?;

        // Store input objects, move calls and recipients separately for transaction query indexing.
        let input_objects = transactions
            .iter()
            .map(|tx| tx.get_input_objects(checkpoint.epoch))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let changed_objects = transactions
            .iter()
            .flat_map(|tx| tx.get_changed_objects(checkpoint.epoch))
            .collect();
        let move_calls = transactions
            .iter()
            .flat_map(|tx| tx.get_move_calls(checkpoint.epoch))
            .collect();
        let recipients = transactions
            .iter()
            .flat_map(|tx| tx.get_recipients(checkpoint.epoch))
            .collect();

        // NOTE: Index epoch when object checkpoint index has reached the same checkpoint,
        // because epoch info is based on the latest system state object by the current checkpoint.
        let epoch_index = if checkpoint.epoch == 0 && checkpoint.sequence_number == 0 {
            // very first epoch
            let system_state = get_sui_system_state(data)?;
            let system_state: SuiSystemStateSummary = system_state.into_sui_system_state_summary();
            let validators = system_state
                .active_validators
                .iter()
                .map(|v| (system_state.epoch, v.clone()).into())
                .collect();

            Some(TemporaryEpochStore {
                last_epoch: None,
                new_epoch: DBEpochInfo {
                    epoch: 0,
                    first_checkpoint_id: 0,
                    epoch_start_timestamp: system_state.epoch_start_timestamp_ms as i64,
                    ..Default::default()
                },
                system_state: system_state.into(),
                validators,
            })
        } else if let Some(end_of_epoch_data) = &checkpoint.end_of_epoch_data {
            let system_state = get_sui_system_state(data)?;
            let system_state: SuiSystemStateSummary = system_state.into_sui_system_state_summary();
            let epoch_event = transactions.iter().find_map(|tx| {
                tx.events.data.iter().find(|ev| {
                    ev.type_.address == SUI_SYSTEM_ADDRESS
                        && ev.type_.module.as_ident_str() == ident_str!("sui_system_state_inner")
                        && ev.type_.name.as_ident_str() == ident_str!("SystemEpochInfoEvent")
                })
            });

            let event = epoch_event
                .map(|e| bcs::from_bytes::<SystemEpochInfoEvent>(&e.bcs))
                .transpose()?;

            let validators = system_state
                .active_validators
                .iter()
                .map(|v| (system_state.epoch, v.clone()).into())
                .collect();

            let epoch_commitments = end_of_epoch_data
                .epoch_commitments
                .iter()
                .map(|c| match c {
                    CheckpointCommitment::ECMHLiveObjectSetDigest(d) => {
                        Some(d.digest.into_inner().to_vec())
                    }
                })
                .collect();

            let (next_epoch_committee, next_epoch_committee_stake) =
                end_of_epoch_data.next_epoch_committee.iter().fold(
                    (vec![], vec![]),
                    |(mut names, mut stakes), (name, stake)| {
                        names.push(Some(name.as_bytes().to_vec()));
                        stakes.push(Some(*stake as i64));
                        (names, stakes)
                    },
                );

            let event = event.as_ref();

            let last_epoch = system_state.epoch as i64 - 1;
            let network_tx_count_prev_epoch = state
                .get_network_total_transactions_previous_epoch(last_epoch)
                .await?;
            Some(TemporaryEpochStore {
                last_epoch: Some(DBEpochInfo {
                    epoch: last_epoch,
                    first_checkpoint_id: 0,
                    last_checkpoint_id: Some(checkpoint.sequence_number as i64),
                    epoch_start_timestamp: 0,
                    epoch_end_timestamp: Some(checkpoint.timestamp_ms as i64),
                    epoch_total_transactions: checkpoint.network_total_transactions as i64
                        - network_tx_count_prev_epoch,
                    next_epoch_version: Some(
                        end_of_epoch_data.next_epoch_protocol_version.as_u64() as i64,
                    ),
                    next_epoch_committee,
                    next_epoch_committee_stake,
                    stake_subsidy_amount: event.map(|e| e.stake_subsidy_amount),
                    reference_gas_price: event.map(|e| e.reference_gas_price),
                    storage_fund_balance: event.map(|e| e.storage_fund_balance),
                    total_gas_fees: event.map(|e| e.total_gas_fees),
                    total_stake_rewards_distributed: event
                        .map(|e| e.total_stake_rewards_distributed),
                    total_stake: event.map(|e| e.total_stake),
                    storage_fund_reinvestment: event.map(|e| e.storage_fund_reinvestment),
                    storage_charge: event.map(|e| e.storage_charge),
                    protocol_version: event.map(|e| e.protocol_version),
                    storage_rebate: event.map(|e| e.storage_rebate),
                    leftover_storage_fund_inflow: event.map(|e| e.leftover_storage_fund_inflow),
                    epoch_commitments,
                }),
                new_epoch: DBEpochInfo {
                    epoch: system_state.epoch as i64,
                    first_checkpoint_id: checkpoint.sequence_number as i64 + 1,
                    epoch_start_timestamp: system_state.epoch_start_timestamp_ms as i64,
                    ..Default::default()
                },
                system_state: system_state.into(),
                validators,
            })
        } else {
            None
        };
        let total_transactions = db_transactions.iter().map(|t| t.transaction_count).sum();
        let total_successful_transaction_blocks = db_transactions
            .iter()
            .filter(|t| t.execution_success)
            .count();
        let total_successful_transactions = db_transactions
            .iter()
            .filter(|t| t.execution_success)
            .map(|t| t.transaction_count)
            .sum();

        Ok((
            TemporaryCheckpointStore {
                checkpoint: Checkpoint::from(
                    checkpoint,
                    total_transactions,
                    total_successful_transactions,
                    total_successful_transaction_blocks as i64,
                )?,
                transactions: db_transactions,
                events,
                packages,
                input_objects,
                changed_objects,
                move_calls,
                recipients,
            },
            epoch_index,
        ))
    }
}

#[allow(clippy::type_complexity)]
struct CheckpointObjectsProcessor {
    metrics: IndexerMetrics,
    object_changes_sender:
        Arc<Mutex<Sender<(CheckpointSequenceNumber, Vec<TransactionObjectChanges>)>>>,
    downloaded_object_data_receiver: Receiver<CheckpointData>,
}

impl CheckpointObjectsProcessor {
    async fn run(&mut self) -> Result<(), IndexerError> {
        loop {
            let checkpoint_data = self
                .downloaded_object_data_receiver
                .recv()
                .await
                .expect("Sender of Checkpoint Processor's rx should not be closed.");
            let checkpoint_seq = checkpoint_data.checkpoint.sequence_number;

            // Index checkpoint data
            let index_timer = self.metrics.checkpoint_index_latency.start_timer();

            let object_changes = Self::index_checkpoint_objects(&checkpoint_data)
                .await
                .tap_err(|e| {
                    error!(
                        "Failed to index checkpoints {:?} with error: {}",
                        checkpoint_data,
                        e.to_string()
                    );
                })?;
            index_timer.stop_and_record();

            self.object_changes_sender
                .lock()
                .await
                .send((checkpoint_seq, object_changes))
                .await
                .unwrap_or_else(|e| {
                    panic!(
                        "checkpoint channel send should not fail, but got error: {:?}",
                        e
                    )
                });
        }
    }

    async fn index_checkpoint_objects(
        data: &CheckpointData,
    ) -> Result<Vec<TransactionObjectChanges>, IndexerError> {
        let CheckpointData {
            checkpoint,
            transactions,
            changed_objects,
        } = data;

        // Index objects
        let tx_objects = changed_objects
            .iter()
            // Unwrap safe here as we requested previous tx data in the request.
            .fold(BTreeMap::<_, Vec<_>>::new(), |mut acc, (status, o)| {
                if let Some(digest) = &o.previous_transaction {
                    acc.entry(*digest).or_default().push((status, o));
                }
                acc
            });

        let objects_changes = transactions
            .iter()
            .map(|tx| {
                let changed_db_objects = tx_objects
                    .get(&tx.digest)
                    .unwrap_or(&vec![])
                    .iter()
                    .map(|(status, o)| {
                        Object::from(
                            checkpoint.epoch,
                            Some(checkpoint.sequence_number),
                            status,
                            o,
                        )
                    })
                    .collect::<Vec<_>>();
                let deleted_objects = get_deleted_db_objects(
                    &tx.effects,
                    checkpoint.epoch,
                    Some(checkpoint.sequence_number),
                );

                TransactionObjectChanges {
                    changed_objects: changed_db_objects,
                    deleted_objects,
                }
            })
            .collect();
        Ok(objects_changes)
    }
}

// TODO(gegaowp): re-organize object util functions below
pub fn get_object_changes(
    effects: &SuiTransactionBlockEffects,
) -> Vec<(ObjectID, SequenceNumber, ObjectStatus)> {
    let created = effects.created().iter().map(|o: &OwnedObjectRef| {
        (
            o.reference.object_id,
            o.reference.version,
            ObjectStatus::Created,
        )
    });
    let mutated = effects.mutated().iter().map(|o: &OwnedObjectRef| {
        (
            o.reference.object_id,
            o.reference.version,
            ObjectStatus::Mutated,
        )
    });
    let unwrapped = effects.unwrapped().iter().map(|o: &OwnedObjectRef| {
        (
            o.reference.object_id,
            o.reference.version,
            ObjectStatus::Unwrapped,
        )
    });
    created.chain(mutated).chain(unwrapped).collect()
}

pub async fn fetch_changed_objects(
    http_client: HttpClient,
    object_changes: Vec<(ObjectID, SequenceNumber, ObjectStatus)>,
) -> Result<Vec<(ObjectStatus, SuiObjectData)>, IndexerError> {
    join_all(object_changes.chunks(MULTI_GET_CHUNK_SIZE).map(|objects| {
        let wanted_past_object_statuses: Vec<ObjectStatus> =
            objects.iter().map(|(_, _, status)| *status).collect();

        let wanted_past_object_request = objects
            .iter()
            .map(|(id, seq_num, _)| SuiGetPastObjectRequest {
                object_id: *id,
                version: *seq_num,
            })
            .collect();
        http_client
            .try_multi_get_past_objects(
                wanted_past_object_request,
                Some(SuiObjectDataOptions::bcs_lossless()),
            )
            .map(move |resp| (resp, wanted_past_object_statuses))
    }))
    .await
    .into_iter()
    .try_fold(vec![], |mut acc, chunk| {
        let object_data = chunk.0?.into_iter().try_fold(vec![], |mut acc, resp| {
            let object_data = resp.into_object()?;
            acc.push(object_data);
            Ok::<Vec<SuiObjectData>, Error>(acc)
        })?;
        let mutated_object_chunk = chunk.1.into_iter().zip(object_data);
        acc.extend(mutated_object_chunk);
        Ok::<_, Error>(acc)
    })
    .map_err(|e| {
        IndexerError::SerdeError(format!(
            "Failed to generate changed objects of checkpoint with err {:?}",
            e
        ))
    })
}

pub fn get_deleted_db_objects(
    effects: &SuiTransactionBlockEffects,
    epoch: EpochId,
    checkpoint: Option<CheckpointSequenceNumber>,
) -> Vec<DeletedObject> {
    let deleted = effects.deleted().iter();
    let deleted = deleted.map(|o| (ObjectStatus::Deleted, o));
    let wrapped = effects.wrapped().iter();
    let wrapped = wrapped.map(|o| (ObjectStatus::Wrapped, o));
    let unwrapped_then_deleted = effects.unwrapped_then_deleted().iter();
    let unwrapped_then_deleted =
        unwrapped_then_deleted.map(|o| (ObjectStatus::UnwrappedThenDeleted, o));
    deleted
        .chain(wrapped)
        .chain(unwrapped_then_deleted)
        .map(|(status, oref)| {
            DeletedObject::from(
                epoch,
                checkpoint.map(<u64>::from),
                oref,
                effects.transaction_digest(),
                &status,
            )
        })
        .collect::<Vec<_>>()
}
