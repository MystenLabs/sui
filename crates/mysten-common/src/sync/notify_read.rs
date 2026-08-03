// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ZipDebugEqIteratorExt;
use crate::debug_fatal;

use futures::future::{Either, join_all};
use parking_lot::Mutex;
use parking_lot::MutexGuard;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::mem;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::Instant;
use tokio::time::interval_at;
use tracing::warn;

type Registrations<V> = Vec<oneshot::Sender<V>>;

/// Interval duration for logging waiting keys when reads take too long
const LONG_WAIT_LOG_INTERVAL_SECS: u64 = 10;

/// Minimum interval between stall reports for a given task name, across every
/// read blocked on it.
const STALL_LOG_INTERVAL_SECS: u64 = 30;

/// Number of this read's own keys included in a stall report.
const MAX_SAMPLED_KEYS: usize = 32;

pub const CHECKPOINT_BUILDER_NOTIFY_READ_TASK_NAME: &str =
    "CheckpointBuilder::notify_read_executed_effects";

pub struct NotifyRead<K, V> {
    pending: Vec<Mutex<HashMap<K, Registrations<V>>>>,
    count_pending: AtomicUsize,
    // Last stall report per task name.
    last_stall_log: Mutex<HashMap<&'static str, Instant>>,
}

impl<K: Eq + Hash + Clone, V: Clone> NotifyRead<K, V> {
    pub fn new() -> Self {
        let pending = (0..255).map(|_| Default::default()).collect();
        let count_pending = Default::default();
        Self {
            pending,
            count_pending,
            last_stall_log: Default::default(),
        }
    }

    /// Returns true if this caller should emit the stall report for `task_name`.
    /// Any number of reads may be blocked at once; only one of them logs.
    fn throttle_stall_log(&self, task_name: &'static str) -> bool {
        let now = Instant::now();
        let mut last_log = self.last_stall_log.lock();
        match last_log.get(task_name) {
            Some(last)
                if now.duration_since(*last) < Duration::from_secs(STALL_LOG_INTERVAL_SECS) =>
            {
                false
            }
            _ => {
                last_log.insert(task_name, now);
                true
            }
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

    pub fn register_one(&self, key: &K) -> Registration<'_, K, V> {
        self.count_pending.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        self.register(key, sender);
        Registration {
            this: self,
            registration: Some((key.clone(), receiver)),
        }
    }

    pub fn register_all(&self, keys: &[K]) -> Vec<Registration<'_, K, V>> {
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

    fn pending(&self, key: &K) -> MutexGuard<'_, HashMap<K, Registrations<V>>> {
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
        task_name: &'static str,
        keys: &[K],
        fetch: impl FnOnce(&[K]) -> Vec<Option<V>>,
    ) -> Vec<V> {
        let _metrics_scope = mysten_metrics::monitored_scope(task_name);
        let registrations = self.register_all(keys);

        let results = fetch(keys);
        // Snapshot of what was missing at fetch time.
        let waiting_keys: Vec<K> = keys
            .iter()
            .zip_debug_eq(results.iter())
            .filter(|(_key, result)| result.is_none())
            .map(|(key, _result)| key.clone())
            .collect();

        let results = results
            .into_iter()
            .zip_debug_eq(registrations)
            .map(|(a, r)| match a {
                // Note that Some() clause also drops registration that is already fulfilled
                Some(ready) => Either::Left(futures::future::ready(ready)),
                None => Either::Right(r),
            });

        let join = join_all(results);
        if waiting_keys.is_empty() {
            return join.await;
        }

        tokio::pin!(join);
        let start_time = Instant::now();
        let mut interval = interval_at(
            start_time + Duration::from_secs(LONG_WAIT_LOG_INTERVAL_SECS),
            Duration::from_secs(LONG_WAIT_LOG_INTERVAL_SECS),
        );

        loop {
            tokio::select! {
                values = &mut join => return values,
                _ = interval.tick() => {
                    let elapsed_secs = start_time.elapsed().as_secs();

                    // Deduplicate logging by task name. When many reads are blocked,
                    // logs will sample blocked reads per task and the keys we are
                    // waiting on for those reads.
                    if self.throttle_stall_log(task_name) {
                        let mut sample: Vec<&K> = Vec::with_capacity(MAX_SAMPLED_KEYS);
                        let mut outstanding = 0usize;
                        for key in &waiting_keys {
                            if self.pending(key).contains_key(key) {
                                outstanding += 1;
                                if sample.len() < MAX_SAMPLED_KEYS {
                                    sample.push(key);
                                }
                            }
                        }

                        warn!(
                            "[{task_name}] Still waiting {elapsed_secs}s. {} registrations pending, this read still blocked on {outstanding} of {} key(s): {sample:?}",
                            self.num_pending(),
                            waiting_keys.len(),
                        );
                    }

                    if task_name == CHECKPOINT_BUILDER_NOTIFY_READ_TASK_NAME && elapsed_secs >= 60 {
                        debug_fatal!("{} is stuck", task_name);
                    }
                }
            }
        }
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
    use std::sync::Arc;
    use tokio::time::timeout;

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

    #[tokio::test]
    pub async fn test_notify_read_cancellation() {
        let notify_read = Arc::new(NotifyRead::<u64, u64>::new());

        // Start a read that will wait indefinitely
        let read_future = notify_read.read(
            "test_task",
            &[1, 2, 3],
            |_keys| vec![None, None, None], // All keys will wait
        );

        // Use timeout to cancel the read after a short duration
        let result = timeout(Duration::from_millis(100), read_future).await;

        // Verify the read was cancelled
        assert!(result.is_err());

        // Give some time for cleanup to complete
        tokio::time::sleep(Duration::from_millis(50)).await;

        // When the read is cancelled, the registrations are cleaned up
        // so the pending count should be 0
        assert_eq!(0, notify_read.count_pending.load(Ordering::Relaxed));

        // Verify all pending maps are empty (cleanup was performed)
        for pending in &notify_read.pending {
            assert!(pending.lock().is_empty());
        }
    }

    #[tokio::test(start_paused = true)]
    pub async fn test_stall_log_throttle() {
        let notify_read = NotifyRead::<u64, u64>::new();

        assert!(notify_read.throttle_stall_log("task_a"));
        assert!(!notify_read.throttle_stall_log("task_a"));

        // A report for one task name must not silence a different one.
        assert!(notify_read.throttle_stall_log("task_b"));
        assert!(!notify_read.throttle_stall_log("task_a"));

        tokio::time::advance(Duration::from_secs(STALL_LOG_INTERVAL_SECS + 1)).await;
        assert!(notify_read.throttle_stall_log("task_a"));
    }

    #[tokio::test(start_paused = true)]
    pub async fn test_read_blocked_past_log_interval() {
        let notify_read = Arc::new(NotifyRead::<u64, u64>::new());

        let reader = notify_read.clone();
        let handle = tokio::spawn(async move {
            reader
                .read("test_task", &[1, 2, 3], |_keys| vec![Some(10), None, None])
                .await
        });

        // Outlive several ticks so the read exercises the stall reporting path.
        tokio::time::advance(Duration::from_secs(LONG_WAIT_LOG_INTERVAL_SECS * 4)).await;
        assert!(!handle.is_finished());

        notify_read.notify(&2, &20);
        notify_read.notify(&3, &30);

        // Values are returned in key order, not completion order.
        assert_eq!(handle.await.unwrap(), vec![10, 20, 30]);
        assert_eq!(0, notify_read.count_pending.load(Ordering::Relaxed));
        for pending in &notify_read.pending {
            assert!(pending.lock().is_empty());
        }
    }
}
