// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use diesel::dsl::now;
use diesel::{Connection, OptionalExtension, QueryDsl, RunQueryDsl, SelectableHelper};
use diesel::{ExpressionMethods, TextExpressionMethods};
use tracing::info;

use sui_bridge::events::{
    MoveTokenDepositedEvent, MoveTokenTransferApproved, MoveTokenTransferClaimed,
};
use sui_indexer_builder::indexer_builder::{DataMapper, IndexerProgressStore, Persistent};
use sui_indexer_builder::sui_datasource::CheckpointTxnData;
use sui_indexer_builder::Task;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::event::Event;
use sui_types::execution_status::ExecutionStatus;
use sui_types::full_checkpoint_content::CheckpointTransaction;
use sui_types::{BRIDGE_ADDRESS, SUI_BRIDGE_OBJECT_ID};

use crate::metrics::BridgeIndexerMetrics;
use crate::postgres_manager::PgPool;
use crate::schema::progress_store::{columns, dsl};
use crate::schema::{sui_error_transactions, token_transfer, token_transfer_data};
use crate::{
    models, schema, BridgeDataSource, ProcessedTxnData, SuiTxnError, TokenTransfer,
    TokenTransferData, TokenTransferStatus,
};

/// Persistent layer impl
#[derive(Clone)]
pub struct PgBridgePersistent {
    pool: PgPool,
}

impl PgBridgePersistent {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// TODO: this is shared between SUI and ETH, move to different file.
#[async_trait]
impl Persistent<ProcessedTxnData> for PgBridgePersistent {
    async fn write(&self, data: Vec<ProcessedTxnData>) -> Result<(), Error> {
        if data.is_empty() {
            return Ok(());
        }
        let connection = &mut self.pool.get()?;
        connection.transaction(|conn| {
            for d in data {
                match d {
                    ProcessedTxnData::TokenTransfer(t) => {
                        diesel::insert_into(token_transfer::table)
                            .values(&t.to_db())
                            .on_conflict_do_nothing()
                            .execute(conn)?;

                        if let Some(d) = t.to_data_maybe() {
                            diesel::insert_into(token_transfer_data::table)
                                .values(&d)
                                .on_conflict_do_nothing()
                                .execute(conn)?;
                        }
                    }
                    ProcessedTxnData::Error(e) => {
                        diesel::insert_into(sui_error_transactions::table)
                            .values(&e.to_db())
                            .on_conflict_do_nothing()
                            .execute(conn)?;
                    }
                }
            }
            Ok(())
        })
    }
}

#[async_trait]
impl IndexerProgressStore for PgBridgePersistent {
    async fn load_progress(&self, task_name: String) -> anyhow::Result<u64> {
        let mut conn = self.pool.get()?;
        let cp: Option<models::ProgressStore> = dsl::progress_store
            .find(&task_name)
            .select(models::ProgressStore::as_select())
            .first(&mut conn)
            .optional()?;
        Ok(cp
            .ok_or(anyhow!("Cannot found progress for task {task_name}"))?
            .checkpoint as u64)
    }

    async fn save_progress(
        &mut self,
        task_name: String,
        checkpoint_number: u64,
    ) -> anyhow::Result<()> {
        let mut conn = self.pool.get()?;
        diesel::insert_into(schema::progress_store::table)
            .values(&models::ProgressStore {
                task_name,
                checkpoint: checkpoint_number as i64,
                // Target checkpoint and timestamp will only be written for new entries
                target_checkpoint: i64::MAX,
                // Timestamp is defaulted to current time in DB if None
                timestamp: None,
            })
            .on_conflict(dsl::task_name)
            .do_update()
            .set((
                columns::checkpoint.eq(checkpoint_number as i64),
                columns::timestamp.eq(now),
            ))
            .execute(&mut conn)?;
        Ok(())
    }

    async fn tasks(&self, prefix: &str) -> Result<Vec<Task>, anyhow::Error> {
        let mut conn = self.pool.get()?;
        // get all unfinished tasks
        let cp: Vec<models::ProgressStore> = dsl::progress_store
            // TODO: using like could be error prone, change the progress store schema to stare the task name properly.
            .filter(columns::task_name.like(format!("{prefix} - %")))
            .filter(columns::checkpoint.lt(columns::target_checkpoint))
            .order_by(columns::target_checkpoint.desc())
            .load(&mut conn)?;
        Ok(cp.into_iter().map(|d| d.into()).collect())
    }

    async fn register_task(
        &mut self,
        task_name: String,
        checkpoint: u64,
        target_checkpoint: u64,
    ) -> Result<(), anyhow::Error> {
        let mut conn = self.pool.get()?;
        diesel::insert_into(schema::progress_store::table)
            .values(models::ProgressStore {
                task_name,
                checkpoint: checkpoint as i64,
                target_checkpoint: target_checkpoint as i64,
                // Timestamp is defaulted to current time in DB if None
                timestamp: None,
            })
            .execute(&mut conn)?;
        Ok(())
    }

