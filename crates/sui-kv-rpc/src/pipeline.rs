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
use sui_futures::task::TaskGuard;
use sui_inverted_index::ScanStop;
use sui_rpc_api::RpcError;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;
use tokio::task::JoinError;

// Re-export so handler-layer code can spell the marker type without
// directly importing from sui-inverted-index. The pipeline shape lives
// in this module; sui-inverted-index just happens to define the carrier
// type the bitmap eval already produces.
pub(crate) use sui_inverted_index::Watermarked;

/// One ordered frame handed from the [`pipelined_chunks`] dispatcher task to
/// its consumer. `Items` carries a live row receiver (filled by a spawned
/// drainer that owns the permit-holding BigTable stream), the drainer's
/// abort/join handle, and the drainer slot held until the frame is fully
/// consumed. Watermarks and terminal errors are zero-cost passthroughs.
enum FrameHandle<O, P, E> {
    Items {
        rx: mpsc::UnboundedReceiver<Result<O, E>>,
        drain: TaskGuard<()>,
        _slot: OwnedSemaphorePermit,
    },
    Watermark(P),
    Err(E),
}

/// Resolve a drainer/dispatcher `JoinHandle` awaited at an EOF boundary.
///
/// A **panic** is re-raised: it fires during a live request (a bug in the work
/// the drainer runs), and swallowing it would let the handler above emit its
/// terminal `QueryEnd` — a clean "complete" over truncated data. The
/// `with_deadline` wrapper turns the re-raised panic into an `Internal` status.
///
/// A **cancellation** is only reachable at runtime shutdown: we abort our own
/// tasks via `Drop`, never while awaiting their handles, and nobody else holds
/// an abort handle. The stack is tearing down and the client connection is
/// already dead, so truncating is harmless — treat it as EOF.
fn surface_panic(res: Result<(), JoinError>) {
    match res {
        Ok(()) => {}
        Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
        Err(_) => {}
    }
}

