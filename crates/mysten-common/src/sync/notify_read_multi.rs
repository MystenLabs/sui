// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use dashmap::DashMap;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::oneshot;

pub struct NotifyReadMulti<K, V> {
    index: AtomicU64,
    pending: DashMap<K, HashMap<u64, Arc<Waiter<K, V>>>>,
}

/// A waiter for a set of keys.
/// Only when all keys are notified, the waiter is notified.
struct Waiter<K, V> {
    inner: Mutex<WaiterInner<K, V>>,
}

struct WaiterInner<K, V> {
    missing_keys: HashSet<K>,
    ready_values: Option<HashMap<K, V>>,
    notify: Option<oneshot::Sender<HashMap<K, V>>>,
}

/// Registration resolves to the value but also provides safe cancellation
/// When Registration is dropped before it is resolved, we de-register from the pending list
pub struct Registration<'a, K: Eq + Hash + Clone, V: Clone> {
    this: &'a NotifyReadMulti<K, V>,
    index: u64,
    waiter: Arc<Waiter<K, V>>,
    receiver: Option<oneshot::Receiver<HashMap<K, V>>>,
}

impl<K: Eq + Hash + Clone, V: Clone> Waiter<K, V> {
    fn new(missing_keys: HashSet<K>, notify: oneshot::Sender<HashMap<K, V>>) -> Self {
        let notify = if missing_keys.is_empty() {
            notify.send(HashMap::new()).ok();
            None
        } else {
            Some(notify)
        };
        Self {
            inner: Mutex::new(WaiterInner {
                missing_keys,
                ready_values: Some(HashMap::new()),
                notify,
            }),
        }
    }

    fn notify(&self, key: K, value: V) {
        let mut inner = self.inner.lock();
        let Some(mut values) = inner.ready_values.take() else {
            return;
        };
        inner.missing_keys.remove(&key);
        values.insert(key, value);
        if inner.missing_keys.is_empty() {
            let notify = inner.notify.take().unwrap();
            notify.send(values).ok();
        } else {
            inner.ready_values = Some(values);
        }
    }

    fn notify_multi(&self, key_values: impl IntoIterator<Item = (K, V)>) {
        let mut inner = self.inner.lock();
        let Some(mut values) = inner.ready_values.take() else {
            return;
        };
        for (key, value) in key_values {
            inner.missing_keys.remove(&key);
            values.insert(key, value);
        }
        if inner.missing_keys.is_empty() {
            let notify = inner.notify.take().unwrap();
            notify.send(values).ok();
        } else {
            inner.ready_values = Some(values);
        }
    }

    fn get_missing_keys(&self) -> HashSet<K> {
        self.inner.lock().missing_keys.clone()
    }
}

impl<K: Eq + Hash + Clone, V: Clone> NotifyReadMulti<K, V> {
    pub fn new() -> Self {
        Self {
            index: AtomicU64::new(0),
            pending: DashMap::new(),
        }
    }

    pub fn notify(&self, key: &K, value: &V) {
        let Some((_, waiters)) = self.pending.remove(key) else {
            return;
        };
        for (_, waiter) in waiters {
            waiter.notify(key.clone(), value.clone());
        }
    }

    pub fn register(&self, keys: &[K]) -> Registration<K, V> {
        let index = self.index.fetch_add(1, Ordering::Relaxed);
        let unique_keys = keys.iter().cloned().collect::<HashSet<_>>();
        let (sender, receiver) = oneshot::channel();
        let waiter = Arc::new(Waiter::new(unique_keys.clone(), sender));
        for key in &unique_keys {
            self.pending
                .entry(key.clone())
                .or_default()
                .insert(index, waiter.clone());
        }
        Registration {
            this: self,
            index,
            waiter,
            receiver: Some(receiver),
        }
    }

    fn deregister<'a>(&'a self, index: u64, keys: impl Iterator<Item = &'a K>) {
        for key in keys {
            if let Some(mut waiters) = self.pending.get_mut(key) {
                waiters.remove(&index);
            }
            self.pending.remove_if(key, |_, waiters| waiters.is_empty());
        }
    }
}

impl<K: Eq + Hash + Clone + Unpin, V: Clone + Unpin> NotifyReadMulti<K, V> {
    pub async fn read(
        &self,
        keys: &[K],
        fetch: impl FnOnce(&[K]) -> Vec<Option<V>>,
    ) -> HashMap<K, V> {
        let registration = self.register(keys);

        let results = fetch(keys);

        let available_keys = keys
            .iter()
            .zip(results)
            .filter_map(|(k, v)| v.map(|v| (k.clone(), v)))
            .collect::<Vec<_>>();
        self.deregister(registration.index, available_keys.iter().map(|(k, _)| k));
        registration.waiter.notify_multi(available_keys.into_iter());

        registration.await
    }
}

impl<K: Eq + Hash + Clone + Unpin, V: Clone + Unpin> Future for Registration<'_, K, V> {
    type Output = HashMap<K, V>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let receiver = self
            .receiver
            .as_mut()
            .expect("poll can not be called after drop");
        let poll = Pin::new(receiver).poll(cx);
        poll.map(|r| r.expect("Sender never drops when registration is pending"))
    }
}

