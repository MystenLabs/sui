// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use async_trait::async_trait;
use diesel::query_dsl::methods::FilterDsl;
use diesel::upsert::excluded;
use diesel::ExpressionMethods;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::{AsyncConnection, RunQueryDsl};
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::StructTag;
use std::sync::Arc;
use sui_bridge::events::{
    MoveTokenDepositedEvent, MoveTokenTransferApproved, MoveTokenTransferClaimed,
};
use sui_bridge_schema::models::{
    BridgeDataSource, TokenTransfer, TokenTransferData, TokenTransferStatus,
};
use sui_bridge_schema::schema::{token_transfer, token_transfer_data};
use sui_indexer_alt_framework::db::Db;
use sui_indexer_alt_framework::pipeline::concurrent::Handler;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_framework::types::effects::TransactionEffectsAPI;
use sui_indexer_alt_framework::types::full_checkpoint_content::{
    CheckpointData, CheckpointTransaction,
};
use sui_indexer_alt_framework::types::BRIDGE_ADDRESS;
use sui_indexer_alt_framework::FieldCount;
use tracing::info;
const TOKEN_DEPOSITED_EVENT: &IdentStr = ident_str!("TokenDepositedEvent");
const TOKEN_TRANSFER_APPROVED: &IdentStr = ident_str!("TokenTransferApproved");
const TOKEN_TRANSFER_CLAIMED: &IdentStr = ident_str!("TokenTransferClaimed");
const BRIDGE_MODULE: &IdentStr = ident_str!("bridge");

pub struct TokenTransferHandler {
    deposited_event_type: StructTag,
    approved_event_type: StructTag,
    claimed_event_type: StructTag,
}

impl TokenTransferHandler {
    pub fn new() -> Self {
        Self {
            deposited_event_type: StructTag {
                address: BRIDGE_ADDRESS,
                module: BRIDGE_MODULE.into(),
                name: TOKEN_DEPOSITED_EVENT.into(),
                type_params: vec![],
            },
            approved_event_type: StructTag {
                address: BRIDGE_ADDRESS,
                module: BRIDGE_MODULE.into(),
                name: TOKEN_TRANSFER_APPROVED.into(),
                type_params: vec![],
            },
            claimed_event_type: StructTag {
                address: BRIDGE_ADDRESS,
                module: BRIDGE_MODULE.into(),
                name: TOKEN_TRANSFER_CLAIMED.into(),
                type_params: vec![],
            },
        }
    }
}

impl Processor for TokenTransferHandler {
    const NAME: &'static str = "";
    type Value = TokenTransferDataWrapper;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>, anyhow::Error> {
        let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms as i64;
        let block_height = checkpoint.checkpoint_summary.sequence_number as i64;
        checkpoint
            .transactions
            .iter()
            .try_fold(vec![], |results, tx| {
                tx.events.iter().flat_map(|events| &events.data).try_fold(
                    results,
                    |mut results, ev| {
                        if self.deposited_event_type == ev.type_ {
                            info!("Observed Sui Deposit {:?}", ev);
                            // todo: metrics.total_sui_token_deposited.inc();
                            results.push(TokenTransferDataWrapper::from_deposit_event(
                                bcs::from_bytes(&ev.contents)?,
                                tx,
                                block_height,
                                timestamp_ms,
                            ));
                        } else if self.approved_event_type == ev.type_ {
                            info!("Observed Sui Approval {:?}", ev);
                            // todo: metrics.total_sui_token_transfer_approved.inc();
                            results.push(TokenTransferDataWrapper::from_approve_event(
                                bcs::from_bytes(&ev.contents)?,
                                tx,
                                block_height,
                                timestamp_ms,
                            ));
                        } else if self.claimed_event_type == ev.type_ {
                            info!("Observed Sui Claim {:?}", ev);
                            // todo: metrics.total_sui_token_transfer_claimed.inc();
                            results.push(TokenTransferDataWrapper::from_claimed_event(
                                bcs::from_bytes(&ev.contents)?,
                                tx,
                                block_height,
                                timestamp_ms,
                            ));
                        }

                        Ok(results)
                    },
                )
            })
    }
}

#[async_trait]
impl Handler for TokenTransferHandler {
    type Store = Db;

