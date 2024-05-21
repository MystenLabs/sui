// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::postgres_writer::{get_connection_pool, write, PgPool};
use crate::{TokenTransfer, TokenTransferData, TokenTransferStatus};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::BTreeSet;
use sui_bridge::events::{
    MoveTokenDepositedEvent, MoveTokenTransferApproved, MoveTokenTransferClaimed,
};
use sui_data_ingestion_core::Worker;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::{
    base_types::ObjectID,
    full_checkpoint_content::{CheckpointData, CheckpointTransaction},
    transaction::{TransactionDataAPI, TransactionKind},
    BRIDGE_ADDRESS, SUI_BRIDGE_OBJECT_ID,
};
use tracing::info;

pub struct BridgeWorker {
    bridge_object_ids: BTreeSet<ObjectID>,
    pg_pool: PgPool,
}

impl BridgeWorker {
    pub fn new(bridge_object_ids: Vec<ObjectID>, db_url: String) -> Self {
        let mut bridge_object_ids = bridge_object_ids.into_iter().collect::<BTreeSet<_>>();
        bridge_object_ids.insert(SUI_BRIDGE_OBJECT_ID);
        let pg_pool = get_connection_pool(db_url);
        Self {
            bridge_object_ids,
            pg_pool,
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
    fn process_transaction(&self, tx: &CheckpointTransaction, checkpoint: u64, timestamp_ms: u64) {
        if let Some(event) = &tx.events {
            event.data.iter().for_each(|ev| {
                if ev.type_.address == BRIDGE_ADDRESS {
                    println!("SUI: Processing bridge event : {:?}", ev.type_);
                    let token_transfer = match ev.type_.name.as_str() {
                        "TokenDepositedEvent" => {
                            // todo: handle deserialization error
                            let event: MoveTokenDepositedEvent =
                                bcs::from_bytes(&ev.contents).unwrap();
                            Some(TokenTransfer {
                                chain_id: event.source_chain,
                                nonce: event.seq_num,
                                block_height: checkpoint,
                                timestamp_ms,
                                txn_hash: tx.transaction.digest().inner().to_vec(),
                                status: TokenTransferStatus::Deposited,
                                gas_usage: tx.effects.gas_cost_summary().net_gas_usage(),
                                data: Some(TokenTransferData {
                                    sender_address: event.sender_address,
                                    destination_chain: event.target_chain,
                                    recipient_address: event.target_address,
                                    token_id: event.token_type,
                                    amount: event.amount_sui_adjusted,
                                }),
                            })
                        }
                        "TokenTransferApproved" => {
                            let event: MoveTokenTransferApproved =
                                bcs::from_bytes(&ev.contents).unwrap();
                            Some(TokenTransfer {
                                chain_id: event.message_key.source_chain,
                                nonce: event.message_key.bridge_seq_num,
                                block_height: checkpoint,
                                timestamp_ms,
                                txn_hash: tx.transaction.digest().inner().to_vec(),
                                status: TokenTransferStatus::Approved,
                                gas_usage: tx.effects.gas_cost_summary().net_gas_usage(),
                                data: None,
                            })
                        }
                        "TokenTransferClaimed" => {
                            let event: MoveTokenTransferClaimed =
                                bcs::from_bytes(&ev.contents).unwrap();
                            Some(TokenTransfer {
                                chain_id: event.message_key.source_chain,
                                nonce: event.message_key.bridge_seq_num,
                                block_height: checkpoint,
                                timestamp_ms,
                                txn_hash: tx.transaction.digest().inner().to_vec(),
                                status: TokenTransferStatus::Claimed,
                                gas_usage: tx.effects.gas_cost_summary().net_gas_usage(),
                                data: None,
                            })
                        }
                        _ => None,
                    };

                    if let Some(transfer) = token_transfer {
                        println!("SUI: Storing bridge event : {:?}", ev.type_);
                        write(&self.pg_pool, transfer);
                    }
                };
            });
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
        checkpoint
            .transactions
            .iter()
            .filter(|txn| self.is_bridge_transaction(txn))
            .for_each(|txn| self.process_transaction(txn, checkpoint_num, timestamp_ms));
        Ok(())
    }
}
