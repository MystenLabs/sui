// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream;
use futures::stream::BoxStream;
use futures::stream::TryReadyChunksError;
use sui_rpc_api::RpcError;

/// Chunk an upstream `Result` stream and run an async fn over each chunk, with
/// up to `max_concurrent_chunks` chunk futures at a time, preserving upstream
/// order in the output.
///
/// The closure returns a permit-holding BigTable stream. This helper drains it
/// to a local `Vec` inside the chunk future, so the permit is released before
/// any rows are emitted to the next stage. That avoids stacked `.buffered()`
/// deadlocks where downstream futures are waiting on the same semaphore needed
/// to drain upstream streams.
pub(crate) fn pipelined_chunks<I, O, F, Fut>(
    upstream: BoxStream<'static, Result<I, RpcError>>,
    chunk_size: usize,
    max_concurrent_chunks: usize,
    f: F,
) -> BoxStream<'static, Result<O, RpcError>>
where
    F: Fn(Vec<I>) -> Fut + Send + Sync + 'static,
    Fut:
        Future<Output = Result<BoxStream<'static, Result<O, RpcError>>, RpcError>> + Send + 'static,
    I: Send + 'static,
    O: Send + 'static,
{
    let f = Arc::new(f);
    upstream
        .try_ready_chunks(chunk_size)
        .map(move |chunk| {
            let f = f.clone();
            async move {
                let chunk = chunk.map_err(|TryReadyChunksError(_, err)| err)?;
                if chunk.is_empty() {
                    return Ok::<Vec<O>, RpcError>(Vec::new());
                }
                f(chunk).await?.try_collect::<Vec<_>>().await
            }
        })
        .buffered(max_concurrent_chunks)
        .flat_map(|result| match result {
            Ok(rows) => stream::iter(rows.into_iter().map(Ok)).boxed(),
            Err(err) => stream::once(async { Err(err) }).boxed(),
        })
        .boxed()
}

/// Buffers values arriving keyed-but-out-of-input-order and emits them in
/// input order as their slots become contiguous. Used by chunk fetch stages
/// to translate BigTable's arrival-order multi_get responses into the
/// input-key order expected downstream without first draining the entire
/// response into a HashMap.
///
/// At any moment the helper holds at most `(input.len() - emitted_so_far)`
/// values in `buffered`, so worst-case memory for one in-flight chunk equals
/// the input chunk size — same as the non-streaming `Vec`-collected version,
/// just spread across the stream's lifetime instead of all at once.
pub(crate) struct InputOrderEmitter<K: Eq + std::hash::Hash + Clone, V> {
    input: Vec<K>,
    next_idx: usize,
    buffered: HashMap<K, V>,
    pending: HashSet<K>,
}

impl<K: Eq + std::hash::Hash + Clone, V> InputOrderEmitter<K, V> {
    pub(crate) fn new(input: Vec<K>) -> Self {
        let pending = input.iter().cloned().collect();
        Self {
            input,
            next_idx: 0,
            buffered: HashMap::new(),
            pending,
        }
    }

    /// Insert a row. Returns rows now emittable in input order. Stalls past
    /// any input key whose value hasn't yet arrived; call `finish` once the
    /// upstream completes to require all remaining inputs.
    pub(crate) fn push(&mut self, key: K, value: V, context: &str) -> Result<Vec<V>, RpcError> {
        if !self.pending.remove(&key) {
            return Err(RpcError::new(
                tonic::Code::Internal,
                format!("{context}: unexpected row"),
            ));
        }

        self.buffered.insert(key, value);
        let mut out = Vec::new();
        while self.next_idx < self.input.len() {
            let k = self.input[self.next_idx].clone();
            if let Some(v) = self.buffered.remove(&k) {
                out.push(v);
                self.next_idx += 1;
            } else {
                break;
            }
        }
        Ok(out)
    }

    /// Yield remaining values only if every requested input arrived.
    pub(crate) fn finish(mut self, context: &str) -> Result<Vec<V>, RpcError> {
        let mut out = Vec::new();
        while self.next_idx < self.input.len() {
            let k = self.input[self.next_idx].clone();
            if let Some(v) = self.buffered.remove(&k) {
                out.push(v);
                self.next_idx += 1;
            } else {
                let missing = self.input[self.next_idx..]
                    .iter()
                    .filter(|k| !self.buffered.contains_key(*k))
                    .count();
                return Err(RpcError::new(
                    tonic::Code::Internal,
                    format!("{context}: missing {missing} row(s)"),
                ));
            }
        }
        Ok(out)
    }
}

/// Output item of `pipelined_keyed_batches`: an upstream item paired with
/// a map containing only that item's own keys.
pub(crate) type KeyedBatchOutput<I, K, V> = (I, Arc<HashMap<K, V>>);