    async fn commit<'a>(
        values: &[Self::Value],
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> sui_indexer_alt_framework::Result<usize> {
        let (token_transfers, data) = values.iter().fold(
            (vec![], vec![]),
            |(mut token_transfers, mut data_vec),
             TokenTransferDataWrapper {
                 token_transfer,
                 data,
             }| {
                token_transfers.push(token_transfer.clone());
                if let Some(d) = data {
                    data_vec.push(d.clone());
                }
                (token_transfers, data_vec)
            },
        );

        conn.transaction(|conn| {
            async move {
                {
                    use token_transfer::columns::*;
                    diesel::insert_into(token_transfer::table)
                        .values(&token_transfers)
                        .on_conflict((chain_id, nonce, status))
                        .do_update()
                        .set((
                            chain_id.eq(excluded(chain_id)),
                            nonce.eq(excluded(nonce)),
                            status.eq(excluded(status)),
                            block_height.eq(excluded(block_height)),
                            timestamp_ms.eq(excluded(timestamp_ms)),
                            txn_hash.eq(excluded(txn_hash)),
                            txn_sender.eq(excluded(txn_sender)),
                            gas_usage.eq(excluded(gas_usage)),
                            data_source.eq(excluded(data_source)),
                            is_finalized.eq(excluded(is_finalized)),
                        ))
                        .filter(is_finalized.eq(false))
                        .execute(conn)
                        .await?;
                }

                {
                    use token_transfer_data::columns::*;
                    Ok(diesel::insert_into(token_transfer_data::table)
                        .values(&data)
                        .on_conflict((chain_id, nonce))
                        .do_update()
                        .set((
                            chain_id.eq(excluded(chain_id)),
                            nonce.eq(excluded(nonce)),
                            block_height.eq(excluded(block_height)),
                            timestamp_ms.eq(excluded(timestamp_ms)),
                            txn_hash.eq(excluded(txn_hash)),
                            sender_address.eq(excluded(sender_address)),
                            destination_chain.eq(excluded(destination_chain)),
                            recipient_address.eq(excluded(recipient_address)),
                            token_id.eq(excluded(token_id)),
                            amount.eq(excluded(amount)),
                            is_finalized.eq(excluded(is_finalized)),
                        ))
                        .filter(is_finalized.eq(false))
                        .execute(conn)
                        .await?)
                }
            }
            .scope_boxed()
        })
        .await
    }
}

#[derive(FieldCount)]
pub struct TokenTransferDataWrapper {
    token_transfer: TokenTransfer,
    data: Option<TokenTransferData>,
}

impl TokenTransferDataWrapper {
    fn from_deposit_event(
        event: MoveTokenDepositedEvent,
        tx: &CheckpointTransaction,
        block_height: i64,
        timestamp_ms: i64,
    ) -> Self {
        Self {
            token_transfer: TokenTransfer {
                chain_id: event.source_chain as i32,
                nonce: event.seq_num as i64,
                block_height,
                timestamp_ms,
                status: TokenTransferStatus::Deposited,
                data_source: BridgeDataSource::SUI,
                is_finalized: true,
                txn_hash: tx.transaction.digest().inner().to_vec(),
                txn_sender: tx.transaction.sender_address().to_vec(),
                gas_usage: tx.effects.gas_cost_summary().net_gas_usage(),
            },
            data: Some(TokenTransferData {
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
            }),
        }
    }

    fn from_approve_event(
        event: MoveTokenTransferApproved,
        tx: &CheckpointTransaction,
        block_height: i64,
        timestamp_ms: i64,
    ) -> Self {
        Self {
            token_transfer: TokenTransfer {
                chain_id: event.message_key.source_chain as i32,
                nonce: event.message_key.bridge_seq_num as i64,
                block_height,
                timestamp_ms,
                txn_hash: tx.transaction.digest().inner().to_vec(),
                txn_sender: tx.transaction.sender_address().to_vec(),
                status: TokenTransferStatus::Approved,
                gas_usage: tx.effects.gas_cost_summary().net_gas_usage(),
                data_source: BridgeDataSource::SUI,
                is_finalized: true,
            },
            data: None,
        }
    }

    fn from_claimed_event(
        event: MoveTokenTransferClaimed,
        tx: &CheckpointTransaction,
        block_height: i64,
        timestamp_ms: i64,
    ) -> Self {
        Self {
            token_transfer: TokenTransfer {
                chain_id: event.message_key.source_chain as i32,
                nonce: event.message_key.bridge_seq_num as i64,
                block_height,
                timestamp_ms,
                txn_hash: tx.transaction.digest().inner().to_vec(),
                txn_sender: tx.transaction.sender_address().to_vec(),
                status: TokenTransferStatus::Claimed,
                gas_usage: tx.effects.gas_cost_summary().net_gas_usage(),
                data_source: BridgeDataSource::SUI,
                is_finalized: true,
            },
            data: None,
        }
    }
}
