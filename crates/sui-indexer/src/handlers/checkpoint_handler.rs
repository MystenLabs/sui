// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::metrics::IndexerCheckpointHandlerMetrics;
use crate::models::checkpoints::Checkpoint;
use crate::models::move_calls::MoveCall;
use crate::models::objects::{DeletedObject, Object, ObjectStatus};
use crate::models::packages::Package;
use crate::models::recipients::Recipient;
use crate::models::transactions::Transaction;
use crate::multi_get_full_transactions;
use crate::store::{
    CheckpointData, IndexerStore, TemporaryCheckpointStore, TemporaryEpochStore,
    TransactionObjectChanges,
};
use crate::types::SuiTransactionFullResponse;
use futures::future::join_all;
use futures::FutureExt;
use mysten_metrics::spawn_monitored_task;
use prometheus::Registry;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_core::event_handler::EventHandler;
use sui_json_rpc_types::{
    OwnedObjectRef, SuiCommand, SuiGetPastObjectRequest, SuiObjectData, SuiObjectDataOptions,
    SuiRawData, SuiTransactionDataAPI, SuiTransactionEffectsAPI, SuiTransactionKind,
};
use sui_sdk::error::Error;
use sui_sdk::SuiClient;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Owner;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

const HANDLER_RETRY_INTERVAL_IN_SECS: u64 = 10;
const MULTI_GET_CHUNK_SIZE: usize = 500;

pub struct CheckpointHandler<S> {
    state: S,
    rpc_client: SuiClient,
    event_handler: Arc<EventHandler>,
    metrics: IndexerCheckpointHandlerMetrics,
}

