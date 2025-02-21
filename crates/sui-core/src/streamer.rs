// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::subscription_handler::{SubscriptionMetrics, EVENT_DISPATCH_BUFFER_SIZE};
use futures::Stream;
use mysten_metrics::metered_channel::Sender;
use mysten_metrics::spawn_monitored_task;
use parking_lot::RwLock;
use prometheus::Registry;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;
use sui_json_rpc_types::Filter;
use sui_types::base_types::ObjectID;
use sui_types::error::SuiError;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, warn};

type Subscribers<T, F> = Arc<RwLock<BTreeMap<String, (tokio::sync::mpsc::Sender<T>, F)>>>;

/// The Streamer splits a mpsc channel into multiple mpsc channels using the subscriber's `Filter<T>` object.
/// Data will be sent to the subscribers in parallel and the subscription will be dropped if it received a send error.
pub struct Streamer<T, S, F: Filter<T>> {
    streamer_queue: Sender<T>,
    subscribers: Subscribers<S, F>,
    metrics: Arc<SubscriptionMetrics>,
    metrics_label: &'static str,
}

impl<T, S, F> Streamer<T, S, F>
where
    S: From<T> + Clone + Debug + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
    F: Filter<T> + Clone + Send + Sync + 'static + Clone,
{
    pub fn spawn(
        buffer: usize,
        metrics: Arc<SubscriptionMetrics>,
        metrics_label: &'static str,
    ) -> Self {
        let channel_label = format!("streamer_{}", metrics_label);
        let gauge = if let Some(metrics) = mysten_metrics::get_metrics() {
            metrics
                .channel_inflight
                .with_label_values(&[&channel_label])
        } else {
            // We call init_metrics very early when starting a node. Therefore when this happens,
            // it's probably in a test.
            mysten_metrics::init_metrics(&Registry::default());
            mysten_metrics::get_metrics()
                .unwrap()
                .channel_inflight
                .with_label_values(&[&channel_label])
        };

        let (tx, rx) = mysten_metrics::metered_channel::channel(buffer, &gauge);
        let streamer = Self {
            streamer_queue: tx,
            subscribers: Default::default(),
            metrics: metrics.clone(),
            metrics_label,
        };
        let mut rx = rx;
        let subscribers = streamer.subscribers.clone();
        spawn_monitored_task!(async move {
            while let Some(data) = rx.recv().await {
                Self::send_to_all_subscribers(
                    subscribers.clone(),
                    data,
                    metrics.clone(),
                    metrics_label,
                )
                .await;
            }
        });
        streamer
    }

    async fn send_to_all_subscribers(
        subscribers: Subscribers<S, F>,
        data: T,
        metrics: Arc<SubscriptionMetrics>,
        metrics_label: &'static str,
    ) {
        let success_counter = metrics
            .streaming_success
            .with_label_values(&[metrics_label]);
        let failure_counter = metrics
            .streaming_failure
            .with_label_values(&[metrics_label]);
        let subscriber_count = metrics
            .streaming_active_subscriber_number
            .with_label_values(&[metrics_label]);

        let to_remove = {
            let mut to_remove = vec![];
            let subscribers_snapshot = subscribers.read();
            subscriber_count.set(subscribers_snapshot.len() as i64);

            for (id, (subscriber, filter)) in subscribers_snapshot.iter() {
                if !(filter.matches(&data)) {
                    continue;
                }
                let data = data.clone();
                match subscriber.try_send(data.into()) {
                    Ok(_) => {
                        debug!(subscription_id = id, "Streaming data to subscriber.");
                        success_counter.inc();
                    }
                    Err(e) => {
                        warn!(
                            subscription_id = id,
                            "Error when streaming data, removing subscriber. Error: {e}"
                        );
                        // It does not matter what the error is - channel full or closed, we remove the subscriber.
                        // In the case of a full channel, this nudges the subscriber to catch up separately and not
                        // miss any data.
                        to_remove.push(id.clone());
                        failure_counter.inc();
                    }
                }
            }
            to_remove
        };
        if !to_remove.is_empty() {
            let mut subscribers = subscribers.write();
            for sub in to_remove {
                subscribers.remove(&sub);
            }
        }
    }

    /// Subscribe to the data stream filtered by the filter object.
    pub fn subscribe(&self, filter: F) -> impl Stream<Item = S> {
        let (tx, rx) = mpsc::channel::<S>(EVENT_DISPATCH_BUFFER_SIZE);
        self.subscribers
            .write()
            .insert(ObjectID::random().to_string(), (tx, filter));
        ReceiverStream::new(rx)
    }

    pub fn try_send(&self, data: T) -> Result<(), SuiError> {
        self.streamer_queue.try_send(data).map_err(|e| {
            self.metrics
                .dropped_submissions
                .with_label_values(&[self.metrics_label])
                .inc();

            SuiError::FailedToDispatchSubscription {
                error: e.to_string(),
            }
        })
    }
}