/// Group `(item, keys)` pairs into batches and fetch each batch's keys.
/// Each emitted item is paired with a map of just its own keys (the
/// helper splits the per-batch superset back out per item, so callers
/// that iterate the map don't see other items' keys).
///
/// - `upstream_chunk_size`: max upstream items pulled together as one
///   ready burst before grouping kicks in (count of items, not keys).
/// - `max_keys_per_request`: max keys handed to `fetch` per call. A
///   batch whose union exceeds this is split across multiple parallel
///   `fetch` calls.
/// - `max_concurrent_fetches`: how many backend `fetch` calls run in
///   parallel. A fat batch contributes multiple `fetch` calls to this
///   budget — fat-item splits parallelize naturally rather than serializing
///   inside one slot.
///
/// Output is in input order. Partial batches flush at upstream `Pending`
/// boundaries, not held across them.
pub(crate) fn pipelined_keyed_batches<I, K, V, FetchFut>(
    upstream: BoxStream<'static, Result<(I, Vec<K>), RpcError>>,
    upstream_chunk_size: usize,
    max_keys_per_request: usize,
    max_concurrent_fetches: usize,
    fetch: impl Fn(Vec<K>) -> FetchFut + Send + Sync + 'static,
) -> BoxStream<'static, Result<KeyedBatchOutput<I, K, V>, RpcError>>
where
    I: Send + 'static,
    K: Ord + std::hash::Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    FetchFut: Future<Output = Result<HashMap<K, V>, RpcError>> + Send + 'static,
{
    assert!(
        upstream_chunk_size > 0,
        "pipelined_keyed_batches: upstream_chunk_size must be > 0"
    );
    assert!(
        max_keys_per_request > 0,
        "pipelined_keyed_batches: max_keys_per_request must be > 0"
    );
    assert!(
        max_concurrent_fetches > 0,
        "pipelined_keyed_batches: max_concurrent_fetches must be > 0"
    );
    let fetch = Arc::new(fetch);
    let fetch_results = upstream
        .try_ready_chunks(upstream_chunk_size)
        .map_err(|TryReadyChunksError(_, e)| e)
        .map_ok(move |upstream_chunk| {
            let requests = plan_fetches(upstream_chunk, max_keys_per_request);
            stream::iter(requests.into_iter().map(Ok::<_, RpcError>))
        })
        .try_flatten()
        .map(move |request_res| {
            let fetch = fetch.clone();
            async move {
                let result = match request_res? {
                    FetchRequest::NewGroup {
                        items,
                        keys,
                        requests_total,
                    } => {
                        let map = if keys.is_empty() {
                            HashMap::new()
                        } else {
                            fetch(keys).await?
                        };
                        FetchResult::NewGroup {
                            items,
                            requests_total,
                            map,
                        }
                    }
                    FetchRequest::Continuation { keys } => FetchResult::Continuation {
                        map: fetch(keys).await?,
                    },
                };
                Ok::<_, RpcError>(result)
            }
        })
        .buffered(max_concurrent_fetches);

    async_stream::try_stream! {
        let mut reassembler = Reassembler::<I, K, V>::new();
        futures::pin_mut!(fetch_results);
        while let Some(result) = fetch_results.next().await {
            let result = result?;
            for emission in reassembler.push(result) {
                yield emission;
            }
        }
    }
    .boxed()
}

// --- Stage 1: plan the per-fetch requests for a burst of items. ---

/// One backend `fetch` call's worth of work. Each logical group of
/// co-fetched items emits one `NewGroup` request followed by
/// `requests_total - 1` `Continuation` requests (continuation only
/// happens when a single fat item's keys exceed the per-fetch cap).
enum FetchRequest<I, K> {
    /// Opens a new logical group: carries the items that will render
    /// from this group's merged map, plus the first chunk of keys to
    /// fetch and the total number of requests the reassembler should
    /// expect for this group.
    NewGroup {
        items: Vec<(I, Vec<K>)>,
        keys: Vec<K>,
        requests_total: usize,
    },
    /// Continues the most recently opened group with another chunk of
    /// keys. The reassembler merges this chunk's map into the group's
    /// pending map.
    Continuation { keys: Vec<K> },
}

/// Plan the `FetchRequest`s needed to satisfy a chunk of `(item, keys)`
/// pairs. Two cases handled in one pass:
///
/// - Small items get GROUPED: consecutive items whose combined deduped
///   keys still fit in one request share a request.
/// - Fat items get SPLIT: an item whose own keys exceed `max_keys` forms
///   its own multi-request fan-out.
///
/// Each item's keys are deduped on entry. An item with no new keys still
/// counts as 1 against the per-request budget, so zero-key items
/// eventually flush instead of being grouped indefinitely.
fn plan_fetches<I, K>(items: Vec<(I, Vec<K>)>, max_keys: usize) -> Vec<FetchRequest<I, K>>
where
    K: Ord + Clone,
{
    assert!(max_keys > 0, "plan_fetches: max_keys must be > 0");
    let mut out: Vec<FetchRequest<I, K>> = Vec::new();
    let mut group = InProgressGroup::<I, K>::new();

    for (item, keys) in items {
        let keys: Vec<K> = keys
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        if keys.len() > max_keys {
            // Fat item: flush whatever group was building, then emit this
            // item as its own multi-request fan-out.
            group.flush_into(&mut out);
            push_fat_item(&mut out, item, keys, max_keys);
            continue;
        }

        if group.union.len() + group.new_keys_count(&keys) > max_keys {
            group.flush_into(&mut out);
        }
        group.push(item, keys);
    }
    group.flush_into(&mut out);
    out
}

/// In-progress group of small items waiting to be emitted as one
/// `FetchRequest`. The fetch budget is the size of the running deduped
/// `union`; group size is also implicitly bounded by the caller's
/// `upstream_chunk_size`, so zero-key / fully-overlapping items don't
/// need a per-item floor to force flushes.
struct InProgressGroup<I, K> {
    items: Vec<(I, Vec<K>)>,
    union: BTreeSet<K>,
}