/// Chunk an upstream stream of `Watermarked<I>` and run an async fn over each
/// chunk of Items, preserving upstream order. Watermarks keep their position
/// relative to Items: frames are consumed strictly in input order and a
/// watermark frame is yielded only after the preceding chunk's rows have all
/// been drained, so "every Item before me has been emitted" holds.
///
/// Each chunk is drained in a **spawned task** that owns the stream returned by
/// the closure and pushes rows to the next stage via a non-blocking send. This
/// lets rows flow as they are drained while ensuring the drainer never blocks on
/// the consumer. If the returned stream holds a scarce resource (for example, a
/// request-scoped BigTable permit), that resource is held only while the stream
/// itself is being drained, never across downstream backpressure. The dispatcher
/// is spawned eagerly (work may start before the first poll); dropping the
/// returned stream aborts it and all live drainers, releasing their
/// permits/slots.
///
/// The per-frame **row** channel is unbounded so those sends never block; it is
/// unbounded in type only — the drainer sends one message per row in its chunk
/// (≤ chunk_size under the ~1:1 call-site contract) plus an optional terminal
/// error, so memory is O(chunk_size) per in-flight frame. The **handle** channel
/// is bounded by `max_concurrent_chunks`, and a per-stage drainer slot caps
/// in-flight *item* frames at `max_concurrent_chunks` (held until each frame is
/// consumed).
pub(crate) fn pipelined_chunks<I, O, P, E, F, Fut>(
    upstream: BoxStream<'static, Result<Watermarked<I, P>, E>>,
    chunk_size: usize,
    max_concurrent_chunks: usize,
    f: F,
) -> BoxStream<'static, Result<Watermarked<O, P>, E>>
where
    F: Fn(Vec<I>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<BoxStream<'static, Result<O, E>>, E>> + Send + 'static,
    I: Send + 'static,
    O: Send + 'static,
    P: Copy + Send + 'static,
    E: Send + 'static,
{
    assert!(
        max_concurrent_chunks > 0,
        "pipelined_chunks: max_concurrent_chunks must be > 0"
    );
    assert!(chunk_size > 0, "pipelined_chunks: chunk_size must be > 0");

    let f = Arc::new(f);
    // Per-stage cap on in-flight *item* frames (queued + being consumed),
    // distinct from the shared BigTable request semaphore: it only bounds how
    // far the dispatcher runs ahead, so a slow consumer can't pile up
    // unbounded drained-but-unconsumed chunks. Held in each item frame until
    // the consumer finishes it.
    let drainer_slots = Arc::new(Semaphore::new(max_concurrent_chunks));
    // Bounded so a sparse stream of watermark/error frames (which take no
    // drainer slot) can't enqueue unboundedly at a stalled consumer.
    let (frame_tx, frame_rx) = mpsc::channel::<FrameHandle<O, P, E>>(max_concurrent_chunks);

    // Dispatcher: pull ordered frames, spawn a permit-owning drainer per item
    // frame, hand ordered frame handles to the consumer. Spawned (not lazy) so
    // drainers make progress — and release their BigTable permits at RPC
    // completion — independently of whether the consumer is currently polling.
    let dispatcher = tokio::spawn({
        let f = f.clone();
        let drainer_slots = drainer_slots.clone();
        async move {
            let mut chunker = chunks_with_watermarks(upstream, chunk_size);
            while let Some(input) = chunker.next().await {
                let frame = match input {
                    Ok(ChunkInput::Items(items)) => {
                        // Acquire a slot before opening so the dispatcher never
                        // holds a BigTable permit while waiting on the
                        // in-flight bound.
                        let Ok(slot) = drainer_slots.clone().acquire_owned().await else {
                            return;
                        };
                        // Invoke `f` synchronously in chunker order so the
                        // closure's synchronous side effects stay ordered; only
                        // the returned future's `.await` + drain run in the
                        // spawned task.
                        let fut = f(items);
                        // Unbounded so the drainer's sends are non-blocking: the
                        // drainer holds the BigTable permit while draining and
                        // must NEVER block on the consumer — a bounded channel
                        // would let a stalled consumer wedge the drainer and hold
                        // the permit across downstream backpressure, the exact
                        // deadlock class this design removes. Unbounded in type
                        // only: the drainer sends one message per row in its
                        // chunk (≤ chunk_size under the ~1:1 call-site contract)
                        // plus an optional terminal error, so memory is
                        // O(chunk_size) per in-flight frame.
                        #[allow(clippy::disallowed_methods)]
                        // non-blocking; bounded in practice by chunk_size
                        let (row_tx, row_rx) = mpsc::unbounded_channel();
                        let drain = tokio::spawn(async move {
                            match fut.await {
                                Ok(stream) => {
                                    let mut stream = stream;
                                    while let Some(row) = stream.next().await {
                                        // The inner stream's item is exactly the
                                        // channel's item (`Result<O, E>`), so
                                        // forward it directly. An Err is terminal
                                        // (prefix-then-error): send it, then stop.
                                        let terminal = row.is_err();
                                        if row_tx.send(row).is_err() {
                                            // Consumer gone.
                                            return;
                                        }
                                        if terminal {
                                            return;
                                        }
                                    }
                                }
                                Err(e) => {
                                    // Open failed: forward the single error.
                                    let _ = row_tx.send(Err(e));
                                }
                            }
                        });
                        FrameHandle::Items {
                            rx: row_rx,
                            drain: TaskGuard::new(drain),
                            _slot: slot,
                        }
                    }
                    Ok(ChunkInput::Watermark(pos)) => FrameHandle::Watermark(pos),
                    Err(e) => {
                        // Terminal error: send in order, then stop.
                        let _ = frame_tx.send(FrameHandle::Err(e)).await;
                        return;
                    }
                };
                // On send failure the consumer is gone: returning drops `frame`
                // (and any item frame's drainer handle + slot inside it),
                // aborting the drainer and releasing the slot.
                if frame_tx.send(frame).await.is_err() {
                    return;
                }
            }
        }
    });

    // Built BEFORE the generator and moved in, so dropping the returned stream
    // (even unpolled) drops the guard and tears the dispatcher down.
    let dispatcher_guard = TaskGuard::new(dispatcher);

    async_stream::try_stream! {
        let dispatcher_guard = dispatcher_guard;
        let mut frame_rx = frame_rx;
        while let Some(frame) = frame_rx.recv().await {
            match frame {
                FrameHandle::Items { mut rx, drain, _slot } => {
                    while let Some(row) = rx.recv().await {
                        yield Watermarked::Item(row?);
                    }
                    // Row channel closed: the drainer finished (or panicked).
                    // Await it so a hidden panic surfaces instead of a silently
                    // truncated chunk. `_slot` releases when this arm's
                    // bindings drop.
                    surface_panic(drain.await);
                }
                FrameHandle::Watermark(pos) => yield Watermarked::Watermark(pos),
                FrameHandle::Err(e) => Err(e)?,
            }
        }
        // Frame channel closed: the dispatcher finished. Surface a dispatcher
        // panic the same way.
        surface_panic(dispatcher_guard.await);
    }
    .boxed()
}

/// One unit of work for `pipelined_chunks`. Items are batched up to the
/// caller-specified chunk size; watermarks are zero-cost passthroughs that
/// occupy a slot in the input-order sequence so the dispatcher forwards them
/// in place between item frames.
enum ChunkInput<I, P = u64> {
    Items(Vec<I>),
    Watermark(P),
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
/// WM in the same Pending-path or next-Item-path). The dispatcher forwards
/// frames in input order over a FIFO handle channel and the consumer drains
/// each item frame's rows before the next frame, so those chunks' rows reach
/// the wire before the WM resolves.
///
/// Waker plumbing: the inner ready-drain loop uses `futures::poll!`,
/// which does not install a waker on `Pending`. We rely on the outer
/// `'outer` loop's `upstream.as_mut().next().await` — a real `.await` —
/// to register a fresh waker against the upstream. The next upstream
/// readiness wakes the chunker reliably.
fn chunks_with_watermarks<I, P, E>(
    upstream: BoxStream<'static, Result<Watermarked<I, P>, E>>,
    chunk_size: usize,
) -> BoxStream<'static, Result<ChunkInput<I, P>, E>>
where
    I: Send + 'static,
    P: Copy + Send + 'static,
    E: Send + 'static,
{
    async_stream::try_stream! {
        futures::pin_mut!(upstream);
        let mut sub: Vec<I> = Vec::with_capacity(chunk_size);
        let mut held_wm: Option<P> = None;

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
            // forwarded as a frame strictly before the WM frame: any items
            // the WM dominates are either already on the wire (earlier
            // chunks) or in this just-flushed sub.
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
pub(crate) fn take_items<T, P, E>(
    stream: BoxStream<'static, Result<Watermarked<T, P>, E>>,
    n: usize,
) -> BoxStream<'static, Result<Watermarked<T, P>, E>>
where
    T: Send + 'static,
    P: Copy + Send + 'static,
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

/// Collapse runs of equal `Watermarked::Item` values into one, forwarding
/// `Watermarked::Watermark` frames and errors unchanged. Items must already be
/// in scan order. Used by `ListCheckpoints` to turn the per-transaction
/// checkpoint ids of a filtered scan (a checkpoint's txs are contiguous in scan
/// order) into a single id per checkpoint. Unlike a per-chunk mapper this
/// carries its dedup state across the whole stream, so duplicates split across
/// chunk boundaries still collapse.
pub(crate) fn dedup_consecutive<T, P, E>(
    stream: BoxStream<'static, Result<Watermarked<T, P>, E>>,
) -> BoxStream<'static, Result<Watermarked<T, P>, E>>
where
    T: PartialEq + Clone + Send + 'static,
    P: Copy + Send + 'static,
    E: Send + 'static,
{
    async_stream::try_stream! {
        futures::pin_mut!(stream);
        let mut last: Option<T> = None;
        while let Some(item) = stream.next().await {
            match item? {
                Watermarked::Item(t) => {
                    if last.as_ref() == Some(&t) {
                        continue;
                    }
                    last = Some(t.clone());
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
/// non-eval handler chains, `ScanStop` for chains downstream of the bitmap
/// evaluator (so the handler can receive and match the typed terminal as the
/// stream's final error).
pub(crate) type MarkedUpstream<I, E, P = u64> = BoxStream<'static, Result<Watermarked<I, P>, E>>;
pub(crate) type MarkedKeyedUpstream<I, K, E, P = u64> = MarkedUpstream<(I, Vec<K>), E, P>;
pub(crate) type MarkedKeyedDownstream<I, K, V, E, P = u64> =
    MarkedUpstream<KeyedBatchOutput<I, K, V>, E, P>;

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
pub(crate) fn pipelined_keyed_batches<I, K, V, P, E, FetchFut>(
    upstream: MarkedKeyedUpstream<I, K, E, P>,
    upstream_chunk_size: usize,
    max_keys_per_request: usize,
    max_concurrent_fetches: usize,
    fetch: impl Fn(Vec<K>) -> FetchFut + Send + Sync + 'static,
) -> MarkedKeyedDownstream<I, K, V, E, P>
where
    I: Send + 'static,
    K: Ord + std::hash::Hash + Clone + std::fmt::Debug + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    P: Copy + Send + 'static,
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
        let mut reassembler = Reassembler::<I, K, V, P>::new();
        while let Some(result) = fetch_results.next().await {
            // `result?` propagates a terminal upstream/fetch error WITHOUT
            // flushing a watermark `reassembler` may be holding. This is
            // deliberate and is the opposite of `resolve_scan_watermarks`'
            // flush-on-error: there the in-flight watermark dominates
            // already-emitted items, so flushing it is safe; here a held
            // `pending_watermark` is set only mid-batch, so it dominates the
            // in-flight batch this error just aborted. Emitting it would
            // advance the client past items that never reached the wire.
            // Dropping it is the correct resume point — the last on-wire
            // cursor stands.
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
enum FetchRequest<I, K, P = u64> {
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
    Watermark(P),
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
fn plan_fetches<I, K, P>(items: Vec<(I, Vec<K>)>, max_keys: usize) -> Vec<FetchRequest<I, K, P>>
where
    K: Ord + Clone,
{
    assert!(max_keys > 0, "plan_fetches: max_keys must be > 0");
    let mut out: Vec<FetchRequest<I, K, P>> = Vec::new();
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
    fn flush_into<P>(&mut self, out: &mut Vec<FetchRequest<I, K, P>>) {
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
fn push_fat_item<I, K, P>(
    out: &mut Vec<FetchRequest<I, K, P>>,
    item: I,
    keys: Vec<K>,
    max_keys: usize,
) where
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

enum FetchResult<I, K, V, P = u64> {
    NewGroup {
        items: Vec<(I, Vec<K>)>,
        requests_total: usize,
        map: HashMap<K, V>,
    },
    Continuation {
        map: HashMap<K, V>,
    },
    Watermark(P),
}

/// One emission from the reassembler. Items come from completed batches
/// (a batch can emit multiple items in one push); Watermarks pass straight
/// through from `FetchResult::Watermark` to preserve their input-order
/// position relative to items on the wire.
enum ReassemblerEmission<I, K, V, P = u64> {
    Item(KeyedBatchOutput<I, K, V>),
    Watermark(P),
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
struct Reassembler<I, K, V, P = u64> {
    pending: Option<PendingBatch<I, K, V>>,
    /// Watermark to flush as soon as `pending` completes (or
    /// immediately, when `pending` is `None`). Collapses to the latest
    /// position if multiple watermarks arrive mid-batch.
    pending_watermark: Option<P>,
}

struct PendingBatch<I, K, V> {
    items: Vec<(I, Vec<K>)>,
    map: HashMap<K, V>,
    requests_remaining: usize,
}

impl<I, K, V, P> Reassembler<I, K, V, P>
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
        result: FetchResult<I, K, V, P>,
    ) -> Result<Vec<ReassemblerEmission<I, K, V, P>>, anyhow::Error> {
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
            let mut emissions: Vec<ReassemblerEmission<I, K, V, P>> =
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

/// Resolved pipeline frame: items pass through; watermarks carry both the
/// original scan-domain position and its checkpoint.
pub(crate) enum ResolvedWatermarked<T, P = u64> {
    Item(T),
    Watermark { position: P, cp: u64 },
}

/// Terminal outcome from [`resolve_scan_watermarks`].
///
/// A scan limit always carries the authoritative terminal position in the same
/// domain as ordinary watermark frames. Its checkpoint is optional because a
/// frontier at a numeric edge can be a safe resume position without proving
/// any checkpoint fully covered.
#[derive(Debug)]
pub(crate) enum ResolvedScanStop<P> {
    ScanLimit {
        position: P,
        checkpoint: Option<u64>,
    },
    Cancelled,
    Fault(anyhow::Error),
}
pub(crate) fn resolved_scan_limit<P>(
    stop: ResolvedScanStop<P>,
) -> Result<(P, Option<u64>), RpcError> {
    match stop {
        ResolvedScanStop::ScanLimit {
            position,
            checkpoint,
        } => Ok((position, checkpoint)),
        ResolvedScanStop::Cancelled => Err(RpcError::new(
            tonic::Code::Cancelled,
            ScanStop::Cancelled.to_string(),
        )),
        ResolvedScanStop::Fault(inner) => Err(RpcError::from(inner)),
    }
}

/// Resolve ordinary watermark frames and the authoritative frontier carried
/// separately by a terminal [`ScanStop::ScanLimit`].
///
/// While the source is running, the scheduler lets items cancel in-flight and
/// pending watermark lookups and coalesces newer watermarks in one pending slot.
/// All four terminations — clean EOF, scan limit, cancellation, and fault —
/// funnel through one epilogue that drains the in-flight and pending lookups in
/// source order before emitting the terminal.
///
/// The adapter retains the most recent completed resolution, including
/// `None`. When the terminal frontier matches that result, or a lookup being
/// drained, it reuses the result instead of dispatching another resolver
/// call. A merely coalesced or item-cancelled position has no completed result
/// and is resolved if it later becomes the authoritative terminal frontier.
/// `scan_frontier_to_position` converts the scanned index's raw member domain
/// into the endpoint position domain.
pub(crate) fn resolve_scan_watermarks<T, P, F, Fut, C>(
    upstream: BoxStream<'static, Result<Watermarked<T, P>, ScanStop>>,
    resolver: F,
    scan_frontier_to_position: C,
) -> BoxStream<'static, Result<ResolvedWatermarked<T, P>, ResolvedScanStop<P>>>
where
    T: Send + 'static,
    P: Copy + Eq + Send + 'static,
    F: Fn(P) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Option<u64>, ScanStop>> + Send,
    C: Fn(u64) -> P + Send + 'static,
{
    enum Race<T, P> {
        LookupDone(Result<Option<u64>, ScanStop>),
        Upstream(Option<Result<Watermarked<T, P>, ScanStop>>),
    }

    fn resolver_error<P>(error: ScanStop) -> ResolvedScanStop<P> {
        match error {
            ScanStop::Cancelled => ResolvedScanStop::Cancelled,
            ScanStop::Fault(inner) => ResolvedScanStop::Fault(inner),
            ScanStop::ScanLimit { scan_frontier } => ResolvedScanStop::Fault(anyhow::anyhow!(
                "unexpected nested scan limit while resolving watermark at frontier \
                 {scan_frontier}"
            )),
        }
    }

    #[allow(clippy::collapsible_if)]
    async_stream::try_stream! {
        let mut upstream = std::pin::pin!(upstream);
        // At most one lookup in flight; a newer watermark waits in `pending`
        // (latest wins). Invariant: `pending` is only Some while `lookup` is
        // Some — the promotion below drains it first otherwise.
        let mut lookup: Option<(P, std::pin::Pin<Box<Fut>>)> = None;
        let mut pending: Option<P> = None;
        // Latest completed lookup, including a completed `None`. One entry is
        // enough: the terminal scan limit commonly repeats the latest watermark
        // position. Written only when a watermark-frame lookup completes; read
        // only by the terminal epilogue's snapshot.
        let mut completed: Option<(P, Option<u64>)> = None;
        // Why upstream stopped; `None` after the loop means clean EOF.
        let mut stop: Option<ScanStop> = None;

        loop {
            if lookup.is_none() {
                if let Some(position) = pending.take() {
                    lookup = Some((position, Box::pin(resolver(position))));
                }
            }

            match lookup.take() {
                None => match upstream.as_mut().next().await {
                    None => break,
                    Some(Ok(Watermarked::Item(item))) => {
                        yield ResolvedWatermarked::Item(item);
                    }
                    Some(Ok(Watermarked::Watermark(position))) => {
                        lookup = Some((position, Box::pin(resolver(position))));
                    }
                    Some(Err(error)) => {
                        stop = Some(error);
                        break;
                    }
                },
                Some((position, mut future)) => {
                    let mut upstream_re = upstream.as_mut();
                    let outcome: Race<T, P> = tokio::select! {
                        result = future.as_mut() => Race::LookupDone(result),
                        next = upstream_re.next() => Race::Upstream(next),
                    };
                    match outcome {
                        Race::LookupDone(result) => {
                            let checkpoint = result.map_err(resolver_error)?;
                            completed = Some((position, checkpoint));
                            if let Some(checkpoint) = checkpoint {
                                yield ResolvedWatermarked::Watermark {
                                    position,
                                    cp: checkpoint,
                                };
                            }
                        }
                        Race::Upstream(Some(Ok(Watermarked::Item(item)))) => {
                            // The dropped future and pending slot have no
                            // completed result. The item carries their progress.
                            pending = None;
                            yield ResolvedWatermarked::Item(item);
                        }
                        Race::Upstream(Some(Ok(Watermarked::Watermark(new_position)))) => {
                            lookup = Some((position, future));
                            pending = Some(new_position);
                        }
                        Race::Upstream(None) => {
                            lookup = Some((position, future));
                            break;
                        }
                        Race::Upstream(Some(Err(error))) => {
                            lookup = Some((position, future));
                            stop = Some(error);
                            break;
                        }
                    }
                }
            }
        }

        // Terminal epilogue — the single exit for clean EOF, ScanLimit,
        // Cancelled, and Fault. A ScanLimit's frontier is the authoritative
        // terminal position; snapshot its cached checkpoint (if any) before
        // the drain below, which is the cache's only read.
        let terminal_position = match &stop {
            Some(ScanStop::ScanLimit { scan_frontier }) => {
                Some(scan_frontier_to_position(*scan_frontier))
            }
            _ => None,
        };
        // A later round can exhaust its budget without advancing past the last
        // beacon, so reuse that beacon's completed resolution.
        let mut terminal_checkpoint = completed
            .filter(|(position, _)| Some(*position) == terminal_position)
            .map(|(_, checkpoint)| checkpoint);

        // Drain earned progress in source order: the in-flight lookup, then
        // the pending (coalesced) position — dispatched only after the
        // in-flight one finishes, same discipline as the main loop. A drain
        // failure stays subordinate to an authoritative terminal error unless
        // it IS the terminal position (its checkpoint is the terminal
        // payload) or the stream ended cleanly (nothing outranks it then).
        let mut draining = lookup.take();
        while let Some((position, mut future)) = draining.take() {
            match future.as_mut().await {
                Ok(checkpoint) => {
                    if terminal_position == Some(position) {
                        terminal_checkpoint = Some(checkpoint);
                    }
                    if let Some(cp) = checkpoint {
                        yield ResolvedWatermarked::Watermark { position, cp };
                    }
                }
                Err(error) if stop.is_none() || terminal_position == Some(position) => {
                    Err(resolver_error(error))?;
                }
                Err(_) => {}
            }
            draining = pending
                .take()
                .map(|position| (position, Box::pin(resolver(position))));
        }

        match stop {
            // Clean EOF: everything already drained.
            None => {}
            Some(ScanStop::Cancelled) => Err(ResolvedScanStop::Cancelled)?,
            Some(ScanStop::Fault(inner)) => Err(ResolvedScanStop::Fault(inner))?,
            Some(ScanStop::ScanLimit { scan_frontier }) => {
                let position = scan_frontier_to_position(scan_frontier);
                let checkpoint = match terminal_checkpoint {
                    Some(checkpoint) => checkpoint,
                    // Never resolved: the terminal position was coalesced
                    // away, item-cancelled, or simply new. One fresh lookup.
                    None => resolver(position).await.map_err(resolver_error)?,
                };
                Err(ResolvedScanStop::ScanLimit {
                    position,
                    checkpoint,
                })?;
            }
        }
    }
    .boxed()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;
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
            // Record in the SYNCHRONOUS part of the closure: the dispatcher
            // invokes `f(chunk)` in chunker order, so this captures invocation
            // order even though the per-chunk drains run concurrently after.
            seen_for_closure.lock().unwrap().push(chunk.clone());
            async move {
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
    async fn emits_rows_as_chunk_stream_drains() {
        // The drainer streams rows to the consumer as they arrive: row 0 must
        // be observable while the closure's inner stream is still blocked
        // before row 1 — i.e. WITHOUT the chunk being fully drained first. This
        // is the whole point of the streaming-drainer design (no per-chunk
        // drain barrier).
        let upstream = ok_items(0..2u64);
        let (release_second_tx, release_second_rx) = oneshot::channel();
        let release_second_rx = Arc::new(Mutex::new(Some(release_second_rx)));

        // chunk_size=2 groups [0, 1] into one chunk; the inner stream yields
        // row 0, then blocks before row 1.
        let stream = pipelined_chunks(upstream, 2, 1, move |chunk: Vec<u64>| {
            let release_second_rx = release_second_rx.clone();
            async move {
                let inner = async_stream::try_stream! {
                    yield chunk[0];
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

        // Row 0 reaches the consumer before we release row 1.
        let first = timeout(Duration::from_secs(1), stream.try_next())
            .await
            .expect("row 0 should be emitted before the chunk fully drains")
            .expect("ok")
            .expect("some");
        assert_eq!(first, 0);

        // Release row 1; the rest streams through in order.
        release_second_tx.send(()).expect("release receiver alive");
        let second = stream.try_next().await.expect("ok").expect("some");
        assert_eq!(second, 1);
        assert!(stream.try_next().await.expect("ok").is_none());
    }

    #[tokio::test]
    async fn propagates_inner_stream_error() {
        // Prefix-then-error: the row drained before the inner error must reach
        // the consumer, followed by the error (not all-or-nothing).
        let upstream = ok_items(0..3u64);
        let stream = pipelined_chunks(upstream, 3, 1, |chunk: Vec<u64>| async move {
            let inner = async_stream::try_stream! {
                yield chunk[0];
                Err(RpcError::new(tonic::Code::Internal, "inner stream boom"))?;
                yield chunk[1];
            };
            Ok::<_, RpcError>(inner.boxed())
        });
        let stream = items_only(stream);
        futures::pin_mut!(stream);
        assert_eq!(
            stream.try_next().await.expect("prefix row emitted"),
            Some(0),
            "the prefix row before the inner error must reach the consumer"
        );
        let err = stream
            .try_next()
            .await
            .expect_err("inner error after prefix");
        let status: tonic::Status = err.into();
        assert_eq!(status.message(), "inner stream boom");
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
    async fn drainer_does_not_block_on_stalled_consumer() {
        // The drainer holds the BigTable permit while draining and MUST NOT
        // block on the consumer: a bounded row channel would let a stalled
        // consumer wedge the drainer and hold the permit across downstream
        // backpressure (the deadlock class this design removes). With the
        // consumer fully stalled, a chunk of several rows must still drain to
        // completion. Guards the unbounded/non-blocking row channel choice — a
        // bounded blocking channel would leave `drained` false.
        struct SetOnDrop(Arc<AtomicBool>);
        impl Drop for SetOnDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        let drained = Arc::new(AtomicBool::new(false));
        let upstream = ok_items(0..5u64); // chunk_size = 5 → one chunk of 5 rows
        let stream = pipelined_chunks(upstream, 5, 1, {
            let drained = drained.clone();
            move |chunk: Vec<u64>| {
                let drained = drained.clone();
                async move {
                    let inner = async_stream::try_stream! {
                        // Dropped only once the drainer finishes draining every
                        // row — which can't happen if a send blocked.
                        let _done = SetOnDrop(drained.clone());
                        for x in chunk {
                            yield x;
                        }
                    };
                    Ok::<_, RpcError>(inner.boxed())
                }
            }
        });

        // Hold the stream WITHOUT consuming; eager dispatch runs the drainer.
        let _stream = stream;
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            drained.load(Ordering::SeqCst),
            "drainer failed to complete with a stalled consumer (blocked on send?)"
        );
    }

    #[tokio::test]
    async fn cancellation_releases_permit_of_queued_frame() {
        // With N>1 and a consumer that never pulls, the eager dispatcher spawns
        // drainers for queued frames ahead of consumption — each acquires a
        // BigTable permit. Dropping the output must abort those queued drainers
        // (via their FrameHandle's TaskGuard) and release their permits.
        let upstream = ok_items(0..4u64);
        let limiter = Arc::new(Semaphore::new(4));

        let stream = pipelined_chunks(upstream, 1, 4, {
            let limiter = limiter.clone();
            move |chunk: Vec<u64>| {
                let limiter = limiter.clone();
                async move {
                    let inner = async_stream::try_stream! {
                        let permit = limiter.clone().acquire_owned().await.map_err(|_| {
                            RpcError::new(tonic::Code::Internal, "test limiter closed")
                        })?;
                        let _permit = permit;
                        yield chunk[0];
                        // Hold the permit open after emitting so the frame stays
                        // queued (unconsumed) with a live permit.
                        std::future::pending::<()>().await;
                    };
                    Ok::<_, RpcError>(inner.boxed())
                }
            }
        });

        // Eager dispatch: drainers run and acquire all 4 permits without the
        // consumer polling.
        wait_for_available_permits(&limiter, 0).await;
        drop(stream);
        wait_for_available_permits(&limiter, 4).await;
    }

    #[tokio::test]
    async fn bounded_in_flight_with_stalled_consumer() {
        // Eager dispatch must not run unboundedly ahead of a stalled consumer:
        // at most max_concurrent_chunks item-frame drainers active at once
        // (the drainer-slot semaphore, held until each frame is consumed).
        let upstream = ok_items(0..50u64);
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
                        yield chunk[0];
                        // Stay alive (slot held) so the consumer stall is what
                        // bounds concurrency.
                        std::future::pending::<()>().await;
                    };
                    Ok::<_, RpcError>(inner.boxed())
                }
            }
        });

        // Hold the stream (dispatcher eager) WITHOUT consuming.
        let _stream = stream;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            peak.load(Ordering::SeqCst) <= 3,
            "ran unboundedly ahead of a stalled consumer: peak {}",
            peak.load(Ordering::SeqCst)
        );
        assert!(
            peak.load(Ordering::SeqCst) >= 1,
            "dispatcher should have started at least one drainer"
        );
    }

    #[tokio::test]
    async fn drainer_panic_surfaces_rather_than_truncating() {
        use futures::FutureExt;
        // A drainer task that panics mid-stream must surface to the consumer
        // (via the joined handle), not silently truncate the chunk. The
        // `black_box` keeps the panic out of the compiler's reachability
        // analysis so the trailing `yield` stays statically reachable.
        let upstream = ok_items(0..1u64);
        let stream = pipelined_chunks(upstream, 1, 1, |_chunk: Vec<u64>| async move {
            let inner = async_stream::try_stream! {
                yield 0u64;
                if std::hint::black_box(true) {
                    panic!("drainer boom");
                }
                yield 1u64;
            };
            Ok::<_, RpcError>(inner.boxed())
        });

        let outcome = std::panic::AssertUnwindSafe(items_only(stream).try_collect::<Vec<u64>>())
            .catch_unwind()
            .await;
        assert!(
            outcome.is_err(),
            "drainer panic must surface, not silently truncate the stream"
        );
    }

    #[tokio::test]
    async fn sparse_watermarks_apply_backpressure() {
        // A sparse stream of watermarks (no items, so no drainer slots) must
        // still be bounded by the handle channel: with a stalled consumer the
        // dispatcher pulls at most ~max_concurrent_chunks watermarks ahead, not
        // the whole stream. Each watermark is separated by a real Pending (the
        // sleep) so the chunker flushes them as individual frames rather than
        // debouncing them all into one.
        let pulled = Arc::new(AtomicUsize::new(0));
        let total = 50u64;
        let n = 3usize;
        let upstream = {
            let pulled = pulled.clone();
            stream::unfold(0u64, move |i| {
                let pulled = pulled.clone();
                async move {
                    if i >= total {
                        return None;
                    }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    pulled.fetch_add(1, Ordering::SeqCst);
                    Some((Ok::<_, RpcError>(Watermarked::Watermark(i)), i + 1))
                }
            })
            .boxed()
        };

        // Closure never runs (no item frames); identity passthrough.
        let stream = pipelined_chunks(upstream, 10, n, |chunk: Vec<u64>| async move {
            Ok::<_, RpcError>(stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed())
        });

        // Hold the stream without consuming; the dispatcher must stall on the
        // bounded handle channel instead of draining all 50 watermarks.
        let _stream = stream;
        tokio::time::sleep(Duration::from_millis(200)).await;
        let pulled_now = pulled.load(Ordering::SeqCst);
        assert!(
            pulled_now <= n + 3,
            "dispatcher ran unboundedly ahead of a stalled consumer: pulled {pulled_now} (bound ~{n})"
        );
        assert!(
            pulled_now >= 1,
            "dispatcher should have pulled at least one watermark"
        );
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
        let reqs = plan_fetches::<_, _, u64>(items, 6);
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
        let reqs = plan_fetches::<_, _, u64>(items, 5);
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
        let reqs = plan_fetches::<_, _, u64>(items, 3);
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
        let reqs = plan_fetches::<_, _, u64>(items, 3);
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
        let reqs = plan_fetches::<_, _, u64>(items, 8);
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
        let helper = pipelined_keyed_batches::<u32, i32, i32, u64, _, _>(
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
        let helper = pipelined_keyed_batches::<u32, i32, i32, u64, _, _>(
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
        let helper = pipelined_keyed_batches::<u32, i32, i32, u64, _, _>(
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
        let helper = pipelined_keyed_batches::<u32, i32, i32, u64, _, _>(
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
        let helper = pipelined_keyed_batches::<u32, i32, i32, u64, _, _>(
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
        let helper = pipelined_keyed_batches::<u32, i32, i32, u64, _, _>(
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
        let helper = pipelined_keyed_batches::<u32, i32, i32, u64, _, _>(
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
        let helper = pipelined_keyed_batches::<u32, i32, i32, u64, _, _>(
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

    // ---- resolve_scan_watermarks ----

    use futures::TryStreamExt;

    /// Future type emitted by the test resolver. Aliased to keep
    /// `slow_resolver`'s signature readable.
    type TestResolverFut =
        std::pin::Pin<Box<dyn Future<Output = Result<Option<u64>, ScanStop>> + Send>>;

    #[derive(Clone, Copy)]
    enum WmTestFrame {
        Item(u64),
        Wm(u64),
        Sleep(u64),
        Err(&'static str),
    }

    /// Assemble a synthetic upstream from a literal sequence of
    /// items/WMs interleaved with brief sleeps so events land at
    /// controlled points relative to the resolver's progress.
    fn wm_upstream_from(
        frames: Vec<WmTestFrame>,
    ) -> BoxStream<'static, Result<Watermarked<u64>, ScanStop>> {
        stream::unfold(frames.into_iter(), |mut it| async move {
            let frame = it.next()?;
            match frame {
                WmTestFrame::Item(t) => Some((Ok(Watermarked::Item(t)), it)),
                WmTestFrame::Wm(p) => Some((Ok(Watermarked::Watermark(p)), it)),
                WmTestFrame::Err(m) => Some((Err(ScanStop::Fault(anyhow::anyhow!(m))), it)),
                WmTestFrame::Sleep(ms) => {
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                    let frame = it.next()?;
                    let out = match frame {
                        WmTestFrame::Item(t) => Ok(Watermarked::Item(t)),
                        WmTestFrame::Wm(p) => Ok(Watermarked::Watermark(p)),
                        WmTestFrame::Err(m) => Err(ScanStop::Fault(anyhow::anyhow!(m))),
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
        stream: BoxStream<'static, Result<ResolvedWatermarked<u64>, ResolvedScanStop<u64>>>,
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

    type ControlledWmFrame = (Result<Watermarked<u64>, ScanStop>, oneshot::Sender<()>);
    type ResolvedWmFrame = Result<ResolvedWatermarked<u64>, ResolvedScanStop<u64>>;

    #[derive(Clone)]
    struct ResolverGate {
        started: Arc<Semaphore>,
        release: Arc<Semaphore>,
    }

    impl ResolverGate {
        fn new() -> Self {
            Self {
                started: Arc::new(Semaphore::new(0)),
                release: Arc::new(Semaphore::new(0)),
            }
        }

        async fn block(&self) {
            self.started.add_permits(1);
            let _permit = self
                .release
                .acquire()
                .await
                .expect("lookup gate should stay open");
        }

        async fn wait_until_blocked(&self) {
            timeout(Duration::from_secs(1), self.started.acquire())
                .await
                .expect("resolver lookup should reach its gate")
                .expect("start semaphore should stay open")
                .forget();
        }

        fn release(&self) {
            self.release.add_permits(1);
        }

        fn available_releases(&self) -> usize {
            self.release.available_permits()
        }
    }

    /// Drives a synthetic upstream one acknowledged frame at a time and
    /// captures resolver output. The acknowledgement proves that the pipeline
    /// polled each input, so race tests never depend on sleeps.
    struct WmResolverFixture {
        input: tokio::sync::mpsc::Sender<ControlledWmFrame>,
        output: tokio::sync::mpsc::Receiver<ResolvedWmFrame>,
        driver: tokio::task::JoinHandle<()>,
    }

    impl WmResolverFixture {
        fn new<F>(resolver: F) -> Self
        where
            F: Fn(u64) -> TestResolverFut + Send + 'static,
        {
            let (input, input_rx) = tokio::sync::mpsc::channel::<ControlledWmFrame>(1);
            let upstream = stream::unfold(input_rx, |mut rx| async move {
                let (frame, pulled) = rx.recv().await?;
                let _ = pulled.send(());
                Some((frame, rx))
            })
            .boxed();
            let mut stream = resolve_scan_watermarks(upstream, resolver, std::convert::identity);

            // The widest scenario emits two progress frames and one terminal
            // frame before its assertions begin draining output.
            let (output_tx, output) = tokio::sync::mpsc::channel::<ResolvedWmFrame>(3);
            let driver = tokio::spawn(async move {
                while let Some(frame) = stream.next().await {
                    let terminal = frame.is_err();
                    if output_tx.send(frame).await.is_err() || terminal {
                        break;
                    }
                }
            });

            Self {
                input,
                output,
                driver,
            }
        }

        async fn send(&self, frame: Result<Watermarked<u64>, ScanStop>) {
            let (pulled_tx, pulled_rx) = oneshot::channel();
            timeout(Duration::from_secs(1), self.input.send((frame, pulled_tx)))
                .await
                .expect("resolver pipeline should accept the upstream frame")
                .expect("resolver pipeline should still be reading upstream");
            timeout(Duration::from_secs(1), pulled_rx)
                .await
                .expect("resolver pipeline should poll the upstream frame")
                .expect("resolver pipeline should acknowledge the upstream frame");
        }

        async fn watermark(&self, position: u64) {
            self.send(Ok(Watermarked::Watermark(position))).await;
        }

        async fn item(&self, item: u64) {
            self.send(Ok(Watermarked::Item(item))).await;
        }

        async fn scan_limit(&self, scan_frontier: u64) {
            self.send(Err(ScanStop::ScanLimit { scan_frontier })).await;
        }

        async fn next(&mut self) -> ResolvedWmFrame {
            timeout(Duration::from_secs(1), self.output.recv())
                .await
                .expect("resolver pipeline should produce its next frame")
                .expect("resolver pipeline ended before producing the expected frame")
        }

        async fn finish(self) {
            self.driver
                .await
                .expect("resolver pipeline driver should finish");
        }
    }

    fn assert_resolved_wm(
        frame: ResolvedWmFrame,
        expected_position: u64,
        expected_checkpoint: u64,
    ) {
        match frame {
            Ok(ResolvedWatermarked::Watermark { position, cp }) => {
                assert_eq!(position, expected_position);
                assert_eq!(cp, expected_checkpoint);
            }
            Ok(ResolvedWatermarked::Item(_)) => {
                panic!("expected a resolved watermark, got an item")
            }
            Err(_) => panic!("expected a resolved watermark, got a terminal error"),
        }
    }

    fn assert_scan_limit(
        frame: ResolvedWmFrame,
        expected_position: u64,
        expected_checkpoint: Option<u64>,
    ) {
        match frame {
            Err(ResolvedScanStop::ScanLimit {
                position,
                checkpoint,
            }) => {
                assert_eq!(position, expected_position);
                assert_eq!(checkpoint, expected_checkpoint);
            }
            Err(_) => panic!("expected a scan-limit terminal error"),
            Ok(_) => panic!("expected a scan-limit terminal error, got a success frame"),
        }
    }

    /// Once a watermark position has been resolved during normal stream
    /// processing, a scan limit at that position must reuse the result.
    #[tokio::test]
    async fn resolve_scan_watermarks_reuses_completed_position_at_scan_limit() {
        let calls = Arc::new(AtomicUsize::new(0));
        let resolver_calls = calls.clone();
        let completed = Arc::new(Semaphore::new(0));
        let resolver_completed = completed.clone();
        let mut fixture = WmResolverFixture::new(move |_position| {
            resolver_calls.fetch_add(1, Ordering::SeqCst);
            let completed = resolver_completed.clone();
            Box::pin(async move {
                completed.add_permits(1);
                Ok(Some(70))
            })
        });

        fixture.watermark(7).await;
        timeout(Duration::from_secs(1), completed.acquire())
            .await
            .expect("resolver lookup should complete")
            .expect("completion semaphore should stay open")
            .forget();
        assert_resolved_wm(fixture.next().await, 7, 70);

        fixture.scan_limit(7).await;
        assert_scan_limit(fixture.next().await, 7, Some(70));

        fixture.finish().await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "the scan limit must reuse the completed watermark lookup"
        );
    }

    /// The resolver preserves a missing terminal checkpoint so consumers can
    /// accept a numeric-edge genesis frontier while rejecting `None` at any
    /// non-edge position as a domain-specific mapping fault.
    #[tokio::test]
    async fn resolve_scan_watermarks_preserves_missing_terminal_checkpoint() {
        let mut fixture = WmResolverFixture::new(move |_position| Box::pin(async { Ok(None) }));

        fixture.scan_limit(11).await;
        assert_scan_limit(fixture.next().await, 11, None);
        fixture.finish().await;
    }

    /// A scan-limit position newer than the last resolved watermark position is
    /// genuinely new work and must be resolved exactly once.
    #[tokio::test]
    async fn resolve_scan_watermarks_resolves_newer_scan_limit_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let resolver_calls = calls.clone();
        let mut fixture = WmResolverFixture::new(move |position| {
            resolver_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(Some(position * 10)) })
        });

        fixture.watermark(5).await;
        assert_resolved_wm(fixture.next().await, 5, 50);

        fixture.scan_limit(8).await;
        assert_scan_limit(fixture.next().await, 8, Some(80));

        fixture.finish().await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "one completed watermark lookup plus one fresh scan-limit lookup"
        );
    }

    /// An item takes priority over an in-progress watermark lookup. It must be
    /// dispatched while that lookup is blocked, and the cancelled position
    /// remains unresolved if a later scan limit names it as the terminal frontier.
    #[tokio::test]
    async fn resolve_scan_watermarks_item_cancels_lookup_and_terminal_retries() {
        let calls = Arc::new(AtomicUsize::new(0));
        let gate = ResolverGate::new();
        let resolver_calls = calls.clone();
        let resolver_gate = gate.clone();
        let mut fixture = WmResolverFixture::new(move |position| {
            let invocation = resolver_calls.fetch_add(1, Ordering::SeqCst);
            let gate = resolver_gate.clone();
            Box::pin(async move {
                if invocation == 0 {
                    gate.block().await;
                }
                Ok(Some(position * 10))
            })
        });

        fixture.watermark(5).await;
        gate.wait_until_blocked().await;

        fixture.item(99).await;
        match fixture.next().await {
            Ok(ResolvedWatermarked::Item(item)) => assert_eq!(item, 99),
            Ok(ResolvedWatermarked::Watermark { .. }) => {
                panic!("the item must win the blocked lookup race")
            }
            Err(_) => panic!("expected the racing item, got a terminal error"),
        }
        assert_eq!(
            gate.available_releases(),
            0,
            "the item must be yielded before the lookup gate opens"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // The first future has been dropped. Opening its gate cannot complete
        // it or manufacture a reusable result.
        gate.release();
        fixture.scan_limit(5).await;
        assert_scan_limit(fixture.next().await, 5, Some(50));

        fixture.finish().await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "the cancelled position must be resolved once when terminal"
        );
    }

    /// ScanLimit arriving during a same-position lookup must await that one
    /// lookup, emit already-earned progress in source order, and reuse its
    /// result in the terminal payload.
    #[tokio::test]
    async fn resolve_scan_watermarks_reuses_same_position_in_flight() {
        let calls = Arc::new(AtomicUsize::new(0));
        let gate = ResolverGate::new();
        let resolver_calls = calls.clone();
        let resolver_gate = gate.clone();
        let mut fixture = WmResolverFixture::new(move |position| {
            resolver_calls.fetch_add(1, Ordering::SeqCst);
            let gate = resolver_gate.clone();
            Box::pin(async move {
                if position == 5 {
                    gate.block().await;
                }
                Ok(Some(position * 10))
            })
        });

        fixture.watermark(3).await;
        assert_resolved_wm(fixture.next().await, 3, 30);

        fixture.watermark(5).await;
        gate.wait_until_blocked().await;
        fixture.scan_limit(5).await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "terminal arrival must not dispatch a duplicate lookup"
        );

        gate.release();
        assert_resolved_wm(fixture.next().await, 5, 50);
        assert_scan_limit(fixture.next().await, 5, Some(50));

        fixture.finish().await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "the in-flight lookup must supply the terminal checkpoint"
        );
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
        let stream = resolve_scan_watermarks(
            upstream,
            slow_resolver(200, calls.clone()),
            std::convert::identity,
        );
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
        let stream = resolve_scan_watermarks(
            upstream,
            slow_resolver(50, calls.clone()),
            std::convert::identity,
        );
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
        let stream = resolve_scan_watermarks(
            upstream,
            slow_resolver(50, calls.clone()),
            std::convert::identity,
        );
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
        let stream = resolve_scan_watermarks(
            upstream,
            slow_resolver(20, calls.clone()),
            std::convert::identity,
        );
        let emits = collect_emits(stream).await;
        assert_eq!(
            emits,
            vec![WmEmit::Wm {
                position: 5,
                cp: 50
            }]
        );
    }

    /// A terminal upstream error arriving while a WM lookup is in flight
    /// must not swallow already-earned progress: the lookup is finished and
    /// emitted before the error ends the stream. For a scan limit, this
    /// preserved watermark precedes the terminal carrying the stopping round's
    /// authoritative frontier.
    #[tokio::test]
    async fn terminal_error_finishes_in_flight_lookup() {
        let calls = Arc::new(AtomicUsize::new(0));
        let upstream = wm_upstream_from(vec![
            WmTestFrame::Wm(5),
            // Error lands while WM(5)'s lookup (50ms) is still in flight.
            WmTestFrame::Sleep(20),
            WmTestFrame::Err("scan limit"),
        ]);
        let stream = resolve_scan_watermarks(
            upstream,
            slow_resolver(50, calls.clone()),
            std::convert::identity,
        );
        futures::pin_mut!(stream);

        let mut emits = Vec::new();
        let err = loop {
            match stream.next().await {
                Some(Ok(ResolvedWatermarked::Item(t))) => emits.push(WmEmit::Item(t)),
                Some(Ok(ResolvedWatermarked::Watermark { position, cp })) => {
                    emits.push(WmEmit::Wm { position, cp })
                }
                Some(Err(e)) => break e,
                None => panic!("expected terminal error, got clean EOF"),
            }
        };

        assert_eq!(
            emits,
            vec![WmEmit::Wm {
                position: 5,
                cp: 50
            }],
            "the already-earned in-flight watermark must reach the client before the error"
        );
        match err {
            ResolvedScanStop::Fault(inner) => assert_eq!(inner.to_string(), "scan limit"),
            other => panic!("expected fault, got {other:?}"),
        }
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
        let stream = resolve_scan_watermarks(
            upstream,
            slow_resolver(0, calls.clone()),
            std::convert::identity,
        );
        let emits = collect_emits(stream).await;
        assert_eq!(
            emits,
            vec![WmEmit::Item(1), WmEmit::Item(2), WmEmit::Item(3)]
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
