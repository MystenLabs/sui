// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::handlers::{
    is_bridge_txn, BRIDGE, TOKEN_DEPOSITED_EVENT, TOKEN_TRANSFER_APPROVED, TOKEN_TRANSFER_CLAIMED,
};
use crate::struct_tag;
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use move_core_types::language_storage::StructTag;
use std::sync::Arc;
use sui_bridge::events::{
    MoveTokenDepositedEvent, MoveTokenTransferApproved, MoveTokenTransferClaimed,
};
use sui_bridge_schema::models::{BridgeDataSource, TokenTransfer, TokenTransferStatus};
use sui_bridge_schema::schema::token_transfer;
use sui_indexer_alt_framework::pipeline::concurrent::Handler;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::postgres::Db;
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_framework::types::effects::TransactionEffectsAPI;
use sui_indexer_alt_framework::types::full_checkpoint_content::CheckpointData;
use sui_indexer_alt_framework::types::BRIDGE_ADDRESS;
use tracing::info;

pub struct TokenTransferHandler {
    deposited_event_type: StructTag,
    approved_event_type: StructTag,
    claimed_event_type: StructTag,
}

impl Default for TokenTransferHandler {
    fn default() -> Self {
        Self {
            deposited_event_type: struct_tag!(BRIDGE_ADDRESS, BRIDGE, TOKEN_DEPOSITED_EVENT),
            approved_event_type: struct_tag!(BRIDGE_ADDRESS, BRIDGE, TOKEN_TRANSFER_APPROVED),
            claimed_event_type: struct_tag!(BRIDGE_ADDRESS, BRIDGE, TOKEN_TRANSFER_CLAIMED),
        }
    }
}

impl Processor for TokenTransferHandler {
    const NAME: &'static str = "token_transfer";
    type Value = TokenTransfer;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>, anyhow::Error> {
        let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms as i64;
        let block_height = checkpoint.checkpoint_summary.sequence_number as i64;

        let mut results = vec![];

        for tx in &checkpoint.transactions {
            if !is_bridge_txn(tx) {
                continue;
            }
            for ev in tx.events.iter().flat_map(|e| &e.data) {
                let (chain_id, nonce) = if self.deposited_event_type == ev.type_ {
                    info!("Observed Sui Deposit {:?}", ev);
                    // todo: metrics.total_sui_token_deposited.inc();
                    let event: MoveTokenDepositedEvent = bcs::from_bytes(&ev.contents)?;
                    (event.source_chain, event.seq_num)
                } else if self.approved_event_type == ev.type_ {
                    info!("Observed Sui Approval {:?}", ev);
                    // todo: metrics.total_sui_token_transfer_approved.inc();
                    let event: MoveTokenTransferApproved = bcs::from_bytes(&ev.contents)?;
                    (
                        event.message_key.source_chain,
                        event.message_key.bridge_seq_num,
                    )
                } else if self.claimed_event_type == ev.type_ {
                    info!("Observed Sui Claim {:?}", ev);
                    // todo: metrics.total_sui_token_transfer_claimed.inc();
                    let event: MoveTokenTransferClaimed = bcs::from_bytes(&ev.contents)?;
                    (
                        event.message_key.source_chain,
                        event.message_key.bridge_seq_num,
                    )
                } else {
                    return Ok(results);
                };

                results.push(TokenTransfer {
                    chain_id: chain_id as i32,
                    nonce: nonce as i64,
                    block_height,
                    timestamp_ms,
                    status: TokenTransferStatus::Deposited,
                    data_source: BridgeDataSource::SUI,
                    is_finalized: true,
                    txn_hash: tx.transaction.digest().inner().to_vec(),
                    txn_sender: tx.transaction.sender_address().to_vec(),
                    gas_usage: tx.effects.gas_cost_summary().net_gas_usage(),
                });
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Handler for TokenTransferHandler {
    type Store = Db;
    async fn commit<'a>(
        values: &[Self::Value],
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        Ok(diesel::insert_into(token_transfer::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
