// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Error;
use tracing::{info, warn};

use sui_bridge::events::{
    EmergencyOpEvent, MoveBlocklistValidatorEvent, MoveNewTokenEvent, MoveTokenDepositedEvent,
    MoveTokenRegistrationEvent, MoveTokenTransferApproved, MoveTokenTransferClaimed,
    UpdateRouteLimitEvent, UpdateTokenPriceEvent,
};
use sui_indexer_builder::indexer_builder::DataMapper;
use sui_indexer_builder::sui_datasource::CheckpointTxnData;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::event::Event;
use sui_types::execution_status::ExecutionStatus;
use sui_types::full_checkpoint_content::CheckpointTransaction;
use sui_types::{BRIDGE_ADDRESS, SUI_BRIDGE_OBJECT_ID};

use crate::metrics::BridgeIndexerMetrics;
use crate::{
    BridgeDataSource, GovernanceAction, GovernanceActionType, ProcessedTxnData, SuiTxnError,
    TokenTransfer, TokenTransferData, TokenTransferStatus,
};

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
                    is_finalized: true,
                    data: Some(TokenTransferData {
                        destination_chain: move_event.target_chain,
                        sender_address: move_event.sender_address.clone(),
                        recipient_address: move_event.target_address.clone(),
                        token_id: move_event.token_type,
                        amount: move_event.amount_sui_adjusted,
                        is_finalized: true,
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
                    is_finalized: true,
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
                    is_finalized: true,
                }))
            }
            "UpdateRouteLimitEvent" => {
                info!("Observed Sui Route Limit Update {:?}", ev);
                let event: UpdateRouteLimitEvent = bcs::from_bytes(&ev.contents)?;

                Some(ProcessedTxnData::GovernanceAction(GovernanceAction {
                    nonce: None,
                    data_source: BridgeDataSource::Sui,
                    tx_digest: tx.transaction.digest().inner().to_vec(),
                    sender: ev.sender.to_vec(),
                    timestamp_ms,
                    action: GovernanceActionType::UpdateBridgeLimit,
                    data: serde_json::to_value(event)?,
                }))
            }
            "EmergencyOpEvent" => {
                info!("Observed Sui Emergency Op {:?}", ev);
                let event: EmergencyOpEvent = bcs::from_bytes(&ev.contents)?;

                Some(ProcessedTxnData::GovernanceAction(GovernanceAction {
                    nonce: None,
                    data_source: BridgeDataSource::Sui,
                    tx_digest: tx.transaction.digest().inner().to_vec(),
                    sender: ev.sender.to_vec(),
                    timestamp_ms,
                    action: GovernanceActionType::EmergencyOperation,
                    data: serde_json::to_value(event)?,
                }))
            }
            "BlocklistValidatorEvent" => {
                info!("Observed Sui Blocklist Validator {:?}", ev);
                let event: MoveBlocklistValidatorEvent = bcs::from_bytes(&ev.contents)?;

                Some(ProcessedTxnData::GovernanceAction(GovernanceAction {
                    nonce: None,
                    data_source: BridgeDataSource::Sui,
                    tx_digest: tx.transaction.digest().inner().to_vec(),
                    sender: ev.sender.to_vec(),
                    timestamp_ms,
                    action: GovernanceActionType::UpdateCommitteeBlocklist,
                    data: serde_json::to_value(event)?,
                }))
            }
            "TokenRegistrationEvent" => {
                info!("Observed Sui Token Registration {:?}", ev);
                let event: MoveTokenRegistrationEvent = bcs::from_bytes(&ev.contents)?;

                Some(ProcessedTxnData::GovernanceAction(GovernanceAction {
                    nonce: None,
                    data_source: BridgeDataSource::Sui,
                    tx_digest: tx.transaction.digest().inner().to_vec(),
                    sender: ev.sender.to_vec(),
                    timestamp_ms,
                    action: GovernanceActionType::AddSuiTokens,
                    data: serde_json::to_value(event)?,
                }))
            }
            "UpdateTokenPriceEvent" => {
                info!("Observed Sui Token Price Update {:?}", ev);
                let event: UpdateTokenPriceEvent = bcs::from_bytes(&ev.contents)?;

                Some(ProcessedTxnData::GovernanceAction(GovernanceAction {
                    nonce: None,
                    data_source: BridgeDataSource::Sui,
                    tx_digest: tx.transaction.digest().inner().to_vec(),
                    sender: ev.sender.to_vec(),
                    timestamp_ms,
                    action: GovernanceActionType::UpdateTokenPrices,
                    data: serde_json::to_value(event)?,
                }))
            }
            "NewTokenEvent" => {
                info!("Observed Sui New token event {:?}", ev);
                let event: MoveNewTokenEvent = bcs::from_bytes(&ev.contents)?;

                Some(ProcessedTxnData::GovernanceAction(GovernanceAction {
                    nonce: None,
                    data_source: BridgeDataSource::Sui,
                    tx_digest: tx.transaction.digest().inner().to_vec(),
                    sender: ev.sender.to_vec(),
                    timestamp_ms,
                    action: GovernanceActionType::AddSuiTokens,
                    data: serde_json::to_value(event)?,
                }))
            }
            _ => {
                // todo: metrics.total_sui_bridge_txn_other.inc();
                warn!("Unexpected event {ev:?}.");
                None
            }
        }
    } else {
        None
    })
}