impl<I, K: Ord + Clone> InProgressGroup<I, K> {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            union: BTreeSet::new(),
        }
    }

    /// New unique keys this item would add to the group's deduped union.
    fn new_keys_count(&self, keys: &[K]) -> usize {
        keys.iter().filter(|k| !self.union.contains(*k)).count()
    }

    fn push(&mut self, item: I, keys: Vec<K>) {
        self.union.extend(keys.iter().cloned());
        self.items.push((item, keys));
    }

    /// Emit the current group as one `NewGroup` request (if non-empty)
    /// and reset.
    fn flush_into(&mut self, out: &mut Vec<FetchRequest<I, K>>) {
        if self.items.is_empty() {
            return;
        }
        out.push(FetchRequest::NewGroup {
            items: std::mem::take(&mut self.items),
            keys: std::mem::take(&mut self.union).into_iter().collect(),
            requests_total: 1,
        });
    }
}

/// Emit one fat item as a self-contained run of K requests, where K =
/// ceil(item.keys / max_keys). One `NewGroup` carries the item itself;
/// the remaining keys ride on `Continuation` requests.
fn push_fat_item<I, K>(out: &mut Vec<FetchRequest<I, K>>, item: I, keys: Vec<K>, max_keys: usize)
where
    K: Clone,
{
    let chunks: Vec<Vec<K>> = keys.chunks(max_keys).map(<[K]>::to_vec).collect();
    let requests_total = chunks.len();
    let mut iter = chunks.into_iter();
    let first = iter.next().expect("fat item has ≥ 1 chunk");
    out.push(FetchRequest::NewGroup {
        items: vec![(item, keys)],
        keys: first,
        requests_total,
    });
    for chunk in iter {
        out.push(FetchRequest::Continuation { keys: chunk });
    }
}

// --- Stage 2: reassemble fetch results back into rendered items. ---

enum FetchResult<I, K, V> {
    NewGroup {
        items: Vec<(I, Vec<K>)>,
        requests_total: usize,
        map: HashMap<K, V>,
    },
    Continuation {
        map: HashMap<K, V>,
    },
}

/// Reassembles a logical batch's `FetchResult`s as they emerge in input
/// order from `.buffered`. Holds at most one pending batch at a time —
/// `.buffered` preserves order, so all results for batch B arrive
/// contiguously and before any result for batch C.
struct Reassembler<I, K, V> {
    pending: Option<PendingBatch<I, K, V>>,
}

struct PendingBatch<I, K, V> {
    items: Vec<(I, Vec<K>)>,
    map: HashMap<K, V>,
    requests_remaining: usize,
}

