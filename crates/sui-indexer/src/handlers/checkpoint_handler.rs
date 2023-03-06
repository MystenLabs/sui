// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use chrono::NaiveDateTime;
use futures::future::join_all;
use mysten_metrics::spawn_monitored_task;
use prometheus::Registry;
use std::collections::BTreeMap;
use sui_json_rpc_types::{
    SuiObjectData, SuiObjectDataOptions, SuiParsedData, SuiTransactionDataAPI,
    SuiTransactionEffectsAPI, SuiTransactionKind, SuiTransactionResponse,
};
use sui_sdk::SuiClient;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::errors::IndexerError;
use crate::metrics::IndexerCheckpointHandlerMetrics;
use crate::models::checkpoints::Checkpoint;
use crate::models::events::Event;
use crate::models::move_calls::MoveCall;
use crate::models::packages::Package;
use crate::models::transactions::Transaction;
use crate::store::{CheckpointData, IndexerStore, TemporaryCheckpointStore, TemporaryEpochStore};
use sui_sdk::error::Error;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

const HANDLER_RETRY_INTERVAL_IN_SECS: u64 = 10;

pub struct CheckpointHandler<S> {
    state: S,
    rpc_client: SuiClient,
    metrics: IndexerCheckpointHandlerMetrics,
}

