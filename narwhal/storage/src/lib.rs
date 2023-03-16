// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod certificate_store;
mod header_store;
mod node_store;
mod payload_store;
mod proposer_store;
mod vote_digest_store;

pub use certificate_store::*;
use dashmap::DashMap;
pub use header_store::*;
pub use node_store::*;
pub use payload_store::*;
pub use proposer_store::*;
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::sync::oneshot::{Receiver, Sender};
use tracing::warn;
pub use vote_digest_store::*;

// A simple pub/sub to notify subscribers when a value becomes available.
#[derive(Clone)]
struct NotifySubscribers<K: Eq + Hash + Clone, V: Clone> {
    notify_subscribers: Arc<DashMap<K, Vec<Sender<V>>>>,
}

impl<K: Eq + Hash + Clone, V: Clone> NotifySubscribers<K, V> {
    fn new() -> Self {
        Self {
            notify_subscribers: Arc::new(DashMap::new()),
        }
    }

    // Subscribe in order to be notified once the value for the corresponding key becomes available.
    fn subscribe(&self, key: &K) -> Receiver<V> {
        let (sender, receiver) = oneshot::channel();
        self.notify_subscribers
            .entry(key.clone())
            .or_insert_with(Vec::new)
            .push(sender);
        receiver
    }

    // Notify the subscribers that are waiting on the value for the corresponding key.
    fn notify(&self, key: &K, value: &V) {
        if let Some((_, mut senders)) = self.notify_subscribers.remove(key) {
            while let Some(s) = senders.pop() {
                if s.send(value.clone()).is_err() {
                    warn!("Couldn't notify subscriber");
                }
            }
        }
    }
}
