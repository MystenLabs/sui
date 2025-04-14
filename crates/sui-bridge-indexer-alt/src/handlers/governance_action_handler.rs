// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::handlers::{is_bridge_txn, BRIDGE, COMMITTEE, LIMITER, TREASURY};
use crate::struct_tag;
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::StructTag;
use std::sync::Arc;
use sui_bridge::events::{
    EmergencyOpEvent, MoveBlocklistValidatorEvent, MoveNewTokenEvent, MoveTokenRegistrationEvent,
    UpdateRouteLimitEvent, UpdateTokenPriceEvent,
};
use sui_bridge_schema::models::{BridgeDataSource, GovernanceAction, GovernanceActionType};
use sui_bridge_schema::schema;
use sui_indexer_alt_framework::db::Db;
use sui_indexer_alt_framework::pipeline::concurrent::Handler;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_framework::types::full_checkpoint_content::CheckpointData;
use sui_indexer_alt_framework::types::BRIDGE_ADDRESS;
use tracing::info;

const UPDATE_ROUTE_LIMIT_EVENT: &IdentStr = ident_str!("UpdateRouteLimitEvent");
const EMERGENCY_OP_EVENT: &IdentStr = ident_str!("EmergencyOpEvent");
const BLOCKLIST_VALIDATOR_EVENT: &IdentStr = ident_str!("BlocklistValidatorEvent");
const TOKEN_REGISTRATION_EVENT: &IdentStr = ident_str!("TokenRegistrationEvent");
const UPDATE_TOKEN_PRICE_EVENT: &IdentStr = ident_str!("UpdateTokenPriceEvent");
const NEW_TOKEN_EVENT: &IdentStr = ident_str!("NewTokenEvent");

pub struct GovernanceActionHandler {
    update_limit_event_type: StructTag,
    emergency_op_event_type: StructTag,
    blocklist_event_type: StructTag,
    token_reg_event_type: StructTag,
    update_price_event_type: StructTag,
    new_token_event_type: StructTag,
}

impl GovernanceActionHandler {
    pub fn new() -> Self {
        Self {
            update_limit_event_type: struct_tag!(BRIDGE_ADDRESS, LIMITER, UPDATE_ROUTE_LIMIT_EVENT),
            emergency_op_event_type: struct_tag!(BRIDGE_ADDRESS, BRIDGE, EMERGENCY_OP_EVENT),
            blocklist_event_type: struct_tag!(BRIDGE_ADDRESS, COMMITTEE, BLOCKLIST_VALIDATOR_EVENT),
            token_reg_event_type: struct_tag!(BRIDGE_ADDRESS, TREASURY, TOKEN_REGISTRATION_EVENT),
            update_price_event_type: struct_tag!(
                BRIDGE_ADDRESS,
                TREASURY,
                UPDATE_TOKEN_PRICE_EVENT
            ),
            new_token_event_type: struct_tag!(BRIDGE_ADDRESS, TREASURY, NEW_TOKEN_EVENT),
        }
    }
}

impl Processor for GovernanceActionHandler {
    const NAME: &'static str = "GovernanceAction";
    type Value = GovernanceAction;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>> {
        let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms as i64;
        checkpoint
            .transactions
            .iter()
            .try_fold(vec![], |results, tx| {
                if !is_bridge_txn(tx) {
                    return Ok(results);
                }
                let txn_digest = tx.transaction.digest().inner();
                let sender_address = tx.transaction.sender_address();
                tx.events.iter().flat_map(|events| &events.data).try_fold(
                    results,
                    |mut results, ev| {
                        let data = if self.update_limit_event_type == ev.type_ {
                            info!("Observed Sui Route Limit Update {:?}", ev);
                            // todo: metrics.total_sui_token_deposited.inc();
                            let event: UpdateRouteLimitEvent = bcs::from_bytes(&ev.contents)?;
                            Some((
                                GovernanceActionType::UpdateBridgeLimit,
                                serde_json::to_value(event)?,
                            ))
                        } else if self.emergency_op_event_type == ev.type_ {
                            info!("Observed Sui Emergency Op {:?}", ev);
                            let event: EmergencyOpEvent = bcs::from_bytes(&ev.contents)?;
                            Some((
                                GovernanceActionType::EmergencyOperation,
                                serde_json::to_value(event)?,
                            ))
                        } else if self.blocklist_event_type == ev.type_ {
                            info!("Observed Sui Blocklist Validator {:?}", ev);
                            let event: MoveBlocklistValidatorEvent = bcs::from_bytes(&ev.contents)?;
                            Some((
                                GovernanceActionType::UpdateCommitteeBlocklist,
                                serde_json::to_value(event)?,
                            ))
                        } else if self.token_reg_event_type == ev.type_ {
                            info!("Observed Sui Token Registration {:?}", ev);
                            let event: MoveTokenRegistrationEvent = bcs::from_bytes(&ev.contents)?;
                            Some((
                                GovernanceActionType::AddSuiTokens,
                                serde_json::to_value(event)?,
                            ))
                        } else if self.update_price_event_type == ev.type_ {
                            info!("Observed Sui Token Price Update {:?}", ev);
                            let event: UpdateTokenPriceEvent = bcs::from_bytes(&ev.contents)?;
                            Some((
                                GovernanceActionType::UpdateTokenPrices,
                                serde_json::to_value(event)?,
                            ))
                        } else if self.new_token_event_type == ev.type_ {
                            info!("Observed Sui New token event {:?}", ev);
                            let event: MoveNewTokenEvent = bcs::from_bytes(&ev.contents)?;
                            Some((
                                GovernanceActionType::AddSuiTokens,
                                serde_json::to_value(event)?,
                            ))
                        } else {
                            None
                        };

                        if let Some((action, data)) = data {
                            results.push(GovernanceAction {
                                nonce: None,
                                data_source: BridgeDataSource::SUI,
                                txn_digest: txn_digest.to_vec(),
                                sender_address: sender_address.to_vec(),
                                timestamp_ms,
                                action,
                                data,
                            });
                        }

                        Ok(results)
                    },
                )
            })
    }
}

#[async_trait]
impl Handler for GovernanceActionHandler {
    type Store = Db;

    async fn commit<'a>(
        values: &[Self::Value],
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        Ok(diesel::insert_into(schema::governance_actions::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