impl<K: Eq + Hash + Clone, V: Clone> Drop for Registration<'_, K, V> {
    fn drop(&mut self) {
        let missing_keys = self.waiter.get_missing_keys();
        self.this.deregister(self.index, missing_keys.iter());
    }
}

impl<K: Eq + Hash + Clone, V: Clone> Default for NotifyReadMulti<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_notify() {
        let notifier = NotifyReadMulti::<String, i32>::new();
        let keys = vec!["key1".to_string(), "key2".to_string()];

        // Start registration before any values are available
        let registration = notifier.register(&keys);

        // Notify values one by one
        notifier.notify(&"key1".to_string(), &1);
        notifier.notify(&"key2".to_string(), &2);

        let result = registration.await;
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("key1"), Some(&1));
        assert_eq!(result.get("key2"), Some(&2));
    }

    #[tokio::test]
    async fn test_notify_before_registration() {
        let notifier = NotifyReadMulti::<String, i32>::new();
        let keys = vec!["key1".to_string()];

        // Notify before registration
        notifier.notify(&"key1".to_string(), &1);

        // Registration should still wait
        let registration = notifier.register(&keys);
        assert!(!registration.waiter.get_missing_keys().is_empty());

        // Now notify and wait for completion
        notifier.notify(&"key1".to_string(), &1);
        let result = registration.await;

        assert_eq!(result.len(), 1);
        assert_eq!(result.get("key1"), Some(&1));
        assert!(notifier.pending.is_empty());
    }

    #[tokio::test]
    async fn test_read_with_fetch_immediate() {
        let notifier = NotifyReadMulti::<String, i32>::new();
        let keys = vec!["key1".to_string(), "key2".to_string()];

        let result = notifier
            .read(&keys, |keys| {
                // All keys are immediately available
                keys.iter()
                    .map(|k| match k.as_str() {
                        "key1" => Some(1),
                        "key2" => Some(2),
                        _ => None,
                    })
                    .collect()
            })
            .await;

        assert_eq!(result.len(), 2);
        assert_eq!(result.get("key1"), Some(&1));
        assert_eq!(result.get("key2"), Some(&2));
        assert!(notifier.pending.is_empty());
    }

    #[tokio::test]
    async fn test_duplicate_keys() {
        let notifier = NotifyReadMulti::<String, i32>::new();
        let keys = vec![
            "key1".to_string(),
            "key1".to_string(), // Duplicate
            "key2".to_string(),
        ];

        let registration = notifier.register(&keys);

        notifier.notify(&"key1".to_string(), &1);
        notifier.notify(&"key2".to_string(), &2);

        let result = registration.await;
        assert_eq!(result.len(), 2); // Should only have 2 unique keys
        assert_eq!(result.get("key1"), Some(&1));
        assert_eq!(result.get("key2"), Some(&2));
    }

    #[tokio::test]
    async fn test_notify_multiple_waiters() {
        let notifier = NotifyReadMulti::<String, i32>::new();
        let keys1 = vec!["key1".to_string(), "key2".to_string()];
        let keys2 = vec!["key2".to_string(), "key3".to_string()];

        let registration1 = notifier.register(&keys1);
        let registration2 = notifier.register(&keys2);

        notifier.notify(&"key1".to_string(), &1);
        notifier.notify(&"key2".to_string(), &2);
        notifier.notify(&"key3".to_string(), &3);

        let result1 = registration1.await;
        let result2 = registration2.await;

        assert_eq!(result1.len(), 2);
        assert_eq!(result1.get("key1"), Some(&1));
        assert_eq!(result1.get("key2"), Some(&2));

        assert_eq!(result2.len(), 2);
        assert_eq!(result2.get("key2"), Some(&2));
        assert_eq!(result2.get("key3"), Some(&3));

        assert!(notifier.pending.is_empty());
    }

    #[tokio::test]
    async fn test_registration_drop() {
        let notifier = NotifyReadMulti::<String, i32>::new();
        let keys = vec!["key1".to_string(), "key2".to_string()];

        // Create and immediately drop a registration
        {
            let _registration = notifier.register(&keys);
        }

        // Verify the pending map is empty
        assert!(notifier.pending.is_empty());
    }

    #[tokio::test]
    async fn test_empty_keys() {
        let notifier = NotifyReadMulti::<String, i32>::new();
        let keys: Vec<String> = vec![];

        let registration = notifier.register(&keys);
        let result = registration.await;

        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_notify_after_completion() {
        let notifier = NotifyReadMulti::<String, i32>::new();
        let keys = vec!["key1".to_string()];

        let registration = notifier.register(&keys);

        // Complete the registration
        notifier.notify(&"key1".to_string(), &1);
        let result = registration.await;
        assert_eq!(result.get("key1"), Some(&1));

        // Additional notify should have no effect
        notifier.notify(&"key1".to_string(), &2);

        // Verify the pending map is empty
        assert!(notifier.pending.is_empty());
    }
}