impl<S> CheckpointHandler<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    pub fn new(state: S, rpc_client: SuiClient, prometheus_registry: &Registry) -> Self {
        Self {
            state,
            rpc_client,
            metrics: IndexerCheckpointHandlerMetrics::new(prometheus_registry),
        }
    }

    pub fn spawn(self) -> JoinHandle<()> {
        spawn_monitored_task!(async move {
            let mut checkpoint_handler_exec_res = self.start().await;
            while let Err(e) = &checkpoint_handler_exec_res {
                warn!(
                    "Indexer checkpoint handler failed with error: {:?}, retrying after {:?} secs...",
                    e, HANDLER_RETRY_INTERVAL_IN_SECS
                );
                tokio::time::sleep(std::time::Duration::from_secs(
                    HANDLER_RETRY_INTERVAL_IN_SECS,
                ))
                .await;
                checkpoint_handler_exec_res = self.start().await;
            }
        })
    }

    async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer checkpoint handler started...");
        let mut next_cursor_sequence_number = self.state.get_latest_checkpoint_sequence_number()?;
        if next_cursor_sequence_number > 0 {
            info!("Resuming from checkpoint {next_cursor_sequence_number}");
        }
        next_cursor_sequence_number += 1;

        loop {
            self.metrics.total_checkpoint_requested.inc();
            let request_guard = self.metrics.full_node_read_request_latency.start_timer();

            let mut checkpoint = self
                .download_checkpoint_data(next_cursor_sequence_number as u64)
                .await;
            // this happens very often b/c checkpoint indexing is faster than checkpoint
            // generation. Ideally we will want to differentiate between a real error and
            // a checkpoint not generated yet.
            while checkpoint.is_err() {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                checkpoint = self
                    .download_checkpoint_data(next_cursor_sequence_number as u64)
                    .await
            }
            // unwrap here is safe because we checked for error above
            let checkpoint = checkpoint.unwrap();
            request_guard.stop_and_record();
            self.metrics.total_checkpoint_received.inc();

            // Index checkpoint data
            // TODO: Metrics
            let (indexed_checkpoint, indexed_epoch) = self.index_checkpoint(checkpoint)?;

            // Write to DB
            let db_guard = self.metrics.db_write_request_latency.start_timer();
            let tx_count = indexed_checkpoint.transactions.len();
            let object_count = indexed_checkpoint.objects.len();

            self.state.persist_checkpoint(&indexed_checkpoint)?;
            info!(
                "Checkpoint {} committed with {tx_count} transactions and {object_count} objects.",
                next_cursor_sequence_number
            );
            self.metrics.total_checkpoint_processed.inc();
            db_guard.stop_and_record();

            if let Some(indexed_epoch) = indexed_epoch {
                self.state.persist_epoch(&indexed_epoch)?;
            }
            next_cursor_sequence_number += 1;
        }
    }

    /// Download all the data we need for one checkpoint.
    async fn download_checkpoint_data(
        &self,
        seq: CheckpointSequenceNumber,
    ) -> Result<CheckpointData, Error> {
        let checkpoint = self
            .rpc_client
            .read_api()
            .get_checkpoint(seq.into())
            .await?;

        let transactions = self
            .rpc_client
            .read_api()
            .multi_get_transactions(checkpoint.transactions.to_vec())
            .await?;

        let all_mutated = transactions
            .iter()
            .flat_map(|tx| {
                tx.effects
                    .created()
                    .iter()
                    .cloned()
                    .chain(tx.effects.mutated().iter().cloned())
                    .chain(tx.effects.unwrapped().iter().cloned())
            })
            .map(|o| (o.reference.object_id, o.reference.version));

        // TODO: Use multi get objects
        // TODO: Error handling.
        let new_objects = join_all(all_mutated.map(|(id, version)| {
            self.rpc_client.read_api().try_get_parsed_past_object(
                id,
                version,
                SuiObjectDataOptions::full_content(),
            )
        }))
        .await
        .into_iter()
        .flatten()
        .flat_map(|o| o.into_object())
        .collect();

        Ok(CheckpointData {
            checkpoint,
            transactions,
            objects: new_objects,
        })
    }

    fn index_checkpoint(
        &self,
        data: CheckpointData,
    ) -> Result<(TemporaryCheckpointStore, Option<TemporaryEpochStore>), IndexerError> {
        let CheckpointData {
            checkpoint,
            transactions,
            objects,
        } = data;

        let previous_cp = if checkpoint.sequence_number == 0 {
            Checkpoint::default()
        } else {
            self.state
                .get_checkpoint((checkpoint.sequence_number - 1).into())?
        };

        // Index transaction
        let db_transactions = transactions
            .iter()
            .map(|tx| tx.clone().try_into())
            .collect::<Result<Vec<_>, _>>()?;

        // Index events
        let events = transactions
            .iter()
            .flat_map(|tx| {
                let mut event_sequence = 0;
                tx.events.data.iter().map(move |event| {
                    // TODO: we should rethink how we store the raw event in DB
                    let event_content = serde_json::to_string(event).map_err(|err| {
                        IndexerError::InsertableParsingError(format!(
                            "Failed converting event to JSON with error: {:?}",
                            err
                        ))
                    })?;
                    let event = Event {
                        id: None,
                        transaction_digest: tx.effects.transaction_digest().to_string(),
                        event_sequence,
                        event_time: tx
                            .timestamp_ms
                            .and_then(|t| NaiveDateTime::from_timestamp_millis(t as i64)),
                        event_type: event.get_event_type(),
                        event_content,
                    };
                    event_sequence += 1;
                    Ok::<_, IndexerError>(event)
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Index objects
        let db_objects = objects.iter().map(|o| o.clone().into()).collect::<Vec<_>>();

        // Index addresses
        let addresses = db_transactions
            .iter()
            .map(|tx: &Transaction| tx.into())
            .collect();

        // Index packages
        let packages = Self::index_packages(&transactions, &objects)?;

        let move_calls: Vec<MoveCall> = transactions
            .iter()
            .flat_map(|t| {
                t.transaction.data.transactions().iter().map(move |tx| {
                    (
                        tx.clone(),
                        t.effects.transaction_digest(),
                        checkpoint.sequence_number,
                        checkpoint.epoch,
                        t.transaction.data.sender(),
                    )
                })
            })
            .filter_map(
                |(tx_kind, txn_digest, checkpoint_seq, epoch, sender)| match tx_kind {
                    SuiTransactionKind::Call(sui_move_call) => Some(MoveCall {
                        id: None,
                        transaction_digest: txn_digest.to_string(),
                        checkpoint_sequence_number: checkpoint_seq as i64,
                        epoch: epoch as i64,
                        sender: sender.to_string(),
                        move_package: sui_move_call.package.to_string(),
                        move_module: sui_move_call.module,
                        move_function: sui_move_call.function,
                    }),
                    _ => None,
                },
            )
            .collect();

        // Index epoch
        // TODO: Aggregate all object owner changes into owner index at epoch change.
        let epoch_index =
            checkpoint
                .end_of_epoch_data
                .as_ref()
                .map(|_epoch_change| TemporaryEpochStore {
                    owner_index: vec![],
                });

        Ok((
            TemporaryCheckpointStore {
                checkpoint: Checkpoint::from(&checkpoint, &previous_cp)?,
                transactions: db_transactions,
                events,
                objects: db_objects,
                owner_changes: vec![],
                addresses,
                packages,
                move_calls,
            },
            epoch_index,
        ))
    }

    fn index_packages(
        transactions: &[SuiTransactionResponse],
        objects: &[SuiObjectData],
    ) -> Result<Vec<Package>, IndexerError> {
        let object_map = objects
            .iter()
            .filter_map(|o| {
                if let SuiParsedData::Package(p) = &o
                    .content
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
                    object_map.get(&oref.reference.object_id).map(|o| {
                        Package::try_from(
                            oref.reference.object_id,
                            *tx.transaction.data.sender(),
                            o,
                        )
                    })
                })
            })
            .flatten()
            .collect()
    }
}
