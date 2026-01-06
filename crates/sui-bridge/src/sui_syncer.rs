// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The SuiSyncer module is responsible for synchronizing Events emitted
//! on Sui blockchain from concerned modules of bridge package 0x9.
//!
//! There are two modes of operation:
//! - Event-based (legacy): Uses JSON-RPC to query events by module
//! - gRPC-based (new): Iterates over bridge records using LinkedTable iteration
//!
//! As of now, only the event-based mode is being used.

use crate::{
    error::BridgeResult,
    events::{EmittedSuiToEthTokenBridgeV1, SuiBridgeEvent},
    metrics::BridgeMetrics,
    retry_with_max_elapsed_time,
    sui_client::{SuiClient, SuiClientInner},
    types::BridgeAction,
};
use mysten_metrics::spawn_logged_monitored_task;
use std::{collections::HashMap, sync::Arc};
use sui_json_rpc_types::SuiEvent;
use sui_types::BRIDGE_PACKAGE_ID;
use sui_types::{Identifier, event::EventID};
use tokio::{
    sync::Notify,
    task::JoinHandle,
    time::{self, Duration},
};

const SUI_EVENTS_CHANNEL_SIZE: usize = 1000;

/// Map from contract address to their start cursor (exclusive)
pub type SuiTargetModules = HashMap<Identifier, Option<EventID>>;

pub type GrpcSyncedEvents = (u64, Vec<SuiBridgeEvent>);

pub struct SuiSyncer<C> {
    sui_client: Arc<SuiClient<C>>,
    // The last transaction that the syncer has fully processed.
    // Syncer will resume post this transaction (i.e. exclusive), when it starts.
    cursors: SuiTargetModules,
    metrics: Arc<BridgeMetrics>,
}