impl<I, K, V> Reassembler<I, K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    fn new() -> Self {
        Self { pending: None }
    }

    /// Ingest one `FetchResult`. Returns the rendered items if this
    /// result completes the current batch, otherwise an empty Vec.
    fn push(&mut self, result: FetchResult<I, K, V>) -> Vec<KeyedBatchOutput<I, K, V>> {
        match result {
            FetchResult::NewGroup {
                items,
                requests_total,
                map,
            } => {
                assert!(
                    self.pending.is_none(),
                    "previous batch did not complete before next started"
                );
                self.pending = Some(PendingBatch {
                    items,
                    map,
                    requests_remaining: requests_total - 1,
                });
            }
            FetchResult::Continuation { map } => {
                let pending = self
                    .pending
                    .as_mut()
                    .expect("continuation request arrived without a NewGroup");
                pending.map.extend(map);
                pending.requests_remaining -= 1;
            }
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.requests_remaining == 0)
        {
            let pending = self.pending.take().expect("just-checked");
            // Split the per-batch superset back out into per-item maps
            // so callers that iterate the map (e.g. ObjectSet builders)
            // don't see other items' keys. Missing keys are dropped —
            // matches the prior `objects.get(k)`-and-skip contract.
            return pending
                .items
                .into_iter()
                .map(|(item, keys)| {
                    let item_map: HashMap<K, V> = keys
                        .into_iter()
                        .filter_map(|k| pending.map.get(&k).map(|v| (k, v.clone())))
                        .collect();
                    (item, Arc::new(item_map))
                })
                .collect();
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use futures::stream;
    use tokio::sync::Semaphore;
    use tokio::sync::oneshot;
    use tokio::time::timeout;

    use super::*;

    struct ActiveGuard {
        active: Arc<AtomicUsize>,
    }

    impl ActiveGuard {
        fn new(active: Arc<AtomicUsize>, peak: Arc<AtomicUsize>) -> Self {
            let current = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(current, Ordering::SeqCst);
            Self { active }
        }
    }

    impl Drop for ActiveGuard {
        fn drop(&mut self) {
            self.active.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn preserves_input_order_when_chunks_complete_out_of_order() {
        // Upstream: 0..50, in chunks of 5, processed with delay inversely
        // proportional to chunk index so later chunks finish first. Output
        // must still be 0..50 in order.
        let upstream: BoxStream<'static, Result<u64, RpcError>> =
            stream::iter((0..50u64).map(Ok::<_, RpcError>)).boxed();

        let stream = pipelined_chunks(upstream, 5, 8, |chunk: Vec<u64>| async move {
            let first = chunk[0];
            let delay = Duration::from_millis(50u64.saturating_sub(first));
            tokio::time::sleep(delay).await;
            Ok::<_, RpcError>(stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed())
        });

        let collected: Vec<u64> = stream.try_collect().await.expect("ok");
        let expected: Vec<u64> = (0..50).collect();
        assert_eq!(collected, expected);
    }

    #[tokio::test]
    async fn closure_in_chunk_order_is_preserved_per_call() {
        // The closure receives chunks; verify each chunk is contiguous and
        // the per-chunk Vec is identical to what was received (the helper
        // doesn't reorder within a chunk).
        let upstream: BoxStream<'static, Result<u64, RpcError>> =
            stream::iter((0..20u64).map(Ok::<_, RpcError>)).boxed();
        let seen = Arc::new(Mutex::new(Vec::<Vec<u64>>::new()));
        let seen_for_closure = seen.clone();

        let stream = pipelined_chunks(upstream, 4, 2, move |chunk: Vec<u64>| {
            let seen = seen_for_closure.clone();
            async move {
                seen.lock().unwrap().push(chunk.clone());
                Ok::<_, RpcError>(stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed())
            }
        });

        let collected: Vec<u64> = stream.try_collect().await.expect("ok");
        assert_eq!(collected, (0..20).collect::<Vec<_>>());
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 5, "20 items / chunk_size=4 = 5 chunks");
        for (i, c) in seen.iter().enumerate() {
            let start = (i * 4) as u64;
            assert_eq!(c, &(start..start + 4).collect::<Vec<_>>());
        }
    }

    #[tokio::test]
    async fn limits_active_chunk_work_to_max_concurrent_chunks() {
        let upstream: BoxStream<'static, Result<u64, RpcError>> =
            stream::iter((0..20u64).map(Ok::<_, RpcError>)).boxed();
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let stream = pipelined_chunks(upstream, 1, 3, {
            let active = active.clone();
            let peak = peak.clone();
            move |chunk: Vec<u64>| {
                let guard = ActiveGuard::new(active.clone(), peak.clone());
                async move {
                    let inner = async_stream::try_stream! {
                        let _guard = guard;
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        for item in chunk {
                            yield item;
                        }
                    };
                    Ok::<_, RpcError>(inner.boxed())
                }
            }
        });

        let collected: Vec<u64> = stream.try_collect().await.expect("ok");
        assert_eq!(collected, (0..20).collect::<Vec<_>>());
        assert!(
            peak.load(Ordering::SeqCst) <= 3,
            "active chunk work exceeded max_concurrent_chunks"
        );
    }

    #[tokio::test]
    async fn propagates_upstream_error_after_prior_completed_chunks() {
        let upstream: BoxStream<'static, Result<u64, RpcError>> = stream::iter(vec![
            Ok(0u64),
            Ok(1),
            Ok(2),
            Ok(3),
            Err(RpcError::new(tonic::Code::Internal, "boom")),
            Ok(4),
        ])
        .boxed();

        let stream = pipelined_chunks(upstream, 2, 2, |chunk: Vec<u64>| async move {
            Ok::<_, RpcError>(stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed())
        });
        futures::pin_mut!(stream);

        for expected in 0..4u64 {
            assert_eq!(stream.try_next().await.expect("ok"), Some(expected));
        }
        let err = stream.try_next().await.expect_err("upstream error");
        let status: tonic::Status = err.into();
        assert_eq!(status.message(), "boom");
    }

    #[tokio::test]
    async fn does_not_emit_rows_until_chunk_stream_drains() {
        let upstream: BoxStream<'static, Result<u64, RpcError>> =
            stream::iter((0..2u64).map(Ok::<_, RpcError>)).boxed();
        let (first_drained_tx, first_drained_rx) = oneshot::channel();
        let (release_second_tx, release_second_rx) = oneshot::channel();
        let first_drained_tx = Arc::new(Mutex::new(Some(first_drained_tx)));
        let release_second_rx = Arc::new(Mutex::new(Some(release_second_rx)));

        let stream = pipelined_chunks(upstream, 2, 1, move |chunk: Vec<u64>| {
            let first_drained_tx = first_drained_tx.clone();
            let release_second_rx = release_second_rx.clone();
            async move {
                let inner = async_stream::try_stream! {
                    yield chunk[0];
                    if let Some(tx) = first_drained_tx.lock().unwrap().take() {
                        let _ = tx.send(());
                    }
                    let release_second_rx = release_second_rx
                        .lock()
                        .unwrap()
                        .take()
                        .expect("release receiver present");
                    let _ = release_second_rx.await;
                    yield chunk[1];
                };
                Ok::<_, RpcError>(inner.boxed())
            }
        });

        futures::pin_mut!(stream);
        let next = stream.try_next();
        tokio::pin!(next);
        tokio::select! {
            item = &mut next => panic!("row emitted before chunk drained: {item:?}"),
            res = first_drained_rx => res.expect("first row drained inside helper"),
        }

        release_second_tx.send(()).expect("release receiver alive");
        let first = next.await.expect("ok").expect("some");
        assert_eq!(first, 0);
        let second = stream.try_next().await.expect("ok").expect("some");
        assert_eq!(second, 1);
        assert!(stream.try_next().await.expect("ok").is_none());
    }

    #[tokio::test]
    async fn propagates_inner_stream_error() {
        let upstream: BoxStream<'static, Result<u64, RpcError>> =
            stream::iter((0..3u64).map(Ok::<_, RpcError>)).boxed();
        let stream = pipelined_chunks(upstream, 3, 1, |chunk: Vec<u64>| async move {
            let inner = async_stream::try_stream! {
                yield chunk[0];
                Err(RpcError::new(tonic::Code::Internal, "inner stream boom"))?;
                yield chunk[1];
            };
            Ok::<_, RpcError>(inner.boxed())
        });
        let result: Result<Vec<u64>, RpcError> = stream.try_collect().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cancellation_releases_permit_while_opening_chunk() {
        let upstream: BoxStream<'static, Result<u64, RpcError>> =
            stream::iter(vec![Ok::<_, RpcError>(1u64)]).boxed();
        let limiter = Arc::new(Semaphore::new(1));

        let stream = pipelined_chunks(upstream, 1, 1, {
            let limiter = limiter.clone();
            move |_chunk: Vec<u64>| {
                let limiter = limiter.clone();
                async move {
                    let permit =
                        limiter.clone().acquire_owned().await.map_err(|_| {
                            RpcError::new(tonic::Code::Internal, "test limiter closed")
                        })?;
                    let _permit = permit;
                    std::future::pending::<
                        Result<BoxStream<'static, Result<u64, RpcError>>, RpcError>,
                    >()
                    .await
                }
            }
        });

        let handle = tokio::spawn(async move { stream.try_collect::<Vec<_>>().await });
        wait_for_available_permits(&limiter, 0).await;
        handle.abort();
        let _ = handle.await;
        wait_for_available_permits(&limiter, 1).await;
    }

    #[tokio::test]
    async fn cancellation_releases_permit_while_draining_chunk() {
        let upstream: BoxStream<'static, Result<u64, RpcError>> =
            stream::iter(vec![Ok::<_, RpcError>(1u64)]).boxed();
        let limiter = Arc::new(Semaphore::new(1));

        let stream = pipelined_chunks(upstream, 1, 1, {
            let limiter = limiter.clone();
            move |chunk: Vec<u64>| {
                let limiter = limiter.clone();
                async move {
                    let inner = async_stream::try_stream! {
                        let permit = limiter.clone().acquire_owned().await.map_err(|_| {
                            RpcError::new(tonic::Code::Internal, "test limiter closed")
                        })?;
                        let _permit = permit;
                        std::future::pending::<()>().await;
                        yield chunk[0];
                    };
                    Ok::<_, RpcError>(inner.boxed())
                }
            }
        });

        let handle = tokio::spawn(async move { stream.try_collect::<Vec<_>>().await });
        wait_for_available_permits(&limiter, 0).await;
        handle.abort();
        let _ = handle.await;
        wait_for_available_permits(&limiter, 1).await;
    }

    #[tokio::test]
    async fn stacked_pipelines_do_not_deadlock_with_one_permit() {
        let limiter = Arc::new(Semaphore::new(1));
        let upstream: BoxStream<'static, Result<u64, RpcError>> =
            stream::iter((0..10u64).map(Ok::<_, RpcError>)).boxed();

        let stage1 = pipelined_chunks(upstream, 2, 1, {
            let limiter = limiter.clone();
            move |chunk| gated_chunk_stream(limiter.clone(), chunk)
        });
        let stage2 = pipelined_chunks(stage1, 1, 1, {
            let limiter = limiter.clone();
            move |chunk| gated_chunk_stream(limiter.clone(), chunk)
        });

        let out = timeout(Duration::from_secs(1), stage2.try_collect::<Vec<_>>())
            .await
            .expect("stacked pipeline timed out")
            .expect("stacked pipeline ok");
        assert_eq!(out, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn input_order_emitter_emits_immediately_when_in_order() {
        let mut e: InputOrderEmitter<u64, &'static str> = InputOrderEmitter::new(vec![1, 2, 3]);
        assert_eq!(e.push(1, "a", "test").expect("1 present"), vec!["a"]);
        assert_eq!(e.push(2, "b", "test").expect("2 present"), vec!["b"]);
        assert_eq!(e.push(3, "c", "test").expect("3 present"), vec!["c"]);
        assert!(e.finish("test").expect("all rows present").is_empty());
    }

    #[test]
    fn input_order_emitter_buffers_until_prefix_completes() {
        let mut e: InputOrderEmitter<u64, &'static str> = InputOrderEmitter::new(vec![1, 2, 3, 4]);
        assert!(
            e.push(3, "c", "test").expect("3 present").is_empty(),
            "3 alone can't emit"
        );
        assert!(
            e.push(2, "b", "test").expect("2 present").is_empty(),
            "2 alone can't emit (1 missing)"
        );
        assert!(
            e.push(4, "d", "test").expect("4 present").is_empty(),
            "4 alone can't emit (1 missing)"
        );
        assert_eq!(
            e.push(1, "a", "test").expect("1 present"),
            vec!["a", "b", "c", "d"]
        );
        assert!(e.finish("test").expect("all rows present").is_empty());
    }

    #[test]
    fn input_order_emitter_finish_errors_on_missing_middle() {
        let mut e: InputOrderEmitter<u64, &'static str> = InputOrderEmitter::new(vec![1, 2, 3, 4]);
        assert_eq!(e.push(1, "a", "test").expect("1 present"), vec!["a"]);
        assert!(
            e.push(3, "c", "test").expect("3 present").is_empty(),
            "3 stalls behind missing 2"
        );
        assert_eq!(
            e.push(4, "d", "test").expect("4 present"),
            Vec::<&'static str>::new(),
            "still stalled"
        );
        assert!(e.finish("test").is_err());
    }

    #[test]
    fn input_order_emitter_finish_errors_on_missing_at_start() {
        let e: InputOrderEmitter<u64, &'static str> = InputOrderEmitter::new(vec![1, 2]);
        assert!(e.finish("test").is_err());
    }

    #[test]
    fn input_order_emitter_push_errors_on_unexpected_row() {
        let mut e: InputOrderEmitter<u64, &'static str> = InputOrderEmitter::new(vec![1, 2]);
        assert!(e.push(3, "c", "test").is_err());
    }

    #[test]
    fn input_order_emitter_push_errors_on_duplicate_row() {
        let mut e: InputOrderEmitter<u64, &'static str> = InputOrderEmitter::new(vec![1, 2]);
        assert_eq!(e.push(1, "a", "test").expect("1 present"), vec!["a"]);
        assert!(e.push(1, "a again", "test").is_err());
    }

    #[tokio::test]
    async fn propagates_closure_error() {
        let upstream: BoxStream<'static, Result<u64, RpcError>> =
            stream::iter((0..10u64).map(Ok::<_, RpcError>)).boxed();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_closure = calls.clone();

        let stream = pipelined_chunks(upstream, 4, 2, move |chunk: Vec<u64>| {
            let calls = calls_for_closure.clone();
            async move {
                let n = calls.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Ok::<_, RpcError>(
                        stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed(),
                    )
                } else {
                    Err(RpcError::new(tonic::Code::Internal, "fail"))
                }
            }
        });
        let result: Result<Vec<u64>, RpcError> = stream.try_collect().await;
        assert!(result.is_err());
    }

    async fn gated_chunk_stream(
        limiter: Arc<Semaphore>,
        chunk: Vec<u64>,
    ) -> Result<BoxStream<'static, Result<u64, RpcError>>, RpcError> {
        let inner = async_stream::try_stream! {
            let permit = limiter.clone().acquire_owned().await.map_err(|_| {
                RpcError::new(tonic::Code::Internal, "test limiter closed")
            })?;
            let _permit = permit;
            for item in chunk {
                yield item;
                tokio::task::yield_now().await;
            }
        };
        Ok::<_, RpcError>(inner.boxed())
    }

    async fn wait_for_available_permits(limiter: &Semaphore, permits: usize) {
        for _ in 0..50 {
            if limiter.available_permits() == permits {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!(
            "expected {permits} available permits, got {}",
            limiter.available_permits()
        );
    }

    // ---- pack_keyed_batches & pipelined_keyed_batches tests ----

    use futures::SinkExt;
    use futures::channel::mpsc;

    type TestItem = (u32, Vec<i32>);

    fn iter_upstream(
        items: Vec<Result<TestItem, RpcError>>,
    ) -> BoxStream<'static, Result<TestItem, RpcError>> {
        stream::iter(items).boxed()
    }

    /// Pattern-match helper: assert request is a NewGroup and return
    /// `(item_ids, keys, requests_total)`.
    fn unwrap_new_group<I: Copy, K: Clone>(req: &FetchRequest<I, K>) -> (Vec<I>, Vec<K>, usize) {
        match req {
            FetchRequest::NewGroup {
                items,
                keys,
                requests_total,
            } => (
                items.iter().map(|(i, _)| *i).collect(),
                keys.clone(),
                *requests_total,
            ),
            FetchRequest::Continuation { .. } => panic!("expected NewGroup, got Continuation"),
        }
    }

    fn unwrap_continuation<I, K: Clone>(req: &FetchRequest<I, K>) -> Vec<K> {
        match req {
            FetchRequest::Continuation { keys } => keys.clone(),
            FetchRequest::NewGroup { .. } => panic!("expected Continuation, got NewGroup"),
        }
    }

    #[test]
    fn plan_fetches_groups_small_items_into_shared_requests() {
        let items = vec![
            (0u32, vec![0, 1, 2]),
            (1, vec![3, 4, 5]),
            (2, vec![6, 7, 8]),
            (3, vec![9, 10, 11]),
        ];
        let reqs = plan_fetches(items, 6);
        // max = 6: items 0+1 fit (3+3=6), close request, items 2+3 fit.
        assert_eq!(reqs.len(), 2);
        let (ids0, keys0, total0) = unwrap_new_group(&reqs[0]);
        let (ids1, keys1, total1) = unwrap_new_group(&reqs[1]);
        assert_eq!(ids0, [0, 1]);
        assert_eq!(ids1, [2, 3]);
        assert_eq!(keys0.len(), 6);
        assert_eq!(keys1.len(), 6);
        assert_eq!(total0, 1);
        assert_eq!(total1, 1);
    }

    #[test]
    fn plan_fetches_dedupes_in_running_union() {
        // Overlapping keys across items keep request size accurate.
        let items = vec![
            (0u32, vec![1, 2, 3]),
            (1, vec![2, 3, 4]),
            (2, vec![3, 4, 5]),
        ];
        let reqs = plan_fetches(items, 5);
        // Deltas 3+1+1 = 5 = max. All three share one request.
        assert_eq!(reqs.len(), 1);
        let (ids, keys, _) = unwrap_new_group(&reqs[0]);
        assert_eq!(ids, [0, 1, 2]);
        assert_eq!(keys, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn plan_fetches_dedupes_within_a_single_item() {
        // [A, A, A] must count as 1 against the budget (not 3) so we
        // don't flush prematurely. Items also carry deduped key vecs.
        let items = vec![(0u32, vec![1, 1, 1]), (1, vec![2, 2]), (2, vec![3])];
        let reqs = plan_fetches(items, 3);
        assert_eq!(reqs.len(), 1);
        let FetchRequest::NewGroup {
            items: items_in_req,
            keys,
            ..
        } = &reqs[0]
        else {
            panic!("expected NewGroup");
        };
        assert_eq!(
            items_in_req.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(keys, &vec![1, 2, 3]);
        assert_eq!(items_in_req[0].1, vec![1]);
        assert_eq!(items_in_req[1].1, vec![2]);
        assert_eq!(items_in_req[2].1, vec![3]);
    }

    #[test]
    fn plan_fetches_collapses_zero_key_items_into_one_group() {
        // Zero-key items add nothing to the running union, so they all
        // collapse into a single group with one no-op fetch — preferable
        // to splitting them across N pointless empty requests.
        let items: Vec<_> = (0u32..7).map(|i| (i, Vec::<i32>::new())).collect();
        let reqs = plan_fetches(items, 3);
        assert_eq!(reqs.len(), 1);
        let (ids, keys, _) = unwrap_new_group(&reqs[0]);
        assert_eq!(ids.len(), 7);
        assert!(keys.is_empty());
    }

    #[test]
    fn plan_fetches_fat_item_flushes_group_then_self_splits() {
        // Two small items pre-flush as one request; a fat item then
        // forms its own multi-request fan-out (NewGroup + 2 Continuations).
        let items = vec![
            (0u32, vec![0, 1, 2]),
            (1, vec![3, 4, 5]),
            (2, (10..30).collect::<Vec<i32>>()), // 20 keys, max 8 → 3 requests
        ];
        let reqs = plan_fetches(items, 8);
        assert_eq!(reqs.len(), 4);
        // Request 0: pre-flush of the small group.
        let (ids0, keys0, total0) = unwrap_new_group(&reqs[0]);
        assert_eq!(ids0, [0, 1]);
        assert_eq!(keys0.len(), 6);
        assert_eq!(total0, 1);
        // Request 1: opens the fat item's group.
        let (ids1, keys1, total1) = unwrap_new_group(&reqs[1]);
        assert_eq!(ids1, [2]);
        assert_eq!(keys1.len(), 8);
        assert_eq!(total1, 3);
        // Requests 2-3: continuation chunks for the fat item.
        assert_eq!(unwrap_continuation(&reqs[2]).len(), 8);
        assert_eq!(unwrap_continuation(&reqs[3]).len(), 4);
    }

    #[tokio::test]
    async fn helper_flushes_tail_of_burst_when_upstream_goes_pending() {
        // mpsc channel: when nothing is queued the receiver returns
        // `Pending`. `try_ready_chunks` inside the helper therefore yields
        // whatever is buffered as one burst, which becomes one or more
        // sub-batches — the trailing partial sub-batch fires immediately
        // rather than stalling waiting for more upstream items.
        let (mut tx, rx) = mpsc::channel::<Result<TestItem, RpcError>>(8);
        let upstream = rx.boxed();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_fetch = calls.clone();
        let mut helper = pipelined_keyed_batches::<u32, i32, i32, _>(
            upstream,
            100,
            100,
            4,
            move |keys: Vec<i32>| {
                let calls = calls_for_fetch.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, RpcError>(keys.into_iter().map(|k| (k, k)).collect())
                }
            },
        )
        .map_ok(|(item, _map)| item);

        tx.send(Ok((0, vec![1, 2, 3]))).await.expect("send");
        tx.send(Ok((1, vec![4, 5, 6]))).await.expect("send");

        // Helper must emit both items without us having to send a third —
        // the burst boundary triggers the flush.
        let first = timeout(Duration::from_millis(500), helper.next())
            .await
            .expect("helper stalled past pending — partial burst should have flushed")
            .expect("some")
            .expect("ok");
        let second = timeout(Duration::from_millis(500), helper.next())
            .await
            .expect("helper stalled before second item")
            .expect("some")
            .expect("ok");
        assert_eq!(first, 0);
        assert_eq!(second, 1);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "single fetch for one burst"
        );

        tx.close_channel();
        assert!(helper.next().await.is_none());
    }

    #[tokio::test]
    async fn helper_packing_math_one_fetch_per_batch() {
        // 10 items × 5 keys each (50 keys total), budget = 10.
        // Expected: 5 fetches, each ≤ 10 keys, items emitted in order.
        let items: Vec<_> = (0u32..10)
            .map(|i| {
                Ok((
                    i,
                    ((i as i32) * 5..(i as i32) * 5 + 5).collect::<Vec<i32>>(),
                ))
            })
            .collect();
        let upstream = iter_upstream(items);
        let calls = Arc::new(AtomicUsize::new(0));
        let max_keys_seen = Arc::new(AtomicUsize::new(0));
        let calls_for_fetch = calls.clone();
        let max_keys_for_fetch = max_keys_seen.clone();
        let out: Vec<u32> = pipelined_keyed_batches::<u32, i32, i32, _>(
            upstream,
            10,
            10,
            4,
            move |keys: Vec<i32>| {
                let calls = calls_for_fetch.clone();
                let max_keys = max_keys_for_fetch.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    max_keys.fetch_max(keys.len(), Ordering::SeqCst);
                    Ok::<_, RpcError>(keys.into_iter().map(|k| (k, k)).collect())
                }
            },
        )
        .map_ok(|(item, _map)| item)
        .try_collect()
        .await
        .expect("ok");

        assert_eq!(out, (0u32..10).collect::<Vec<_>>());
        let n = calls.load(Ordering::SeqCst);
        assert_eq!(n, 5, "expected 5 fetches (10 items × 5 keys / 10 budget)");
        assert!(
            max_keys_seen.load(Ordering::SeqCst) <= 10,
            "no fetch should exceed the budget"
        );
    }

    #[tokio::test]
    async fn helper_fat_item_splits_into_chunks() {
        // One item with 25 keys, max=10 → 3 fetches of sizes {10, 10, 5}
        // (run in parallel up to max_concurrent_fetches; merged via the
        // reassembler before the item renders).
        let upstream = iter_upstream(vec![Ok((42u32, (0i32..25).collect::<Vec<_>>()))]);
        let call_sizes = Arc::new(Mutex::new(Vec::<usize>::new()));
        let call_sizes_for_fetch = call_sizes.clone();
        let out: Vec<(u32, usize)> = pipelined_keyed_batches::<u32, i32, i32, _>(
            upstream,
            10,
            10,
            4,
            move |keys: Vec<i32>| {
                let sizes = call_sizes_for_fetch.clone();
                async move {
                    sizes.lock().expect("mutex").push(keys.len());
                    Ok::<_, RpcError>(keys.into_iter().map(|k| (k, k)).collect())
                }
            },
        )
        .map_ok(|(item, map)| (item, map.len()))
        .try_collect()
        .await
        .expect("ok");

        assert_eq!(out, vec![(42u32, 25)]);
        // Fetches can complete in any order under buffered concurrency;
        // assert the multiset, not the sequence.
        let mut sizes = call_sizes.lock().expect("mutex").clone();
        sizes.sort_unstable();
        assert_eq!(sizes, vec![5, 10, 10]);
    }

    #[tokio::test]
    async fn helper_fat_item_chunks_run_in_parallel() {
        // Each chunk's fetch sleeps for 100ms. With max_concurrent_fetches
        // = 4 (≥ chunk count of 3), all three chunks should run
        // concurrently — total wall time well under 3 × 100ms.
        let upstream = iter_upstream(vec![Ok((0u32, (0i32..30).collect::<Vec<_>>()))]);
        let started = std::time::Instant::now();
        let _: Vec<u32> = pipelined_keyed_batches::<u32, i32, i32, _>(
            upstream,
            10,
            10,
            4,
            move |keys: Vec<i32>| async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok::<_, RpcError>(keys.into_iter().map(|k| (k, k)).collect())
            },
        )
        .map_ok(|(item, _map)| item)
        .try_collect()
        .await
        .expect("ok");
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_millis(250),
            "fat-item chunks should run in parallel; got {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn helper_preserves_input_order_under_out_of_order_fetch_completion() {
        // Each item is its own batch (budget = 1). Fetch closure delays
        // inversely by item index so later batches resolve first; helper
        // must still emit items in input order.
        let items: Vec<_> = (0u32..6).map(|i| Ok((i, vec![i as i32]))).collect();
        let upstream = iter_upstream(items);
        let out: Vec<u32> = pipelined_keyed_batches::<u32, i32, i32, _>(
            upstream,
            1,
            1,
            6,
            move |keys: Vec<i32>| async move {
                let key = keys[0];
                let delay = Duration::from_millis(50u64.saturating_sub(key as u64 * 8));
                tokio::time::sleep(delay).await;
                Ok::<_, RpcError>(keys.into_iter().map(|k| (k, k)).collect())
            },
        )
        .map_ok(|(item, _map)| item)
        .try_collect()
        .await
        .expect("ok");

        assert_eq!(out, (0u32..6).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn helper_propagates_fetch_error() {
        let upstream = iter_upstream(vec![Ok((0u32, vec![1, 2, 3])), Ok((1u32, vec![4, 5, 6]))]);
        let result: Result<Vec<u32>, RpcError> = pipelined_keyed_batches::<u32, i32, i32, _>(
            upstream,
            10,
            10,
            1,
            move |_keys: Vec<i32>| async move {
                Err::<HashMap<i32, i32>, _>(RpcError::new(tonic::Code::Internal, "boom"))
            },
        )
        .map_ok(|(item, _map)| item)
        .try_collect()
        .await;
        let err = result.expect_err("expected fetch error to propagate");
        let status: tonic::Status = err.into();
        assert_eq!(status.message(), "boom");
    }

    #[tokio::test]
    async fn helper_per_item_map_contains_only_its_own_keys() {
        // Two items packed into one batch share a single backend fetch;
        // the helper splits the superset back out so each item only sees
        // its own keys. Prevents callers that iterate the map (e.g. the
        // ObjectSet builder in list_checkpoints) from contaminating one
        // item's view with another's keys.
        let upstream = iter_upstream(vec![Ok((0u32, vec![10, 11])), Ok((1u32, vec![20, 21]))]);
        let out: Vec<(u32, Vec<(i32, i32)>)> = pipelined_keyed_batches::<u32, i32, i32, _>(
            upstream,
            10,
            10,
            2,
            move |keys: Vec<i32>| async move {
                Ok::<_, RpcError>(keys.into_iter().map(|k| (k, k)).collect())
            },
        )
        .map_ok(|(item, map)| {
            let mut entries: Vec<(i32, i32)> = map.iter().map(|(k, v)| (*k, *v)).collect();
            entries.sort_unstable();
            (item, entries)
        })
        .try_collect()
        .await
        .expect("ok");

        assert_eq!(
            out,
            vec![
                (0u32, vec![(10, 10), (11, 11)]),
                (1u32, vec![(20, 20), (21, 21)]),
            ]
        );
    }

    #[tokio::test]
    async fn helper_per_item_map_drops_keys_missing_from_fetch_result() {
        // Fetch returns nothing; per-item map should be empty rather than
        // containing entries with missing values. Matches the prior
        // `objects.get(k)`-and-skip contract.
        let upstream = iter_upstream(vec![Ok((0u32, vec![1, 2, 3]))]);
        let out: Vec<(u32, usize)> = pipelined_keyed_batches::<u32, i32, i32, _>(
            upstream,
            10,
            10,
            1,
            move |_keys: Vec<i32>| async move { Ok::<_, RpcError>(HashMap::new()) },
        )
        .map_ok(|(item, map)| (item, map.len()))
        .try_collect()
        .await
        .expect("ok");

        assert_eq!(out, vec![(0u32, 0)]);
    }
}
