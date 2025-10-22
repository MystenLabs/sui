// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::handlers::{BRIDGE, COMMITTEE, LIMITER, TREASURY, is_bridge_txn};
use crate::metrics::BridgeIndexerMetrics;
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
use sui_bridge_schema::models::{BridgeDataSource, GovernanceAction};
use sui_bridge_schema::schema;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::Handler;
use sui_indexer_alt_framework::postgres::Db;
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_framework::types::BRIDGE_ADDRESS;
use sui_indexer_alt_framework::types::full_checkpoint_content::CheckpointData;
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
    metrics: Arc<BridgeIndexerMetrics>,
}

impl GovernanceActionHandler {
    pub fn new(metrics: Arc<BridgeIndexerMetrics>) -> Self {
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
            metrics,
        }
    }
}

impl Default for GovernanceActionHandler {
    fn default() -> Self {
        use prometheus::Registry;
        let registry = Registry::new();
        let metrics = BridgeIndexerMetrics::new(&registry);
        Self::new(metrics)
    }
}

#[async_trait]
impl Processor for GovernanceActionHandler {
    const NAME: &'static str = "governance_action";
    type Value = GovernanceAction;

    async fn process(&self, checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>> {
        let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms as i64;

        let mut results = vec![];

        for tx in &checkpoint.transactions {
            if !is_bridge_txn(tx) {
                continue;
            }
            let txn_digest = tx.transaction.digest().inner();
            let sender_address = tx.transaction.sender_address();

            for ev in tx.events.iter().flat_map(|e| &e.data) {
                use sui_bridge_schema::models::GovernanceActionType::*;

                let (action, data) = match &ev.type_ {
                    t if t == &self.update_limit_event_type => {
                        info!(?ev, "Observed Sui Route Limit Update");
                        let event: UpdateRouteLimitEvent = bcs::from_bytes(&ev.contents)?;

                        // Critical bridge limit update metrics
                        self.metrics
                            .governance_actions_total
                            .with_label_values(&["update_bridge_limit", "sui"])
                            .inc();
                        self.metrics
                            .bridge_events_total
                            .with_label_values(&["update_route_limit", "sui"])
                            .inc();

                        (UpdateBridgeLimit, serde_json::to_value(event)?)
                    }
                    t if t == &self.emergency_op_event_type => {
                        info!(?ev, "Observed Sui Emergency Op");
                        let event: EmergencyOpEvent = bcs::from_bytes(&ev.contents)?;

                        // Critical security event - emergency bridge pause/unpause
                        self.metrics
                            .bridge_emergency_events_total
                            .with_label_values(&["emergency_operation", "critical"])
                            .inc();
                        self.metrics
                            .governance_actions_total
                            .with_label_values(&["emergency_operation", "sui"])
                            .inc();
                        self.metrics
                            .bridge_pause_status
                            .with_label_values(&["bridge_main"])
                            .set(if event.frozen { 0 } else { 1 });

                        (EmergencyOperation, serde_json::to_value(event)?)
                    }
                    t if t == &self.blocklist_event_type => {
                        info!(?ev, "Observed Sui Blocklist Validator");
                        let event: MoveBlocklistValidatorEvent = bcs::from_bytes(&ev.contents)?;
                        (UpdateCommitteeBlocklist, serde_json::to_value(event)?)
                    }
                    t if t == &self.token_reg_event_type => {
                        info!(?ev, "Observed Sui Token Registration");
                        let event: MoveTokenRegistrationEvent = bcs::from_bytes(&ev.contents)?;
                        (AddSuiTokens, serde_json::to_value(event)?)
                    }
                    t if t == &self.update_price_event_type => {
                        info!(?ev, "Observed Sui Token Price Update");
                        let event: UpdateTokenPriceEvent = bcs::from_bytes(&ev.contents)?;
                        (UpdateTokenPrices, serde_json::to_value(event)?)
                    }
                    t if t == &self.new_token_event_type => {
                        info!(?ev, "Observed Sui New token event");
                        let event: MoveNewTokenEvent = bcs::from_bytes(&ev.contents)?;
                        (AddSuiTokens, serde_json::to_value(event)?)
                    }
                    _ => continue,
                };

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
        }
        Ok(results)
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
