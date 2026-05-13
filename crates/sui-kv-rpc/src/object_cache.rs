// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use futures::FutureExt;
use futures::StreamExt;
use futures::future::BoxFuture;
use futures::future::Shared;
use futures::stream::BoxStream;
use sui_rpc_api::RpcError;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use tokio::sync::oneshot;
use tokio::task::JoinSet;

use crate::bigtable_client::BigTableClient;

pub(crate) type ObjectMap = Arc<HashMap<ObjectKey, Object>>;

// Slot futures are shared, so their output must be cheaply cloneable.
type CachedError = Arc<tonic::Status>;

fn cached_error(err: RpcError) -> CachedError {
    Arc::new(err.into())
}

fn cancelled_error() -> CachedError {
    Arc::new(tonic::Status::internal("object fetch cancelled"))
}

fn rpc_error(err: CachedError) -> RpcError {
    RpcError::new(err.code(), err.message())
}

type Slot = Shared<BoxFuture<'static, Result<Option<Object>, CachedError>>>;
type SenderMap = HashMap<ObjectKey, oneshot::Sender<Result<Option<Object>, CachedError>>>;

struct ReservedSlots {
    placeholders: Vec<(ObjectKey, Slot)>,
    new_keys: Vec<ObjectKey>,
    new_senders: SenderMap,
}

/// Pluggable backend for `ObjectCache`. Production uses `BigTableObjectFetcher`;
/// tests inject a stub that doesn't require a real BigTable client.
#[async_trait::async_trait]
pub(crate) trait ObjectFetcher: Send + Sync + 'static {
    async fn fetch(
        &self,
        keys: Vec<ObjectKey>,
    ) -> Result<BoxStream<'static, Result<Object, RpcError>>, RpcError>;
}

pub(crate) struct BigTableObjectFetcher {
    client: BigTableClient,
}

impl BigTableObjectFetcher {
    pub(crate) fn new(client: BigTableClient) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl ObjectFetcher for BigTableObjectFetcher {
    async fn fetch(
        &self,
        keys: Vec<ObjectKey>,
    ) -> Result<BoxStream<'static, Result<Object, RpcError>>, RpcError> {
        let stream = self.client.get_objects_stream(keys).await?;
        Ok(stream.map(|r| r.map_err(RpcError::from)).boxed())
    }
}

/// Request-scoped object cache. Slots are inserted on first `get_many` and
/// **never evicted** for the cache's lifetime, so subsequent calls for the
/// same key (overlapping or sequential) reuse the resolved `Shared` rather
/// than re-dispatching a BigTable read.
pub(crate) struct ObjectCache {
    inner: Arc<ObjectCacheInner>,
    dispatch_tasks: Mutex<JoinSet<()>>,
}

struct ObjectCacheInner {
    fetcher: Arc<dyn ObjectFetcher>,
    slots: DashMap<ObjectKey, Slot>,
}

impl ObjectCache {
    pub(crate) fn new(fetcher: Arc<dyn ObjectFetcher>) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(ObjectCacheInner {
                fetcher,
                slots: DashMap::new(),
            }),
            dispatch_tasks: Mutex::new(JoinSet::new()),
        })
    }

    /// Fetch the given object keys, deduplicating against any in-flight or
    /// previously-resolved fetches in this cache. Missing keys are simply
    /// absent from the returned map (preserving the contract of
    /// `BigTableClient::get_objects_stream`'s direct callers).
    pub(crate) async fn get_many(
        &self,
        keys: Vec<ObjectKey>,
    ) -> Result<HashMap<ObjectKey, Object>, RpcError> {
        let ReservedSlots {
            placeholders,
            new_keys,
            new_senders,
        } = self.inner.reserve_slots(keys);
        if placeholders.is_empty() {
            return Ok(HashMap::new());
        }

        self.spawn_dispatches(new_keys, new_senders);

        let results = futures::future::join_all(
            placeholders
                .into_iter()
                .map(|(k, slot)| async move { (k, slot.await) }),
        )
        .await;

        let mut out = HashMap::new();
        let mut first_err: Option<CachedError> = None;
        for (key, res) in results {
            match res {
                Ok(Some(obj)) => {
                    out.insert(key, obj);
                }
                Ok(None) => {}
                Err(e) => {
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
            }
        }
        if let Some(e) = first_err {
            return Err(rpc_error(e));
        }
        Ok(out)
    }

    fn spawn_dispatches(&self, new_keys: Vec<ObjectKey>, senders: SenderMap) {
        if new_keys.is_empty() {
            return;
        }

        let mut tasks = self
            .dispatch_tasks
            .lock()
            .expect("ObjectCache dispatch_tasks mutex poisoned");
        let inner = self.inner.clone();
        // One backend fetch per dispatch; call sites own request sizing.
        tasks.spawn(async move {
            inner.dispatch_batch(new_keys, senders).await;
        });
    }
}

