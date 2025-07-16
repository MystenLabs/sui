// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::{join_all, Either};
use parking_lot::Mutex;
use parking_lot::MutexGuard;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::collections::HashSet;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::mem;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use tokio::time::interval;
use tracing::warn;

type Registrations<V> = Vec<oneshot::Sender<V>>;

/// Interval duration for logging waiting keys when reads take too long
const LONG_WAIT_LOG_INTERVAL_SECS: u64 = 10;

pub struct NotifyRead<K, V> {
    pending: Vec<Mutex<HashMap<K, Registrations<V>>>>,
    count_pending: AtomicUsize,
}

impl<K: Eq + Hash + Clone, V: Clone> NotifyRead<K, V> {
    pub fn new() -> Self {
        let pending = (0..255).map(|_| Default::default()).collect();
        let count_pending = Default::default();
        Self {
            pending,
            count_pending,
        }
    }

    /// Asynchronously notifies waiters and return number of remaining pending registration
    pub fn notify(&self, key: &K, value: &V) -> usize {
        let registrations = self.pending(key).remove(key);
        let Some(registrations) = registrations else {
            return self.count_pending.load(Ordering::Relaxed);
        };
        let rem = self
            .count_pending
            .fetch_sub(registrations.len(), Ordering::Relaxed);
        for registration in registrations {
            registration.send(value.clone()).ok();
        }
        rem
    }

    pub fn register_one(&self, key: &K) -> Registration<K, V> {
        self.count_pending.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        self.register(key, sender);
        Registration {
            this: self,
            registration: Some((key.clone(), receiver)),
        }
    }

    pub fn register_all(&self, keys: &[K]) -> Vec<Registration<K, V>> {
        self.count_pending.fetch_add(keys.len(), Ordering::Relaxed);
        let mut registrations = vec![];
        for key in keys.iter() {
            let (sender, receiver) = oneshot::channel();
            self.register(key, sender);
            let registration = Registration {
                this: self,
                registration: Some((key.clone(), receiver)),
            };
            registrations.push(registration);
        }
        registrations
    }

    fn register(&self, key: &K, sender: oneshot::Sender<V>) {
        self.pending(key)
            .entry(key.clone())
            .or_default()
            .push(sender);
    }

    fn pending(&self, key: &K) -> MutexGuard<HashMap<K, Registrations<V>>> {
        let mut state = DefaultHasher::new();
        key.hash(&mut state);
        let hash = state.finish();
        let pending = self
            .pending
            .get((hash % self.pending.len() as u64) as usize)
            .unwrap();
        pending.lock()
    }

    pub fn num_pending(&self) -> usize {
        self.count_pending.load(Ordering::Relaxed)
    }

    fn cleanup(&self, key: &K) {
        let mut pending = self.pending(key);
        // it is possible that registration was fulfilled before we get here
        let Some(registrations) = pending.get_mut(key) else {
            return;
        };
        let mut count_deleted = 0usize;
        registrations.retain(|s| {
            let delete = s.is_closed();
            if delete {
                count_deleted += 1;
            }
            !delete
        });
        self.count_pending
            .fetch_sub(count_deleted, Ordering::Relaxed);
        if registrations.is_empty() {
            pending.remove(key);
        }
    }
}

