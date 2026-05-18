// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;
use std::task::Poll;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream;
use futures::stream::BoxStream;
use sui_rpc_api::RpcError;

// Re-export so handler-layer code can spell the marker type without
// directly importing from sui-inverted-index. The pipeline shape lives
// in this module; sui-inverted-index just happens to define the carrier
// type the bitmap eval already produces.
pub(crate) use sui_inverted_index::Watermarked;

/// Chunk an upstream stream of `Watermarked<I>` and run an async fn over each
/// chunk of Items, preserving upstream order in the output. Up to
/// `max_concurrent_chunks` chunk futures run at a time.
///
/// `Watermarked::Watermark`s travel through with their original ordering
/// relative to Items: a marker that arrived between Items A and B in the
/// upstream will arrive between A's transformed output and B's transformed
/// output downstream. The watermark's invariant — "every Item before me has
/// been emitted" — is preserved across the chunk boundary because chunks
/// complete in input order and watermarks are queued behind their preceding
/// chunk.
///
/// The closure returns a permit-holding BigTable stream. This helper drains
/// it to a local `Vec` inside the chunk future, so the permit is released
/// before any rows are emitted to the next stage. That avoids stacked
/// `.buffered()` deadlocks where downstream futures are waiting on the same
/// semaphore needed to drain upstream streams.
pub(crate) fn pipelined_chunks<I, O, E, F, Fut>(
    upstream: BoxStream<'static, Result<Watermarked<I>, E>>,
    chunk_size: usize,
    max_concurrent_chunks: usize,
    f: F,
) -> BoxStream<'static, Result<Watermarked<O>, E>>
where
    F: Fn(Vec<I>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<BoxStream<'static, Result<O, E>>, E>> + Send + 'static,
    I: Send + 'static,
    O: Send + 'static,
    E: Send + 'static,
{
    // Local-scope output enum — the `Items(rows)` carrier between the
    // `.buffered()` boundary and the downstream `.flat_map` is private
    // to this function; ChunkInput is shared with pipelined_keyed_batches,
    // hence module-level.
    enum ChunkOutput<O> {
        Items(Vec<O>),
        Watermark(u64),
    }

    let f = Arc::new(f);
    let inputs = chunks_with_watermarks(upstream, chunk_size);
    inputs
        .map(move |input| {
            let f = f.clone();
            async move {
                match input? {
                    ChunkInput::Items(items) => {
                        let rows = f(items).await?.try_collect::<Vec<_>>().await?;
                        Ok::<ChunkOutput<O>, E>(ChunkOutput::Items(rows))
                    }
                    ChunkInput::Watermark(pos) => Ok(ChunkOutput::Watermark(pos)),
                }
            }
        })
        .buffered(max_concurrent_chunks)
        .flat_map(|result| match result {
            Ok(ChunkOutput::Items(rows)) => {
                stream::iter(rows.into_iter().map(|r| Ok(Watermarked::Item(r)))).boxed()
            }
            Ok(ChunkOutput::Watermark(pos)) => {
                stream::once(async move { Ok(Watermarked::Watermark(pos)) }).boxed()
            }
            Err(err) => stream::once(async { Err(err) }).boxed(),
        })
        .boxed()
}

/// One unit of work for `pipelined_chunks`. Items are batched up to the
/// caller-specified chunk size; watermarks are zero-cost passthroughs that
/// occupy a slot in the input-order sequence so `buffered()` re-emits them
/// in place.
enum ChunkInput<I> {
    Items(Vec<I>),
    Watermark(u64),
}

/// Split a `Watermarked<I>` stream into `ChunkInput`. Drives upstream with a
/// manual ready-loop so `chunk_size` only counts `Watermarked::Item`s; watermarks
/// don't consume chunk capacity. Partial Items chunks flush at upstream-pending
/// boundaries so a trickle of items doesn't stall behind a half-full chunk.
///
/// Watermarks debounce into a single `held_wm` slot — only the latest matters
/// for resume. The held watermark flushes:
/// - Before the next Items chunk (so a client that times out during the
///   downstream multiget for that chunk has a valid resume point at the
///   chunk's pre-state).
/// - On upstream Pending, AFTER flushing any partial sub (so sparse scans
///   that produce watermarks without producing items still surface fresh
///   progress to the wire before the request deadline).
/// - Before propagating a terminal error (so the latest in-flight watermark
///   isn't dropped).
/// - At EOF (so a trailing post-bucket watermark with no further items is
///   still observed).
///
/// Ordering invariant: a watermark `P` is only flushed AFTER any items it
/// dominates have already been emitted to the chunker's output. Items
/// dominated by held_wm are either in earlier emitted chunks (already
/// downstream) or in the current `sub` (flushed as a chunk just before the
/// WM in the same Pending-path or next-Item-path). Downstream
/// `.buffered(...)` preserves input order, so those chunks' multigets
/// complete and their items reach the wire before the WM resolves.
///
/// Waker plumbing: the inner ready-drain loop uses `futures::poll!`,
/// which does not install a waker on `Pending`. We rely on the outer
/// `'outer` loop's `upstream.as_mut().next().await` — a real `.await` —
/// to register a fresh waker against the upstream. The next upstream
/// readiness wakes the chunker reliably.
fn chunks_with_watermarks<I, E>(
    upstream: BoxStream<'static, Result<Watermarked<I>, E>>,
    chunk_size: usize,
) -> BoxStream<'static, Result<ChunkInput<I>, E>>
where
    I: Send + 'static,
    E: Send + 'static,
{
    async_stream::try_stream! {
        futures::pin_mut!(upstream);
        let mut sub: Vec<I> = Vec::with_capacity(chunk_size);
        let mut held_wm: Option<u64> = None;

        'outer: loop {
            // Block until at least one entry is available.
            let first = match upstream.as_mut().next().await {
                None => break 'outer,
                Some(Ok(w)) => w,
                Some(Err(e)) => {
                    if !sub.is_empty() {
                        yield ChunkInput::Items(std::mem::replace(&mut sub, Vec::with_capacity(chunk_size)));
                    }
                    if let Some(p) = held_wm.take() {
                        yield ChunkInput::Watermark(p);
                    }
                    Err(e)?;
                    unreachable!();
                }
            };

            // Drain everything else that's immediately ready. Only Items count
            // toward chunk_size; watermarks are free.
            let mut next = Some(first);
            loop {
                let w = match next.take() {
                    Some(w) => w,
                    None => match futures::poll!(upstream.as_mut().next()) {
                        Poll::Ready(Some(Ok(w))) => w,
                        Poll::Ready(Some(Err(e))) => {
                            if !sub.is_empty() {
                                yield ChunkInput::Items(std::mem::replace(&mut sub, Vec::with_capacity(chunk_size)));
                            }
                            if let Some(p) = held_wm.take() {
                                yield ChunkInput::Watermark(p);
                            }
                            Err(e)?;
                            unreachable!();
                        }
                        Poll::Ready(None) => break 'outer,
                        Poll::Pending => break,
                    },
                };
                match w {
                    Watermarked::Item(i) => {
                        // Emit held WM before this item's chunk so a client
                        // that times out during the downstream multiget can
                        // resume at the chunk's pre-state.
                        if let Some(p) = held_wm.take() {
                            if !sub.is_empty() {
                                yield ChunkInput::Items(std::mem::replace(&mut sub, Vec::with_capacity(chunk_size)));
                            }
                            yield ChunkInput::Watermark(p);
                        }
                        sub.push(i);
                        if sub.len() >= chunk_size {
                            yield ChunkInput::Items(std::mem::replace(&mut sub, Vec::with_capacity(chunk_size)));
                        }
                    }
                    Watermarked::Watermark(p) => {
                        held_wm = Some(p);
                    }
                }
            }
            // Upstream pending: flush partial Items so the trickle
            // doesn't stall, then flush any held watermark so sparse
            // scans (long gaps between items) still surface fresh
            // progress to the wire before the request deadline. The
            // ordering invariant survives because the sub chunk is
            // queued through `.buffered(...)` strictly before the WM
            // future: any items the WM dominates are either already on
            // the wire (earlier chunks) or in this just-flushed sub.
            if !sub.is_empty() {
                yield ChunkInput::Items(std::mem::replace(&mut sub, Vec::with_capacity(chunk_size)));
            }
            if let Some(p) = held_wm.take() {
                yield ChunkInput::Watermark(p);
            }
        }
        // EOF: trailing flush.
        if !sub.is_empty() {
            yield ChunkInput::Items(sub);
        }
        if let Some(p) = held_wm.take() {
            yield ChunkInput::Watermark(p);
        }
    }
    .boxed()
}