impl ObjectCacheInner {
    fn reserve_slots(&self, keys: Vec<ObjectKey>) -> ReservedSlots {
        let mut unique = keys;
        unique.sort_unstable();
        unique.dedup();
        if unique.is_empty() {
            return ReservedSlots {
                placeholders: Vec::new(),
                new_keys: Vec::new(),
                new_senders: HashMap::new(),
            };
        }

        let mut placeholders: Vec<(ObjectKey, Slot)> = Vec::with_capacity(unique.len());
        let mut new_keys: Vec<ObjectKey> = Vec::new();
        let mut new_senders: SenderMap = HashMap::new();

        for key in unique {
            match self.slots.entry(key) {
                Entry::Occupied(entry) => {
                    placeholders.push((key, entry.get().clone()));
                }
                Entry::Vacant(entry) => {
                    let (tx, rx) = oneshot::channel();
                    let slot: Slot =
                        async move { rx.await.unwrap_or_else(|_| Err(cancelled_error())) }
                            .boxed()
                            .shared();
                    entry.insert(slot.clone());
                    new_keys.push(key);
                    new_senders.insert(key, tx);
                    placeholders.push((key, slot));
                }
            }
        }

        ReservedSlots {
            placeholders,
            new_keys,
            new_senders,
        }
    }

    async fn dispatch_batch(&self, new_keys: Vec<ObjectKey>, mut senders: SenderMap) {
        // Concurrency gating lives inside the fetcher (see
        // `BigTableObjectFetcher::fetch`, which calls
        // `BigTableClient::get_objects_stream`). The returned stream
        // is drained here, so the permit does not escape this dispatch.
        let stream = match self.fetcher.fetch(new_keys).await {
            Ok(s) => s,
            Err(e) => return drain_with(senders, cached_error(e)),
        };
        futures::pin_mut!(stream);
        while let Some(row) = stream.next().await {
            match row {
                Ok(obj) => {
                    let key = ObjectKey(obj.id(), obj.version());
                    if let Some(tx) = senders.remove(&key) {
                        let _ = tx.send(Ok(Some(obj)));
                    }
                }
                Err(e) => return drain_with(senders, cached_error(e)),
            }
        }
        // Stream ended cleanly; remaining keys not returned by BigTable are
        // misses and resolve to `Ok(None)`.
        for (_, tx) in senders {
            let _ = tx.send(Ok(None));
        }
    }
}

