// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The SuiSyncer module is responsible for synchronizing Events emitted
//! on Sui blockchain from concerned modules of bridge package 0x9.

use crate::{
    error::BridgeResult,
    metrics::BridgeMetrics,
    retry_with_max_elapsed_time,
    sui_client::{SuiClient, SuiClientInner},
};
use mysten_metrics::spawn_logged_monitored_task;
use std::{collections::HashMap, sync::Arc};
use sui_json_rpc_types::SuiEvent;
use sui_types::BRIDGE_PACKAGE_ID;
use sui_types::{event::EventID, Identifier};
use tokio::{
    sync::Notify,
    task::JoinHandle,
    time::{self, Duration},
};

const SUI_EVENTS_CHANNEL_SIZE: usize = 1000;

/// Map from contract address to their start cursor (exclusive)
pub type SuiTargetModules = HashMap<Identifier, Option<EventID>>;

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
                    tracing::error!("Failed to query latest checkpoint sequence number from sui client after retry");
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
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{sui_client::SuiClient, sui_mock_client::SuiMockClient};
    use prometheus::Registry;
    use sui_json_rpc_types::EventPage;
    use sui_types::{digests::TransactionDigest, event::EventID, Identifier};
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

    async fn assert_no_more_events(
        interval: Duration,
        events_rx: &mut mysten_metrics::metered_channel::Receiver<(Identifier, Vec<SuiEvent>)>,
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
}
