// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::hash::Hash;
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
use tokio::sync::oneshot;
use tokio::task::JoinSet;

// Slot futures are shared, so their output must be cheaply cloneable.
type CachedError = Arc<tonic::Status>;

fn cached_error(err: RpcError) -> CachedError {
    Arc::new(err.into())
}

fn cancelled_error() -> CachedError {
    Arc::new(tonic::Status::internal("cached read cancelled"))
}

fn rpc_error(err: CachedError) -> RpcError {
    RpcError::new(err.code(), err.message())
}

type Slot<V> = Shared<BoxFuture<'static, Result<Option<V>, CachedError>>>;
type SenderMap<K, V> = HashMap<K, oneshot::Sender<Result<Option<V>, CachedError>>>;

/// The value of one reserved key: `Ok(Some(row))`, `Ok(None)` when its read ended without a
/// row, or the read's error.
pub(crate) type CachedValue<V> = BoxFuture<'static, Result<Option<V>, RpcError>>;

/// Resolves when a [`RequestCache::read`] has ended.
pub(crate) type ReadEnd = BoxFuture<'static, ()>;

/// Backend of a [`RequestCache`]: reads `keys` and streams back the rows found,
/// each with its key. Keys without a row are left out. Each
/// [`RequestCache::read`] makes exactly one `fetch` call with exactly the keys
/// it was given, so callers own read sizing. Production fetchers read
/// BigTable; tests inject stubs.
#[async_trait::async_trait]
pub(crate) trait CacheFetcher<K, V>: Send + Sync + 'static {
    async fn fetch(
        &self,
        keys: Vec<K>,
    ) -> Result<BoxStream<'static, Result<(K, V), RpcError>>, RpcError>;
}

/// Request-scoped single-flight read cache, used in two steps: reserve, then
/// read. [`reserve`](Self::reserve) gives every key a slot on its first
/// reservation; slots are **never evicted** for the cache's lifetime, so a
/// later reservation of the same key (overlapping or sequential) shares that
/// slot's value rather than reading the key again. The keys a reservation
/// reserved first are its unread keys: the caller takes them in reads it sizes
/// itself and passes each to exactly one [`read`](Self::read), which fills
/// their slots from one backend fetch. Dropping the cache aborts its in-flight
/// reads.
pub(crate) struct RequestCache<K, V> {
    inner: Arc<RequestCacheInner<K, V>>,
    read_tasks: Mutex<JoinSet<()>>,
}

struct RequestCacheInner<K, V> {
    fetcher: Arc<dyn CacheFetcher<K, V>>,
    slots: DashMap<K, Slot<V>>,
}

/// What one [`RequestCache::reserve`] call reserved.
pub(crate) struct Reservation<K, V> {
    /// The slot of every distinct key passed to `reserve`.
    slots: HashMap<K, Slot<V>>,
    /// Senders of the keys this reservation reserved first and has not yet
    /// taken for a read. Dropping one resolves its slot to the cancelled error.
    unread: SenderMap<K, V>,
}

/// Unread keys taken from a [`Reservation`] for one [`RequestCache::read`].
/// Dropping them unread resolves their slots to the cancelled error.
pub(crate) struct UnreadKeys<K, V> {
    /// The keys in the order they were taken, which is the order the read
    /// passes them to [`CacheFetcher::fetch`].
    keys: Vec<K>,
    senders: SenderMap<K, V>,
}