impl<K: Eq + Hash + Clone + Unpin + std::fmt::Debug + Send + Sync + 'static, V: Clone + Unpin>
    NotifyRead<K, V>
{
    pub async fn read(
        &self,
        task_name: &str,
        keys: &[K],
        fetch: impl FnOnce(&[K]) -> Vec<Option<V>>,
    ) -> Vec<V> {
        let registrations = self.register_all(keys);

        let results = fetch(keys);

        // Track which keys are still waiting
        let waiting_keys: Arc<Mutex<HashSet<K>>> = Arc::new(Mutex::new(
            keys.iter()
                .zip(results.iter())
                .filter(|&(_key, result)| result.is_none())
                .map(|(key, _result)| key.clone())
                .collect(),
        ));

        // Spawn logging task if there are waiting keys
        let log_handle = if !waiting_keys.lock().is_empty() {
            let waiting_keys_clone = Arc::clone(&waiting_keys);
            let start_time = Instant::now();
            let task_name = task_name.to_string();

            Some(tokio::spawn(async move {
                let mut interval = interval(Duration::from_secs(LONG_WAIT_LOG_INTERVAL_SECS));
                interval.tick().await; // Skip first immediate tick

                loop {
                    interval.tick().await;
                    let current_waiting = waiting_keys_clone.lock();
                    if current_waiting.is_empty() {
                        break;
                    }
                    let keys_vec: Vec<_> = current_waiting.iter().cloned().collect();
                    drop(current_waiting); // Release lock before logging

                    let elapsed_secs = start_time.elapsed().as_secs();

                    warn!(
                        "[{}] Still waiting for {}s for {} keys: {:?}",
                        task_name,
                        elapsed_secs,
                        keys_vec.len(),
                        keys_vec
                    );

                    if task_name == "CheckpointBuilder::resolve_checkpoint_transactions" {
                        debug_assert!(
                            elapsed_secs < 60,
                            "CheckpointBuilder::resolve_checkpoint_transactions is stuck"
                        );
                    }
                }
            }))
        } else {
            None
        };

        let results =
            results
                .into_iter()
                .zip(registrations)
                .zip(keys.iter())
                .map(|((a, r), key)| match a {
                    // Note that Some() clause also drops registration that is already fulfilled
                    Some(ready) => Either::Left(futures::future::ready(ready)),
                    None => {
                        let waiting_keys = Arc::clone(&waiting_keys);
                        let key = key.clone();
                        Either::Right(async move {
                            let result = r.await;
                            // Remove this key from the waiting set
                            waiting_keys.lock().remove(&key);
                            result
                        })
                    }
                });

        let resolved_values = join_all(results).await;

        // Cancel the logging task
        if let Some(handle) = log_handle {
            handle.abort();
        }

        resolved_values
    }
}

/// Registration resolves to the value but also provides safe cancellation
/// When Registration is dropped before it is resolved, we de-register from the pending list
pub struct Registration<'a, K: Eq + Hash + Clone, V: Clone> {
    this: &'a NotifyRead<K, V>,
    registration: Option<(K, oneshot::Receiver<V>)>,
}

impl<K: Eq + Hash + Clone + Unpin, V: Clone + Unpin> Future for Registration<'_, K, V> {
    type Output = V;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let receiver = self
            .registration
            .as_mut()
            .map(|(_key, receiver)| receiver)
            .expect("poll can not be called after drop");
        let poll = Pin::new(receiver).poll(cx);
        if poll.is_ready() {
            // When polling complete we no longer need to cancel
            self.registration.take();
        }
        poll.map(|r| r.expect("Sender never drops when registration is pending"))
    }
}

impl<K: Eq + Hash + Clone, V: Clone> Drop for Registration<'_, K, V> {
    fn drop(&mut self) {
        if let Some((key, receiver)) = self.registration.take() {
            mem::drop(receiver);
            // Receiver is dropped before cleanup
            self.this.cleanup(&key)
        }
    }
}
impl<K: Eq + Hash + Clone, V: Clone> Default for NotifyRead<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::join_all;

    #[tokio::test]
    pub async fn test_notify_read() {
        let notify_read = NotifyRead::<u64, u64>::new();
        let mut registrations = notify_read.register_all(&[1, 2, 3]);
        assert_eq!(3, notify_read.count_pending.load(Ordering::Relaxed));
        registrations.pop();
        assert_eq!(2, notify_read.count_pending.load(Ordering::Relaxed));
        notify_read.notify(&2, &2);
        notify_read.notify(&1, &1);
        let reads = join_all(registrations).await;
        assert_eq!(0, notify_read.count_pending.load(Ordering::Relaxed));
        assert_eq!(reads, vec![1, 2]);
        // ensure cleanup is done correctly
        for pending in &notify_read.pending {
            assert!(pending.lock().is_empty());
        }
    }
}
