// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::event_handler::EVENT_DISPATCH_BUFFER_SIZE;
use futures::Stream;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;
use sui_types::base_types::ObjectID;
use sui_types::error::SuiError;
use tokio::runtime::Handle;
use tokio::sync::mpsc::Sender;
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, warn};

type Subscribers<T> = BTreeMap<String, Sender<T>>;
type FilterMap<F, T> = Arc<RwLock<BTreeMap<F, T>>>;
pub struct Streamer<T, F: Filter<T>> {
    streamer_queue: Sender<T>,
    subscribers: FilterMap<F, Subscribers<T>>,
}

impl<T, F> Streamer<T, F>
where
    T: Clone + Debug + Send + Sync + 'static,
    F: Filter<T> + Ord + Clone + Send + Sync + 'static + Clone,
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

    async fn send_to_all_subscribers(filter_map: FilterMap<F, Subscribers<T>>, data: T) {
        for (filter, subscribers) in filter_map.clone().read().await.clone() {
            if !(filter.matches(&data)) {
                continue;
            }
            for (id, subscriber) in subscribers {
                let data = data.clone();
                let filter_map = filter_map.clone();
                let filter = filter.clone();
                tokio::spawn(async move {
                    match subscriber.send(data).await {
                        Ok(_) => {
                            debug!("Sending Move event to peer [{id}].")
                        }
                        Err(e) => {
                            if let Some(subscribers) =
                                filter_map.clone().write().await.get_mut(&filter)
                            {
                                subscribers.remove(&id);
                            }
                            warn!("Error sending event, removing peer [{id}] from subscriber list. Error: {e}");
                        }
                    }
                });
            }
        }
    }

    pub fn subscribe(&self, filter: F) -> impl Stream<Item = T> {
        let handle = Handle::current();
        let _ = handle.enter();
        let mut subscribers = futures::executor::block_on(async { self.subscribers.write().await });
        let senders = subscribers.entry(filter).or_default();
        let (tx, rx) = mpsc::channel::<T>(EVENT_DISPATCH_BUFFER_SIZE);
        senders.insert(ObjectID::random().to_string(), tx);
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

pub trait Filter<T> {
    fn matches(&self, item: &T) -> bool;
}