    async fn update_task(&mut self, task: Task) -> Result<(), anyhow::Error> {
        let mut conn = self.pool.get()?;
        diesel::update(dsl::progress_store.filter(columns::task_name.eq(task.task_name)))
            .set((
                columns::checkpoint.eq(task.checkpoint as i64),
                columns::target_checkpoint.eq(task.target_checkpoint as i64),
                columns::timestamp.eq(now),
            ))
            .execute(&mut conn)?;
        Ok(())
    }
}

/// Data mapper impl
#[derive(Clone)]
pub struct SuiBridgeDataMapper {
    pub metrics: BridgeIndexerMetrics,
}

impl DataMapper<CheckpointTxnData, ProcessedTxnData> for SuiBridgeDataMapper {
    fn map(
        &self,
        (data, checkpoint_num, timestamp_ms): CheckpointTxnData,
    ) -> Result<Vec<ProcessedTxnData>, Error> {
        self.metrics.total_sui_bridge_transactions.inc();
        if !data
            .input_objects
            .iter()
            .any(|obj| obj.id() == SUI_BRIDGE_OBJECT_ID)
        {
            return Ok(vec![]);
        }

        match &data.events {
            Some(events) => {
                let token_transfers = events.data.iter().try_fold(vec![], |mut result, ev| {
                    if let Some(data) = process_sui_event(ev, &data, checkpoint_num, timestamp_ms)?
                    {
                        result.push(data);
                    }
                    Ok::<_, anyhow::Error>(result)
                })?;

                if !token_transfers.is_empty() {
                    info!(
                        "SUI: Extracted {} bridge token transfer data entries for tx {}.",
                        token_transfers.len(),
                        data.transaction.digest()
                    );
                }
                Ok(token_transfers)
            }
            None => {
                if let ExecutionStatus::Failure { error, command } = data.effects.status() {
                    Ok(vec![ProcessedTxnData::Error(SuiTxnError {
                        tx_digest: *data.transaction.digest(),
                        sender: data.transaction.sender_address(),
                        timestamp_ms,
                        failure_status: error.to_string(),
                        cmd_idx: command.map(|idx| idx as u64),
                    })])
                } else {
                    Ok(vec![])
                }
            }
        }
    }
}

fn process_sui_event(
    ev: &Event,
    tx: &CheckpointTransaction,
    checkpoint: u64,
    timestamp_ms: u64,
) -> Result<Option<ProcessedTxnData>, anyhow::Error> {
    Ok(if ev.type_.address == BRIDGE_ADDRESS {
        match ev.type_.name.as_str() {
            "TokenDepositedEvent" => {
                info!("Observed Sui Deposit {:?}", ev);
                // todo: metrics.total_sui_token_deposited.inc();
                let move_event: MoveTokenDepositedEvent = bcs::from_bytes(&ev.contents)?;
                Some(ProcessedTxnData::TokenTransfer(TokenTransfer {
                    chain_id: move_event.source_chain,
                    nonce: move_event.seq_num,
                    block_height: checkpoint,
                    timestamp_ms,
                    txn_hash: tx.transaction.digest().inner().to_vec(),
                    txn_sender: ev.sender.to_vec(),
                    status: TokenTransferStatus::Deposited,
                    gas_usage: tx.effects.gas_cost_summary().net_gas_usage(),
                    data_source: BridgeDataSource::Sui,
                    data: Some(TokenTransferData {
                        destination_chain: move_event.target_chain,
                        sender_address: move_event.sender_address.clone(),
                        recipient_address: move_event.target_address.clone(),
                        token_id: move_event.token_type,
                        amount: move_event.amount_sui_adjusted,
                    }),
                }))
            }
            "TokenTransferApproved" => {
                info!("Observed Sui Approval {:?}", ev);
                // todo: metrics.total_sui_token_transfer_approved.inc();
                let event: MoveTokenTransferApproved = bcs::from_bytes(&ev.contents)?;
                Some(ProcessedTxnData::TokenTransfer(TokenTransfer {
                    chain_id: event.message_key.source_chain,
                    nonce: event.message_key.bridge_seq_num,
                    block_height: checkpoint,
                    timestamp_ms,
                    txn_hash: tx.transaction.digest().inner().to_vec(),
                    txn_sender: ev.sender.to_vec(),
                    status: TokenTransferStatus::Approved,
                    gas_usage: tx.effects.gas_cost_summary().net_gas_usage(),
                    data_source: BridgeDataSource::Sui,
                    data: None,
                }))
            }
            "TokenTransferClaimed" => {
                info!("Observed Sui Claim {:?}", ev);
                // todo: metrics.total_sui_token_transfer_claimed.inc();
                let event: MoveTokenTransferClaimed = bcs::from_bytes(&ev.contents)?;
                Some(ProcessedTxnData::TokenTransfer(TokenTransfer {
                    chain_id: event.message_key.source_chain,
                    nonce: event.message_key.bridge_seq_num,
                    block_height: checkpoint,
                    timestamp_ms,
                    txn_hash: tx.transaction.digest().inner().to_vec(),
                    txn_sender: ev.sender.to_vec(),
                    status: TokenTransferStatus::Claimed,
                    gas_usage: tx.effects.gas_cost_summary().net_gas_usage(),
                    data_source: BridgeDataSource::Sui,
                    data: None,
                }))
            }
            _ => {
                // todo: metrics.total_sui_bridge_txn_other.inc();
                None
            }
        }
    } else {
        None
    })
}