/// `take(n)` adapted for `Watermarked` streams: count only `Watermarked::Item`s
/// against the limit, pass `Watermarked::Watermark` through transparently, and
/// terminate once `n` items have been emitted. The last watermark
/// emitted before the cutoff is still useful — it bounds the actual scan
/// position even if the handler is now stopped on item count.
pub(crate) fn take_items<T, E>(
    stream: BoxStream<'static, Result<Watermarked<T>, E>>,
    n: usize,
) -> BoxStream<'static, Result<Watermarked<T>, E>>
where
    T: Send + 'static,
    E: Send + 'static,
{
    async_stream::try_stream! {
        futures::pin_mut!(stream);
        let mut emitted = 0usize;
        while emitted < n {
            let Some(item) = stream.next().await else { break; };
            match item? {
                Watermarked::Item(t) => {
                    emitted += 1;
                    yield Watermarked::Item(t);
                }
                Watermarked::Watermark(p) => yield Watermarked::Watermark(p),
            }
        }
    }
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

/// Convenience aliases for the marker-aware pipeline boundary types. Avoid
/// repeating the deeply-nested `BoxStream<'static, Result<Watermarked<...>, _>>`
/// at every signature. `E` is the pipeline's error type — `RpcError` for the
/// non-eval handler chains, `anyhow::Error` for chains downstream of the
/// bitmap evaluator (so `ScanLimitExceeded` survives in-band for the handler
/// to downcast).
pub(crate) type MarkedUpstream<I, E> = BoxStream<'static, Result<Watermarked<I>, E>>;
pub(crate) type MarkedKeyedUpstream<I, K, E> = MarkedUpstream<(I, Vec<K>), E>;
pub(crate) type MarkedKeyedDownstream<I, K, V, E> = MarkedUpstream<KeyedBatchOutput<I, K, V>, E>;

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
pub(crate) fn pipelined_keyed_batches<I, K, V, E, FetchFut>(
    upstream: MarkedKeyedUpstream<I, K, E>,
    upstream_chunk_size: usize,
    max_keys_per_request: usize,
    max_concurrent_fetches: usize,
    fetch: impl Fn(Vec<K>) -> FetchFut + Send + Sync + 'static,
) -> MarkedKeyedDownstream<I, K, V, E>
where
    I: Send + 'static,
    K: Ord + std::hash::Hash + Clone + std::fmt::Debug + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: From<anyhow::Error> + Send + 'static,
    FetchFut: Future<Output = Result<HashMap<K, V>, E>> + Send + 'static,
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
    // Flat pipeline: each `ChunkInput` expands into a Vec<FetchRequest>,
    // those flatten into a single stream, each request runs as a future
    // through ONE `.buffered(max_concurrent_fetches)` (so total in-flight
    // fetches across all chunks is bounded by N, not N*N). The reassembler
    // drains the buffered results in input order; watermarks ride through
    // as zero-cost `FetchRequest::Watermark` units that resolve instantly
    // and the reassembler passes them straight through between batches.
    let fetch_results = chunks_with_watermarks(upstream, upstream_chunk_size)
        .map_ok(move |input| {
            let requests = match input {
                ChunkInput::Items(items) => plan_fetches(items, max_keys_per_request),
                ChunkInput::Watermark(pos) => vec![FetchRequest::Watermark(pos)],
            };
            stream::iter(requests.into_iter().map(Ok::<_, E>))
        })
        .try_flatten()
        .map(move |request_res| {
            let fetch = fetch.clone();
            async move {
                match request_res? {
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
                        Ok::<_, E>(FetchResult::NewGroup {
                            items,
                            requests_total,
                            map,
                        })
                    }
                    FetchRequest::Continuation { keys } => Ok(FetchResult::Continuation {
                        map: fetch(keys).await?,
                    }),
                    FetchRequest::Watermark(pos) => Ok(FetchResult::Watermark(pos)),
                }
            }
        })
        .buffered(max_concurrent_fetches);

    async_stream::try_stream! {
        futures::pin_mut!(fetch_results);
        let mut reassembler = Reassembler::<I, K, V>::new();
        while let Some(result) = fetch_results.next().await {
            for emission in reassembler.push(result?)? {
                match emission {
                    ReassemblerEmission::Item(item) => yield Watermarked::Item(item),
                    ReassemblerEmission::Watermark(pos) => yield Watermarked::Watermark(pos),
                }
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
/// `Watermark` is a zero-cost passthrough that the reassembler emits
/// between batches so per-source progress markers stay ordered with
/// items on the wire.
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
    /// Per-source progress marker. No fetch work — resolves instantly
    /// into `FetchResult::Watermark(pos)` and threads through the
    /// `.buffered(N)` queue at the same input position as items, so the
    /// reassembler can yield it in order between completed batches.
    Watermark(u64),
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
    Watermark(u64),
}

/// One emission from the reassembler. Items come from completed batches
/// (a batch can emit multiple items in one push); Watermarks pass straight
/// through from `FetchResult::Watermark` to preserve their input-order
/// position relative to items on the wire.
enum ReassemblerEmission<I, K, V> {
    Item(KeyedBatchOutput<I, K, V>),
    Watermark(u64),
}

/// Reassembles a logical batch's `FetchResult`s as they emerge in input
/// order from `.buffered`. Holds at most one pending batch at a time —
/// `.buffered` preserves order, so all results for batch B arrive
/// contiguously and before any result for batch C.
///
/// Watermark ordering: `chunks_with_watermarks` debounces so the steady
/// state is "a Watermark only ever arrives between completed batches."
/// We don't *rely* on that invariant defensively — if a Watermark
/// arrives mid-batch (e.g., from a future chunker refactor), it's held
/// in `pending_watermark` and flushed in input order right after the
/// in-flight batch completes, collapsing into the latest if more
/// watermarks pile up before the batch finishes.
struct Reassembler<I, K, V> {
    pending: Option<PendingBatch<I, K, V>>,
    /// Watermark to flush as soon as `pending` completes (or
    /// immediately, when `pending` is `None`). Collapses to the latest
    /// position if multiple watermarks arrive mid-batch.
    pending_watermark: Option<u64>,
}

struct PendingBatch<I, K, V> {
    items: Vec<(I, Vec<K>)>,
    map: HashMap<K, V>,
    requests_remaining: usize,
}

impl<I, K, V> Reassembler<I, K, V>
where
    K: Eq + std::hash::Hash + Clone + std::fmt::Debug,
    V: Clone,
{
    fn new() -> Self {
        Self {
            pending: None,
            pending_watermark: None,
        }
    }

    /// Ingest one `FetchResult`. Returns any rendered items (if the result
    /// completed a batch) and/or a Watermark emission, in input order. An
    /// empty Vec means the result advanced an in-progress batch without
    /// completing it.
    ///
    /// Errors if a batch's `fetch` result is missing a key the read stage
    /// requested — that indicates divergence between the read stage and the
    /// keyed-fetch backend (e.g., an index lookup that promised a row the
    /// object store can't find). Surfaced as an `anyhow::Error` for the
    /// pipeline helper to convert into its caller-typed `E`.
    fn push(
        &mut self,
        result: FetchResult<I, K, V>,
    ) -> Result<Vec<ReassemblerEmission<I, K, V>>, anyhow::Error> {
        match result {
            FetchResult::Watermark(pos) => {
                if self.pending.is_some() {
                    // Buffer until the in-flight batch completes; collapse
                    // any prior held WM into the latest (only the freshest
                    // resume point matters).
                    self.pending_watermark = Some(pos);
                    return Ok(Vec::new());
                }
                // No in-flight batch: flush immediately, after any
                // already-buffered WM (also dominated by `pos`).
                self.pending_watermark = None;
                return Ok(vec![ReassemblerEmission::Watermark(pos)]);
            }
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
            // Split the per-batch superset back out into per-item maps so
            // callers that iterate the map (e.g. ObjectSet builders) don't
            // see other items' keys. A key requested by the read stage but
            // absent from the fetch result indicates index/storage
            // divergence — error rather than render a quietly-partial item.
            let mut emissions: Vec<ReassemblerEmission<I, K, V>> =
                Vec::with_capacity(pending.items.len() + 1);
            for (item, keys) in pending.items {
                let mut item_map: HashMap<K, V> = HashMap::with_capacity(keys.len());
                for k in keys {
                    let v = pending.map.get(&k).ok_or_else(|| {
                        anyhow::anyhow!(
                            "keyed-fetch result missing key {:?} requested by the read stage \
                             (indicates index/storage divergence)",
                            k
                        )
                    })?;
                    item_map.insert(k, v.clone());
                }
                emissions.push(ReassemblerEmission::Item((item, Arc::new(item_map))));
            }
            // Flush any watermark that arrived while this batch was in
            // flight — strictly after the batch's items so the WM's
            // "items dominated by me are emitted" guarantee survives.
            if let Some(pos) = self.pending_watermark.take() {
                emissions.push(ReassemblerEmission::Watermark(pos));
            }
            return Ok(emissions);
        }
        Ok(Vec::new())
    }
}

/// Output of [`resolve_watermarks`]: items pass through; watermarks
/// carry both the original bitmap-domain position and the resolved cp.
pub(crate) enum ResolvedWatermarked<T> {
    Item(T),
    Watermark { position: u64, cp: u64 },
}

/// Final stream stage: pass items through and resolve standalone WMs
/// to cp via one row read per WM. O(1) memory.
///
/// While a WM lookup is in flight, `tokio::select!` polls upstream
/// concurrently — `.buffered(N)` chunk fetchers keep dispatching instead
/// of stalling on the slowest single call in the pipeline.
///
/// Cancellation rules:
/// - **Item arrives during a lookup**: cancel the lookup (drop the
///   future). The item's cursor carries equivalent progress info, so
///   suppressing the WM does not lose progress.
/// - **Newer WM arrives during a lookup**: do NOT cancel. The new WM
///   stashes in a single-slot `pending`; the in-flight lookup keeps
///   running with upstream still being polled. WMs arriving later
///   overwrite `pending` (latest wins — synchronous-burst coalesce).
///   When the lookup completes, `pending` (if any) starts the next
///   lookup; intermediate WMs were coalesced out.
///
/// Backpressure: no internal buffer. A slow downstream blocks `yield`,
/// which stops the loop, which stops polling upstream — the chain
/// stalls cleanly without growing memory.
///
/// Callers construct `resolver` via
/// [`crate::bigtable_client::BigTableClient::tx_wm_resolver`] or
/// [`crate::bigtable_client::BigTableClient::event_wm_resolver`].
pub(crate) fn resolve_watermarks<T, E, F, Fut>(
    upstream: BoxStream<'static, Result<Watermarked<T>, E>>,
    resolver: F,
) -> BoxStream<'static, Result<ResolvedWatermarked<T>, E>>
where
    T: Send + 'static,
    E: Send + 'static,
    F: Fn(u64) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Option<u64>, E>> + Send,
{
    /// Result of racing one in-flight WM lookup against the next upstream
    /// frame. Lifted out of the `tokio::select!` block so the post-select
    /// `match` can take ownership of the lookup future.
    enum Race<T, E> {
        LookupDone(Result<Option<u64>, E>),
        Upstream(Option<Result<Watermarked<T>, E>>),
    }

    // `let_chains` inside the `async_stream::try_stream!` macro body
    // doesn't compile cleanly (the macro's expansion context isn't
    // edition-2024), so the nested-`if let` collapses clippy suggests
    // can't be applied here. The pattern reads fine as nested ifs.
    #[allow(clippy::collapsible_if)]
    async_stream::try_stream! {
        let mut upstream = std::pin::pin!(upstream);
        // At most one lookup in flight. WMs that arrive while a lookup
        // is in flight coalesce into `pending` (latest wins); items
        // cancel both.
        let mut lookup: Option<(u64, std::pin::Pin<Box<Fut>>)> = None;
        let mut pending: Option<u64> = None;

        loop {
            // Promote pending → lookup when nothing is in flight.
            if lookup.is_none() {
                if let Some(p) = pending.take() {
                    lookup = Some((p, Box::pin(resolver(p))));
                }
            }

            match lookup.take() {
                None => match upstream.as_mut().next().await {
                    None => break,
                    Some(Err(e)) => Err(e)?,
                    Some(Ok(Watermarked::Item(t))) => yield ResolvedWatermarked::Item(t),
                    Some(Ok(Watermarked::Watermark(p))) => {
                        lookup = Some((p, Box::pin(resolver(p))));
                    }
                },
                Some((position, mut fut)) => {
                    // Bind the upstream re-borrow to a local so the
                    // `Next` future has a place to anchor its borrow
                    // (an inline `upstream.as_mut().next()` would let
                    // the intermediate Pin temporary drop before
                    // select! polls).
                    let mut upstream_re = upstream.as_mut();
                    let outcome: Race<T, E> = tokio::select! {
                        res = fut.as_mut() => Race::LookupDone(res),
                        next = upstream_re.next() => Race::Upstream(next),
                    };
                    match outcome {
                        Race::LookupDone(res) => {
                            // `fut` drops here; `lookup` stays None
                            // so the next iteration promotes pending
                            // (if any) into the next lookup.
                            if let Some(cp) = res? {
                                yield ResolvedWatermarked::Watermark { position, cp };
                            }
                        }
                        Race::Upstream(Some(Ok(Watermarked::Item(t)))) => {
                            // Item supersedes both in-flight and
                            // pending. `fut` drops here (cancels the
                            // RPC); progress is carried by the item.
                            pending = None;
                            yield ResolvedWatermarked::Item(t);
                        }
                        Race::Upstream(Some(Ok(Watermarked::Watermark(new_p)))) => {
                            // Don't cancel the in-flight lookup. Stash
                            // the new WM as pending (overwriting any
                            // earlier pending — latest wins).
                            lookup = Some((position, fut));
                            pending = Some(new_p);
                        }
                        Race::Upstream(None) => {
                            // Upstream done. Finish the in-flight
                            // lookup, then drain any pending WM.
                            if let Some(cp) = fut.as_mut().await? {
                                yield ResolvedWatermarked::Watermark { position, cp };
                            }
                            if let Some(pp) = pending.take() {
                                if let Some(cp) = resolver(pp).await? {
                                    yield ResolvedWatermarked::Watermark { position: pp, cp };
                                }
                            }
                            return;
                        }
                        Race::Upstream(Some(Err(e))) => Err(e)?,
                    }
                }
            }
        }
    }
    .boxed()
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

    /// Wrap an iterator of plain values into a `Watermarked` upstream for
    /// pipeline helpers that take `Watermarked<T>`. Tests that don't
    /// exercise watermarks use this to keep bodies close to the pre-marked
    /// shape.
    fn ok_items<T, II>(items: II) -> BoxStream<'static, Result<Watermarked<T>, RpcError>>
    where
        T: Send + 'static,
        II: IntoIterator<Item = T>,
        II::IntoIter: Send + 'static,
    {
        stream::iter(
            items
                .into_iter()
                .map(|t| Ok::<_, RpcError>(Watermarked::Item(t))),
        )
        .boxed()
    }

    /// Strip watermarks from a `Watermarked` stream, leaving only items.
    fn items_only<T>(
        stream: BoxStream<'static, Result<Watermarked<T>, RpcError>>,
    ) -> BoxStream<'static, Result<T, RpcError>>
    where
        T: Send + 'static,
    {
        stream
            .try_filter_map(|m| async move {
                Ok(match m {
                    Watermarked::Item(t) => Some(t),
                    Watermarked::Watermark(_) => None,
                })
            })
            .boxed()
    }

    #[tokio::test]
    async fn preserves_input_order_when_chunks_complete_out_of_order() {
        // Upstream: 0..50, in chunks of 5, processed with delay inversely
        // proportional to chunk index so later chunks finish first. Output
        // must still be 0..50 in order.
        let upstream = ok_items(0..50u64);

        let stream = pipelined_chunks(upstream, 5, 8, |chunk: Vec<u64>| async move {
            let first = chunk[0];
            let delay = Duration::from_millis(50u64.saturating_sub(first));
            tokio::time::sleep(delay).await;
            Ok::<_, RpcError>(stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed())
        });

        let collected: Vec<u64> = items_only(stream).try_collect().await.expect("ok");
        let expected: Vec<u64> = (0..50).collect();
        assert_eq!(collected, expected);
    }

    #[tokio::test]
    async fn closure_in_chunk_order_is_preserved_per_call() {
        // The closure receives chunks; verify each chunk is contiguous and
        // the per-chunk Vec is identical to what was received (the helper
        // doesn't reorder within a chunk).
        let upstream = ok_items(0..20u64);
        let seen = Arc::new(Mutex::new(Vec::<Vec<u64>>::new()));
        let seen_for_closure = seen.clone();

        let stream = pipelined_chunks(upstream, 4, 2, move |chunk: Vec<u64>| {
            let seen = seen_for_closure.clone();
            async move {
                seen.lock().unwrap().push(chunk.clone());
                Ok::<_, RpcError>(stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed())
            }
        });

        let collected: Vec<u64> = items_only(stream).try_collect().await.expect("ok");
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
        let upstream = ok_items(0..20u64);
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

        let collected: Vec<u64> = items_only(stream).try_collect().await.expect("ok");
        assert_eq!(collected, (0..20).collect::<Vec<_>>());
        assert!(
            peak.load(Ordering::SeqCst) <= 3,
            "active chunk work exceeded max_concurrent_chunks"
        );
    }

    #[tokio::test]
    async fn propagates_upstream_error_after_prior_completed_chunks() {
        let upstream: BoxStream<'static, Result<Watermarked<u64>, RpcError>> = stream::iter(vec![
            Ok(Watermarked::Item(0u64)),
            Ok(Watermarked::Item(1)),
            Ok(Watermarked::Item(2)),
            Ok(Watermarked::Item(3)),
            Err(RpcError::new(tonic::Code::Internal, "boom")),
            Ok(Watermarked::Item(4)),
        ])
        .boxed();

        let stream = pipelined_chunks(upstream, 2, 2, |chunk: Vec<u64>| async move {
            Ok::<_, RpcError>(stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed())
        });
        let stream = items_only(stream);
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
        let upstream = ok_items(0..2u64);
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

        let stream = items_only(stream);
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
        let upstream = ok_items(0..3u64);
        let stream = pipelined_chunks(upstream, 3, 1, |chunk: Vec<u64>| async move {
            let inner = async_stream::try_stream! {
                yield chunk[0];
                Err(RpcError::new(tonic::Code::Internal, "inner stream boom"))?;
                yield chunk[1];
            };
            Ok::<_, RpcError>(inner.boxed())
        });
        let result: Result<Vec<u64>, RpcError> = items_only(stream).try_collect().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cancellation_releases_permit_while_opening_chunk() {
        let upstream = ok_items(vec![1u64]);
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
        let upstream = ok_items(vec![1u64]);
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
        let upstream = ok_items(0..10u64);

        let stage1 = pipelined_chunks(upstream, 2, 1, {
            let limiter = limiter.clone();
            move |chunk| gated_chunk_stream(limiter.clone(), chunk)
        });
        let stage2 = pipelined_chunks(stage1, 1, 1, {
            let limiter = limiter.clone();
            move |chunk| gated_chunk_stream(limiter.clone(), chunk)
        });

        let out = timeout(
            Duration::from_secs(1),
            items_only(stage2).try_collect::<Vec<u64>>(),
        )
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

    /// CORRECTNESS PIVOT: a watermark that arrives between items must
    /// reach the output AFTER the items it dominates. This is what makes the
    /// marker a safe resume cursor — its arrival at the handler proves the
    /// items at earlier positions also reached the handler.
    #[tokio::test]
    async fn pipelined_chunks_orders_watermark_after_preceding_items() {
        let upstream: BoxStream<'static, Result<Watermarked<u64>, RpcError>> = stream::iter(vec![
            Ok(Watermarked::Item(0u64)),
            Ok(Watermarked::Item(1)),
            Ok(Watermarked::Watermark(100)),
            Ok(Watermarked::Item(2)),
            Ok(Watermarked::Item(3)),
            Ok(Watermarked::Watermark(200)),
        ])
        .boxed();

        let stream = pipelined_chunks(upstream, 10, 4, |chunk: Vec<u64>| async move {
            Ok::<_, RpcError>(stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed())
        });
        let collected: Vec<Watermarked<u64>> = stream.try_collect().await.expect("ok");

        assert_eq!(
            collected,
            vec![
                Watermarked::Item(0),
                Watermarked::Item(1),
                Watermarked::Watermark(100),
                Watermarked::Item(2),
                Watermarked::Item(3),
                Watermarked::Watermark(200),
            ]
        );
    }

    /// Consecutive watermarks with no intervening Item collapse to the
    /// latest. Verifies the debouncer's behaviour at the `pipelined_chunks`
    /// boundary so the wire never sees more than one watermark frame per
    /// upstream burst per items-batch boundary.
    #[tokio::test]
    async fn pipelined_chunks_collapses_consecutive_watermarks() {
        let upstream: BoxStream<'static, Result<Watermarked<u64>, RpcError>> = stream::iter(vec![
            Ok(Watermarked::Item(0u64)),
            Ok(Watermarked::Watermark(10)),
            Ok(Watermarked::Watermark(20)),
            Ok(Watermarked::Watermark(30)),
            Ok(Watermarked::Item(1)),
            Ok(Watermarked::Watermark(40)),
            Ok(Watermarked::Watermark(50)),
        ])
        .boxed();
        let stream = pipelined_chunks(upstream, 10, 4, |chunk: Vec<u64>| async move {
            Ok::<_, RpcError>(stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed())
        });
        let collected: Vec<Watermarked<u64>> = stream.try_collect().await.expect("ok");

        // 10, 20, 30 collapsed to 30 before item 1; 40, 50 collapsed to 50
        // at burst boundary after item 1.
        assert_eq!(
            collected,
            vec![
                Watermarked::Item(0),
                Watermarked::Watermark(30),
                Watermarked::Item(1),
                Watermarked::Watermark(50),
            ]
        );
    }

    /// Frontier that arrives mid-chunk forces a sub-chunk flush so the
    /// marker stays ordered after the items in front of it, even when the
    /// nominal chunk size isn't yet full.
    #[tokio::test]
    async fn pipelined_chunks_flushes_partial_chunk_at_watermark() {
        let upstream: BoxStream<'static, Result<Watermarked<u64>, RpcError>> = stream::iter(vec![
            Ok(Watermarked::Item(0u64)),
            Ok(Watermarked::Watermark(50)),
            Ok(Watermarked::Item(1)),
            Ok(Watermarked::Item(2)),
            Ok(Watermarked::Item(3)),
        ])
        .boxed();
        let observed_chunks = Arc::new(Mutex::new(Vec::<Vec<u64>>::new()));
        let observed_for_closure = observed_chunks.clone();

        let stream = pipelined_chunks(upstream, 100, 4, move |chunk: Vec<u64>| {
            let observed = observed_for_closure.clone();
            async move {
                observed.lock().unwrap().push(chunk.clone());
                Ok::<_, RpcError>(stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed())
            }
        });
        let collected: Vec<Watermarked<u64>> = stream.try_collect().await.expect("ok");

        // First chunk had to flush at the watermark even though chunk_size=100
        // wasn't reached, so the marker stays after item 0 in the output.
        let chunks = observed_chunks.lock().unwrap().clone();
        assert_eq!(chunks, vec![vec![0u64], vec![1u64, 2, 3]]);
        assert_eq!(
            collected,
            vec![
                Watermarked::Item(0),
                Watermarked::Watermark(50),
                Watermarked::Item(1),
                Watermarked::Item(2),
                Watermarked::Item(3),
            ]
        );
    }

    /// Watermarks bracketing a run of items don't steal budget from the
    /// underlying ready-burst, so a run of N items between two watermarks
    /// packs into ⌈N / chunk_size⌉ chunks of the largest possible width.
    /// The old `try_ready_chunks(chunk_size)` chunker would split a run
    /// of 6 items wrapped in 2 watermarks into chunks of 3 + 3 (each burst
    /// of 4 ate one slot for a WM); the new chunker emits 4 + 2.
    #[tokio::test]
    async fn chunks_with_watermarks_excludes_watermarks_from_chunk_budget() {
        let upstream: BoxStream<'static, Result<Watermarked<u64>, RpcError>> = stream::iter(vec![
            Ok(Watermarked::Watermark(1)),
            Ok(Watermarked::Item(0u64)),
            Ok(Watermarked::Item(1)),
            Ok(Watermarked::Item(2)),
            Ok(Watermarked::Item(3)),
            Ok(Watermarked::Item(4)),
            Ok(Watermarked::Item(5)),
            Ok(Watermarked::Watermark(2)),
        ])
        .boxed();

        let frames: Vec<ChunkInput<u64>> = chunks_with_watermarks(upstream, 4)
            .try_collect()
            .await
            .expect("ok");

        let shapes: Vec<(&str, Option<u64>, usize)> = frames
            .iter()
            .map(|f| match f {
                ChunkInput::Items(v) => ("Items", None, v.len()),
                ChunkInput::Watermark(p) => ("Watermark", Some(*p), 0),
            })
            .collect();
        assert_eq!(
            shapes,
            vec![
                ("Watermark", Some(1), 0),
                ("Items", None, 4),
                ("Items", None, 2),
                ("Watermark", Some(2), 0),
            ]
        );
    }

    /// CORRECTNESS: on upstream Pending the held watermark flushes
    /// AFTER any partial Items chunk, not retained until the next
    /// Item. This is the fix for the resumption-on-timeout case for
    /// sparse scans: if a sparse upstream emits items rarely (or never)
    /// between watermarks, the wire still surfaces the latest WM during
    /// Pending so a hard deadline doesn't drop the freshest progress.
    #[tokio::test]
    async fn chunks_with_watermarks_flushes_held_wm_at_pending() {
        let (mut tx, rx) = mpsc::channel::<Result<Watermarked<u64>, RpcError>>(8);
        let upstream = rx.boxed();
        let mut chunker = chunks_with_watermarks(upstream, 100);

        tx.send(Ok(Watermarked::Item(10))).await.expect("send");
        tx.send(Ok(Watermarked::Watermark(50))).await.expect("send");

        // The Item flushes as a partial chunk on Pending.
        let first = timeout(Duration::from_millis(500), chunker.next())
            .await
            .expect("partial Items chunk should flush at Pending")
            .expect("some")
            .expect("ok");
        match first {
            ChunkInput::Items(v) => assert_eq!(v, vec![10]),
            ChunkInput::Watermark(p) => panic!("expected Items chunk first, got WM({p})"),
        }

        // The held WM(50) flushes on the SAME Pending boundary, right
        // after the partial Items chunk. The client sees this WM during
        // the gap, so a deadline-exceeded here still leaves a fresh
        // resume cursor on the wire.
        let second = timeout(Duration::from_millis(500), chunker.next())
            .await
            .expect("held WM should flush on Pending")
            .expect("some")
            .expect("ok");
        match second {
            ChunkInput::Watermark(p) => assert_eq!(p, 50),
            ChunkInput::Items(v) => panic!("expected held WM, got Items({v:?})"),
        }

        tx.send(Ok(Watermarked::Item(20))).await.expect("send");

        let third = timeout(Duration::from_millis(500), chunker.next())
            .await
            .expect("next Items chunk arrives after WM flush")
            .expect("some")
            .expect("ok");
        match third {
            ChunkInput::Items(v) => assert_eq!(v, vec![20]),
            ChunkInput::Watermark(p) => panic!("unexpected WM({p}) after held WM"),
        }

        tx.close_channel();
        assert!(chunker.next().await.is_none());
    }

    /// Sparse-scan extreme: upstream emits WMs but no Items. The chunker
    /// must still flush the WM on every Pending so the wire sees fresh
    /// progress. Without this, a request that scans many empty buckets
    /// would emit zero frames on the wire until either an Item finally
    /// surfaces or the deadline fires — dropping the held WM with the
    /// stream.
    #[tokio::test]
    async fn chunks_with_watermarks_flushes_wm_on_item_less_pending() {
        let (mut tx, rx) = mpsc::channel::<Result<Watermarked<u64>, RpcError>>(8);
        let upstream = rx.boxed();
        let mut chunker = chunks_with_watermarks(upstream, 100);

        tx.send(Ok(Watermarked::Watermark(10))).await.expect("send");
        tx.send(Ok(Watermarked::Watermark(20))).await.expect("send");

        let first = timeout(Duration::from_millis(500), chunker.next())
            .await
            .expect("held WM should flush on Pending without any prior Item")
            .expect("some")
            .expect("ok");
        match first {
            ChunkInput::Watermark(p) => assert_eq!(p, 20, "latest WM in the burst wins"),
            ChunkInput::Items(v) => panic!("expected WM, got Items({v:?})"),
        }

        tx.send(Ok(Watermarked::Watermark(30))).await.expect("send");
        let second = timeout(Duration::from_millis(500), chunker.next())
            .await
            .expect("next WM also flushes on Pending")
            .expect("some")
            .expect("ok");
        match second {
            ChunkInput::Watermark(p) => assert_eq!(p, 30),
            ChunkInput::Items(v) => panic!("expected WM, got Items({v:?})"),
        }

        tx.close_channel();
        assert!(chunker.next().await.is_none());
    }

    #[tokio::test]
    async fn take_items_counts_items_only_passing_inter_item_watermarks() {
        let upstream: BoxStream<'static, Result<Watermarked<u64>, RpcError>> = stream::iter(vec![
            Ok(Watermarked::Item(0u64)),
            Ok(Watermarked::Watermark(10)),
            Ok(Watermarked::Item(1)),
            Ok(Watermarked::Watermark(20)),
            Ok(Watermarked::Item(2)),
            Ok(Watermarked::Item(3)),
        ])
        .boxed();
        let limited = take_items(upstream, 2);
        let collected: Vec<Watermarked<u64>> = limited.try_collect().await.expect("ok");

        // Took exactly 2 Items and passed the Frontier that appeared between
        // them. Stops at the item-count cutoff: trailing watermarks after the
        // Nth item are not drained (the upstream stream is dropped).
        assert_eq!(
            collected,
            vec![
                Watermarked::Item(0),
                Watermarked::Watermark(10),
                Watermarked::Item(1),
            ]
        );
    }

    #[tokio::test]
    async fn propagates_closure_error() {
        let upstream = ok_items(0..10u64);
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
        let result: Result<Vec<u64>, RpcError> = items_only(stream).try_collect().await;
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
    ) -> BoxStream<'static, Result<Watermarked<TestItem>, RpcError>> {
        stream::iter(items.into_iter().map(|r| r.map(Watermarked::Item))).boxed()
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
            other => panic!("expected NewGroup, got {}", request_kind(other)),
        }
    }

    fn unwrap_continuation<I, K: Clone>(req: &FetchRequest<I, K>) -> Vec<K> {
        match req {
            FetchRequest::Continuation { keys } => keys.clone(),
            other => panic!("expected Continuation, got {}", request_kind(other)),
        }
    }

    fn request_kind<I, K>(req: &FetchRequest<I, K>) -> &'static str {
        match req {
            FetchRequest::NewGroup { .. } => "NewGroup",
            FetchRequest::Continuation { .. } => "Continuation",
            FetchRequest::Watermark(_) => "Watermark",
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
        // `Pending`. The chunker inside the helper therefore yields
        // whatever is buffered as one burst, which becomes one or more
        // sub-batches — the trailing partial sub-batch fires immediately
        // rather than stalling waiting for more upstream items.
        let (mut tx, rx) = mpsc::channel::<Result<Watermarked<TestItem>, RpcError>>(8);
        let upstream = rx.boxed();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_fetch = calls.clone();
        let helper = pipelined_keyed_batches::<u32, i32, i32, _, _>(
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
        );
        let mut helper = items_only(helper).map_ok(|(item, _map)| item);

        tx.send(Ok(Watermarked::Item((0, vec![1, 2, 3]))))
            .await
            .expect("send");
        tx.send(Ok(Watermarked::Item((1, vec![4, 5, 6]))))
            .await
            .expect("send");

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
        let helper = pipelined_keyed_batches::<u32, i32, i32, _, _>(
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
        );
        let out: Vec<u32> = items_only(helper)
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
        let helper = pipelined_keyed_batches::<u32, i32, i32, _, _>(
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
        );
        let out: Vec<(u32, usize)> = items_only(helper)
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
        let helper = pipelined_keyed_batches::<u32, i32, i32, _, _>(
            upstream,
            10,
            10,
            4,
            move |keys: Vec<i32>| async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok::<_, RpcError>(keys.into_iter().map(|k| (k, k)).collect())
            },
        );
        let _: Vec<u32> = items_only(helper)
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
        let helper = pipelined_keyed_batches::<u32, i32, i32, _, _>(
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
        );
        let out: Vec<u32> = items_only(helper)
            .map_ok(|(item, _map)| item)
            .try_collect()
            .await
            .expect("ok");

        assert_eq!(out, (0u32..6).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn helper_propagates_fetch_error() {
        let upstream = iter_upstream(vec![Ok((0u32, vec![1, 2, 3])), Ok((1u32, vec![4, 5, 6]))]);
        let helper = pipelined_keyed_batches::<u32, i32, i32, _, _>(
            upstream,
            10,
            10,
            1,
            move |_keys: Vec<i32>| async move {
                Err::<HashMap<i32, i32>, _>(RpcError::new(tonic::Code::Internal, "boom"))
            },
        );
        let result: Result<Vec<u32>, RpcError> = items_only(helper)
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
        let helper = pipelined_keyed_batches::<u32, i32, i32, _, _>(
            upstream,
            10,
            10,
            2,
            move |keys: Vec<i32>| async move {
                Ok::<_, RpcError>(keys.into_iter().map(|k| (k, k)).collect())
            },
        );
        let out: Vec<(u32, Vec<(i32, i32)>)> = items_only(helper)
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
    async fn helper_errors_when_fetch_result_misses_a_requested_key() {
        // Fetch returns an empty map even though the read stage asked for
        // keys 1, 2, 3 — that's index/storage divergence and must surface
        // as an error rather than rendering an item with a silently
        // truncated key set.
        let upstream = iter_upstream(vec![Ok((0u32, vec![1, 2, 3]))]);
        let helper = pipelined_keyed_batches::<u32, i32, i32, _, _>(
            upstream,
            10,
            10,
            1,
            move |_keys: Vec<i32>| async move { Ok::<_, RpcError>(HashMap::new()) },
        );
        let err = items_only(helper)
            .try_collect::<Vec<_>>()
            .await
            .expect_err("missing key should error");
        let msg = err.to_string();
        assert!(
            msg.contains("missing key"),
            "expected a missing-key error, got: {msg}"
        );
    }

    /// `Reassembler` defensively handles a `Watermark` arriving while a
    /// batch is in flight: the WM is buffered until the batch's items
    /// emit, then flushed strictly after them. Today the chunker
    /// debouncer guarantees this case doesn't occur in steady state,
    /// but the defensive handling means a future refactor that
    /// interleaves WMs into a batch's continuations doesn't silently
    /// reorder items past the WM's "items dominated by me are emitted"
    /// invariant.
    #[test]
    fn reassembler_holds_mid_batch_watermark_until_batch_completes() {
        let mut r = Reassembler::<u32, i32, i32>::new();

        // Open a 2-request batch carrying two items.
        let mut map_first = HashMap::new();
        map_first.insert(1, 10);
        let out1 = r
            .push(FetchResult::NewGroup {
                items: vec![(7u32, vec![1, 2]), (8u32, vec![3])],
                requests_total: 2,
                map: map_first,
            })
            .expect("ok");
        assert!(out1.is_empty(), "batch not yet complete");

        // WM arrives BEFORE the continuation lands. Must be held.
        let out_wm = r.push(FetchResult::Watermark(99)).expect("ok");
        assert!(out_wm.is_empty(), "WM held until batch completes");

        // A second WM mid-batch collapses into the latest position.
        let out_wm2 = r.push(FetchResult::Watermark(123)).expect("ok");
        assert!(out_wm2.is_empty(), "second mid-batch WM also held");

        // Continuation completes the batch — items emit first, then the
        // buffered WM (collapsed to 123).
        let mut map_rest = HashMap::new();
        map_rest.insert(2, 20);
        map_rest.insert(3, 30);
        let final_emissions = r
            .push(FetchResult::Continuation { map: map_rest })
            .expect("ok");

        let kinds: Vec<&'static str> = final_emissions
            .iter()
            .map(|e| match e {
                ReassemblerEmission::Item(_) => "item",
                ReassemblerEmission::Watermark(_) => "watermark",
            })
            .collect();
        assert_eq!(
            kinds,
            vec!["item", "item", "watermark"],
            "items emit before the held WM"
        );
        match final_emissions.last().expect("non-empty") {
            ReassemblerEmission::Watermark(p) => assert_eq!(*p, 123),
            ReassemblerEmission::Item(_) => panic!("last emission should be the held WM"),
        }
    }

    /// With no batch in flight, the Reassembler emits the WM immediately
    /// — same end-to-end behavior as before the buffering change. Any
    /// previously buffered WM (e.g., across a now-completed batch) is
    /// dominated by the new one and discarded.
    #[test]
    fn reassembler_flushes_wm_immediately_when_no_batch_pending() {
        let mut r = Reassembler::<u32, i32, i32>::new();

        let out = r.push(FetchResult::Watermark(42)).expect("ok");
        assert_eq!(out.len(), 1);
        match &out[0] {
            ReassemblerEmission::Watermark(p) => assert_eq!(*p, 42),
            ReassemblerEmission::Item(_) => panic!("expected WM emission"),
        }
    }

    /// A key requested by the read stage but absent from the fetch
    /// result indicates index/storage divergence. The Reassembler must
    /// surface this as an error rather than silently rendering the item
    /// with fewer keys than expected.
    #[test]
    fn reassembler_errors_on_missing_key() {
        let mut r = Reassembler::<u32, i32, i32>::new();
        // Request keys 1, 2, 3 but the fetch result only contains 1, 3.
        let mut map = HashMap::new();
        map.insert(1, 10);
        map.insert(3, 30);
        let err = match r.push(FetchResult::NewGroup {
            items: vec![(7u32, vec![1, 2, 3])],
            requests_total: 1,
            map,
        }) {
            Ok(_) => panic!("missing key 2 should error"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("missing key 2"),
            "error should name the missing key, got: {msg}"
        );
    }

    // ---- resolve_watermarks ----

    use futures::TryStreamExt;
    use sui_rpc_api::RpcError;

    /// Future type emitted by the test resolver. Aliased to keep
    /// `slow_resolver`'s signature readable.
    type TestResolverFut =
        std::pin::Pin<Box<dyn Future<Output = Result<Option<u64>, RpcError>> + Send>>;

    #[derive(Clone, Copy)]
    enum WmTestFrame {
        Item(u64),
        Wm(u64),
        Sleep(u64),
    }

    /// Assemble a synthetic upstream from a literal sequence of
    /// items/WMs interleaved with brief sleeps so events land at
    /// controlled points relative to the resolver's progress.
    fn wm_upstream_from(
        frames: Vec<WmTestFrame>,
    ) -> BoxStream<'static, Result<Watermarked<u64>, RpcError>> {
        stream::unfold(frames.into_iter(), |mut it| async move {
            let frame = it.next()?;
            match frame {
                WmTestFrame::Item(t) => Some((Ok(Watermarked::Item(t)), it)),
                WmTestFrame::Wm(p) => Some((Ok(Watermarked::Watermark(p)), it)),
                WmTestFrame::Sleep(ms) => {
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                    let frame = it.next()?;
                    let out = match frame {
                        WmTestFrame::Item(t) => Ok(Watermarked::Item(t)),
                        WmTestFrame::Wm(p) => Ok(Watermarked::Watermark(p)),
                        WmTestFrame::Sleep(_) => panic!("consecutive sleeps in test stream"),
                    };
                    Some((out, it))
                }
            }
        })
        .boxed()
    }

    /// Resolver that records its invocations and respects a configurable
    /// delay so we can stage races against upstream arrivals. The
    /// trivial mapping `cp = position * 10` lets tests identify which
    /// WM produced which emit.
    fn slow_resolver(
        delay_ms: u64,
        calls: Arc<AtomicUsize>,
    ) -> impl Fn(u64) -> TestResolverFut + Send + 'static {
        move |position| {
            let calls = calls.clone();
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                Ok(Some(position * 10))
            })
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    enum WmEmit {
        Item(u64),
        Wm { position: u64, cp: u64 },
    }

    async fn collect_emits(
        stream: BoxStream<'static, Result<ResolvedWatermarked<u64>, RpcError>>,
    ) -> Vec<WmEmit> {
        stream
            .map_ok(|w| match w {
                ResolvedWatermarked::Item(t) => WmEmit::Item(t),
                ResolvedWatermarked::Watermark { position, cp } => WmEmit::Wm { position, cp },
            })
            .try_collect()
            .await
            .expect("stream completed without error")
    }

    /// Item arriving during a WM lookup cancels it (the lookup future
    /// is dropped) and the item is emitted in its place.
    #[tokio::test]
    async fn item_during_lookup_cancels_wm() {
        let calls = Arc::new(AtomicUsize::new(0));
        let upstream = wm_upstream_from(vec![
            WmTestFrame::Wm(5),
            // Long enough that the WM lookup is still pending when the item arrives.
            WmTestFrame::Sleep(20),
            WmTestFrame::Item(10),
        ]);
        // Lookup much slower than the upstream delay → upstream wins the race.
        let stream = resolve_watermarks(upstream, slow_resolver(200, calls.clone()));
        let emits = collect_emits(stream).await;
        assert_eq!(emits, vec![WmEmit::Item(10)]);
    }

    /// Newer WM arriving during a WM lookup does NOT cancel — both WMs emit.
    #[tokio::test]
    async fn new_wm_during_lookup_does_not_cancel() {
        let calls = Arc::new(AtomicUsize::new(0));
        let upstream = wm_upstream_from(vec![
            WmTestFrame::Wm(5),
            WmTestFrame::Sleep(20),
            WmTestFrame::Wm(7),
        ]);
        let stream = resolve_watermarks(upstream, slow_resolver(50, calls.clone()));
        let emits = collect_emits(stream).await;
        assert_eq!(
            emits,
            vec![
                WmEmit::Wm {
                    position: 5,
                    cp: 50
                },
                WmEmit::Wm {
                    position: 7,
                    cp: 70
                },
            ]
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    /// Multiple WMs arriving during a single in-flight lookup get
    /// coalesced — only the latest survives in `pending` and becomes
    /// the next lookup. Intermediate WMs are dropped.
    #[tokio::test]
    async fn pending_wm_coalesces_to_latest() {
        let calls = Arc::new(AtomicUsize::new(0));
        let upstream = wm_upstream_from(vec![
            WmTestFrame::Wm(5),
            // Both arrive while WM(5)'s lookup is still in flight.
            WmTestFrame::Wm(7),
            WmTestFrame::Wm(9),
        ]);
        let stream = resolve_watermarks(upstream, slow_resolver(50, calls.clone()));
        let emits = collect_emits(stream).await;
        // WM(7) coalesced out; WM(5) and WM(9) emit.
        assert_eq!(
            emits,
            vec![
                WmEmit::Wm {
                    position: 5,
                    cp: 50
                },
                WmEmit::Wm {
                    position: 9,
                    cp: 90
                },
            ]
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    /// Upstream ending while a lookup is still in flight: the final WM
    /// is awaited and emitted before the stream terminates.
    #[tokio::test]
    async fn upstream_done_finishes_in_flight_lookup() {
        let calls = Arc::new(AtomicUsize::new(0));
        let upstream = wm_upstream_from(vec![WmTestFrame::Wm(5)]);
        let stream = resolve_watermarks(upstream, slow_resolver(20, calls.clone()));
        let emits = collect_emits(stream).await;
        assert_eq!(
            emits,
            vec![WmEmit::Wm {
                position: 5,
                cp: 50
            }]
        );
    }

    /// Items not racing any lookup are pure passthrough.
    #[tokio::test]
    async fn items_pass_through() {
        let calls = Arc::new(AtomicUsize::new(0));
        let upstream = wm_upstream_from(vec![
            WmTestFrame::Item(1),
            WmTestFrame::Item(2),
            WmTestFrame::Item(3),
        ]);
        let stream = resolve_watermarks(upstream, slow_resolver(0, calls.clone()));
        let emits = collect_emits(stream).await;
        assert_eq!(
            emits,
            vec![WmEmit::Item(1), WmEmit::Item(2), WmEmit::Item(3)]
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
