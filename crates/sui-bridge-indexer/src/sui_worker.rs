// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::postgres_manager::{get_connection_pool, write, PgPool};
use crate::{
    metrics::BridgeIndexerMetrics, BridgeDataSource, TokenTransfer, TokenTransferData,
    TokenTransferStatus,
};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::BTreeSet;
use sui_bridge::events::{
    MoveTokenDepositedEvent, MoveTokenTransferApproved, MoveTokenTransferClaimed,
};
use sui_data_ingestion_core::Worker;
use sui_types::event::Event;
use sui_types::{
    base_types::ObjectID,
    effects::TransactionEffectsAPI,
    full_checkpoint_content::{CheckpointData, CheckpointTransaction},
    transaction::{TransactionDataAPI, TransactionKind},
    BRIDGE_ADDRESS, SUI_BRIDGE_OBJECT_ID,
};
use tracing::info;

pub struct SuiBridgeWorker {
    bridge_object_ids: BTreeSet<ObjectID>,
    pg_pool: PgPool,
    metrics: BridgeIndexerMetrics,
}

impl SuiBridgeWorker {
    pub fn new(
        bridge_object_ids: Vec<ObjectID>,
        db_url: String,
        metrics: BridgeIndexerMetrics,
    ) -> Self {
        let mut bridge_object_ids = bridge_object_ids.into_iter().collect::<BTreeSet<_>>();
        bridge_object_ids.insert(SUI_BRIDGE_OBJECT_ID);
        let pg_pool = get_connection_pool(db_url);
        Self {
            bridge_object_ids,
            pg_pool,
            metrics,
        }
    }

    // Return true if the transaction relates to the bridge and is of interest.
    fn is_bridge_transaction(&self, tx: &CheckpointTransaction) -> bool {
        // TODO: right now this returns true for programmable transactions that
        //       have the bridge object as input. We can extend later to cover other cases
        let txn_data = tx.transaction.transaction_data();
        if let TransactionKind::ProgrammableTransaction(_pt) = txn_data.kind() {
            return tx
                .input_objects
                .iter()
                .any(|obj| self.bridge_object_ids.contains(&obj.id()));
        };
        false
    }

    // Process a transaction that has been identified as a bridge transaction.
    fn process_transaction(
        &self,
        tx: &CheckpointTransaction,
        checkpoint: u64,
        timestamp_ms: u64,
    ) -> Result<Vec<TokenTransfer>> {
        self.metrics.total_sui_bridge_transactions.inc();
        if let Some(events) = &tx.events {
            let token_transfers = events.data.iter().try_fold(vec![], |mut result, ev| {
                if let Some(data) =
                    Self::process_sui_event(ev, tx, checkpoint, timestamp_ms, &self.metrics)?
                {
                    result.push(data);
                }
                Ok::<_, anyhow::Error>(result)
            })?;

            if !token_transfers.is_empty() {
                info!(
                    "SUI: Extracted {} bridge token transfer data entries for tx {}.",
                    token_transfers.len(),
                    tx.transaction.digest()
                );
            }
            Ok(token_transfers)
        } else {
            Ok(vec![])
        }
    }

    fn process_sui_event(
        ev: &Event,
        tx: &CheckpointTransaction,
        checkpoint: u64,
        timestamp_ms: u64,
        metrics: &BridgeIndexerMetrics,
    ) -> Result<Option<TokenTransfer>> {
        Ok(if ev.type_.address == BRIDGE_ADDRESS {
            match ev.type_.name.as_str() {
                "TokenDepositedEvent" => {
                    info!("Observed Sui Deposit {:?}", ev);
                    metrics.total_sui_token_deposited.inc();
                    let move_event: MoveTokenDepositedEvent = bcs::from_bytes(&ev.contents)?;
                    Some(TokenTransfer {
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
                    })
                }
                "TokenTransferApproved" => {
                    info!("Observed Sui Approval {:?}", ev);
                    metrics.total_sui_token_transfer_approved.inc();
                    let event: MoveTokenTransferApproved = bcs::from_bytes(&ev.contents)?;
                    Some(TokenTransfer {
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
                    })
                }
                "TokenTransferClaimed" => {
                    info!("Observed Sui Claim {:?}", ev);
                    metrics.total_sui_token_transfer_claimed.inc();
                    let event: MoveTokenTransferClaimed = bcs::from_bytes(&ev.contents)?;
                    Some(TokenTransfer {
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
                    })
                }
                _ => {
                    metrics.total_sui_bridge_txn_other.inc();
                    None
                }
            }
        } else {
            None
        })
    }
}

#[async_trait]
impl Worker for SuiBridgeWorker {
    async fn process_checkpoint(&self, checkpoint: CheckpointData) -> Result<()> {
        info!(
            "Processing checkpoint [{}] {}: {}",
            checkpoint.checkpoint_summary.epoch,
            checkpoint.checkpoint_summary.sequence_number,
            checkpoint.transactions.len(),
        );
        let checkpoint_num = checkpoint.checkpoint_summary.sequence_number;
        let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms;

        let bridge_data = checkpoint
            .transactions
            .iter()
            .filter(|txn| self.is_bridge_transaction(txn))
            .try_fold(vec![], |mut result, txn| {
                result.append(&mut self.process_transaction(txn, checkpoint_num, timestamp_ms)?);
                Ok::<_, anyhow::Error>(result)
            })?;

        write(&self.pg_pool, bridge_data)
    }
}