fn drain_with(senders: SenderMap, err: CachedError) {
    for (_, tx) in senders {
        let _ = tx.send(Err(err.clone()));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use futures::stream;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SequenceNumber;
    use tokio::sync::Semaphore;

    use super::*;

    /// Stub fetcher backed by an in-memory map. Returns rows in arrival order,
    /// dropping any keys not in the map. Tracks the underlying call count for
    /// dedup assertions. If `limiter` is set, mirrors `BigTableClient`'s
    /// permit-bundling behavior: acquires one permit per `fetch` call and
    /// holds it inside the returned stream's state until the stream drops.
    struct StubFetcher {
        objects: HashMap<ObjectKey, Object>,
        calls: Arc<AtomicUsize>,
        keys_seen: Arc<Mutex<Vec<Vec<ObjectKey>>>>,
        delay: Duration,
        limiter: Option<Arc<Semaphore>>,
    }

    impl StubFetcher {
        fn new(objects: HashMap<ObjectKey, Object>) -> Self {
            Self {
                objects,
                calls: Arc::new(AtomicUsize::new(0)),
                keys_seen: Arc::new(Mutex::new(Vec::new())),
                delay: Duration::ZERO,
                limiter: None,
            }
        }

        fn with_delay(mut self, delay: Duration) -> Self {
            self.delay = delay;
            self
        }

        fn with_limiter(mut self, limiter: Arc<Semaphore>) -> Self {
            self.limiter = Some(limiter);
            self
        }

        fn calls(&self) -> Arc<AtomicUsize> {
            self.calls.clone()
        }
    }

    #[async_trait::async_trait]
    impl ObjectFetcher for StubFetcher {
        async fn fetch(
            &self,
            keys: Vec<ObjectKey>,
        ) -> Result<BoxStream<'static, Result<Object, RpcError>>, RpcError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.keys_seen
                .lock()
                .expect("stub mutex")
                .push(keys.clone());
            let delay = self.delay;
            let permit =
                match &self.limiter {
                    Some(l) => Some(l.clone().acquire_owned().await.map_err(|_| {
                        RpcError::new(tonic::Code::Internal, "stub limiter closed")
                    })?),
                    None => None,
                };
            let rows: Vec<Result<Object, RpcError>> = keys
                .iter()
                .filter_map(|k| self.objects.get(k).cloned().map(Ok))
                .collect();
            Ok(async_stream::try_stream! {
                let _permit = permit;
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                for row in rows {
                    let r = row?;
                    yield r;
                }
            }
            .boxed())
        }
    }

    /// Faulting fetcher: returns an error mid-stream after yielding `ok_count` rows.
    struct ErrorFetcher {
        objects: HashMap<ObjectKey, Object>,
        ok_count: usize,
    }

    #[async_trait::async_trait]
    impl ObjectFetcher for ErrorFetcher {
        async fn fetch(
            &self,
            keys: Vec<ObjectKey>,
        ) -> Result<BoxStream<'static, Result<Object, RpcError>>, RpcError> {
            let rows: Vec<Object> = keys
                .iter()
                .take(self.ok_count)
                .filter_map(|k| self.objects.get(k).cloned())
                .collect();
            let s = stream::iter(rows.into_iter().map(Ok::<_, RpcError>))
                .chain(stream::once(async {
                    Err(RpcError::new(tonic::Code::Internal, "boom"))
                }))
                .boxed();
            Ok(s)
        }
    }

    fn object_id(id: usize) -> ObjectID {
        let mut bytes = [0u8; ObjectID::LENGTH];
        bytes[ObjectID::LENGTH - std::mem::size_of::<usize>()..].copy_from_slice(&id.to_be_bytes());
        ObjectID::new(bytes)
    }

    fn make_object(id: usize, version: u64) -> Object {
        let id = object_id(id);
        // Build a minimal immutable test object at the requested id/version.
        let mut obj = Object::immutable_with_id_for_testing(id);
        // Force version: rebuild with desired sequence number via testing API.
        obj = Object::with_id_owner_version_for_testing(
            id,
            SequenceNumber::from_u64(version),
            obj.owner().clone(),
        );
        obj
    }

    fn key(id: usize, version: u64) -> ObjectKey {
        ObjectKey(object_id(id), SequenceNumber::from_u64(version))
    }

    fn cache(fetcher: Arc<dyn ObjectFetcher>) -> Arc<ObjectCache> {
        ObjectCache::new(fetcher)
    }

    #[tokio::test]
    async fn dedupes_concurrent_get_many() {
        let mut objs = HashMap::new();
        for i in 0..5 {
            objs.insert(key(i, 1), make_object(i, 1));
        }
        let stub = StubFetcher::new(objs).with_delay(Duration::from_millis(50));
        let calls = stub.calls();
        let keys_seen = stub.keys_seen.clone();
        let cache = cache(Arc::new(stub));

        // Two concurrent get_many calls overlapping on keys 1, 2, 3.
        let a_keys = vec![key(0, 1), key(1, 1), key(2, 1), key(3, 1)];
        let b_keys = vec![key(1, 1), key(2, 1), key(3, 1), key(4, 1)];
        let (a_res, b_res) = tokio::join!(cache.get_many(a_keys), cache.get_many(b_keys));
        let a = a_res.expect("ok");
        let b = b_res.expect("ok");
        assert_eq!(a.len(), 4);
        assert_eq!(b.len(), 4);

        let dispatch_calls = calls.load(Ordering::SeqCst);
        assert!(
            dispatch_calls <= 2,
            "expected <=2 dispatch calls, got {dispatch_calls}"
        );
        // Stronger guarantee: across every fetch issued, no key was fetched
        // twice. `dispatch_calls <= 2` alone could be satisfied by a buggy
        // implementation that re-issued an overlapping key in the second
        // batch — flatten and check uniqueness.
        let seen = keys_seen.lock().expect("stub mutex");
        let flattened: Vec<ObjectKey> = seen.iter().flatten().copied().collect();
        let mut uniq = flattened.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(
            flattened.len(),
            uniq.len(),
            "expected no key to be fetched twice across batches; saw {seen:?}"
        );
    }

    #[tokio::test]
    async fn dedupes_sequential_non_overlapping_get_many() {
        let mut objs = HashMap::new();
        for i in 0..3 {
            objs.insert(key(i, 1), make_object(i, 1));
        }
        let stub = StubFetcher::new(objs);
        let calls = stub.calls();
        let cache = cache(Arc::new(stub));

        let _ = cache
            .clone()
            .get_many(vec![key(0, 1), key(1, 1)])
            .await
            .expect("ok");
        let after_first = calls.load(Ordering::SeqCst);
        assert_eq!(after_first, 1);

        // Second call with one overlapping and one new key: should dispatch
        // exactly one new fetch, and only for the new key.
        let _ = cache
            .clone()
            .get_many(vec![key(1, 1), key(2, 1)])
            .await
            .expect("ok");
        let after_second = calls.load(Ordering::SeqCst);
        assert_eq!(after_second, 2);

        // Third call overlapping entirely: no new fetch.
        let _ = cache
            .clone()
            .get_many(vec![key(0, 1), key(2, 1)])
            .await
            .expect("ok");
        let after_third = calls.load(Ordering::SeqCst);
        assert_eq!(after_third, 2);
    }

    #[tokio::test]
    async fn dedupes_duplicate_keys_within_one_call() {
        let mut objs = HashMap::new();
        objs.insert(key(0, 1), make_object(0, 1));
        let stub = StubFetcher::new(objs);
        let keys_seen = stub.keys_seen.clone();
        let calls = stub.calls();
        let cache = cache(Arc::new(stub));

        let result = cache
            .clone()
            .get_many(vec![key(0, 1), key(0, 1), key(0, 1)])
            .await
            .expect("ok");
        assert_eq!(result.len(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let seen = keys_seen.lock().expect("stub mutex");
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0], vec![key(0, 1)]);
    }

    #[tokio::test]
    async fn omits_missing_keys() {
        let mut objs = HashMap::new();
        objs.insert(key(0, 1), make_object(0, 1));
        // key(9, 1) is intentionally not present.
        let stub = StubFetcher::new(objs);
        let cache = cache(Arc::new(stub));

        let result = cache
            .clone()
            .get_many(vec![key(0, 1), key(9, 1)])
            .await
            .expect("ok");
        assert_eq!(result.len(), 1);
        assert!(result.contains_key(&key(0, 1)));
        assert!(!result.contains_key(&key(9, 1)));
    }

    #[tokio::test]
    async fn propagates_dispatch_error_to_all_in_flight() {
        let mut objs = HashMap::new();
        for i in 0..3 {
            objs.insert(key(i, 1), make_object(i, 1));
        }
        let fetcher = ErrorFetcher {
            objects: objs,
            ok_count: 1,
        };
        let cache = cache(Arc::new(fetcher));

        let result = cache.get_many(vec![key(0, 1), key(1, 1), key(2, 1)]).await;
        assert!(result.is_err(), "expected error from faulting fetcher");

        // Subsequent calls for the same poisoned keys see the cached error;
        // they must not deadlock and must not re-dispatch.
        let result2 = cache.get_many(vec![key(2, 1)]).await;
        assert!(result2.is_err());
    }

    #[tokio::test]
    async fn dropping_cache_aborts_dispatch_and_releases_permit() {
        let mut objs = HashMap::new();
        objs.insert(key(0, 1), make_object(0, 1));
        let limiter = Arc::new(Semaphore::new(1));
        let stub = StubFetcher::new(objs)
            .with_delay(Duration::from_secs(60))
            .with_limiter(limiter.clone());
        let cache = ObjectCache::new(Arc::new(stub));

        let handle = {
            let cache = cache.clone();
            tokio::spawn(async move {
                let _ = cache.get_many(vec![key(0, 1)]).await;
            })
        };

        for _ in 0..50 {
            if limiter.available_permits() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(limiter.available_permits(), 0, "permit should be held");

        handle.abort();
        let _ = handle.await;
        drop(cache);

        for _ in 0..50 {
            if limiter.available_permits() == 1 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("permit was not released after dropping cache with in-flight dispatch");
    }

    #[tokio::test]
    async fn get_many_dispatches_residual_as_single_fetch() {
        // Contract: ObjectCache dedupes against in-flight/resolved slots and
        // dispatches the residual new keys as ONE backend fetch. Request
        // sizing is owned by the caller, not the cache.
        let mut objs = HashMap::new();
        let key_count = 130usize;
        for i in 0..key_count {
            objs.insert(key(i, 1), make_object(i, 1));
        }
        let stub = StubFetcher::new(objs);
        let calls = stub.calls();
        let keys_seen = stub.keys_seen.clone();
        let cache = cache(Arc::new(stub));

        let keys: Vec<_> = (0..key_count).map(|i| key(i, 1)).collect();
        let result = cache.get_many(keys.clone()).await.expect("ok");
        assert_eq!(result.len(), key_count);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        {
            let seen = keys_seen.lock().expect("stub mutex");
            assert_eq!(seen.len(), 1);
            // Cache sorts/dedupes inside `reserve_slots`, so the dispatched
            // batch is the deduped key set in sorted order.
            let mut expected = keys.clone();
            expected.sort();
            expected.dedup();
            assert_eq!(seen[0], expected);
        }

        // Second call: all keys resolved in cache → no new dispatch.
        let result = cache.get_many(keys).await.expect("ok");
        assert_eq!(result.len(), key_count);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