impl<K, V> RequestCache<K, V>
where
    K: Copy + Eq + Hash + Ord + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    pub(crate) fn new(fetcher: Arc<dyn CacheFetcher<K, V>>) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(RequestCacheInner {
                fetcher,
                slots: DashMap::new(),
            }),
            read_tasks: Mutex::new(JoinSet::new()),
        })
    }

    /// Reserve every distinct key of `keys` (duplicates allowed). Synchronous:
    /// once it returns, every later `reserve` call finds these keys reserved.
    /// Keys no earlier call reserved are this reservation's unread keys: each
    /// must be taken with [`Reservation::take_unread`] and passed to exactly
    /// one [`Self::read`].
    pub(crate) fn reserve(&self, keys: &[K]) -> Reservation<K, V> {
        self.inner.reserve_slots(keys)
    }

    /// Start one backend read (exactly one [`CacheFetcher::fetch`] call with
    /// exactly these keys) in a task the cache owns, filling the keys' slots as
    /// rows arrive; a key the read ends without resolves to `None`. A read
    /// error resolves every remaining key to it, and a row for a key the read
    /// did not ask for or already received resolves every remaining key to an
    /// error. Returns a future that resolves when that task has ended; for
    /// empty `keys` no read is started and the future is ready. Dropping the
    /// cache aborts the read.
    pub(crate) fn read(&self, keys: UnreadKeys<K, V>) -> ReadEnd
    where
        K: std::fmt::Debug,
    {
        let UnreadKeys { keys, senders } = keys;
        if keys.is_empty() {
            return futures::future::ready(()).boxed();
        }

        let mut tasks = self
            .read_tasks
            .lock()
            .expect("RequestCache read_tasks mutex poisoned");
        let inner = self.inner.clone();
        // Dropped when the task ends, which resolves `read_end`.
        let (ended, read_end) = oneshot::channel::<()>();
        tasks.spawn(async move {
            let _ended = ended;
            inner.run_read(keys, senders).await;
        });
        read_end.map(|_| ()).boxed()
    }
}

impl<K, V> Reservation<K, V>
where
    K: Copy + Eq + Hash + Ord + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Whether this reservation (not an earlier one) reserved `key` and it is
    /// not yet taken.
    pub(crate) fn is_unread(&self, key: &K) -> bool {
        self.unread.contains_key(key)
    }

    /// Take distinct unread `keys` for one [`RequestCache::read`]. Panics if a
    /// key is not unread.
    pub(crate) fn take_unread(&mut self, keys: &[K]) -> UnreadKeys<K, V> {
        let senders = keys
            .iter()
            .map(|&key| {
                let sender = self.unread.remove(&key).expect(
                    "`take_unread` is given only this reservation's unread keys, each once",
                );
                (key, sender)
            })
            .collect();
        UnreadKeys {
            keys: keys.to_vec(),
            senders,
        }
    }

    /// The value of `key`, which must be one of the keys passed to
    /// [`RequestCache::reserve`] (panics otherwise): `Ok(Some(row))`,
    /// `Ok(None)` when its read ended without a row, or the read's error.
    /// Cheap; callable any number of times; `'static`.
    pub(crate) fn value(&self, key: &K) -> CachedValue<V> {
        self.slots
            .get(key)
            .expect("`value` is asked only for keys passed to `reserve`")
            .clone()
            .map(|value| value.map_err(rpc_error))
            .boxed()
    }
}