impl<C> SuiSyncer<C>
where
    C: SuiClientInner + 'static,
{
    pub fn new(
        sui_client: Arc<SuiClient<C>>,
        cursors: SuiTargetModules,
        metrics: Arc<BridgeMetrics>,
    ) -> Self {
        Self {
            sui_client,
            cursors,
            metrics,
        }
    }

    pub async fn run(
        self,
        query_interval: Duration,
    ) -> BridgeResult<(
        Vec<JoinHandle<()>>,
        mysten_metrics::metered_channel::Receiver<(Identifier, Vec<SuiEvent>)>,
    )> {
        let (events_tx, events_rx) = mysten_metrics::metered_channel::channel(
            SUI_EVENTS_CHANNEL_SIZE,
            &mysten_metrics::get_metrics()
                .unwrap()
                .channel_inflight
                .with_label_values(&["sui_events_queue"]),
        );

        let mut task_handles = vec![];
        for (module, cursor) in self.cursors {
            let metrics = self.metrics.clone();
            let events_rx_clone: mysten_metrics::metered_channel::Sender<(
                Identifier,
                Vec<SuiEvent>,
            )> = events_tx.clone();
            let sui_client_clone = self.sui_client.clone();
            task_handles.push(spawn_logged_monitored_task!(
                Self::run_event_listening_task(
                    module,
                    cursor,
                    events_rx_clone,
                    sui_client_clone,
                    query_interval,
                    metrics,
                )
            ));
        }
        Ok((task_handles, events_rx))
    }

    async fn run_event_listening_task(
        // The module where interested events are defined.
        // Module is always of bridge package 0x9.
        module: Identifier,
        mut cursor: Option<EventID>,
        events_sender: mysten_metrics::metered_channel::Sender<(Identifier, Vec<SuiEvent>)>,
        sui_client: Arc<SuiClient<C>>,
        query_interval: Duration,
        metrics: Arc<BridgeMetrics>,
    ) {
        tracing::info!(?module, ?cursor, "Starting sui events listening task");
        let mut interval = time::interval(query_interval);
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        // Create a task to update metrics
        let notify = Arc::new(Notify::new());
        let notify_clone = notify.clone();
        let sui_client_clone = sui_client.clone();
        let last_synced_sui_checkpoints_metric = metrics
            .last_synced_sui_checkpoints
            .with_label_values(&[&module.to_string()]);
        spawn_logged_monitored_task!(async move {
            loop {
                notify_clone.notified().await;
                let Ok(Ok(latest_checkpoint_sequence_number)) = retry_with_max_elapsed_time!(
                    sui_client_clone.get_latest_checkpoint_sequence_number(),
                    Duration::from_secs(120)
                ) else {
                    tracing::error!(
                        "Failed to query latest checkpoint sequence number from sui client after retry"
                    );
                    continue;
                };
                last_synced_sui_checkpoints_metric.set(latest_checkpoint_sequence_number as i64);
            }
        });

        loop {
            interval.tick().await;
            let Ok(Ok(events)) = retry_with_max_elapsed_time!(
                sui_client.query_events_by_module(BRIDGE_PACKAGE_ID, module.clone(), cursor),
                Duration::from_secs(120)
            ) else {
                tracing::error!("Failed to query events from sui client after retry");
                continue;
            };

            let len = events.data.len();
            if len != 0 {
                if !events.has_next_page {
                    // If this is the last page, it means we have processed all events up to the latest checkpoint
                    // We can then update the latest checkpoint metric.
                    notify.notify_one();
                }
                events_sender
                    .send((module.clone(), events.data))
                    .await
                    .expect("All Sui event channel receivers are closed");
                if let Some(next) = events.next_cursor {
                    cursor = Some(next);
                }
                tracing::info!(?module, ?cursor, "Observed {len} new Sui events");
            }
        }
    }

    pub async fn run_grpc(
        self,
        source_chain_id: u8,
        next_sequence_number: u64,
        query_interval: Duration,
        batch_size: u64,
    ) -> BridgeResult<(
        Vec<JoinHandle<()>>,
        mysten_metrics::metered_channel::Receiver<GrpcSyncedEvents>,
    )> {
        let (events_tx, events_rx) = mysten_metrics::metered_channel::channel(
            SUI_EVENTS_CHANNEL_SIZE,
            &mysten_metrics::get_metrics()
                .unwrap()
                .channel_inflight
                .with_label_values(&["sui_grpc_events_queue"]),
        );

        let task_handle = spawn_logged_monitored_task!(Self::run_grpc_listening_task(
            source_chain_id,
            next_sequence_number,
            events_tx,
            self.sui_client.clone(),
            query_interval,
            batch_size,
            self.metrics.clone(),
        ));

        Ok((vec![task_handle], events_rx))
    }

    async fn run_grpc_listening_task(
        source_chain_id: u8,
        mut next_sequence_cursor: u64,
        events_sender: mysten_metrics::metered_channel::Sender<GrpcSyncedEvents>,
        sui_client: Arc<SuiClient<C>>,
        query_interval: Duration,
        batch_size: u64,
        metrics: Arc<BridgeMetrics>,
    ) {
        tracing::info!(
            source_chain_id,
            next_sequence_cursor,
            "Starting sui grpc records listening task"
        );
        let mut interval = time::interval(query_interval);
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        // Create a task to update metrics
        let notify = Arc::new(Notify::new());
        let notify_clone = notify.clone();
        let sui_client_clone = sui_client.clone();
        let chain_label = source_chain_id.to_string();
        let last_synced_sui_checkpoints_metric = metrics
            .last_synced_sui_checkpoints
            .with_label_values(&[&chain_label]);
        spawn_logged_monitored_task!(async move {
            loop {
                notify_clone.notified().await;
                let Ok(Ok(latest_checkpoint_sequence_number)) = retry_with_max_elapsed_time!(
                    sui_client_clone.get_latest_checkpoint_sequence_number(),
                    Duration::from_secs(120)
                ) else {
                    tracing::error!(
                        "Failed to query latest checkpoint sequence number from sui client after retry"
                    );
                    continue;
                };
                last_synced_sui_checkpoints_metric.set(latest_checkpoint_sequence_number as i64);
            }
        });

        loop {
            interval.tick().await;
            let Ok(Ok(on_chain_next_sequence_index)) = retry_with_max_elapsed_time!(
                sui_client.get_token_transfer_next_seq_number(source_chain_id),
                Duration::from_secs(120)
            ) else {
                tracing::error!(
                    source_chain_id,
                    "Failed to get next seq num from sui client after retry"
                );
                continue;
            };

            // start querying from the next_sequence_cursor till on_chain_next_sequence_index in batches
            let start_index = next_sequence_cursor;
            if start_index >= on_chain_next_sequence_index {
                notify.notify_one();
                continue;
            }

            let end_index = std::cmp::min(
                start_index + batch_size - 1,
                on_chain_next_sequence_index - 1,
            );

            let Ok(Ok(records)) = retry_with_max_elapsed_time!(
                sui_client.get_bridge_records_in_range(source_chain_id, start_index, end_index),
                Duration::from_secs(120)
            ) else {
                tracing::error!(
                    source_chain_id,
                    start_index,
                    end_index,
                    "Failed to get records from sui client after retry"
                );
                continue;
            };

            let len = records.len();
            if len != 0 {
                let mut events = Vec::with_capacity(len);
                let mut batch_last_sequence_index = start_index;

                for (seq_index, record) in records {
                    let event = match Self::bridge_record_to_event(&record, source_chain_id) {
                        Ok(event) => event,
                        Err(e) => {
                            tracing::error!(
                                source_chain_id,
                                seq_index,
                                "Failed to convert record to event: {:?}",
                                e
                            );
                            continue;
                        }
                    };

                    events.push(event);
                    batch_last_sequence_index = seq_index;
                }

                if !events.is_empty() {
                    events_sender
                        .send((batch_last_sequence_index + 1, events))
                        .await
                        .expect("Bridge events channel receiver is closed");

                    next_sequence_cursor = batch_last_sequence_index + 1;
                    tracing::info!(
                        source_chain_id,
                        last_processed_seq = batch_last_sequence_index,
                        next_sequence_cursor,
                        "Processed {len} bridge records"
                    );
                }
            }

            if end_index >= on_chain_next_sequence_index - 1 {
                // we have processed all records up to the latest checkpoint
                // so we can update the latest checkpoint metric
                notify.notify_one();
            }
        }
    }

    fn bridge_record_to_event(
        record: &sui_types::bridge::MoveTypeBridgeRecord,
        source_chain_id: u8,
    ) -> Result<SuiBridgeEvent, crate::error::BridgeError> {
        let action = BridgeAction::try_from_bridge_record(record)?;

        match action {
            BridgeAction::SuiToEthTokenTransfer(transfer) => Ok(
                SuiBridgeEvent::SuiToEthTokenBridgeV1(EmittedSuiToEthTokenBridgeV1 {
                    nonce: transfer.nonce,
                    sui_chain_id: transfer.sui_chain_id,
                    eth_chain_id: transfer.eth_chain_id,
                    sui_address: transfer.sui_address,
                    eth_address: transfer.eth_address,
                    token_id: transfer.token_id,
                    amount_sui_adjusted: transfer.amount_adjusted,
                }),
            ),
            _ => Err(crate::error::BridgeError::Generic(format!(
                "Unexpected action type for source_chain_id {}: {:?}",
                source_chain_id, action
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{sui_client::SuiClient, sui_mock_client::SuiMockClient};
    use prometheus::Registry;
    use sui_json_rpc_types::EventPage;
    use sui_types::bridge::{BridgeChainId, MoveTypeBridgeMessage, MoveTypeBridgeRecord};
    use sui_types::{Identifier, digests::TransactionDigest, event::EventID};
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_sui_syncer_basic() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);
        let metrics = Arc::new(BridgeMetrics::new(&registry));
        let mock = SuiMockClient::default();
        let client = Arc::new(SuiClient::new_for_testing(mock.clone()));
        let module_foo = Identifier::new("Foo").unwrap();
        let module_bar = Identifier::new("Bar").unwrap();
        let empty_events = EventPage::empty();
        let cursor = EventID {
            tx_digest: TransactionDigest::random(),
            event_seq: 0,
        };
        add_event_response(&mock, module_foo.clone(), cursor, empty_events.clone());
        add_event_response(&mock, module_bar.clone(), cursor, empty_events.clone());

        let target_modules = HashMap::from_iter(vec![
            (module_foo.clone(), Some(cursor)),
            (module_bar.clone(), Some(cursor)),
        ]);
        let interval = Duration::from_millis(200);
        let (_handles, mut events_rx) = SuiSyncer::new(client, target_modules, metrics.clone())
            .run(interval)
            .await
            .unwrap();

        // Initially there are no events
        assert_no_more_events(interval, &mut events_rx).await;

        mock.set_latest_checkpoint_sequence_number(999);
        // Module Foo has new events
        let mut event_1: SuiEvent = SuiEvent::random_for_testing();
        let package_id = BRIDGE_PACKAGE_ID;
        event_1.type_.address = package_id.into();
        event_1.type_.module = module_foo.clone();
        let module_foo_events_1: sui_json_rpc_types::Page<SuiEvent, EventID> = EventPage {
            data: vec![event_1.clone(), event_1.clone()],
            next_cursor: Some(event_1.id),
            has_next_page: false,
        };
        add_event_response(&mock, module_foo.clone(), event_1.id, empty_events.clone());
        add_event_response(
            &mock,
            module_foo.clone(),
            cursor,
            module_foo_events_1.clone(),
        );

        let (identifier, received_events) = events_rx.recv().await.unwrap();
        assert_eq!(identifier, module_foo);
        assert_eq!(received_events.len(), 2);
        assert_eq!(received_events[0].id, event_1.id);
        assert_eq!(received_events[1].id, event_1.id);
        // No more
        assert_no_more_events(interval, &mut events_rx).await;
        assert_eq!(
            metrics
                .last_synced_sui_checkpoints
                .get_metric_with_label_values(&["Foo"])
                .unwrap()
                .get(),
            999
        );

        // Module Bar has new events
        let mut event_2: SuiEvent = SuiEvent::random_for_testing();
        event_2.type_.address = package_id.into();
        event_2.type_.module = module_bar.clone();
        let module_bar_events_1 = EventPage {
            data: vec![event_2.clone()],
            next_cursor: Some(event_2.id),
            has_next_page: true, // Set to true so that the syncer will not update the last synced checkpoint
        };
        add_event_response(&mock, module_bar.clone(), event_2.id, empty_events.clone());

        add_event_response(&mock, module_bar.clone(), cursor, module_bar_events_1);

        let (identifier, received_events) = events_rx.recv().await.unwrap();
        assert_eq!(identifier, module_bar);
        assert_eq!(received_events.len(), 1);
        assert_eq!(received_events[0].id, event_2.id);
        // No more
        assert_no_more_events(interval, &mut events_rx).await;
        assert_eq!(
            metrics
                .last_synced_sui_checkpoints
                .get_metric_with_label_values(&["Bar"])
                .unwrap()
                .get(),
            0, // Not updated
        );

        Ok(())
    }

    async fn assert_no_more_events<T: std::fmt::Debug>(
        interval: Duration,
        events_rx: &mut mysten_metrics::metered_channel::Receiver<T>,
    ) {
        match timeout(interval * 2, events_rx.recv()).await {
            Err(_e) => (),
            other => panic!("Should have timed out, but got: {:?}", other),
        };
    }

    fn add_event_response(
        mock: &SuiMockClient,
        module: Identifier,
        cursor: EventID,
        events: EventPage,
    ) {
        mock.add_event_response(BRIDGE_PACKAGE_ID, module.clone(), cursor, events.clone());
    }

    /// Creates a test bridge record with valid BCS-encoded payload
    fn create_test_bridge_record(
        seq_num: u64,
        source_chain: BridgeChainId,
        target_chain: BridgeChainId,
        amount: u64,
    ) -> MoveTypeBridgeRecord {
        // Create the payload struct matching SuiToEthOnChainBcsPayload
        #[derive(serde::Serialize)]
        struct TestPayload {
            sui_address: Vec<u8>,
            target_chain: u8,
            eth_address: Vec<u8>,
            token_type: u8,
            amount: [u8; 8],
        }

        let payload = TestPayload {
            sui_address: vec![0u8; 32], // 32-byte SuiAddress
            target_chain: target_chain as u8,
            eth_address: vec![0u8; 20], // 20-byte EthAddress
            token_type: 1,              // SUI token
            amount: amount.to_be_bytes(),
        };

        let payload_bytes = bcs::to_bytes(&payload).unwrap();

        MoveTypeBridgeRecord {
            message: MoveTypeBridgeMessage {
                message_type: 0, // TokenTransfer
                message_version: 1,
                seq_num,
                source_chain: source_chain as u8,
                payload: payload_bytes,
            },
            verified_signatures: None,
            claimed: false,
        }
    }

    #[tokio::test]
    async fn test_sui_syncer_grpc_basic() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);
        let metrics = Arc::new(BridgeMetrics::new(&registry));
        let mock = SuiMockClient::default();
        let client = Arc::new(SuiClient::new_for_testing(mock.clone()));

        let source_chain_id = BridgeChainId::SuiCustom as u8;
        let target_modules = HashMap::new(); // Not used for gRPC mode

        let interval = Duration::from_millis(200);
        let batch_size = 10;
        let next_sequence_number = 0;

        // Initially, no records on chain
        mock.set_next_seq_num(source_chain_id, 0);

        let (_handles, mut events_rx) =
            SuiSyncer::new(client.clone(), target_modules.clone(), metrics.clone())
                .run_grpc(source_chain_id, next_sequence_number, interval, batch_size)
                .await
                .unwrap();

        // Initially there are no records
        assert_no_more_events(interval, &mut events_rx).await;

        mock.set_latest_checkpoint_sequence_number(1000);

        // Add some bridge records
        let record_0 =
            create_test_bridge_record(0, BridgeChainId::SuiCustom, BridgeChainId::EthCustom, 1000);
        let record_1 =
            create_test_bridge_record(1, BridgeChainId::SuiCustom, BridgeChainId::EthCustom, 2000);

        mock.add_bridge_record(source_chain_id, 0, record_0);
        mock.add_bridge_record(source_chain_id, 1, record_1);
        mock.set_next_seq_num(source_chain_id, 2); // 2 records available (0 and 1)

        let (next_cursor, received_events) = events_rx.recv().await.unwrap();
        assert_eq!(received_events.len(), 2);
        assert_eq!(next_cursor, 2); // Next sequence number to process

        match &received_events[0] {
            SuiBridgeEvent::SuiToEthTokenBridgeV1(event) => {
                assert_eq!(event.nonce, 0);
                assert_eq!(event.sui_chain_id, BridgeChainId::SuiCustom);
                assert_eq!(event.eth_chain_id, BridgeChainId::EthCustom);
                assert_eq!(event.amount_sui_adjusted, 1000);
            }
            _ => panic!("Expected SuiToEthTokenBridgeV1 event"),
        }
        match &received_events[1] {
            SuiBridgeEvent::SuiToEthTokenBridgeV1(event) => {
                assert_eq!(event.nonce, 1);
                assert_eq!(event.amount_sui_adjusted, 2000);
            }
            _ => panic!("Expected SuiToEthTokenBridgeV1 event"),
        }

        // No more events should be received
        assert_no_more_events(interval, &mut events_rx).await;
        assert_eq!(
            metrics
                .last_synced_sui_checkpoints
                .get_metric_with_label_values(&[&source_chain_id.to_string()])
                .unwrap()
                .get(),
            1000
        );

        Ok(())
    }
}
