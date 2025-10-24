// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::handlers::{BRIDGE, TOKEN_DEPOSITED_EVENT, is_bridge_txn};
use crate::struct_tag;
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use move_core_types::language_storage::StructTag;
use std::sync::Arc;
use sui_bridge::events::MoveTokenDepositedEvent;
use sui_bridge_schema::models::TokenTransferData;
use sui_bridge_schema::schema::token_transfer_data;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::Handler;
use sui_indexer_alt_framework::postgres::Db;
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_framework::types::BRIDGE_ADDRESS;
use sui_indexer_alt_framework::types::full_checkpoint_content::CheckpointData;
use tracing::info;

pub struct TokenTransferDataHandler {
    deposited_event_type: StructTag,
}

impl Default for TokenTransferDataHandler {
    fn default() -> Self {
        Self {
            deposited_event_type: struct_tag!(BRIDGE_ADDRESS, BRIDGE, TOKEN_DEPOSITED_EVENT),
        }
    }
}

#[async_trait]
impl Processor for TokenTransferDataHandler {
    const NAME: &'static str = "token_transfer_data";
    type Value = TokenTransferData;

    async fn process(
        &self,
        checkpoint: &Arc<CheckpointData>,
    ) -> Result<Vec<Self::Value>, anyhow::Error> {
        let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms as i64;
        let block_height = checkpoint.checkpoint_summary.sequence_number as i64;

        let mut results = vec![];

        for tx in &checkpoint.transactions {
            if !is_bridge_txn(tx) {
                continue;
            }
            for ev in tx.events.iter().flat_map(|e| &e.data) {
                if self.deposited_event_type != ev.type_ {
                    continue;
                }
                info!(?ev, "Observed Sui Deposit");
                let event: MoveTokenDepositedEvent = bcs::from_bytes(&ev.contents)?;
                results.push(TokenTransferData {
                    chain_id: event.source_chain as i32,
                    nonce: event.seq_num as i64,
                    block_height,
                    timestamp_ms,
                    destination_chain: event.target_chain as i32,
                    sender_address: event.sender_address.clone(),
                    recipient_address: event.target_address.clone(),
                    token_id: event.token_type as i32,
                    amount: event.amount_sui_adjusted as i64,
                    is_finalized: true,
                    txn_hash: tx.transaction.digest().inner().to_vec(),
                });
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Handler for TokenTransferDataHandler {
    type Store = Db;

    async fn commit<'a>(
        values: &[Self::Value],
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> sui_indexer_alt_framework::Result<usize> {
        Ok(diesel::insert_into(token_transfer_data::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