impl<K, V> RequestCacheInner<K, V>
where
    K: Copy + Eq + Hash + Ord + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn reserve_slots(&self, keys: &[K]) -> Reservation<K, V> {
        let mut slots: HashMap<K, Slot<V>> = HashMap::with_capacity(keys.len());
        let mut unread: SenderMap<K, V> = HashMap::new();

        for &key in keys {
            if slots.contains_key(&key) {
                continue;
            }
            let slot = match self.slots.entry(key) {
                Entry::Occupied(entry) => entry.get().clone(),
                Entry::Vacant(entry) => {
                    let (tx, rx) = oneshot::channel();
                    let slot: Slot<V> =
                        async move { rx.await.unwrap_or_else(|_| Err(cancelled_error())) }
                            .boxed()
                            .shared();
                    entry.insert(slot.clone());
                    unread.insert(key, tx);
                    slot
                }
            };
            slots.insert(key, slot);
        }

        Reservation { slots, unread }
    }

    async fn run_read(&self, keys: Vec<K>, mut senders: SenderMap<K, V>)
    where
        K: std::fmt::Debug,
    {
        // Concurrency gating lives inside the fetcher (BigTable fetchers open
        // gated streams). The returned stream is drained here, so the permit
        // does not escape this read.
        let stream = match self.fetcher.fetch(keys).await {
            Ok(s) => s,
            Err(e) => return drain_with(senders, cached_error(e)),
        };
        futures::pin_mut!(stream);
        while let Some(row) = stream.next().await {
            match row {
                Ok((key, value)) => match senders.remove(&key) {
                    Some(tx) => {
                        let _ = tx.send(Ok(Some(value)));
                    }
                    // A read by key returns each requested row at most once,
                    // so the fetcher decoded this row's key wrongly.
                    None => {
                        let error = RpcError::new(
                            tonic::Code::Internal,
                            format!("read returned a row it did not request, or twice: {key:?}"),
                        );
                        return drain_with(senders, cached_error(error));
                    }
                },
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

fn drain_with<K, V>(senders: SenderMap<K, V>, err: CachedError) {
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
    use sui_types::object::Object;
    use sui_types::storage::ObjectKey;
    use tokio::sync::Semaphore;

    use super::*;

    type ObjectCache = RequestCache<ObjectKey, Object>;

    /// Stub fetcher backed by an in-memory map. Returns rows in arrival order,
    /// dropping any keys not in the map. Tracks the underlying call count for
    /// dedup assertions. A response can wait before its first row (`delay`).
    /// If `limiter` is set, mirrors `BigTableClient`'s permit-bundling
    /// behavior: acquires one permit per `fetch` call and holds it inside the
    /// returned stream's state until the stream drops.
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
    impl CacheFetcher<ObjectKey, Object> for StubFetcher {
        async fn fetch(
            &self,
            keys: Vec<ObjectKey>,
        ) -> Result<BoxStream<'static, Result<(ObjectKey, Object), RpcError>>, RpcError> {
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
            let rows: Vec<(ObjectKey, Object)> = keys
                .iter()
                .filter_map(|k| self.objects.get(k).cloned().map(|obj| (*k, obj)))
                .collect();
            Ok(async_stream::try_stream! {
                let _permit = permit;
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                for row in rows {
                    yield row;
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
    impl CacheFetcher<ObjectKey, Object> for ErrorFetcher {
        async fn fetch(
            &self,
            keys: Vec<ObjectKey>,
        ) -> Result<BoxStream<'static, Result<(ObjectKey, Object), RpcError>>, RpcError> {
            let rows: Vec<(ObjectKey, Object)> = keys
                .iter()
                .take(self.ok_count)
                .filter_map(|k| self.objects.get(k).cloned().map(|obj| (*k, obj)))
                .collect();
            let s = stream::iter(rows.into_iter().map(Ok::<_, RpcError>))
                .chain(stream::once(async {
                    Err(RpcError::new(tonic::Code::Internal, "boom"))
                }))
                .boxed();
            Ok(s)
        }
    }

    /// Fetcher that returns `rows`, in order, whatever keys it is asked for.
    struct ScriptedFetcher {
        rows: Vec<(ObjectKey, Object)>,
    }

    #[async_trait::async_trait]
    impl CacheFetcher<ObjectKey, Object> for ScriptedFetcher {
        async fn fetch(
            &self,
            _keys: Vec<ObjectKey>,
        ) -> Result<BoxStream<'static, Result<(ObjectKey, Object), RpcError>>, RpcError> {
            Ok(stream::iter(self.rows.clone().into_iter().map(Ok::<_, RpcError>)).boxed())
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

    fn cache(fetcher: Arc<dyn CacheFetcher<ObjectKey, Object>>) -> Arc<ObjectCache> {
        ObjectCache::new(fetcher)
    }

    /// The distinct keys `keys` that `reservation` reserved first, sorted.
    fn unread_keys(
        reservation: &Reservation<ObjectKey, Object>,
        keys: &[ObjectKey],
    ) -> Vec<ObjectKey> {
        let mut unread: Vec<ObjectKey> = keys
            .iter()
            .copied()
            .filter(|key| reservation.is_unread(key))
            .collect();
        unread.sort();
        unread.dedup();
        unread
    }

    /// Reserve `keys`, read every key the reservation reserved first as one
    /// read, and collect the rows of `keys`: the first error, or each key that
    /// has a row.
    async fn read_rows(
        cache: &ObjectCache,
        keys: &[ObjectKey],
    ) -> Result<HashMap<ObjectKey, Object>, RpcError> {
        let mut reservation = cache.reserve(keys);
        let unread = unread_keys(&reservation, keys);
        let _read_end = cache.read(reservation.take_unread(&unread));
        let mut rows = HashMap::new();
        for key in keys {
            if let Some(row) = reservation.value(key).await? {
                rows.insert(*key, row);
            }
        }
        Ok(rows)
    }

    fn sorted_keys_seen(keys_seen: &Mutex<Vec<Vec<ObjectKey>>>) -> Vec<Vec<ObjectKey>> {
        let mut seen = keys_seen.lock().expect("stub mutex").clone();
        seen.sort();
        seen
    }

    #[tokio::test]
    async fn dedupes_concurrent_reservations() {
        let mut objs = HashMap::new();
        for i in 0..5 {
            objs.insert(key(i, 1), make_object(i, 1));
        }
        let stub = StubFetcher::new(objs).with_delay(Duration::from_millis(50));
        let keys_seen = stub.keys_seen.clone();
        let cache = cache(Arc::new(stub));

        // Two concurrent requests overlapping on keys 1, 2, 3: the second
        // reserves while the first's read is in flight, so it reads only key 4.
        let a_keys = vec![key(0, 1), key(1, 1), key(2, 1), key(3, 1)];
        let b_keys = vec![key(1, 1), key(2, 1), key(3, 1), key(4, 1)];
        let (a_res, b_res) = tokio::join!(read_rows(&cache, &a_keys), read_rows(&cache, &b_keys));
        assert_eq!(a_res.expect("ok").len(), 4);
        assert_eq!(b_res.expect("ok").len(), 4);
        assert_eq!(sorted_keys_seen(&keys_seen), vec![a_keys, vec![key(4, 1)]]);
    }

    #[tokio::test]
    async fn dedupes_sequential_reservations() {
        let mut objs = HashMap::new();
        for i in 0..3 {
            objs.insert(key(i, 1), make_object(i, 1));
        }
        let stub = StubFetcher::new(objs);
        let calls = stub.calls();
        let keys_seen = stub.keys_seen.clone();
        let cache = cache(Arc::new(stub));

        read_rows(&cache, &[key(0, 1), key(1, 1)])
            .await
            .expect("ok");
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // One overlapping and one new key: one read, of the new key only.
        let rows = read_rows(&cache, &[key(1, 1), key(2, 1)])
            .await
            .expect("ok");
        assert_eq!(rows.len(), 2);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(keys_seen.lock().expect("stub mutex")[1], vec![key(2, 1)]);

        // Every key already read: no read.
        let rows = read_rows(&cache, &[key(0, 1), key(2, 1)])
            .await
            .expect("ok");
        assert_eq!(rows.len(), 2);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn second_reservation_finds_shared_keys_reserved() {
        let mut objs = HashMap::new();
        for i in 0..3 {
            objs.insert(key(i, 1), make_object(i, 1));
        }
        let stub = StubFetcher::new(objs);
        let keys_seen = stub.keys_seen.clone();
        let cache = cache(Arc::new(stub));

        // No await between the two reservations.
        let mut first = cache.reserve(&[key(0, 1), key(1, 1)]);
        let mut second = cache.reserve(&[key(1, 1), key(2, 1)]);
        assert!(first.is_unread(&key(0, 1)));
        assert!(first.is_unread(&key(1, 1)));
        assert!(!second.is_unread(&key(1, 1)));
        assert!(second.is_unread(&key(2, 1)));

        let first_read = cache.read(first.take_unread(&[key(0, 1), key(1, 1)]));
        let second_read = cache.read(second.take_unread(&[key(2, 1)]));
        first_read.await;
        second_read.await;
        assert!(second.value(&key(1, 1)).await.expect("ok").is_some());
        assert!(second.value(&key(2, 1)).await.expect("ok").is_some());
        assert_eq!(
            sorted_keys_seen(&keys_seen),
            vec![vec![key(0, 1), key(1, 1)], vec![key(2, 1)]]
        );
    }

    #[tokio::test]
    async fn reads_of_disjoint_unread_subsets_fetch_exactly_their_keys() {
        let mut objs = HashMap::new();
        for i in 0..5 {
            objs.insert(key(i, 1), make_object(i, 1));
        }
        let stub = StubFetcher::new(objs);
        let keys_seen = stub.keys_seen.clone();
        let cache = cache(Arc::new(stub));

        let keys: Vec<ObjectKey> = (0..5).map(|i| key(i, 1)).collect();
        let mut reservation = cache.reserve(&keys);
        let front = reservation.take_unread(&[key(0, 1), key(1, 1)]);
        assert!(!reservation.is_unread(&key(0, 1)));
        assert!(reservation.is_unread(&key(2, 1)));
        let back = reservation.take_unread(&[key(3, 1), key(2, 1), key(4, 1)]);
        let front_read = cache.read(front);
        let back_read = cache.read(back);
        front_read.await;
        back_read.await;

        for key in &keys {
            assert!(reservation.value(key).await.expect("ok").is_some());
        }
        assert_eq!(
            sorted_keys_seen(&keys_seen),
            vec![
                vec![key(0, 1), key(1, 1)],
                vec![key(3, 1), key(2, 1), key(4, 1)],
            ]
        );
    }

    #[tokio::test]
    async fn dedupes_duplicate_keys_within_one_reservation() {
        let mut objs = HashMap::new();
        objs.insert(key(0, 1), make_object(0, 1));
        let stub = StubFetcher::new(objs);
        let keys_seen = stub.keys_seen.clone();
        let cache = cache(Arc::new(stub));

        let rows = read_rows(&cache, &[key(0, 1), key(0, 1), key(0, 1)])
            .await
            .expect("ok");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            *keys_seen.lock().expect("stub mutex"),
            vec![vec![key(0, 1)]]
        );
    }

    #[tokio::test]
    async fn key_without_a_row_resolves_to_none() {
        let mut objs = HashMap::new();
        objs.insert(key(0, 1), make_object(0, 1));
        // key(9, 1) is intentionally not present.
        let stub = StubFetcher::new(objs);
        let cache = cache(Arc::new(stub));

        let mut reservation = cache.reserve(&[key(0, 1), key(9, 1)]);
        cache
            .read(reservation.take_unread(&[key(0, 1), key(9, 1)]))
            .await;
        assert!(reservation.value(&key(0, 1)).await.expect("ok").is_some());
        assert!(reservation.value(&key(9, 1)).await.expect("ok").is_none());
    }

    #[tokio::test]
    async fn reading_no_keys_fetches_nothing() {
        let stub = StubFetcher::new(HashMap::new());
        let calls = stub.calls();
        let cache = cache(Arc::new(stub));

        let mut reservation = cache.reserve(&[key(0, 1)]);
        let none = reservation.take_unread(&[]);
        assert!(
            cache.read(none).now_or_never().is_some(),
            "a read of no keys has ended at once"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        cache.read(reservation.take_unread(&[key(0, 1)])).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // A reservation of keys already read has nothing to read.
        let mut again = cache.reserve(&[key(0, 1)]);
        assert!(!again.is_unread(&key(0, 1)));
        cache.read(again.take_unread(&[])).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(again.value(&key(0, 1)).await.expect("ok").is_none());
    }

    #[tokio::test]
    async fn dropping_unread_keys_resolves_them_to_an_error() {
        let mut objs = HashMap::new();
        for i in 0..3 {
            objs.insert(key(i, 1), make_object(i, 1));
        }
        let stub = StubFetcher::new(objs);
        let calls = stub.calls();
        let cache = cache(Arc::new(stub));

        let mut reservation = cache.reserve(&[key(0, 1), key(1, 1), key(2, 1)]);
        let read = reservation.take_unread(&[key(0, 1)]);
        // Taken but never read.
        drop(reservation.take_unread(&[key(1, 1)]));
        cache.read(read).await;
        let values: Vec<CachedValue<Object>> = [key(0, 1), key(1, 1), key(2, 1)]
            .iter()
            .map(|key| reservation.value(key))
            .collect();
        // Never taken.
        drop(reservation);

        let mut values = values.into_iter();
        let read_value = values.next().expect("three values");
        assert!(read_value.await.expect("ok").is_some());
        for value in values {
            assert!(value.await.is_err(), "an unread key resolves to an error");
        }

        // Later reservations share the cancelled slots and read nothing.
        let later = cache.reserve(&[key(1, 1), key(2, 1)]);
        assert!(!later.is_unread(&key(1, 1)));
        assert!(!later.is_unread(&key(2, 1)));
        assert!(later.value(&key(2, 1)).await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn propagates_read_error_to_every_remaining_key() {
        let mut objs = HashMap::new();
        for i in 0..3 {
            objs.insert(key(i, 1), make_object(i, 1));
        }
        let fetcher = ErrorFetcher {
            objects: objs,
            ok_count: 1,
        };
        let cache = cache(Arc::new(fetcher));

        let keys = [key(0, 1), key(1, 1), key(2, 1)];
        let mut reservation = cache.reserve(&keys);
        cache.read(reservation.take_unread(&keys)).await;
        assert!(reservation.value(&key(0, 1)).await.expect("ok").is_some());
        assert!(reservation.value(&key(1, 1)).await.is_err());
        assert!(reservation.value(&key(2, 1)).await.is_err());

        // A later reservation of a failed key shares the cached error; it
        // must not deadlock and has nothing to read.
        let later = cache.reserve(&[key(2, 1)]);
        assert!(!later.is_unread(&key(2, 1)));
        assert!(later.value(&key(2, 1)).await.is_err());
        assert!(read_rows(&cache, &[key(2, 1)]).await.is_err());
    }

    /// A row the read did not ask for, or a second copy of one, fails every
    /// key the read has not resolved yet; keys resolved before it keep their
    /// rows.
    #[tokio::test]
    async fn unrequested_or_repeated_row_fails_unresolved_keys() {
        let row = |id: usize| (key(id, 1), make_object(id, 1));
        let keys = [key(0, 1), key(1, 1)];
        // (case, the rows the fetcher returns, whether key 0 keeps its row)
        let cases = [
            ("unrequested", vec![row(9), row(0), row(1)], false),
            ("repeated", vec![row(0), row(0), row(1)], true),
        ];
        for (case, rows, first_key_resolved) in cases {
            let cache = cache(Arc::new(ScriptedFetcher { rows }));
            let mut reservation = cache.reserve(&keys);
            cache.read(reservation.take_unread(&keys)).await;
            let first = reservation.value(&key(0, 1)).await;
            assert_eq!(
                first.is_ok_and(|row| row.is_some()),
                first_key_resolved,
                "{case}"
            );
            let error: tonic::Status = reservation.value(&key(1, 1)).await.expect_err(case).into();
            assert!(
                error.message().contains("did not request"),
                "{case}: {}",
                error.message()
            );
        }
    }

    #[tokio::test]
    async fn dropping_cache_aborts_read_and_releases_permit() {
        let mut objs = HashMap::new();
        objs.insert(key(0, 1), make_object(0, 1));
        let limiter = Arc::new(Semaphore::new(1));
        let stub = StubFetcher::new(objs)
            .with_delay(Duration::from_secs(60))
            .with_limiter(limiter.clone());
        let cache = ObjectCache::new(Arc::new(stub));

        let mut reservation = cache.reserve(&[key(0, 1)]);
        let read_end = cache.read(reservation.take_unread(&[key(0, 1)]));
        let value = reservation.value(&key(0, 1));

        for _ in 0..50 {
            if limiter.available_permits() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(limiter.available_permits(), 0, "permit should be held");

        drop(cache);
        tokio::time::timeout(Duration::from_secs(5), read_end)
            .await
            .expect("dropping the cache ends its reads");
        assert!(
            value.await.is_err(),
            "an aborted read resolves its keys to an error"
        );

        for _ in 0..50 {
            if limiter.available_permits() == 1 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("permit was not released after dropping cache with an in-flight read");
    }

    #[tokio::test]
    async fn read_fetches_its_keys_in_one_call() {
        // The cache dedupes against in-flight/resolved slots and reads the
        // unread keys it is given as ONE backend fetch. Read sizing is owned
        // by the caller, not the cache.
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
        let rows = read_rows(&cache, &keys).await.expect("ok");
        assert_eq!(rows.len(), key_count);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(*keys_seen.lock().expect("stub mutex"), vec![keys.clone()]);

        // Every key resolved in the cache: no read.
        let rows = read_rows(&cache, &keys).await.expect("ok");
        assert_eq!(rows.len(), key_count);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
