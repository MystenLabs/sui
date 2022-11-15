// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_store::EffectsStore;
use crate::authority::AuthorityStore;
use async_trait::async_trait;
use either::Either;
use futures::future::join_all;
use parking_lot::Mutex;
use parking_lot::MutexGuard;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::mem;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::task::{Context, Poll};
use sui_types::base_types::TransactionDigest;
use sui_types::error::SuiResult;
use sui_types::messages::TransactionEffects;
use tokio::sync::oneshot;

#[async_trait]
pub trait EffectsNotifyRead: Send + Sync + 'static {
    /// This method reads transaction effects from database.
    /// If effects are not available immediately, the method blocks until they are persisted
    /// in the database.
    ///
    /// This method **does not** schedule transactions for execution - it is responsibility of the caller
    /// to schedule transactions for execution before calling this method.
    async fn notify_read(
        &self,
        digests: Vec<TransactionDigest>,
    ) -> SuiResult<Vec<TransactionEffects>>;

    fn get_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>>;
}

type Registrations<V> = Vec<oneshot::Sender<V>>;

pub(crate) struct NotifyRead<K, V> {
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

    pub fn register_all(&self, keys: Vec<K>) -> Vec<Registration<K, V>> {
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
        let Some(registrations) = pending.get_mut(key) else { return; };
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

/// Registration resolves to the value but also provides safe cancellation
/// When Registration is dropped before it is resolved, we de-register from the pending list
pub struct Registration<'a, K: Eq + Hash + Clone, V: Clone> {
    this: &'a NotifyRead<K, V>,
    registration: Option<(K, oneshot::Receiver<V>)>,
}

impl<'a, K: Eq + Hash + Clone + Unpin, V: Clone + Unpin> Future for Registration<'a, K, V> {
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

impl<'a, K: Eq + Hash + Clone, V: Clone> Drop for Registration<'a, K, V> {
    fn drop(&mut self) {
        if let Some((key, receiver)) = self.registration.take() {
            mem::drop(receiver);
            // Receiver is dropped before cleanup
            self.this.cleanup(&key)
        }
    }
}

#[async_trait]
impl EffectsNotifyRead for Arc<AuthorityStore> {
    async fn notify_read(
        &self,
        digests: Vec<TransactionDigest>,
    ) -> SuiResult<Vec<TransactionEffects>> {
        // We need to register waiters _before_ reading from the database to avoid race conditions
        let registrations = self.effects_notify_read.register_all(digests.clone());
        let effects = EffectsStore::get_effects(self, digests.iter())?;
        // Zipping together registrations and effects ensures returned order is the same as order of digests
        let results = effects
            .into_iter()
            .zip(registrations.into_iter())
            .map(|(e, r)| match e {
                // Note that Some() clause also drops registration that is already fulfilled
                Some(ready) => Either::Left(futures::future::ready(ready)),
                None => Either::Right(r),
            });

        Ok(join_all(results).await)
    }

    fn get_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        EffectsStore::get_effects(self, digests.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    pub async fn test_notify_read() {
        let notify_read = NotifyRead::<u64, u64>::new();
        let mut registrations = notify_read.register_all(vec![1, 2, 3]);
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
