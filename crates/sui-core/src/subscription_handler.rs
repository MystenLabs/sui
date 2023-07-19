// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prometheus::{
    register_int_counter_vec_with_registry, register_int_gauge_vec_with_registry, IntCounterVec,
    IntGaugeVec, Registry,
};
use tokio_stream::Stream;
use tracing::{error, instrument, trace};

use crate::streamer::Streamer;
use sui_json_rpc_types::{
    EffectsWithInput, EventFilter, SuiTransactionBlockEffects, SuiTransactionBlockEvents,
    TransactionFilter,
};
use sui_json_rpc_types::{SuiEvent, SuiTransactionBlockEffectsAPI};
use sui_types::error::SuiResult;
use sui_types::transaction::TransactionData;

#[cfg(test)]
#[path = "unit_tests/subscription_handler_tests.rs"]
mod subscription_handler_tests;

pub const EVENT_DISPATCH_BUFFER_SIZE: usize = 1000;

pub struct SubscriptionMetrics {
    pub streaming_success: IntCounterVec,
    pub streaming_failure: IntCounterVec,
    pub streaming_active_subscriber_number: IntGaugeVec,
}

impl SubscriptionMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            streaming_success: register_int_counter_vec_with_registry!(
                "streaming_success",
                "Total number of items that are streamed successfully",
                &["type"],
                registry,
            )
            .unwrap(),
            streaming_failure: register_int_counter_vec_with_registry!(
                "streaming_failure",
                "Total number of items that fail to be streamed",
                &["type"],
                registry,
            )
            .unwrap(),
            streaming_active_subscriber_number: register_int_gauge_vec_with_registry!(
                "streaming_active_subscriber_number",
                "Current number of active subscribers",
                &["type"],
                registry,
            )
            .unwrap(),
        }
    }
}

pub struct SubscriptionHandler {
    event_streamer: Streamer<SuiEvent, SuiEvent, EventFilter>,
    transaction_streamer: Streamer<EffectsWithInput, SuiTransactionBlockEffects, TransactionFilter>,
}

impl SubscriptionHandler {
    pub fn new(registry: &Registry) -> Self {
        let metrics = Arc::new(SubscriptionMetrics::new(registry));
        Self {
            event_streamer: Streamer::spawn(EVENT_DISPATCH_BUFFER_SIZE, metrics.clone(), "event"),
            transaction_streamer: Streamer::spawn(EVENT_DISPATCH_BUFFER_SIZE, metrics, "tx"),
        }
    }
}

impl SubscriptionHandler {
    #[instrument(level = "debug", skip_all, fields(tx_digest = ? effects.transaction_digest()), err)]
    pub async fn process_tx(
        &self,
        input: &TransactionData,
        effects: &SuiTransactionBlockEffects,
        events: &SuiTransactionBlockEvents,
    ) -> SuiResult {
        trace!(
            num_events = events.data.len(),
            tx_digest =? effects.transaction_digest(),
            "Processing tx/event subscription"
        );

        if let Err(e) = self
            .transaction_streamer
            .send(EffectsWithInput {
                input: input.clone(),
                effects: effects.clone(),
            })
            .await
        {
            error!(error =? e, "Failed to send transaction to dispatch");
        }

        // serially dispatch event processing to honor events' orders.
        for event in events.data.clone() {
            if let Err(e) = self.event_streamer.send(event).await {
                error!(error =? e, "Failed to send event to dispatch");
            }
        }
        Ok(())
    }

    pub fn subscribe_events(&self, filter: EventFilter) -> impl Stream<Item = SuiEvent> {
        self.event_streamer.subscribe(filter)
    }

    pub fn subscribe_transactions(
        &self,
        filter: TransactionFilter,
    ) -> impl Stream<Item = SuiTransactionBlockEffects> {
        self.transaction_streamer.subscribe(filter)
    }
}
