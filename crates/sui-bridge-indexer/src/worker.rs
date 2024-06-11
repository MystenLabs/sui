// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    metrics::BridgeIndexerMetrics,
    postgres_writer::{get_connection_pool, write, PgPool},
    BridgeDataSource, TokenTransfer, TokenTransferData, TokenTransferStatus,
};
use anyhow::Result;
use async_trait::async_trait;
use ethers::providers::Provider;
use ethers::providers::{Http, Middleware};
use ethers::types::{Address as EthAddress, H256};
use mysten_metrics::metered_channel::Receiver;
use std::collections::BTreeSet;
use std::sync::Arc;
use sui_bridge::abi::{EthBridgeEvent, EthSuiBridgeEvents};
use sui_bridge::events::{
    MoveTokenDepositedEvent, MoveTokenTransferApproved, MoveTokenTransferClaimed,
};
use sui_bridge::types::EthLog;
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

pub struct BridgeWorker {
    bridge_object_ids: BTreeSet<ObjectID>,
    pg_pool: PgPool,
    metrics: BridgeIndexerMetrics,
}

impl BridgeWorker {
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

pub async fn process_eth_transaction(
    mut eth_events_rx: Receiver<(EthAddress, u64, Vec<EthLog>)>,
    provider: Arc<Provider<Http>>,
    pool: PgPool,
    metrics: BridgeIndexerMetrics,
) {
    while let Some((_, _, logs)) = eth_events_rx.recv().await {
        let mut data = vec![];
        for log in &logs {
            let Some(bridge_event) = EthBridgeEvent::try_from_eth_log(log) else {
                continue;
            };
            let block_number = log.block_number;
            let block = provider.get_block(log.block_number).await.unwrap().unwrap();
            let timestamp = block.timestamp.as_u64() * 1000;
            let transaction = provider
                .get_transaction(log.tx_hash)
                .await
                .unwrap()
                .unwrap();
            let gas = transaction.gas;
            let tx_hash = log.tx_hash;
            info!("Observed Eth bridge event: {:?}", bridge_event);
            metrics.total_eth_bridge_transactions.inc();
            if let Some(token_transfer) = process_eth_event(
                bridge_event,
                block_number,
                timestamp,
                tx_hash,
                gas.as_u64(),
                &metrics,
            ) {
                data.push(token_transfer)
            }
        }
        write(&pool, data).unwrap();
        // TODO: record last processed block
        // TODO: handle error for the ETH data ingestion
    }
}

fn process_eth_event(
    bridge_event: EthBridgeEvent,
    block_height: u64,
    timestamp_ms: u64,
    tx_hash: H256,
    gas: u64,
    metrics: &BridgeIndexerMetrics,
) -> Option<TokenTransfer> {
    match bridge_event {
        EthBridgeEvent::EthSuiBridgeEvents(bridge_event) => match bridge_event {
            EthSuiBridgeEvents::TokensDepositedFilter(bridge_event) => {
                info!("Observed Eth Deposit {:?}", bridge_event);
                metrics.total_eth_token_deposited.inc();
                Some(TokenTransfer {
                    chain_id: bridge_event.source_chain_id,
                    nonce: bridge_event.nonce,
                    block_height,
                    timestamp_ms,
                    txn_hash: tx_hash.as_bytes().to_vec(),
                    txn_sender: bridge_event.sender_address.as_bytes().to_vec(),
                    status: TokenTransferStatus::Deposited,
                    gas_usage: gas as i64,
                    data_source: BridgeDataSource::Eth,
                    data: Some(TokenTransferData {
                        destination_chain: bridge_event.destination_chain_id,
                        sender_address: bridge_event.sender_address.as_bytes().to_vec(),
                        recipient_address: bridge_event.recipient_address.to_vec(),
                        token_id: bridge_event.token_id,
                        amount: bridge_event.sui_adjusted_amount,
                    }),
                })
            }
            EthSuiBridgeEvents::TokensClaimedFilter(bridge_event) => {
                info!("Observed Eth Claim {:?}", bridge_event);
                metrics.total_eth_token_transfer_claimed.inc();
                Some(TokenTransfer {
                    chain_id: bridge_event.source_chain_id,
                    nonce: bridge_event.nonce,
                    block_height,
                    timestamp_ms,
                    txn_hash: tx_hash.as_bytes().to_vec(),
                    txn_sender: bridge_event.sender_address.to_vec(),
                    status: TokenTransferStatus::Claimed,
                    gas_usage: gas as i64,
                    data_source: BridgeDataSource::Eth,
                    data: None,
                })
            }
            EthSuiBridgeEvents::PausedFilter(_)
            | EthSuiBridgeEvents::UnpausedFilter(_)
            | EthSuiBridgeEvents::UpgradedFilter(_)
            | EthSuiBridgeEvents::InitializedFilter(_) => {
                metrics.total_eth_bridge_txn_other.inc();
                None
            }
        },
        EthBridgeEvent::EthBridgeCommitteeEvents(_)
        | EthBridgeEvent::EthBridgeLimiterEvents(_)
        | EthBridgeEvent::EthBridgeConfigEvents(_)
        | EthBridgeEvent::EthCommitteeUpgradeableContractEvents(_) => {
            metrics.total_eth_bridge_txn_other.inc();
            None
        }
    }
}

#[async_trait]
impl Worker for BridgeWorker {
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
