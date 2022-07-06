// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::event_handler::EVENT_DISPATCH_BUFFER_SIZE;
use futures::Stream;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;
use sui_types::base_types::ObjectID;
use sui_types::error::SuiError;
use sui_types::event_filter::Filter;
use tokio::runtime::Handle;
use tokio::sync::mpsc::Sender;
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, warn};

type Subscribers<T, F> = Arc<RwLock<BTreeMap<String, (Sender<T>, F)>>>;

/// The Streamer splits a mpsc channel into multiple mpsc channels using the subscriber's `Filter<T>` object.
/// Data will be sent to the subscribers in parallel and the subscription will be dropped if it received a send error.
pub struct Streamer<T, F: Filter<T>> {
    streamer_queue: Sender<T>,
    subscribers: Subscribers<T, F>,
}

impl<T, F> Streamer<T, F>
where
    T: Clone + Debug + Send + Sync + 'static,
    F: Filter<T> + Clone + Send + Sync + 'static + Clone,
{
    pub fn spawn(buffer: usize) -> Self {
        let (tx, rx) = mpsc::channel::<T>(buffer);
        let streamer = Self {
            streamer_queue: tx,
            subscribers: Default::default(),
        };
        let mut rx = rx;
        let subscribers = streamer.subscribers.clone();
        tokio::spawn(async move {
            while let Some(data) = rx.recv().await {
                Self::send_to_all_subscribers(subscribers.clone(), data).await;
            }
        });
        streamer
    }

    async fn send_to_all_subscribers(subscribers: Subscribers<T, F>, data: T) {
        for (id, (subscriber, filter)) in subscribers.read().await.clone() {
            if !(filter.matches(&data)) {
                continue;
            }
            let data = data.clone();
            let subscribers = subscribers.clone();
            tokio::spawn(async move {
                match subscriber.send(data).await {
                    Ok(_) => {
                        debug!("Sending Move event to subscriber [{id}].")
                    }
                    Err(e) => {
                        subscribers.write().await.remove(&id);
                        warn!("Error sending event, removing subscriber [{id}] from subscriber list. Error: {e}");
                    }
                }
            });
        }
    }

    /// Subscribe to the data stream filtered by the filter object.
    pub fn subscribe(&self, filter: F) -> impl Stream<Item = T> {
        let handle = Handle::current();
        let _ = handle.enter();
        let mut subscribers = futures::executor::block_on(async { self.subscribers.write().await });
        let (tx, rx) = mpsc::channel::<T>(EVENT_DISPATCH_BUFFER_SIZE);
        subscribers.insert(ObjectID::random().to_string(), (tx, filter));
        ReceiverStream::new(rx)
    }

    pub async fn send(&self, data: T) -> Result<(), SuiError> {
        self.streamer_queue
            .send(data)
            .await
            .map_err(|e| SuiError::EventFailedToDispatch {
                error: e.to_string(),
            })
    }
}