impl<S> CheckpointHandler<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    pub fn new(
        state: S,
        rpc_client: SuiClient,
        event_handler: Arc<EventHandler>,
        prometheus_registry: &Registry,
    ) -> Self {
        Self {
            state,
            rpc_client,
            event_handler,
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

            let checkpoint = self
                .download_checkpoint_data(next_cursor_sequence_number as u64)
                .await.map_err(|e| {
                    error!(
                        "Failed to download checkpoint data with checkpoint sequence number {} and error {:?}, retrying...",
                        next_cursor_sequence_number, e
                    );
                    e
                })?;
            request_guard.stop_and_record();
            self.metrics.total_checkpoint_received.inc();

            // Index checkpoint data
            // TODO: Metrics
            let (indexed_checkpoint, indexed_epoch) = self.index_checkpoint(&checkpoint)?;

            // Write to DB
            let db_guard = self.metrics.db_write_request_latency.start_timer();
            let tx_count = indexed_checkpoint.transactions.len();
            let object_count = indexed_checkpoint.objects_changes.len();

            self.state.persist_checkpoint(&indexed_checkpoint)?;
            info!(
                "Checkpoint {} committed with {tx_count} transactions and {object_count} objects.",
                next_cursor_sequence_number
            );
            self.metrics.total_checkpoint_processed.inc();
            db_guard.stop_and_record();

            // Process websocket subscription
            let db_guard = self.metrics.db_write_request_latency.start_timer();
            for tx in &checkpoint.transactions {
                self.event_handler
                    .process_events(&tx.effects, &tx.events)
                    .await?;
            }
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
    ) -> Result<CheckpointData, IndexerError> {
        let mut checkpoint = self
            .rpc_client
            .read_api()
            .get_checkpoint(seq.into())
            .await
            .map_err(|e| {
                IndexerError::FullNodeReadingError(format!(
                    "Failed to get checkpoint with sequence number {} and error {:?}",
                    seq, e
                ))
            });
        // this happens very often b/c checkpoint indexing is faster than checkpoint
        // generation. Ideally we will want to differentiate between a real error and
        // a checkpoint not generated yet.
        while checkpoint.is_err() {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            checkpoint = self
                .rpc_client
                .read_api()
                .get_checkpoint(seq.into())
                .await
                .map_err(|e| {
                    IndexerError::FullNodeReadingError(format!(
                        "Failed to get checkpoint with sequence number {} and error {:?}",
                        seq, e
                    ))
                })
        }
        // unwrap here is safe because we checked for error above
        let checkpoint = checkpoint.unwrap();

        let transactions = join_all(checkpoint.transactions.chunks(MULTI_GET_CHUNK_SIZE).map(
            |digests| multi_get_full_transactions(self.rpc_client.read_api(), digests.to_vec()),
        ))
        .await
        .into_iter()
        .try_fold(vec![], |mut acc, chunk| {
            acc.extend(chunk?);
            Ok::<_, IndexerError>(acc)
        })?;

        let object_changes = transactions
            .iter()
            .flat_map(|tx| {
                let effects = &tx.effects;
                let created = effects.created().iter();
                let created = created.map(|o: &OwnedObjectRef| (o, ObjectStatus::Created));
                let mutated = effects.mutated().iter();
                let mutated = mutated.map(|o: &OwnedObjectRef| (o, ObjectStatus::Mutated));
                let unwrapped = effects.unwrapped().iter();
                let unwrapped = unwrapped.map(|o: &OwnedObjectRef| (o, ObjectStatus::Unwrapped));
                created.chain(mutated).chain(unwrapped)
            })
            .fold(
                vec![],
                |mut acc, (o, status): (&OwnedObjectRef, ObjectStatus)| {
                    acc.push((o.reference.object_id, o.reference.version, status));
                    acc
                },
            );

        let rpc = self.rpc_client.clone();
        let changed_objects =
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

                rpc.read_api()
                    .try_multi_get_parsed_past_object(
                        wanted_past_object_request,
                        SuiObjectDataOptions::bcs_lossless(),
                    )
                    .map(move |resp| (resp, wanted_past_object_statuses))
            }))
            .await
            .into_iter()
            .try_fold(vec![], |mut acc, chunk| {
                let object_datas = chunk.0?.into_iter().try_fold(vec![], |mut acc, resp| {
                    let object_data = resp.into_object()?;
                    acc.push(object_data);
                    Ok::<Vec<SuiObjectData>, Error>(acc)
                })?;
                let mutated_object_chunk = chunk.1.into_iter().zip(object_datas);
                acc.extend(mutated_object_chunk);
                Ok::<_, Error>(acc)
            })
            .map_err(|e| {
                IndexerError::SerdeError(format!(
                    "Failed to generate changed objects of checkpoint sequence {} with err {:?}",
                    seq, e
                ))
            })?;

        Ok(CheckpointData {
            checkpoint,
            transactions,
            changed_objects,
        })
    }

    fn index_checkpoint(
        &self,
        data: &CheckpointData,
    ) -> Result<(TemporaryCheckpointStore, Option<TemporaryEpochStore>), IndexerError> {
        let CheckpointData {
            checkpoint,
            transactions,
            changed_objects,
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
            .flat_map(|tx| tx.events.data.iter().map(move |event| event.clone().into()))
            .collect::<Vec<_>>();

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
                let changed_objects = tx_objects
                    .get(&tx.digest)
                    .unwrap_or(&vec![])
                    .iter()
                    .map(|(status, o)| {
                        Object::from(&checkpoint.epoch, &checkpoint.sequence_number, status, o)
                    })
                    .collect::<Vec<_>>();
                let deleted = tx.effects.deleted().iter();
                let deleted = deleted.map(|o| (ObjectStatus::Deleted, o));
                let wrapped = tx.effects.wrapped().iter();
                let wrapped = wrapped.map(|o| (ObjectStatus::Wrapped, o));
                let unwrapped_then_deleted = tx.effects.unwrapped_then_deleted().iter();
                let unwrapped_then_deleted =
                    unwrapped_then_deleted.map(|o| (ObjectStatus::UnwrappedThenDeleted, o));
                let all_deleted_objects = deleted
                    .chain(wrapped)
                    .chain(unwrapped_then_deleted)
                    .map(|(status, oref)| {
                        DeletedObject::from(
                            &checkpoint.epoch,
                            &checkpoint.sequence_number,
                            oref,
                            &tx.digest,
                            status,
                        )
                    })
                    .collect();
                TransactionObjectChanges {
                    mutated_objects: changed_objects,
                    deleted_objects: all_deleted_objects,
                }
            })
            .collect();

        // Index addresses
        let addresses = db_transactions
            .iter()
            .map(|tx: &Transaction| tx.into())
            .collect();

        // Index packages
        let packages = Self::index_packages(transactions, changed_objects)?;

        let move_calls: Vec<MoveCall> = transactions
            .iter()
            .map(|t| {
                let tx = t.transaction.data.transaction();
                (
                    tx.clone(),
                    t.digest,
                    checkpoint.sequence_number,
                    checkpoint.epoch,
                    t.transaction.data.sender(),
                )
            })
            .filter_map(
                |(tx_kind, txn_digest, checkpoint_seq, epoch, sender)| match tx_kind {
                    SuiTransactionKind::ProgrammableTransaction(pt) => Some(
                        pt.commands
                            .into_iter()
                            .filter_map(move |command| match command {
                                SuiCommand::MoveCall(m) => Some(MoveCall {
                                    id: None,
                                    transaction_digest: txn_digest.to_string(),
                                    checkpoint_sequence_number: checkpoint_seq as i64,
                                    epoch: epoch as i64,
                                    sender: sender.to_string(),
                                    move_package: m.package.to_string(),
                                    move_module: m.module,
                                    move_function: m.function,
                                }),
                                _ => None,
                            }),
                    ),

                    _ => None,
                },
            )
            .flatten()
            .collect();

        let recipients: Vec<Recipient> = transactions
            .iter()
            .flat_map(|tx| {
                let created = tx.effects.created().iter();
                let mutated = tx.effects.mutated().iter();
                let unwrapped = tx.effects.unwrapped().iter();
                created
                    .chain(mutated)
                    .chain(unwrapped)
                    .filter_map(|obj_ref| match obj_ref.owner {
                        Owner::AddressOwner(address) => Some(Recipient {
                            id: None,
                            transaction_digest: tx.effects.transaction_digest().to_string(),
                            checkpoint_sequence_number: checkpoint.sequence_number as i64,
                            epoch: checkpoint.epoch as i64,
                            recipient: address.to_string(),
                        }),
                        _ => None,
                    })
            })
            .collect();

        // Index epoch
        // TODO: Aggregate all object owner changes into owner index at epoch change.
        let epoch_index =
            checkpoint
                .end_of_epoch_data
                .as_ref()
                .map(|_epoch_change| TemporaryEpochStore {
                    owner_index: vec![],
                    epoch_id: checkpoint.epoch,
                });

        Ok((
            TemporaryCheckpointStore {
                checkpoint: Checkpoint::from(checkpoint, &previous_cp)?,
                transactions: db_transactions,
                events,
                objects_changes,
                addresses,
                packages,
                move_calls,
                recipients,
            },
            epoch_index,
        ))
    }

    fn index_packages(
        transactions: &[SuiTransactionFullResponse],
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
