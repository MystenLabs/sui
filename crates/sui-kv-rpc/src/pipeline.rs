// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::collections::HashSet;
use std::future::Future;
use std::ops::Range;
use std::sync::Arc;
use std::task::Poll;

use futures::StreamExt;
use futures::future;
use futures::stream::BoxStream;
use sui_futures::task::TaskGuard;
use sui_inverted_index::ScanStop;
use sui_rpc_api::RpcError;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;
use tokio::task::JoinError;

use self::read_plan::ReadLimits;
use self::read_plan::Segment;
use self::read_plan::plan_reads;
use crate::request_cache::CacheFetcher;
use crate::request_cache::CachedValue;
use crate::request_cache::RequestCache;
use crate::request_cache::UnreadKeys;

mod read_plan;

// Re-export so handler-layer code can spell the marker type without
// directly importing from sui-inverted-index. The pipeline shape lives
// in this module; sui-inverted-index just happens to define the carrier
// type the bitmap eval already produces.
pub(crate) use sui_inverted_index::Watermarked;

/// One ordered frame handed from the [`pipelined_reads`] dispatcher task to
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

/// What one seek covers on a [`pipelined_chunks`] stage, whose
/// `max_seeks_per_read` and `min_seeks_per_split_read` count seeks. A read
/// costs about one seek per stretch of adjacent rows it touches, so where keys
/// cluster, a key count misjudges a read's cost.
pub(crate) enum SeekUnit<I> {
    /// One item, read by its own key. For tables keyed by hashes
    /// (`transactions`), where no two keys are neighbours and every key costs
    /// its own seek.
    Item,
    /// One run of consecutive items with equal `group`, whose rows are adjacent
    /// and read with one seek: the 1,000-transaction buckets of
    /// `tx_seq_digest`. Consecutive items with equal `key` share one row, as
    /// the events of one transaction do on `list_events`. Reads take whole runs
    /// and carry at most `max_keys_per_read` keys, so a read ends at the last
    /// run boundary before that cap. A run is cut only when it alone holds more
    /// keys: into consecutive pieces of at most `max_keys_per_read` keys, each a
    /// seek of its own, and never between items that share a key.
    BucketRun {
        group: fn(&I) -> u64,
        key: fn(&I) -> u64,
        max_keys_per_read: usize,
    },
}

impl<I> Clone for SeekUnit<I> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<I> Copy for SeekUnit<I> {}

impl<I> SeekUnit<I> {
    /// Most keys one read carries; `None` when every key is its own seek.
    fn max_keys_per_read(self) -> Option<usize> {
        match self {
            SeekUnit::Item => None,
            SeekUnit::BucketRun {
                max_keys_per_read, ..
            } => Some(max_keys_per_read),
        }
    }

    /// Whether `item` is covered by the same seek as `previous`, the item before
    /// it.
    fn continues(self, previous: &I, item: &I) -> bool {
        match self {
            SeekUnit::Item => false,
            SeekUnit::BucketRun { group, .. } => group(previous) == group(item),
        }
    }

    /// Whether `item` reads the same row as `previous`, the item before it.
    fn shares_key(self, previous: &I, item: &I) -> bool {
        match self {
            SeekUnit::Item => false,
            SeekUnit::BucketRun { key, .. } => key(previous) == key(item),
        }
    }

    /// The seeks of the burst `items` as planner segments, in order: one
    /// segment per seek, except that a run of more than `max_keys_per_read`
    /// keys is cut, between keys, into consecutive pieces of at most that many
    /// keys, each its own segment. Every segment adds a seek. Alongside, the
    /// number of the burst's items each segment covers, which
    /// [`split_burst`] slices the burst by.
    fn segments(self, items: &[I]) -> (Vec<Segment>, Vec<usize>) {
        let max_keys = self.max_keys_per_read().unwrap_or(usize::MAX);
        let mut segments: Vec<Segment> = Vec::new();
        let mut segment_items: Vec<usize> = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let previous = index.checked_sub(1).map(|previous| &items[previous]);
            match (segments.last_mut(), segment_items.last_mut(), previous) {
                (Some(_), Some(covered), Some(previous)) if self.shares_key(previous, item) => {
                    *covered += 1;
                }
                (Some(segment), Some(covered), Some(previous))
                    if segment.keys < max_keys && self.continues(previous, item) =>
                {
                    segment.keys += 1;
                    *covered += 1;
                }
                _ => {
                    segments.push(Segment {
                        adds_seek: true,
                        keys: 1,
                    });
                    segment_items.push(1);
                }
            }
        }
        (segments, segment_items)
    }
}

/// Run an async fn over reads of an upstream stream of `Watermarked<I>`,
/// preserving upstream order. Watermarks keep their position relative to
/// Items: frames are consumed strictly in input order and a watermark frame is
/// yielded only after the preceding reads' rows have all been drained, so
/// "every Item before me has been emitted" holds.
///
/// The stage collects a **burst** — the items ready before upstream returns
/// `Pending`, closed early by a watermark, an upstream error, EOF, or the burst
/// bound of `max_concurrent_chunks ×` the read limits, in seeks and in keys
/// (see [`chunks_with_watermarks`]) — and cuts it into reads once, with
/// [`plan_reads`]:
/// - `max_seeks_per_read` and `min_seeks_per_split_read` count seeks: one per
///   item, or one per run of items whose rows are adjacent (see [`SeekUnit`]).
/// - `max_seeks_per_read` caps a read. With [`SeekUnit::BucketRun`], so does
///   its `max_keys_per_read`; reads take whole runs, and a run is cut only when
///   it alone exceeds that key cap, so a read ends at the last run boundary
///   before it.
/// - `min_seeks_per_split_read` is the least a read costs when the burst is
///   split over the stage's free drainer slots: with it set, the burst goes
///   out as up to one read per slot free at planning time, each of at least
///   that many seeks (unless the whole burst costs less) and at most
///   `max_seeks_per_read`. Unset, the burst goes out as the fewest near-equal
///   reads of at most `max_seeks_per_read` seeks. Either way, when the key cap
///   ends reads before their share of seeks, a burst can take more reads than
///   that (see [`plan_reads`]).
///
/// The reads are consecutive slices of the burst made of whole runs, and are
/// consumed in that order, so output order is unchanged. `f` is called once per
/// read, synchronously and in read order, so its synchronous side effects stay
/// ordered. A read that finds no free slot waits for one with its planned size
/// (see [`pipelined_reads`]).
///
/// Each read is drained in a **spawned task** that owns the stream returned by
/// `f` and pushes rows to the next stage via a non-blocking send. This lets rows
/// flow as they are drained while ensuring the drainer never blocks on the
/// consumer. If the returned stream holds a scarce resource (for example, a
/// request-scoped BigTable permit), that resource is held only while the stream
/// itself is being drained, never across downstream backpressure. The
/// dispatcher is spawned eagerly (work may start before the first poll);
/// dropping the returned stream aborts it and all live drainers, releasing
/// their permits/slots. Memory is O(burst items) per stage: at most
/// `max_concurrent_chunks` read frames are in flight, each holding at most one
/// read's rows.
pub(crate) fn pipelined_chunks<I, O, P, E, F, Fut>(
    upstream: BoxStream<'static, Result<Watermarked<I, P>, E>>,
    max_seeks_per_read: usize,
    max_concurrent_chunks: usize,
    min_seeks_per_split_read: Option<usize>,
    seek_unit: SeekUnit<I>,
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
    assert!(
        max_seeks_per_read > 0,
        "pipelined_chunks: max_seeks_per_read must be > 0"
    );
    assert!(
        min_seeks_per_split_read != Some(0),
        "pipelined_chunks: min_seeks_per_split_read must be > 0"
    );

    let limits = ReadLimits {
        max_seeks_per_read,
        max_keys_per_read: seek_unit.max_keys_per_read(),
        min_seeks_per_split_read,
    };
    // With `SeekUnit::Item` every item is one seek and one key.
    let max_keys_per_read = limits.max_keys_per_read.unwrap_or(max_seeks_per_read);
    let bursts = chunks_with_watermarks(
        upstream,
        seek_unit,
        max_concurrent_chunks.saturating_mul(max_seeks_per_read),
        max_concurrent_chunks.saturating_mul(max_keys_per_read),
    );
    pipelined_reads(
        bursts,
        max_concurrent_chunks,
        move |items: Vec<I>, free_slots| {
            let (segments, segment_items) = seek_unit.segments(&items);
            let reads = plan_reads(&segments, limits, free_slots);
            split_burst(items, &segment_items, &reads)
        },
        f,
    )
}

/// Cut a burst's `items` into the consecutive reads `plan_reads` planned over
/// its segments, moving each item once (no copies of the items).
/// `segment_items` is the number of items each segment covers.
fn split_burst<I>(
    mut items: Vec<I>,
    segment_items: &[usize],
    reads: &[Range<usize>],
) -> Vec<Vec<I>> {
    let mut item_reads: Vec<Vec<I>> = Vec::with_capacity(reads.len());
    // Split from the back so each `split_off` moves only that read's items;
    // the first read keeps the burst's allocation.
    for read in reads.iter().skip(1).rev() {
        let read_items: usize = segment_items[read.clone()].iter().sum();
        item_reads.push(items.split_off(items.len() - read_items));
    }
    item_reads.push(items);
    item_reads.reverse();
    item_reads
}

/// The dispatcher and consumer behind [`pipelined_chunks`] and
/// [`pipelined_keyed_batches`]: turn ordered `bursts` into reads and run them
/// with at most `max_concurrent_reads` read frames in flight, preserving input
/// order.
///
/// For each burst the dispatcher calls `plan(items, free_slots)` **once**, with
/// the number of drainer slots free at that moment, and then opens the planned
/// reads in order: per read it waits for a drainer slot, then calls `open`
/// synchronously and spawns the drainer of the stream it returns (see
/// [`open_items_frame`]). A read that finds no free slot waits for one with
/// its planned size; the burst is deliberately not re-planned as slots free
/// up. Watermark and error frames pass through in order; an error ends the
/// stream.
///
/// The per-frame **row** channel is unbounded so the drainer's sends never
/// block; it is unbounded in type only — the drainer sends one message per row
/// of its read (at most the read's item count under the ~1:1 call-site
/// contract) plus an optional terminal error, so memory is O(read items) per
/// in-flight frame. The **frame** channel is bounded by
/// `max_concurrent_reads`, and a per-stage drainer slot caps in-flight *item*
/// frames at `max_concurrent_reads` (held until each frame is consumed).
///
/// The dispatcher is spawned eagerly, so reads progress (and release their
/// BigTable permits at RPC completion) whether or not the consumer is polling.
/// Dropping the returned stream aborts the dispatcher and every live drainer.
/// A drainer or dispatcher panic surfaces to the consumer instead of silently
/// truncating the stream.
fn pipelined_reads<B, R, O, P, E, Plan, Open, Fut>(
    bursts: BoxStream<'static, Result<ChunkInput<B, P>, E>>,
    max_concurrent_reads: usize,
    mut plan: Plan,
    open: Open,
) -> BoxStream<'static, Result<Watermarked<O, P>, E>>
where
    B: Send + 'static,
    R: Send + 'static,
    O: Send + 'static,
    P: Copy + Send + 'static,
    E: Send + 'static,
    Plan: FnMut(Vec<B>, usize) -> Vec<R> + Send + 'static,
    Open: Fn(R) -> Fut + Send + 'static,
    Fut: Future<Output = Result<BoxStream<'static, Result<O, E>>, E>> + Send + 'static,
{
    // Per-stage cap on in-flight *item* frames (queued + being consumed),
    // distinct from the shared BigTable request semaphore: it only bounds how
    // far the dispatcher runs ahead, so a slow consumer can't pile up
    // unbounded drained-but-unconsumed reads. Held in each item frame until
    // the consumer finishes it.
    let drainer_slots = Arc::new(Semaphore::new(max_concurrent_reads));
    // Bounded so a sparse stream of watermark/error frames (which take no
    // drainer slot) can't enqueue unboundedly at a stalled consumer.
    let (frame_tx, frame_rx) = mpsc::channel::<FrameHandle<O, P, E>>(max_concurrent_reads);

    // Dispatcher: pull ordered bursts, plan each, spawn a permit-owning
    // drainer per read, hand ordered frame handles to the consumer. Spawned
    // (not lazy) so drainers make progress — and release their BigTable
    // permits at RPC completion — independently of whether the consumer is
    // currently polling.
    let dispatcher = tokio::spawn({
        let drainer_slots = drainer_slots.clone();
        async move {
            let mut bursts = bursts;
            while let Some(input) = bursts.next().await {
                match input {
                    Ok(ChunkInput::Items(items)) => {
                        // Planned once against the slots free now; the reads
                        // below that find no free slot wait with their
                        // planned size.
                        for read in plan(items, drainer_slots.available_permits()) {
                            // Acquire a slot before opening so the dispatcher
                            // never holds a BigTable permit while waiting on
                            // the in-flight bound.
                            let Ok(slot) = drainer_slots.clone().acquire_owned().await else {
                                return;
                            };
                            let frame = open_items_frame(&open, read, slot);
                            // On send failure the consumer is gone: returning
                            // drops `frame` (its drainer handle and slot),
                            // aborting the drainer and releasing the slot.
                            if frame_tx.send(frame).await.is_err() {
                                return;
                            }
                        }
                    }
                    Ok(ChunkInput::Watermark(pos)) => {
                        if frame_tx.send(FrameHandle::Watermark(pos)).await.is_err() {
                            return;
                        }
                    }
                    Err(e) => {
                        // Terminal error: send in order, then stop.
                        let _ = frame_tx.send(FrameHandle::Err(e)).await;
                        return;
                    }
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
                    // truncated read. `_slot` releases when this arm's
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

/// Open one item frame: invoke `open` synchronously, in read order, so its
/// synchronous side effects stay ordered, and spawn the drainer that awaits
/// the returned future and forwards its rows. `slot` is held until the
/// consumer finishes the frame.
fn open_items_frame<R, O, P, E, Open, Fut>(
    open: &Open,
    read: R,
    slot: OwnedSemaphorePermit,
) -> FrameHandle<O, P, E>
where
    Open: Fn(R) -> Fut,
    Fut: Future<Output = Result<BoxStream<'static, Result<O, E>>, E>> + Send + 'static,
    O: Send + 'static,
    E: Send + 'static,
{
    let fut = open(read);
    // Unbounded so the drainer's sends are non-blocking: the drainer holds the
    // BigTable permit while draining and must NEVER block on the consumer — a
    // bounded channel would let a stalled consumer wedge the drainer and hold
    // the permit across downstream backpressure, the exact deadlock class this
    // design removes. Unbounded in type only: the drainer sends one message per
    // row of its read (at most the read's item count under the ~1:1 call-site
    // contract) plus an optional terminal error, so memory is O(read items) per
    // in-flight frame.
    #[allow(clippy::disallowed_methods)] // non-blocking; bounded by the read's item count
    let (row_tx, row_rx) = mpsc::unbounded_channel();
    let drain = tokio::spawn(async move {
        match fut.await {
            Ok(mut stream) => {
                while let Some(row) = stream.next().await {
                    // The inner stream's item is exactly the channel's item
                    // (`Result<O, E>`), so forward it directly. An Err is
                    // terminal (prefix-then-error): send it, then stop.
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

/// One frame for [`pipelined_reads`]: a burst of items or a watermark.
/// Watermarks are zero-cost passthroughs that occupy a slot in the input-order
/// sequence so the dispatcher forwards them in place between item frames.
enum ChunkInput<I, P = u64> {
    /// A burst: items that were ready together, in upstream order.
    Items(Vec<I>),
    Watermark(P),
}

/// Split a `Watermarked<I>` stream into bursts and watermarks. Drives upstream
/// with a manual ready-loop so the burst bounds only count the
/// `Watermarked::Item`s; watermarks don't consume burst capacity.
///
/// A burst is everything ready before upstream returns `Pending`, so a trickle
/// of items doesn't stall behind a half-full burst. It closes early before a
/// held watermark, before an upstream error, at EOF, and at its bounds: before
/// an item that starts a new group once it holds `max_burst_groups` groups, and
/// before an item that starts a new key once it holds `max_burst_keys` keys. The
/// bounds may be `usize::MAX`; nothing is preallocated from them. A group is
/// the items one [`SeekUnit`] takes together: one item, or a run of items in
/// one bucket. A key is one item, or consecutive items that share a row (see
/// [`SeekUnit::BucketRun`]), so a burst never ends between them. On
/// [`pipelined_chunks`] a group is one seek; [`pipelined_keyed_batches`] passes
/// [`SeekUnit::Item`], which makes every item its own group and key, so its
/// bursts are bounded in items.
///
/// Watermarks debounce into a single `held_wm` slot — only the latest matters
/// for resume. The held watermark flushes:
/// - Before the next Items burst (so a client that times out during the
///   downstream multiget for that burst has a valid resume point at the
///   burst's pre-state). A burst never spans a watermark.
/// - On upstream Pending, AFTER flushing any open burst (so sparse scans that
///   produce watermarks without producing items still surface fresh progress
///   to the wire before the request deadline).
/// - Before propagating a terminal error (so the latest in-flight watermark
///   isn't dropped).
/// - At EOF (so a trailing post-bucket watermark with no further items is
///   still observed).
///
/// Ordering invariant: a watermark `P` is only flushed AFTER any items it
/// dominates have already been emitted to the chunker's output. Items
/// dominated by held_wm are either in earlier emitted bursts (already
/// downstream) or in the open burst (flushed just before the WM in the same
/// Pending-path or next-Item-path). The dispatcher forwards frames in input
/// order over a FIFO frame channel and the consumer drains each item frame's
/// rows before the next frame, so those bursts' rows reach the wire before the
/// WM resolves.
///
/// Waker plumbing: the inner ready-drain loop uses `futures::poll!`,
/// which does not install a waker on `Pending`. We rely on the outer
/// `'outer` loop's `upstream.as_mut().next().await` — a real `.await` —
/// to register a fresh waker against the upstream. The next upstream
/// readiness wakes the chunker reliably.
fn chunks_with_watermarks<I, P, E>(
    upstream: BoxStream<'static, Result<Watermarked<I, P>, E>>,
    seek_unit: SeekUnit<I>,
    max_burst_groups: usize,
    max_burst_keys: usize,
) -> BoxStream<'static, Result<ChunkInput<I, P>, E>>
where
    I: Send + 'static,
    P: Copy + Send + 'static,
    E: Send + 'static,
{
    async_stream::try_stream! {
        futures::pin_mut!(upstream);
        let mut open = OpenBurst::new(seek_unit, max_burst_groups, max_burst_keys);
        let mut held_wm: Option<P> = None;

        'outer: loop {
            // Block until at least one entry is available.
            let first = match upstream.as_mut().next().await {
                None => break 'outer,
                Some(Ok(w)) => w,
                Some(Err(e)) => {
                    if let Some(burst) = open.take() {
                        yield burst;
                    }
                    if let Some(p) = held_wm.take() {
                        yield ChunkInput::Watermark(p);
                    }
                    Err(e)?;
                    unreachable!();
                }
            };

            // Drain everything else that's immediately ready. Only Items count
            // toward the burst bounds; watermarks are free.
            let mut next = Some(first);
            loop {
                let w = match next.take() {
                    Some(w) => w,
                    None => match futures::poll!(upstream.as_mut().next()) {
                        Poll::Ready(Some(Ok(w))) => w,
                        Poll::Ready(Some(Err(e))) => {
                            if let Some(burst) = open.take() {
                                yield burst;
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
                        // Emit held WM before this item's burst so a client
                        // that times out during the downstream multiget can
                        // resume at the burst's pre-state.
                        if let Some(p) = held_wm.take() {
                            if let Some(burst) = open.take() {
                                yield burst;
                            }
                            yield ChunkInput::Watermark(p);
                        }
                        if let Some(burst) = open.close_before(&i) {
                            yield burst;
                        }
                        open.push(i);
                    }
                    Watermarked::Watermark(p) => {
                        held_wm = Some(p);
                    }
                }
            }
            // Upstream pending: flush the open burst so the trickle doesn't
            // stall, then flush any held watermark so sparse scans (long gaps
            // between items) still surface fresh progress to the wire before
            // the request deadline. The ordering invariant survives because
            // the open burst is forwarded as frames strictly before the WM
            // frame: any items the WM dominates are either already on the wire
            // (earlier bursts) or in this just-flushed burst.
            if let Some(burst) = open.take() {
                yield burst;
            }
            if let Some(p) = held_wm.take() {
                yield ChunkInput::Watermark(p);
            }
        }
        // EOF: trailing flush.
        if let Some(burst) = open.finish() {
            yield burst;
        }
        if let Some(p) = held_wm.take() {
            yield ChunkInput::Watermark(p);
        }
    }
    .boxed()
}

/// The burst [`chunks_with_watermarks`] is filling.
struct OpenBurst<I> {
    items: Vec<I>,
    /// Number of groups the items make up (see [`chunks_with_watermarks`]).
    groups: usize,
    /// Number of keys the items make up (see [`chunks_with_watermarks`]).
    keys: usize,
    seek_unit: SeekUnit<I>,
    max_groups: usize,
    max_keys: usize,
}

impl<I> OpenBurst<I> {
    fn new(seek_unit: SeekUnit<I>, max_groups: usize, max_keys: usize) -> Self {
        Self {
            items: Vec::new(),
            groups: 0,
            keys: 0,
            seek_unit,
            max_groups,
            max_keys,
        }
    }

    fn starts_group(&self, item: &I) -> bool {
        !matches!(self.items.last(), Some(last) if self.seek_unit.continues(last, item))
    }

    fn starts_key(&self, item: &I) -> bool {
        !matches!(self.items.last(), Some(last) if self.seek_unit.shares_key(last, item))
    }

    /// Close the burst if `item` starts a group once it holds `max_groups`
    /// groups, or starts a key once it holds `max_keys` keys. A run's end, and
    /// a key's, is known only once the next item arrives, so a burst is closed
    /// here rather than when its last group or key begins.
    fn close_before<P>(&mut self, item: &I) -> Option<ChunkInput<I, P>> {
        let groups_full = self.groups >= self.max_groups && self.starts_group(item);
        let keys_full = self.keys >= self.max_keys && self.starts_key(item);
        if groups_full || keys_full {
            self.take()
        } else {
            None
        }
    }

    fn push(&mut self, item: I) {
        if self.starts_group(&item) {
            self.groups += 1;
        }
        if self.starts_key(&item) {
            self.keys += 1;
        }
        self.items.push(item);
    }

    /// Close the burst, leaving an empty one open; `None` if it is empty.
    fn take<P>(&mut self) -> Option<ChunkInput<I, P>> {
        if self.items.is_empty() {
            return None;
        }
        self.groups = 0;
        self.keys = 0;
        Some(ChunkInput::Items(std::mem::take(&mut self.items)))
    }

    /// Close the burst at end of input; `None` if it is empty.
    fn finish<P>(self) -> Option<ChunkInput<I, P>> {
        (!self.items.is_empty()).then_some(ChunkInput::Items(self.items))
    }
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

/// Read the keys of `(item, keys)` pairs through one request-scoped
/// [`RequestCache`] over `fetcher`, and emit each item, in input order, with a
/// map of only its own keys' rows. A key without a row is an error naming the
/// key (index/storage divergence).
///
/// Batching follows the same rule as [`pipelined_chunks`]: the stage collects a
/// **burst** of the items ready before upstream returns `Pending` (closed early
/// by a watermark, an upstream error, EOF, or `max_concurrent_reads ×
/// max_seeks_per_read` items) and plans its reads once, against the drainer
/// slots free at that moment, with [`plan_reads`]. A seek is a key still to
/// read: a key this stream's cache already holds or is reading, for an earlier
/// item or an earlier burst, is not read again and does not count. Each key
/// still to read is claimed by the first item of the burst that lists it, and
/// an item's claimed keys may be divided between reads, whether the burst is
/// packed or split: the item is emitted by the read carrying its last claimed
/// key, after waiting for every key it lists.
/// - `max_seeks_per_read` is the largest read, in keys. With
///   `min_seeks_per_split_read` unset, a burst is packed into the fewest
///   near-equal reads of at most `max_seeks_per_read` keys.
/// - `min_seeks_per_split_read` is the least a read gets when a burst is split
///   over free slots (up to one read per free slot).
///
/// A read that finds no free slot waits for one with its planned size. A slot
/// is held from a read's start until the consumer has taken its items, and
/// the read's frame ends only once its backend read has ended, so dropping the
/// stream after it ends never cuts a read off mid-response.
///
/// Like [`pipelined_chunks`], the stage is eager (reads start, and complete,
/// without the consumer polling) and dropping the returned stream aborts every
/// read. Memory per stage is bounded by every row the stream has read (the
/// cache keeps each row for later items) plus the items, each with its map, of
/// `max_concurrent_reads` reads awaiting the consumer. Output and watermarks
/// keep input order.
pub(crate) fn pipelined_keyed_batches<I, K, V, P, E>(
    upstream: MarkedKeyedUpstream<I, K, E, P>,
    max_seeks_per_read: usize,
    max_concurrent_reads: usize,
    min_seeks_per_split_read: Option<usize>,
    fetcher: Arc<dyn CacheFetcher<K, V>>,
) -> MarkedKeyedDownstream<I, K, V, E, P>
where
    I: Send + 'static,
    K: Copy + Eq + std::hash::Hash + Ord + std::fmt::Debug + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    P: Copy + Send + 'static,
    E: From<anyhow::Error> + Send + 'static,
{
    assert!(
        max_seeks_per_read > 0,
        "pipelined_keyed_batches: max_seeks_per_read must be > 0"
    );
    assert!(
        max_concurrent_reads > 0,
        "pipelined_keyed_batches: max_concurrent_reads must be > 0"
    );
    assert!(
        min_seeks_per_split_read != Some(0),
        "pipelined_keyed_batches: min_seeks_per_split_read must be > 0"
    );

    // Every seek is one key, so there is no separate key cap.
    let limits = ReadLimits {
        max_seeks_per_read,
        max_keys_per_read: None,
        min_seeks_per_split_read,
    };
    let max_burst_items = max_concurrent_reads.saturating_mul(max_seeks_per_read);
    // A burst's seeks are known only once its keys are reserved, so bursts are
    // bounded in items: `SeekUnit::Item` makes every item its own group and key.
    let bursts = chunks_with_watermarks(upstream, SeekUnit::Item, max_burst_items, max_burst_items);
    let cache = RequestCache::new(fetcher);
    pipelined_reads(
        bursts,
        max_concurrent_reads,
        move |items: Vec<(I, Vec<K>)>, free_slots| {
            plan_keyed_reads(cache.clone(), items, limits, free_slots)
        },
        open_keyed_read::<I, K, V, E>,
    )
}

/// An item of a [`KeyedRead`] with the value of every distinct key it lists.
type ItemWithValues<I, K, V> = (I, Vec<(K, CachedValue<V>)>);

/// The rows one keyed read emits: each item with a map of its own keys.
type KeyedRows<I, K, V, E> = BoxStream<'static, Result<KeyedBatchOutput<I, K, V>, E>>;

/// One planned read of a keyed burst: the keys it reads, and the items it
/// emits, each with the value of every distinct key the item lists.
struct KeyedRead<I, K, V> {
    /// Holding the cache keeps the reads this read's items wait on alive.
    cache: Arc<RequestCache<K, V>>,
    unread: UnreadKeys<K, V>,
    items: Vec<ItemWithValues<I, K, V>>,
}

/// A planner segment of a keyed burst: the key it claims, if any, and, on an
/// item's last claimed key (or its only segment when it claims none), the item
/// with the keys it lists.
struct KeyedPiece<I, K> {
    key: Option<K>,
    item: Option<(I, Vec<K>)>,
}

/// Plan the reads of one keyed burst against `cache`.
///
/// All of the burst's keys are reserved at once; the keys this reservation
/// reserved first are its keys to read, each claimed by the first item that
/// lists it. Every claimed key is its own segment adding a seek, so the planner
/// may divide an item's keys between reads; the item rides on the segment of
/// its last claimed key. An item that claims no key is a rider: a segment of
/// its own that adds no seek, which joins the read that is open.
///
/// No frame waits on a later read: keys are claimed in item order and an item
/// rides on its last claimed key, so every key an item lists is read by the
/// read carrying the item, by an earlier read of this burst, or by an earlier
/// burst; an item that claims no key lists only keys claimed by earlier items
/// of the burst or reserved by earlier bursts. Reads are opened in order and
/// run in the cache's own tasks, so the consumer, which drains frames in
/// order, never waits on a read that has not started; this is what keeps the
/// stage deadlock-free.
fn plan_keyed_reads<I, K, V>(
    cache: Arc<RequestCache<K, V>>,
    items: Vec<(I, Vec<K>)>,
    limits: ReadLimits,
    free_slots: usize,
) -> Vec<KeyedRead<I, K, V>>
where
    K: Copy + Eq + std::hash::Hash + Ord + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    let burst_keys: Vec<K> = items
        .iter()
        .flat_map(|(_, keys)| keys.iter().copied())
        .collect();
    let mut reservation = cache.reserve(&burst_keys);
    drop(burst_keys);

    let mut claimed: HashSet<K> = HashSet::new();
    // Every item makes at least one segment.
    let mut segments: Vec<Segment> = Vec::with_capacity(items.len());
    let mut pieces: Vec<KeyedPiece<I, K>> = Vec::with_capacity(items.len());
    for (item, keys) in items {
        let first_piece = pieces.len();
        for key in keys.iter().copied() {
            if reservation.is_unread(&key) && claimed.insert(key) {
                segments.push(Segment {
                    adds_seek: true,
                    keys: 1,
                });
                pieces.push(KeyedPiece {
                    key: Some(key),
                    item: None,
                });
            }
        }
        if pieces.len() == first_piece {
            segments.push(Segment {
                adds_seek: false,
                keys: 0,
            });
            pieces.push(KeyedPiece {
                key: None,
                item: Some((item, keys)),
            });
        } else {
            pieces
                .last_mut()
                .expect("the item claimed a key, so it pushed a piece")
                .item = Some((item, keys));
        }
    }

    let reads = plan_reads(&segments, limits, free_slots);
    let mut pieces = pieces.into_iter();
    let mut keyed_reads = Vec::with_capacity(reads.len());
    for read in reads {
        let mut read_keys: Vec<K> = Vec::with_capacity(read.len());
        let mut read_items = Vec::new();
        for piece in pieces.by_ref().take(read.len()) {
            read_keys.extend(piece.key);
            if let Some((item, keys)) = piece.item {
                let mut listed: HashSet<K> = HashSet::with_capacity(keys.len());
                let values: Vec<(K, CachedValue<V>)> = keys
                    .into_iter()
                    .filter(|key| listed.insert(*key))
                    .map(|key| (key, reservation.value(&key)))
                    .collect();
                read_items.push((item, values));
            }
        }
        keyed_reads.push(KeyedRead {
            cache: cache.clone(),
            unread: reservation.take_unread(&read_keys),
            items: read_items,
        });
    }
    keyed_reads
}

/// Open one keyed read: start its backend read now, in read order, and return
/// the stream that emits each of its items, in order, with a map of the item's
/// own keys once their values have resolved. The stream ends only once the
/// backend read has ended.
fn open_keyed_read<I, K, V, E>(
    read: KeyedRead<I, K, V>,
) -> future::Ready<Result<KeyedRows<I, K, V, E>, E>>
where
    I: Send + 'static,
    K: Copy + Eq + std::hash::Hash + Ord + std::fmt::Debug + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    E: From<anyhow::Error> + Send + 'static,
{
    let KeyedRead {
        cache,
        unread,
        items,
    } = read;
    let read_end = cache.read(unread);
    let rows: KeyedRows<I, K, V, E> = async_stream::try_stream! {
        let _cache = cache;
        for (item, values) in items {
            let mut map: HashMap<K, V> = HashMap::with_capacity(values.len());
            for (key, value) in values {
                let row = value
                    .await
                    .map_err(|e| E::from(anyhow::Error::new(e)))?
                    .ok_or_else(|| {
                        E::from(anyhow::anyhow!(
                            "keyed read missing key {key:?} requested by the read stage \
                             (indicates index/storage divergence)"
                        ))
                    })?;
                map.insert(key, row);
            }
            yield (item, Arc::new(map));
        }
        // The slot covers the whole read: a frame never ends while its read
        // is mid-response.
        read_end.await;
    }
    .boxed();
    future::ready(Ok(rows))
}

/// Resolved pipeline frame: items pass through; watermarks carry both the
/// original scan-domain position and its checkpoint.
pub(crate) enum ResolvedWatermarked<T, P = u64> {
    Item(T),
    Watermark { position: P, cp: u64 },
}

#[derive(Debug)]
pub(crate) enum RenderAheadError<UpstreamError, RenderError> {
    Upstream(UpstreamError),
    Render(RenderError),
}

/// Render resolved items concurrently while preserving item, watermark, and
/// error order.
///
/// The returned stream is lazy until polled. Ordered buffering admits at most
/// `look_ahead` render tasks, and `look_ahead = 1` serializes rendering with
/// downstream emission. Dropping the stream aborts admitted tasks through their
/// [`TaskGuard`]s. Completed responses retained behind an earlier frame are
/// bounded by `look_ahead * rendered page size`; a full-mask fat checkpoint
/// page is about 4–5 MB.
pub(crate) fn render_ahead<I, O, P, UpstreamError, RenderError, RenderFut>(
    upstream: BoxStream<'static, Result<ResolvedWatermarked<I, P>, UpstreamError>>,
    look_ahead: usize,
    render: impl Fn(I) -> RenderFut + Send + Sync + 'static,
) -> BoxStream<
    'static,
    Result<ResolvedWatermarked<O, P>, RenderAheadError<UpstreamError, RenderError>>,
>
where
    I: Send + 'static,
    O: Send + 'static,
    P: Send + 'static,
    UpstreamError: Send + 'static,
    RenderError: Send + 'static,
    RenderFut: Future<Output = Result<O, RenderError>> + Send + 'static,
{
    assert!(look_ahead > 0, "render_ahead: look_ahead must be > 0");
    let render = Arc::new(render);

    upstream
        .map(move |frame| {
            let render = render.clone();
            match frame {
                Ok(ResolvedWatermarked::Item(item)) => {
                    let render_task =
                        TaskGuard::new(tokio::spawn(async move { render(item).await }));
                    future::Either::Left(async move {
                        match render_task.await {
                            Ok(Ok(output)) => Ok(ResolvedWatermarked::Item(output)),
                            Ok(Err(error)) => Err(RenderAheadError::Render(error)),
                            Err(join_error) => {
                                surface_panic(Err(join_error));
                                std::future::pending().await
                            }
                        }
                    })
                }
                Ok(ResolvedWatermarked::Watermark { position, cp }) => {
                    future::Either::Right(future::ready(Ok(ResolvedWatermarked::Watermark {
                        position,
                        cp,
                    })))
                }
                Err(error) => {
                    future::Either::Right(future::ready(Err(RenderAheadError::Upstream(error))))
                }
            }
        })
        .buffered(look_ahead)
        .boxed()
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
impl<P> ResolvedScanStop<P> {
    pub(crate) fn into_scan_limit(self) -> Result<(P, Option<u64>), RpcError> {
        match self {
            Self::ScanLimit {
                position,
                checkpoint,
            } => Ok((position, checkpoint)),
            Self::Cancelled => Err(RpcError::new(
                tonic::Code::Cancelled,
                ScanStop::Cancelled.to_string(),
            )),
            Self::Fault(inner) => Err(RpcError::from(inner)),
        }
    }
}

/// Resolve watermark frames into progress beacons, and resolve the
/// authoritative frontier carried separately by a terminal
/// [`ScanStop::ScanLimit`].
///
/// Watermarks arrive already paced by the bitmap evaluator
/// ([`sui_inverted_index::WatermarkPacing`]): it emits one once the configured
/// amount of scan work (bucket rows charged to the scan budget, plus a cost
/// per reopen) has been done without an item; never holds back the scan's
/// first output; discards its held watermark at a stop; and emits it at a
/// clean end only for cursor-bound ranges (see
/// `QueryContext::watermark_pacing`). So every watermark that reaches this
/// stage is resolved, in arrival order, each by one `tx_cp_resolve` lookup,
/// and emitted as a beacon when the lookup finds a checkpoint. Items keep
/// their order relative to watermarks: an item passes through at once unless
/// a lookup for an earlier watermark is still running.
///
/// A lookup always runs to completion: upstream is not polled meanwhile, so
/// frames that arrive during a lookup, items included, wait for it rather than
/// dropping a `ReadRows` already on the wire, which would reset its HTTP/2
/// stream. A failed lookup fails the stream.
///
/// Terminations:
/// - Clean EOF: nothing more is looked up.
/// - Scan limit: resolve the authoritative frontier, reusing the most recent
///   completed lookup (including a completed `None`) when it is for the same
///   position.
/// - Cancellation or fault: propagated as is; any watermark emitted before it
///   has already been resolved into the client's best resume point.
///
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

    async_stream::try_stream! {
        let mut upstream = std::pin::pin!(upstream);
        // Most recent completed lookup, including a completed `None`.
        let mut completed: Option<(P, Option<u64>)> = None;

        while let Some(next) = upstream.next().await {
            match next {
                Ok(Watermarked::Item(item)) => {
                    yield ResolvedWatermarked::Item(item);
                }
                Ok(Watermarked::Watermark(position)) => {
                    let checkpoint = resolver(position).await.map_err(resolver_error)?;
                    completed = Some((position, checkpoint));
                    if let Some(cp) = checkpoint {
                        yield ResolvedWatermarked::Watermark { position, cp };
                    }
                }
                Err(ScanStop::ScanLimit { scan_frontier }) => {
                    let position = scan_frontier_to_position(scan_frontier);
                    let checkpoint = match completed {
                        Some((resolved, checkpoint)) if resolved == position => checkpoint,
                        _ => resolver(position).await.map_err(resolver_error)?,
                    };
                    Err(ResolvedScanStop::ScanLimit {
                        position,
                        checkpoint,
                    })?;
                }
                Err(error) => {
                    Err(resolver_error(error))?;
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
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use futures::FutureExt;
    use futures::TryStreamExt;
    use futures::future::BoxFuture;
    use futures::stream;
    use tokio::sync::Barrier;
    use tokio::sync::Notify;
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
    async fn render_ahead_completes_admitted_work_without_consumer_polls() {
        let upstream: BoxStream<'static, Result<ResolvedWatermarked<u64>, RpcError>> =
            stream::iter([
                Ok(ResolvedWatermarked::Item(0u64)),
                Ok(ResolvedWatermarked::Item(1u64)),
            ])
            .boxed();
        let started = Arc::new(AtomicUsize::new(0));
        let completed = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(3));
        let completion_notify = Arc::new(Notify::new());
        let mut stream = render_ahead(upstream, 2, {
            let started = started.clone();
            let completed = completed.clone();
            let barrier = barrier.clone();
            let completion_notify = completion_notify.clone();
            move |item| {
                let started = started.clone();
                let completed = completed.clone();
                let barrier = barrier.clone();
                let completion_notify = completion_notify.clone();
                async move {
                    started.fetch_add(1, Ordering::SeqCst);
                    barrier.wait().await;
                    if completed.fetch_add(1, Ordering::SeqCst) + 1 == 2 {
                        completion_notify.notify_one();
                    }
                    Ok::<_, RpcError>(item)
                }
            }
        });

        tokio::task::yield_now().await;
        assert_eq!(started.load(Ordering::SeqCst), 0);

        let first_poll =
            futures::future::poll_fn(|cx| Poll::Ready(stream.poll_next_unpin(cx))).await;
        assert!(first_poll.is_pending());

        timeout(Duration::from_millis(500), barrier.wait())
            .await
            .expect("admitted renders did not reach the barrier");
        timeout(Duration::from_millis(500), completion_notify.notified())
            .await
            .expect("admitted renders did not complete");
        assert_eq!(started.load(Ordering::SeqCst), 2);
        assert_eq!(completed.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn render_ahead_preserves_items_and_watermarks_in_input_order() {
        let upstream: BoxStream<'static, Result<ResolvedWatermarked<u64>, RpcError>> =
            stream::iter([
                Ok(ResolvedWatermarked::Item(0u64)),
                Ok(ResolvedWatermarked::Watermark {
                    position: 10,
                    cp: 20,
                }),
                Ok(ResolvedWatermarked::Item(1u64)),
            ])
            .boxed();
        let release_first = Arc::new(Notify::new());
        let second_completed = Arc::new(Notify::new());
        let mut stream = render_ahead(upstream, 3, {
            let release_first = release_first.clone();
            let second_completed = second_completed.clone();
            move |item| {
                let release_first = release_first.clone();
                let second_completed = second_completed.clone();
                async move {
                    if item == 0 {
                        release_first.notified().await;
                    } else {
                        second_completed.notify_one();
                    }
                    Ok::<_, RpcError>(item)
                }
            }
        });

        let first_poll =
            futures::future::poll_fn(|cx| Poll::Ready(stream.poll_next_unpin(cx))).await;
        assert!(first_poll.is_pending());
        timeout(Duration::from_millis(500), second_completed.notified())
            .await
            .expect("later render did not complete");
        release_first.notify_one();

        match stream.next().await {
            Some(Ok(ResolvedWatermarked::Item(0))) => {}
            _ => panic!("expected item 0 first"),
        }
        match stream.next().await {
            Some(Ok(ResolvedWatermarked::Watermark {
                position: 10,
                cp: 20,
            })) => {}
            _ => panic!("expected watermark second"),
        }
        match stream.next().await {
            Some(Ok(ResolvedWatermarked::Item(1))) => {}
            _ => panic!("expected item 1 third"),
        }
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn dropping_render_ahead_stream_aborts_render_tasks() {
        let upstream: BoxStream<'static, Result<ResolvedWatermarked<u64>, RpcError>> =
            stream::iter([Ok(ResolvedWatermarked::Item(0u64))]).boxed();
        let limiter = Arc::new(Semaphore::new(1));
        let mut stream = render_ahead(upstream, 1, {
            let limiter = limiter.clone();
            move |_item| {
                let limiter = limiter.clone();
                async move {
                    let _permit =
                        limiter.clone().acquire_owned().await.map_err(|_| {
                            RpcError::new(tonic::Code::Internal, "test limiter closed")
                        })?;
                    std::future::pending::<Result<u64, RpcError>>().await
                }
            }
        });

        let first_poll =
            futures::future::poll_fn(|cx| Poll::Ready(stream.poll_next_unpin(cx))).await;
        assert!(first_poll.is_pending());
        wait_for_available_permits(&limiter, 0).await;

        drop(stream);
        wait_for_available_permits(&limiter, 1).await;
    }

    #[tokio::test]
    async fn render_ahead_propagates_render_error() {
        let upstream: BoxStream<'static, Result<ResolvedWatermarked<u64>, RpcError>> =
            stream::iter([Ok(ResolvedWatermarked::Item(0u64))]).boxed();
        let mut stream = render_ahead(upstream, 1, |_item| async move {
            Err::<u64, _>(RpcError::new(tonic::Code::Internal, "boom"))
        });

        let error = match stream.next().await {
            Some(Err(RenderAheadError::Render(error))) => error,
            _ => panic!("expected render error"),
        };
        let status: tonic::Status = error.into();
        assert_eq!(status.message(), "boom");
    }

    #[tokio::test]
    async fn render_ahead_resurfaces_render_panic() {
        let upstream: BoxStream<'static, Result<ResolvedWatermarked<u64>, RpcError>> =
            stream::iter([Ok(ResolvedWatermarked::Item(0u64))]).boxed();
        let stream = render_ahead(upstream, 1, |_item| async move {
            if std::hint::black_box(true) {
                panic!("render boom");
            }
            Ok::<_, RpcError>(0u64)
        });

        let outcome = std::panic::AssertUnwindSafe(stream.try_collect::<Vec<_>>())
            .catch_unwind()
            .await;
        assert!(
            outcome.is_err(),
            "render panic must surface, not silently truncate the stream"
        );
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

        let stream = pipelined_chunks(
            upstream,
            5,
            8,
            None,
            SeekUnit::Item,
            |chunk: Vec<u64>| async move {
                let first = chunk[0];
                let delay = Duration::from_millis(50u64.saturating_sub(first));
                tokio::time::sleep(delay).await;
                Ok::<_, RpcError>(stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed())
            },
        );

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

        let stream = pipelined_chunks(
            upstream,
            4,
            2,
            None,
            SeekUnit::Item,
            move |chunk: Vec<u64>| {
                // Record in the SYNCHRONOUS part of the closure: the dispatcher
                // invokes `f(chunk)` in chunker order, so this captures invocation
                // order even though the per-chunk drains run concurrently after.
                seen_for_closure.lock().unwrap().push(chunk.clone());
                async move {
                    Ok::<_, RpcError>(
                        stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed(),
                    )
                }
            },
        );

        let collected: Vec<u64> = items_only(stream).try_collect().await.expect("ok");
        assert_eq!(collected, (0..20).collect::<Vec<_>>());
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 5, "20 items / max_seeks_per_read=4 = 5 chunks");
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

        let stream = pipelined_chunks(upstream, 1, 3, None, SeekUnit::Item, {
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

        let stream = pipelined_chunks(
            upstream,
            2,
            2,
            None,
            SeekUnit::Item,
            |chunk: Vec<u64>| async move {
                Ok::<_, RpcError>(stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed())
            },
        );
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

        // max_seeks_per_read=2 groups [0, 1] into one chunk; the inner stream yields
        // row 0, then blocks before row 1.
        let stream = pipelined_chunks(
            upstream,
            2,
            1,
            None,
            SeekUnit::Item,
            move |chunk: Vec<u64>| {
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
            },
        );

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
        let stream = pipelined_chunks(
            upstream,
            3,
            1,
            None,
            SeekUnit::Item,
            |chunk: Vec<u64>| async move {
                let inner = async_stream::try_stream! {
                    yield chunk[0];
                    Err(RpcError::new(tonic::Code::Internal, "inner stream boom"))?;
                    yield chunk[1];
                };
                Ok::<_, RpcError>(inner.boxed())
            },
        );
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

        let stream = pipelined_chunks(upstream, 1, 1, None, SeekUnit::Item, {
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

        let stream = pipelined_chunks(upstream, 1, 1, None, SeekUnit::Item, {
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
        let upstream = ok_items(0..5u64); // max_seeks_per_read = 5 → one chunk of 5 rows
        let stream = pipelined_chunks(upstream, 5, 1, None, SeekUnit::Item, {
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

        let stream = pipelined_chunks(upstream, 1, 4, None, SeekUnit::Item, {
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

        let stream = pipelined_chunks(upstream, 1, 3, None, SeekUnit::Item, {
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
        // A drainer task that panics mid-stream must surface to the consumer
        // (via the joined handle), not silently truncate the chunk. The
        // `black_box` keeps the panic out of the compiler's reachability
        // analysis so the trailing `yield` stays statically reachable.
        let upstream = ok_items(0..1u64);
        let stream = pipelined_chunks(
            upstream,
            1,
            1,
            None,
            SeekUnit::Item,
            |_chunk: Vec<u64>| async move {
                let inner = async_stream::try_stream! {
                    yield 0u64;
                    if std::hint::black_box(true) {
                        panic!("drainer boom");
                    }
                    yield 1u64;
                };
                Ok::<_, RpcError>(inner.boxed())
            },
        );

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
        let stream = pipelined_chunks(
            upstream,
            10,
            n,
            None,
            SeekUnit::Item,
            |chunk: Vec<u64>| async move {
                Ok::<_, RpcError>(stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed())
            },
        );

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

        let stage1 = pipelined_chunks(upstream, 2, 1, None, SeekUnit::Item, {
            let limiter = limiter.clone();
            move |chunk| gated_chunk_stream(limiter.clone(), chunk)
        });
        let stage2 = pipelined_chunks(stage1, 1, 1, None, SeekUnit::Item, {
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

        let stream = pipelined_chunks(
            upstream,
            10,
            4,
            None,
            SeekUnit::Item,
            |chunk: Vec<u64>| async move {
                Ok::<_, RpcError>(stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed())
            },
        );
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
        let stream = pipelined_chunks(
            upstream,
            10,
            4,
            None,
            SeekUnit::Item,
            |chunk: Vec<u64>| async move {
                Ok::<_, RpcError>(stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed())
            },
        );
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

        let stream = pipelined_chunks(
            upstream,
            100,
            4,
            None,
            SeekUnit::Item,
            move |chunk: Vec<u64>| {
                let observed = observed_for_closure.clone();
                async move {
                    observed.lock().unwrap().push(chunk.clone());
                    Ok::<_, RpcError>(
                        stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed(),
                    )
                }
            },
        );
        let collected: Vec<Watermarked<u64>> = stream.try_collect().await.expect("ok");

        // First chunk had to flush at the watermark even though max_seeks_per_read=100
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

    /// Run `pipelined_chunks` over `frames`, all ready at once, and return the
    /// reads `f` was called with, in call order, and the output frames.
    async fn run_chunks(
        frames: Vec<Watermarked<u64>>,
        max_seeks_per_read: usize,
        max_concurrent_chunks: usize,
        min_seeks_per_split_read: Option<usize>,
        seek_unit: SeekUnit<u64>,
    ) -> (Vec<Vec<u64>>, Vec<Watermarked<u64>>) {
        let observed = Arc::new(Mutex::new(Vec::<Vec<u64>>::new()));
        let upstream = stream::iter(frames.into_iter().map(Ok::<_, RpcError>)).boxed();
        let stream = pipelined_chunks(
            upstream,
            max_seeks_per_read,
            max_concurrent_chunks,
            min_seeks_per_split_read,
            seek_unit,
            {
                let observed = observed.clone();
                move |chunk: Vec<u64>| {
                    observed.lock().unwrap().push(chunk.clone());
                    async move {
                        Ok::<_, RpcError>(
                            stream::iter(chunk.into_iter().map(Ok::<_, RpcError>)).boxed(),
                        )
                    }
                }
            },
        );
        let output: Vec<Watermarked<u64>> = stream.try_collect().await.expect("ok");
        let reads = observed.lock().unwrap().clone();
        (reads, output)
    }

    /// The reads `pipelined_chunks` issues, with 8 drainer slots, for `items`
    /// all ready at once (one burst), checking rows come out in input order.
    async fn reads_for(
        items: Vec<u64>,
        max_seeks_per_read: usize,
        min_seeks_per_split_read: Option<usize>,
        seek_unit: SeekUnit<u64>,
    ) -> Vec<Vec<u64>> {
        let frames = items.iter().copied().map(Watermarked::Item).collect();
        let (reads, output) = run_chunks(
            frames,
            max_seeks_per_read,
            8,
            min_seeks_per_split_read,
            seek_unit,
        )
        .await;
        assert_eq!(
            output,
            items.into_iter().map(Watermarked::Item).collect::<Vec<_>>()
        );
        reads
    }

    /// Groups items by thousands, like `tx_seq_digest` buckets.
    fn thousands(item: &u64) -> u64 {
        item / 1_000
    }

    /// Every item its own key, as transaction sequence numbers are.
    fn own_key(item: &u64) -> u64 {
        *item
    }

    fn bucket_runs(max_keys_per_read: usize) -> SeekUnit<u64> {
        SeekUnit::BucketRun {
            group: thousands,
            key: own_key,
            max_keys_per_read,
        }
    }

    /// Bucket runs whose items share a key in tens, as the events of one
    /// transaction share its `tx_seq_digest` row: items 0-9 are one key.
    fn bucket_runs_of_shared_keys(max_keys_per_read: usize) -> SeekUnit<u64> {
        SeekUnit::BucketRun {
            group: thousands,
            key: |item| item / 10,
            max_keys_per_read,
        }
    }

    /// A ready burst is split over the free slots, each read at least
    /// `min_seeks_per_split_read` seeks and at most `max_seeks_per_read`,
    /// near-equal with the larger reads last, in input order.
    #[tokio::test]
    async fn pipelined_chunks_spreads_burst_over_free_slots() {
        // 13 seeks over 8 free slots: 8 reads, although 2 would fit
        // `max_seeks_per_read`.
        assert_eq!(
            reads_for((0..13).collect(), 10, Some(1), SeekUnit::Item).await,
            vec![
                vec![0],
                vec![1],
                vec![2],
                vec![3, 4],
                vec![5, 6],
                vec![7, 8],
                vec![9, 10],
                vec![11, 12],
            ],
        );
        // A minimum read size of 2 allows 4 of the 8 slots for 9 seeks.
        assert_eq!(
            reads_for((0..9).collect(), 100, Some(2), SeekUnit::Item).await,
            vec![vec![0, 1], vec![2, 3], vec![4, 5], vec![6, 7, 8]],
        );
    }

    /// Without `min_seeks_per_split_read`, a burst goes out as the fewest reads
    /// of at most `max_seeks_per_read` seeks, near-equal in size.
    #[tokio::test]
    async fn pipelined_chunks_unsplit_burst_goes_out_as_fewest_reads() {
        assert_eq!(
            reads_for((0..60).collect(), 25, None, SeekUnit::Item).await,
            vec![
                (0..20).collect::<Vec<u64>>(),
                (20..40).collect(),
                (40..60).collect(),
            ],
        );
        assert_eq!(
            reads_for((0..20).collect(), 25, None, SeekUnit::Item).await,
            vec![(0..20).collect::<Vec<u64>>()],
        );
    }

    /// With bucket runs, one seek each, reads take whole runs:
    /// `max_seeks_per_read` counts runs, a read ends at the last run boundary
    /// before the key cap, and a run is cut only when it alone exceeds that cap.
    #[tokio::test]
    async fn pipelined_chunks_reads_take_whole_runs() {
        // Two runs per read, however many items they hold.
        assert_eq!(
            reads_for(
                vec![0, 1, 2, 1000, 1001, 2000, 3000, 3001, 3002, 3003],
                2,
                None,
                bucket_runs(100)
            )
            .await,
            vec![
                vec![0, 1, 2, 1000, 1001],
                vec![2000, 3000, 3001, 3002, 3003]
            ],
        );
        // Key cap 4: run 0 ends its read short of the cap rather than take
        // part of run 1; run 1 (6 keys) alone exceeds the cap and is cut there.
        assert_eq!(
            reads_for(
                vec![0, 1, 2, 1000, 1001, 1002, 1003, 1004, 1005],
                100,
                None,
                bucket_runs(4)
            )
            .await,
            vec![
                vec![0, 1, 2],
                vec![1000, 1001, 1002, 1003],
                vec![1004, 1005]
            ],
        );
        // Split over free slots, each read gets whole runs.
        let items = vec![0, 1, 2, 3, 4, 1000, 2000, 2001, 3000];
        assert_eq!(
            reads_for(items.clone(), 100, Some(1), bucket_runs(100)).await,
            vec![
                vec![0, 1, 2, 3, 4],
                vec![1000],
                vec![2000, 2001],
                vec![3000]
            ],
        );
        // At least two runs per read: two reads of two runs.
        assert_eq!(
            reads_for(items, 100, Some(2), bucket_runs(100)).await,
            vec![vec![0, 1, 2, 3, 4, 1000], vec![2000, 2001, 3000]],
        );
        // One run: one read, where a seek per item would make 20.
        assert_eq!(
            reads_for((0..20).collect(), 100, Some(1), bucket_runs(100)).await,
            vec![(0..20).collect::<Vec<u64>>()],
        );
    }

    /// Items that share a key, like a transaction's events, count once
    /// against the key cap, and are never divided between reads or bursts.
    #[tokio::test]
    async fn items_sharing_a_key_count_once_and_stay_together() {
        // One bucket: keys 0 (3 items), 1 (2), 2 (1) and 3 (4).
        let items = vec![0, 1, 2, 10, 11, 20, 30, 31, 32, 33];
        // Two keys per read, whatever their items, cut only between keys.
        assert_eq!(
            reads_for(items.clone(), 100, None, bucket_runs_of_shared_keys(2)).await,
            vec![vec![0, 1, 2, 10, 11], vec![20, 30, 31, 32, 33]],
        );
        // A burst of two keys closes before the third key, not inside a key.
        assert_eq!(
            bursts_for(items, bucket_runs_of_shared_keys(100), usize::MAX, 2).await,
            vec![vec![0, 1, 2, 10, 11], vec![20, 30, 31, 32, 33]],
        );
    }

    /// A watermark between ready items closes the burst before it: no read
    /// spans it, and it comes out after the items before it.
    #[tokio::test]
    async fn pipelined_chunks_no_read_spans_a_watermark() {
        let frames: Vec<Watermarked<u64>> = (0..5u64)
            .map(Watermarked::Item)
            .chain([Watermarked::Watermark(100)])
            .chain((5..10u64).map(Watermarked::Item))
            .collect();
        let (reads, output) = run_chunks(frames, 100, 8, Some(1), SeekUnit::Item).await;

        for read in &reads {
            assert!(
                read.iter().all(|&item| item < 5) || read.iter().all(|&item| item >= 5),
                "read {read:?} spans the watermark"
            );
        }
        assert_eq!(reads.concat(), (0..10u64).collect::<Vec<_>>());
        let expected: Vec<Watermarked<u64>> = (0..5u64)
            .map(Watermarked::Item)
            .chain([Watermarked::Watermark(100)])
            .chain((5..10u64).map(Watermarked::Item))
            .collect();
        assert_eq!(output, expected);
    }

    /// An always-ready upstream longer than `max_concurrent_chunks ×
    /// max_seeks_per_read` is cut into bounded bursts: no read exceeds
    /// `max_seeks_per_read`, and reads and rows keep input order.
    #[tokio::test]
    async fn pipelined_chunks_bounds_bursts_of_an_always_ready_upstream() {
        let frames = (0..100u64).map(Watermarked::Item).collect();
        let (reads, output) = run_chunks(frames, 4, 3, Some(1), SeekUnit::Item).await;

        for read in &reads {
            assert!(read.len() <= 4, "read {read:?} exceeds max_seeks_per_read");
        }
        assert_eq!(reads.concat(), (0..100u64).collect::<Vec<_>>());
        assert_eq!(
            output,
            (0..100u64).map(Watermarked::Item).collect::<Vec<_>>()
        );
    }

    /// The bursts `chunks_with_watermarks` cuts `items`, all ready at once,
    /// into.
    async fn bursts_for(
        items: Vec<u64>,
        seek_unit: SeekUnit<u64>,
        max_burst_groups: usize,
        max_burst_keys: usize,
    ) -> Vec<Vec<u64>> {
        let frames: Vec<ChunkInput<u64>> =
            chunks_with_watermarks(ok_items(items), seek_unit, max_burst_groups, max_burst_keys)
                .try_collect()
                .await
                .expect("ok");
        frames
            .into_iter()
            .map(|frame| match frame {
                ChunkInput::Items(items) => items,
                ChunkInput::Watermark(p) => panic!("unexpected WM({p})"),
            })
            .collect()
    }

    /// A burst closes before an item that starts a new group once it holds
    /// `max_burst_groups` groups, and before an item that starts a new key once
    /// it holds `max_burst_keys` keys.
    #[tokio::test]
    async fn chunks_with_watermarks_closes_bursts_at_their_bounds() {
        let items = vec![0, 1, 1000, 1001, 2000];
        assert_eq!(
            bursts_for(items.clone(), bucket_runs(100), 2, usize::MAX).await,
            vec![vec![0, 1, 1000, 1001], vec![2000]],
        );
        assert_eq!(
            bursts_for(items.clone(), bucket_runs(100), usize::MAX, 3).await,
            vec![vec![0, 1, 1000], vec![1001, 2000]],
        );
        assert_eq!(
            bursts_for(items, bucket_runs(100), usize::MAX, usize::MAX).await,
            vec![vec![0, 1, 1000, 1001, 2000]],
        );
    }

    /// Watermarks bracketing a run of items don't take burst capacity: 6 items
    /// between two watermarks with a burst bound of 4 go out as bursts of 4
    /// and 2.
    #[tokio::test]
    async fn chunks_with_watermarks_excludes_watermarks_from_burst_budget() {
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

        let frames: Vec<ChunkInput<u64>> = chunks_with_watermarks(upstream, SeekUnit::Item, 4, 4)
            .try_collect()
            .await
            .expect("ok");

        let shapes: Vec<(&str, Option<u64>, usize)> = frames
            .iter()
            .map(|f| match f {
                ChunkInput::Items(items) => ("Items", None, items.len()),
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
        let mut chunker = chunks_with_watermarks(upstream, SeekUnit::Item, 100, 100);

        tx.send(Ok(Watermarked::Item(10))).await.expect("send");
        tx.send(Ok(Watermarked::Watermark(50))).await.expect("send");

        // The Item flushes as a partial chunk on Pending.
        let first = timeout(Duration::from_millis(500), chunker.next())
            .await
            .expect("partial Items chunk should flush at Pending")
            .expect("some")
            .expect("ok");
        match first {
            ChunkInput::Items(items) => assert_eq!(items, vec![10]),
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
            ChunkInput::Items(items) => panic!("expected held WM, got Items({items:?})"),
        }

        tx.send(Ok(Watermarked::Item(20))).await.expect("send");

        let third = timeout(Duration::from_millis(500), chunker.next())
            .await
            .expect("next Items chunk arrives after WM flush")
            .expect("some")
            .expect("ok");
        match third {
            ChunkInput::Items(items) => assert_eq!(items, vec![20]),
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
        let mut chunker = chunks_with_watermarks(upstream, SeekUnit::Item, 100, 100);

        tx.send(Ok(Watermarked::Watermark(10))).await.expect("send");
        tx.send(Ok(Watermarked::Watermark(20))).await.expect("send");

        let first = timeout(Duration::from_millis(500), chunker.next())
            .await
            .expect("held WM should flush on Pending without any prior Item")
            .expect("some")
            .expect("ok");
        match first {
            ChunkInput::Watermark(p) => assert_eq!(p, 20, "latest WM in the burst wins"),
            ChunkInput::Items(items) => panic!("expected WM, got Items({items:?})"),
        }

        tx.send(Ok(Watermarked::Watermark(30))).await.expect("send");
        let second = timeout(Duration::from_millis(500), chunker.next())
            .await
            .expect("next WM also flushes on Pending")
            .expect("some")
            .expect("ok");
        match second {
            ChunkInput::Watermark(p) => assert_eq!(p, 30),
            ChunkInput::Items(items) => panic!("expected WM, got Items({items:?})"),
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

        let stream = pipelined_chunks(
            upstream,
            4,
            2,
            None,
            SeekUnit::Item,
            move |chunk: Vec<u64>| {
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
            },
        );
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

    // ---- pipelined_keyed_batches ----

    use futures::SinkExt;
    use futures::channel::mpsc;

    type TestItem = (u32, Vec<i32>);
    type TestRows = BoxStream<'static, Result<(i32, i32), RpcError>>;
    type TestFetch = BoxFuture<'static, Result<TestRows, RpcError>>;
    /// The keys of every fetch a stub fetcher has served, in call order.
    type TestFetches = Arc<Mutex<Vec<Vec<i32>>>>;

    /// Stub keyed backend: records the keys of every fetch, in call order, and
    /// answers it with `respond`.
    struct StubFetcher {
        fetches: TestFetches,
        respond: Box<dyn Fn(Vec<i32>) -> TestFetch + Send + Sync>,
    }

    #[async_trait::async_trait]
    impl CacheFetcher<i32, i32> for StubFetcher {
        async fn fetch(&self, keys: Vec<i32>) -> Result<TestRows, RpcError> {
            self.fetches.lock().unwrap().push(keys.clone());
            (self.respond)(keys).await
        }
    }

    /// A stub fetcher answering with `respond`, and the keys of every fetch it
    /// has served.
    fn stub_fetcher(
        respond: impl Fn(Vec<i32>) -> TestFetch + Send + Sync + 'static,
    ) -> (Arc<dyn CacheFetcher<i32, i32>>, TestFetches) {
        let fetches = Arc::new(Mutex::new(Vec::new()));
        let fetcher = StubFetcher {
            fetches: fetches.clone(),
            respond: Box::new(respond),
        };
        (Arc::new(fetcher), fetches)
    }

    /// A row for every key, equal to the key.
    fn echo_rows(keys: Vec<i32>) -> TestFetch {
        async move {
            Ok::<TestRows, RpcError>(
                stream::iter(keys.into_iter().map(|key| Ok::<_, RpcError>((key, key)))).boxed(),
            )
        }
        .boxed()
    }

    /// Each fetch's keys, sorted, in sorted order: the reads of one burst run
    /// concurrently.
    fn sorted_fetches(fetches: &Mutex<Vec<Vec<i32>>>) -> Vec<Vec<i32>> {
        let mut sorted: Vec<Vec<i32>> = fetches
            .lock()
            .unwrap()
            .iter()
            .map(|keys| {
                let mut keys = keys.clone();
                keys.sort_unstable();
                keys
            })
            .collect();
        sorted.sort();
        sorted
    }

    fn keyed_upstream(
        items: Vec<TestItem>,
    ) -> BoxStream<'static, Result<Watermarked<TestItem>, RpcError>> {
        stream::iter(
            items
                .into_iter()
                .map(|item| Ok::<_, RpcError>(Watermarked::Item(item))),
        )
        .boxed()
    }

    /// The emitted items, each with its map's keys sorted, checking every row
    /// is its key's.
    fn keyed_output(
        stream: MarkedKeyedDownstream<u32, i32, i32, RpcError>,
    ) -> BoxStream<'static, Result<(u32, Vec<i32>), RpcError>> {
        items_only(stream)
            .map_ok(|(item, map)| {
                assert!(
                    map.iter().all(|(key, row)| key == row),
                    "a row is not its key's"
                );
                let mut keys: Vec<i32> = map.keys().copied().collect();
                keys.sort_unstable();
                (item, keys)
            })
            .boxed()
    }

    async fn next_keyed(
        output: &mut BoxStream<'static, Result<(u32, Vec<i32>), RpcError>>,
    ) -> (u32, Vec<i32>) {
        timeout(Duration::from_millis(500), output.next())
            .await
            .expect("item emitted")
            .expect("some")
            .expect("ok")
    }

    /// Items still buffered when upstream goes `Pending` form a burst that is
    /// read at once, without waiting for more items.
    #[tokio::test]
    async fn keyed_batches_flush_burst_when_upstream_goes_pending() {
        let (mut tx, rx) = mpsc::channel::<Result<Watermarked<TestItem>, RpcError>>(8);
        let (fetcher, fetches) = stub_fetcher(echo_rows);
        let mut output = keyed_output(pipelined_keyed_batches(rx.boxed(), 100, 4, None, fetcher));

        tx.send(Ok(Watermarked::Item((0, vec![1, 2, 3]))))
            .await
            .expect("send");
        tx.send(Ok(Watermarked::Item((1, vec![4, 5, 6]))))
            .await
            .expect("send");

        assert_eq!(next_keyed(&mut output).await, (0, vec![1, 2, 3]));
        assert_eq!(next_keyed(&mut output).await, (1, vec![4, 5, 6]));
        assert_eq!(
            sorted_fetches(&fetches),
            vec![vec![1, 2, 3, 4, 5, 6]],
            "one read for the burst"
        );

        tx.close_channel();
        assert!(output.next().await.is_none());
    }

    /// Reads that complete out of order still emit items in input order.
    #[tokio::test]
    async fn keyed_batches_emit_in_input_order_under_out_of_order_reads() {
        // One key per item and per read; later reads finish first.
        let (fetcher, fetches) = stub_fetcher(|keys: Vec<i32>| {
            async move {
                let delay = Duration::from_millis(50u64.saturating_sub(keys[0] as u64 * 8));
                tokio::time::sleep(delay).await;
                echo_rows(keys).await
            }
            .boxed()
        });
        let items: Vec<TestItem> = (0u32..6).map(|i| (i, vec![i as i32])).collect();
        let out: Vec<(u32, Vec<i32>)> = keyed_output(pipelined_keyed_batches(
            keyed_upstream(items.clone()),
            1,
            6,
            None,
            fetcher,
        ))
        .try_collect()
        .await
        .expect("ok");

        assert_eq!(out, items);
        assert_eq!(
            sorted_fetches(&fetches),
            (0..6).map(|key| vec![key]).collect::<Vec<_>>()
        );
    }

    /// Items sharing a read each get a map of only their own keys, and a key
    /// two items list, or one item lists twice, is read once.
    #[tokio::test]
    async fn keyed_batches_item_map_holds_only_its_keys() {
        let (fetcher, fetches) = stub_fetcher(echo_rows);
        let items = vec![(0, vec![10, 11]), (1, vec![20, 21]), (2, vec![11, 30, 30])];
        let out: Vec<(u32, Vec<i32>)> = keyed_output(pipelined_keyed_batches(
            keyed_upstream(items),
            10,
            2,
            None,
            fetcher,
        ))
        .try_collect()
        .await
        .expect("ok");

        assert_eq!(
            out,
            vec![(0, vec![10, 11]), (1, vec![20, 21]), (2, vec![11, 30])]
        );
        assert_eq!(sorted_fetches(&fetches), vec![vec![10, 11, 20, 21, 30]]);
    }

    /// A key the read ends without is index/storage divergence: an `Internal`
    /// error naming the key, not a quietly partial item.
    #[tokio::test]
    async fn keyed_batches_error_on_a_missing_row() {
        let (fetcher, _) = stub_fetcher(|keys: Vec<i32>| {
            echo_rows(keys.into_iter().filter(|&key| key != 2).collect())
        });
        let err = keyed_output(pipelined_keyed_batches(
            keyed_upstream(vec![(0, vec![1, 2, 3])]),
            10,
            1,
            None,
            fetcher,
        ))
        .try_collect::<Vec<_>>()
        .await
        .expect_err("a missing row must error");

        let status: tonic::Status = err.into();
        assert_eq!(status.code(), tonic::Code::Internal);
        assert!(
            status.message().contains("missing key 2"),
            "expected a missing-key error, got: {}",
            status.message()
        );
    }

    /// An item whose own keys exceed `max_seeks_per_read` goes out as the
    /// fewest near-equal reads of at most `max_seeks_per_read` keys, run in
    /// parallel (each read waits at a barrier for the others), and is emitted
    /// once with all its keys.
    #[tokio::test]
    async fn keyed_batches_split_fat_item_into_parallel_reads() {
        let barrier = Arc::new(Barrier::new(3));
        let (fetcher, fetches) = stub_fetcher(move |keys: Vec<i32>| {
            let barrier = barrier.clone();
            async move {
                barrier.wait().await;
                echo_rows(keys).await
            }
            .boxed()
        });
        let out: Vec<(u32, Vec<i32>)> = timeout(
            Duration::from_secs(1),
            keyed_output(pipelined_keyed_batches(
                keyed_upstream(vec![(42, (0..25).collect())]),
                10,
                4,
                None,
                fetcher,
            ))
            .try_collect(),
        )
        .await
        .expect("the fat item's reads must run in parallel")
        .expect("ok");

        assert_eq!(out, vec![(42, (0..25).collect::<Vec<i32>>())]);
        let fetches = sorted_fetches(&fetches);
        let mut sizes: Vec<usize> = fetches.iter().map(Vec::len).collect();
        sizes.sort_unstable();
        assert_eq!(sizes, vec![8, 8, 9]);
        let mut keys = fetches.concat();
        keys.sort_unstable();
        assert_eq!(keys, (0..25).collect::<Vec<_>>(), "each key read once");
    }

    /// With `min_seeks_per_split_read`, a ready burst is split over the free slots.
    #[tokio::test]
    async fn keyed_batches_spread_burst_over_free_slots() {
        let (fetcher, fetches) = stub_fetcher(echo_rows);
        let items: Vec<TestItem> = (0u32..4).map(|i| (i, vec![i as i32])).collect();
        let out: Vec<(u32, Vec<i32>)> = keyed_output(pipelined_keyed_batches(
            keyed_upstream(items.clone()),
            10,
            4,
            Some(1),
            fetcher,
        ))
        .try_collect()
        .await
        .expect("ok");

        assert_eq!(out, items);
        assert_eq!(
            sorted_fetches(&fetches),
            vec![vec![0], vec![1], vec![2], vec![3]]
        );
    }

    #[tokio::test]
    async fn keyed_batches_propagate_fetch_error() {
        let (fetcher, _) = stub_fetcher(|_keys: Vec<i32>| {
            async { Err::<TestRows, _>(RpcError::new(tonic::Code::Internal, "boom")) }.boxed()
        });
        let err = keyed_output(pipelined_keyed_batches(
            keyed_upstream(vec![(0, vec![1, 2, 3]), (1, vec![4, 5, 6])]),
            10,
            1,
            None,
            fetcher,
        ))
        .try_collect::<Vec<_>>()
        .await
        .expect_err("expected fetch error to propagate");

        let status: tonic::Status = err.into();
        assert_eq!(status.code(), tonic::Code::Internal);
        assert!(
            status.message().contains("boom"),
            "unexpected message: {}",
            status.message()
        );
    }

    #[tokio::test]
    async fn keyed_batches_keep_watermarks_in_input_order() {
        let upstream: BoxStream<'static, Result<Watermarked<TestItem>, RpcError>> =
            stream::iter(vec![
                Ok(Watermarked::Item((0, vec![1]))),
                Ok(Watermarked::Watermark(5)),
                Ok(Watermarked::Item((1, vec![2]))),
            ])
            .boxed();
        let (fetcher, _) = stub_fetcher(echo_rows);
        let out: Vec<Watermarked<u32>> = pipelined_keyed_batches(upstream, 10, 2, None, fetcher)
            .map_ok(|frame| frame.map_item(|(item, _map)| item))
            .try_collect()
            .await
            .expect("ok");

        assert_eq!(
            out,
            vec![
                Watermarked::Item(0),
                Watermarked::Watermark(5),
                Watermarked::Item(1),
            ]
        );
    }

    /// Reads start and complete without the consumer polling the stream.
    #[tokio::test]
    async fn keyed_batch_reads_complete_without_consumer_polls() {
        let started = Arc::new(AtomicUsize::new(0));
        let completed = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(3));
        let completion_notify = Arc::new(Notify::new());
        let (fetcher, _) = stub_fetcher({
            let started = started.clone();
            let completed = completed.clone();
            let barrier = barrier.clone();
            let completion_notify = completion_notify.clone();
            move |keys: Vec<i32>| {
                let started = started.clone();
                let completed = completed.clone();
                let barrier = barrier.clone();
                let completion_notify = completion_notify.clone();
                async move {
                    started.fetch_add(1, Ordering::SeqCst);
                    barrier.wait().await;
                    if completed.fetch_add(1, Ordering::SeqCst) + 1 == 2 {
                        completion_notify.notify_one();
                    }
                    echo_rows(keys).await
                }
                .boxed()
            }
        });
        let _stream = pipelined_keyed_batches(
            keyed_upstream(vec![(0, vec![10]), (1, vec![20])]),
            1,
            2,
            None,
            fetcher,
        );

        timeout(Duration::from_millis(500), barrier.wait())
            .await
            .expect("reads did not start without consumer polls");
        timeout(Duration::from_millis(500), completion_notify.notified())
            .await
            .expect("reads did not complete without consumer polls");
        assert_eq!(started.load(Ordering::SeqCst), 2);
        assert_eq!(completed.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn dropping_keyed_batch_stream_aborts_reads() {
        let limiter = Arc::new(Semaphore::new(1));
        let (fetcher, _) = stub_fetcher({
            let limiter = limiter.clone();
            move |_keys: Vec<i32>| {
                let limiter = limiter.clone();
                async move {
                    let _permit = limiter
                        .acquire_owned()
                        .await
                        .map_err(|_| RpcError::new(tonic::Code::Internal, "test limiter closed"))?;
                    std::future::pending::<Result<TestRows, RpcError>>().await
                }
                .boxed()
            }
        });
        let stream =
            pipelined_keyed_batches(keyed_upstream(vec![(0, vec![10])]), 1, 1, None, fetcher);

        // Eager: the read holds the permit without the consumer polling.
        wait_for_available_permits(&limiter, 0).await;
        drop(stream);
        wait_for_available_permits(&limiter, 1).await;
    }

    /// A key read in an earlier burst is not read again, and reads are sized on
    /// the keys still to read.
    #[tokio::test]
    async fn keyed_batches_stream_reuse_reads_each_key_once() {
        let (mut tx, rx) = mpsc::channel::<Result<Watermarked<TestItem>, RpcError>>(8);
        let (fetcher, fetches) = stub_fetcher(echo_rows);
        let mut output = keyed_output(pipelined_keyed_batches(rx.boxed(), 2, 4, None, fetcher));

        // Burst 1: an item with 4 keys to read goes out as two reads of 2.
        tx.send(Ok(Watermarked::Item((0, vec![1, 2, 3, 4]))))
            .await
            .expect("send");
        assert_eq!(next_keyed(&mut output).await, (0, vec![1, 2, 3, 4]));
        assert_eq!(sorted_fetches(&fetches), vec![vec![1, 2], vec![3, 4]]);

        // Burst 2: item 1 lists 5 keys but only 5 is still to read, and with
        // item 2's key 6 it fills one read of `max_seeks_per_read`.
        tx.send(Ok(Watermarked::Item((1, vec![1, 2, 3, 4, 5]))))
            .await
            .expect("send");
        tx.send(Ok(Watermarked::Item((2, vec![6]))))
            .await
            .expect("send");
        assert_eq!(next_keyed(&mut output).await, (1, vec![1, 2, 3, 4, 5]));
        assert_eq!(next_keyed(&mut output).await, (2, vec![6]));
        assert_eq!(
            sorted_fetches(&fetches),
            vec![vec![1, 2], vec![3, 4], vec![5, 6]]
        );

        // Burst 3: every key was read before: no fetch.
        tx.send(Ok(Watermarked::Item((3, vec![2, 4, 6]))))
            .await
            .expect("send");
        assert_eq!(next_keyed(&mut output).await, (3, vec![2, 4, 6]));
        assert_eq!(fetches.lock().unwrap().len(), 3);

        // Burst 4: one key still to read among cached ones: one 1-key fetch.
        tx.send(Ok(Watermarked::Item((4, (1..=7).collect()))))
            .await
            .expect("send");
        assert_eq!(next_keyed(&mut output).await, (4, (1..=7).collect()));
        assert_eq!(
            sorted_fetches(&fetches),
            vec![vec![1, 2], vec![3, 4], vec![5, 6], vec![7]]
        );

        tx.close_channel();
        assert!(output.next().await.is_none());
    }

    /// Packing divides items: a burst is packed into the fewest near-equal
    /// reads of keys, with an item's keys divided between reads, and each item
    /// is emitted in order with exactly its own keys.
    #[tokio::test]
    async fn keyed_batches_pack_burst_dividing_items_between_reads() {
        let (fetcher, fetches) = stub_fetcher(echo_rows);
        let items: Vec<TestItem> = (0u32..3)
            .map(|i| (i, (i as i32 * 6..i as i32 * 6 + 6).collect()))
            .collect();
        let out: Vec<(u32, Vec<i32>)> = keyed_output(pipelined_keyed_batches(
            keyed_upstream(items.clone()),
            10,
            4,
            None,
            fetcher,
        ))
        .try_collect()
        .await
        .expect("ok");

        assert_eq!(out, items);
        assert_eq!(
            sorted_fetches(&fetches),
            vec![(0..9).collect::<Vec<i32>>(), (9..18).collect()],
            "two reads of 9 keys, item 1's keys divided between them"
        );
    }

    /// With `min_seeks_per_split_read`, one item's keys are split over the free
    /// slots, and the item is emitted once with all its keys.
    #[tokio::test]
    async fn keyed_batches_spread_one_item_over_free_slots() {
        let (fetcher, fetches) = stub_fetcher(echo_rows);
        let out: Vec<(u32, Vec<i32>)> = keyed_output(pipelined_keyed_batches(
            keyed_upstream(vec![(7, (0..8).collect())]),
            10,
            4,
            Some(2),
            fetcher,
        ))
        .try_collect()
        .await
        .expect("ok");

        assert_eq!(out, vec![(7, (0..8).collect::<Vec<i32>>())]);
        assert_eq!(
            sorted_fetches(&fetches),
            vec![vec![0, 1], vec![2, 3], vec![4, 5], vec![6, 7]]
        );
    }

    /// An item whose keys were all claimed by an earlier item of its burst
    /// issues no fetch of its own, and is emitted in order with its map.
    #[tokio::test]
    async fn keyed_batches_item_without_own_keys_rides_open_read() {
        let (fetcher, fetches) = stub_fetcher(echo_rows);
        let items: Vec<TestItem> = vec![(0, vec![1, 2]), (1, vec![2, 1]), (2, vec![3])];
        let out: Vec<(u32, Vec<i32>)> = keyed_output(pipelined_keyed_batches(
            keyed_upstream(items),
            1,
            4,
            None,
            fetcher,
        ))
        .try_collect()
        .await
        .expect("ok");

        assert_eq!(out, vec![(0, vec![1, 2]), (1, vec![1, 2]), (2, vec![3])]);
        assert_eq!(
            sorted_fetches(&fetches),
            vec![vec![1], vec![2], vec![3]],
            "one fetch per key, none for item 1"
        );
    }

    // ---- resolve_scan_watermarks ----

    type LookupReply = Result<Option<u64>, ScanStop>;

    /// One resolver call: its position, the sender the test uses to complete
    /// it, and whether its future was dropped before the reply arrived.
    struct Lookup {
        position: u64,
        reply: Option<oneshot::Sender<LookupReply>>,
        dropped_unfinished: Arc<AtomicBool>,
    }

    /// Sets its flag if dropped while armed: marks a lookup future that was
    /// dropped before its reply arrived.
    struct DropFlag {
        flag: Arc<AtomicBool>,
        armed: bool,
    }

    impl Drop for DropFlag {
        fn drop(&mut self) {
            if self.armed {
                self.flag.store(true, Ordering::SeqCst);
            }
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Frame {
        Item(u64),
        Beacon(u64, u64),
        ScanLimit(u64, Option<u64>),
        Cancelled,
        Fault(String),
    }

    /// Drives `resolve_scan_watermarks`: the test pushes upstream frames
    /// without waiting for them to be read, completes each resolver call by
    /// hand, and reads whatever the stream produced so far.
    struct ResolverHarness {
        upstream: Option<tokio::sync::mpsc::Sender<Result<Watermarked<u64>, ScanStop>>>,
        output: tokio::sync::mpsc::Receiver<Frame>,
        lookup_rx: tokio::sync::mpsc::Receiver<Lookup>,
        lookups: Vec<Lookup>,
        _driver: TaskGuard<()>,
    }

    impl ResolverHarness {
        fn new() -> Self {
            let (upstream_tx, upstream_rx) = tokio::sync::mpsc::channel(64);
            let upstream = stream::unfold(upstream_rx, |mut rx| async move {
                rx.recv().await.map(|frame| (frame, rx))
            })
            .boxed();
            let (lookup_tx, lookup_rx) = tokio::sync::mpsc::channel::<Lookup>(64);
            let resolver = move |position: u64| {
                let (reply, response) = oneshot::channel::<LookupReply>();
                let dropped_unfinished = Arc::new(AtomicBool::new(false));
                lookup_tx
                    .try_send(Lookup {
                        position,
                        reply: Some(reply),
                        dropped_unfinished: dropped_unfinished.clone(),
                    })
                    .expect("lookup channel has room");
                async move {
                    let mut unfinished = DropFlag {
                        flag: dropped_unfinished,
                        armed: true,
                    };
                    let result = response.await.expect("the test replies to every lookup");
                    unfinished.armed = false;
                    result
                }
            };
            let mut resolved = resolve_scan_watermarks(upstream, resolver, std::convert::identity);
            let (output_tx, output) = tokio::sync::mpsc::channel(64);
            let driver = tokio::spawn(async move {
                while let Some(next) = resolved.next().await {
                    let frame = match next {
                        Ok(ResolvedWatermarked::Item(item)) => Frame::Item(item),
                        Ok(ResolvedWatermarked::Watermark { position, cp }) => {
                            Frame::Beacon(position, cp)
                        }
                        Err(ResolvedScanStop::ScanLimit {
                            position,
                            checkpoint,
                        }) => Frame::ScanLimit(position, checkpoint),
                        Err(ResolvedScanStop::Cancelled) => Frame::Cancelled,
                        Err(ResolvedScanStop::Fault(error)) => Frame::Fault(error.to_string()),
                    };
                    if output_tx.send(frame).await.is_err() {
                        return;
                    }
                }
            });
            Self {
                upstream: Some(upstream_tx),
                output,
                lookup_rx,
                lookups: Vec::new(),
                _driver: TaskGuard::new(driver),
            }
        }

        fn push(&self, frame: Result<Watermarked<u64>, ScanStop>) {
            self.upstream
                .as_ref()
                .expect("upstream still open")
                .try_send(frame)
                .expect("upstream channel has room");
        }

        fn item(&self, item: u64) {
            self.push(Ok(Watermarked::Item(item)));
        }

        fn watermark(&self, position: u64) {
            self.push(Ok(Watermarked::Watermark(position)));
        }

        fn scan_limit(&self, scan_frontier: u64) {
            self.push(Err(ScanStop::ScanLimit { scan_frontier }));
        }

        fn fault(&self, message: &'static str) {
            self.push(Err(ScanStop::Fault(anyhow::anyhow!(message))));
        }

        fn cancel(&self) {
            self.push(Err(ScanStop::Cancelled));
        }

        /// Clean EOF.
        fn close(&mut self) {
            self.upstream = None;
        }

        /// Let the stream process everything that is ready.
        async fn settle(&self) {
            for _ in 0..64 {
                tokio::task::yield_now().await;
            }
        }

        fn lookup_positions(&mut self) -> Vec<u64> {
            while let Ok(lookup) = self.lookup_rx.try_recv() {
                self.lookups.push(lookup);
            }
            self.lookups.iter().map(|lookup| lookup.position).collect()
        }

        fn reply(&mut self, index: usize, reply: LookupReply) {
            self.lookup_positions();
            let sender = self.lookups[index]
                .reply
                .take()
                .expect("each lookup is replied to once");
            assert!(
                sender.send(reply).is_ok(),
                "lookup still waiting for its reply"
            );
        }

        fn any_lookup_dropped_unfinished(&mut self) -> bool {
            self.lookup_positions();
            self.lookups
                .iter()
                .any(|lookup| lookup.dropped_unfinished.load(Ordering::SeqCst))
        }

        /// Frames produced since the last call.
        fn frames(&mut self) -> Vec<Frame> {
            let mut frames = Vec::new();
            while let Ok(frame) = self.output.try_recv() {
                frames.push(frame);
            }
            frames
        }

        /// Whether the stream ended; call after `frames` has drained output.
        fn ended(&mut self) -> bool {
            matches!(
                self.output.try_recv(),
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)
            )
        }
    }

    /// Every watermark is resolved in arrival order, one lookup each, and each
    /// found checkpoint becomes a beacon in source order with the items.
    /// Upstream is not read while a lookup is in flight.
    #[tokio::test]
    async fn every_watermark_resolves_in_order_with_items() {
        let mut h = ResolverHarness::new();
        h.item(1);
        h.watermark(5);
        h.watermark(7);
        h.item(9);
        h.watermark(11);
        h.settle().await;
        assert_eq!(h.frames(), vec![Frame::Item(1)]);
        assert_eq!(h.lookup_positions(), vec![5]);

        h.reply(0, Ok(Some(50)));
        h.settle().await;
        assert_eq!(h.frames(), vec![Frame::Beacon(5, 50)]);
        assert_eq!(h.lookup_positions(), vec![5, 7]);

        h.reply(1, Ok(Some(70)));
        h.settle().await;
        assert_eq!(h.frames(), vec![Frame::Beacon(7, 70), Frame::Item(9)]);
        assert_eq!(h.lookup_positions(), vec![5, 7, 11]);

        h.reply(2, Ok(Some(110)));
        h.close();
        h.settle().await;
        assert_eq!(h.frames(), vec![Frame::Beacon(11, 110)]);
        assert!(h.ended());
        assert_eq!(h.lookup_positions(), vec![5, 7, 11]);
        assert!(!h.any_lookup_dropped_unfinished());
    }

    /// A started lookup is never dropped while the stream runs: an item that
    /// arrives during the lookup waits for it, then both are emitted in
    /// source order.
    #[tokio::test]
    async fn item_waits_for_in_flight_lookup() {
        let mut h = ResolverHarness::new();
        h.watermark(5);
        h.settle().await;
        assert_eq!(h.lookup_positions(), vec![5]);

        h.item(9);
        h.settle().await;
        assert!(h.frames().is_empty(), "the item waits for the lookup");

        h.reply(0, Ok(Some(50)));
        h.settle().await;
        assert_eq!(h.frames(), vec![Frame::Beacon(5, 50), Frame::Item(9)]);
        assert!(!h.any_lookup_dropped_unfinished());
    }

    /// A `None` resolution emits no frame, and the stream carries on with the
    /// next watermark and item.
    #[tokio::test]
    async fn none_resolution_emits_no_frame() {
        let mut h = ResolverHarness::new();
        h.watermark(0);
        h.watermark(5);
        h.item(9);
        h.settle().await;
        assert_eq!(h.lookup_positions(), vec![0]);

        h.reply(0, Ok(None));
        h.settle().await;
        assert!(h.frames().is_empty());
        assert_eq!(h.lookup_positions(), vec![0, 5]);

        h.reply(1, Ok(Some(50)));
        h.settle().await;
        assert_eq!(h.frames(), vec![Frame::Beacon(5, 50), Frame::Item(9)]);
    }

    /// A clean EOF ends the stream without a lookup of its own: only the
    /// watermarks that arrived are resolved.
    #[tokio::test]
    async fn clean_eof_issues_no_lookup() {
        let mut h = ResolverHarness::new();
        h.item(1);
        h.watermark(5);
        h.close();
        h.settle().await;
        assert_eq!(h.lookup_positions(), vec![5]);

        h.reply(0, Ok(Some(50)));
        h.settle().await;
        assert_eq!(h.frames(), vec![Frame::Item(1), Frame::Beacon(5, 50)]);
        assert!(h.ended());
        assert_eq!(h.lookup_positions(), vec![5]);
    }

    /// A scan limit at the position of the most recent completed lookup
    /// reuses its result.
    #[tokio::test]
    async fn scan_limit_reuses_completed_lookup() {
        let mut h = ResolverHarness::new();
        h.watermark(7);
        h.settle().await;
        h.reply(0, Ok(Some(70)));
        h.settle().await;
        assert_eq!(h.frames(), vec![Frame::Beacon(7, 70)]);

        h.scan_limit(7);
        h.settle().await;
        assert_eq!(h.frames(), vec![Frame::ScanLimit(7, Some(70))]);
        assert!(h.ended());
        assert_eq!(h.lookup_positions(), vec![7]);
    }

    /// A completed `None` lookup is reused too: the scan limit at the same
    /// position keeps `None` without a second lookup.
    #[tokio::test]
    async fn scan_limit_reuses_completed_none_lookup() {
        let mut h = ResolverHarness::new();
        h.watermark(7);
        h.scan_limit(7);
        h.settle().await;
        h.reply(0, Ok(None));
        h.settle().await;
        assert_eq!(h.frames(), vec![Frame::ScanLimit(7, None)]);
        assert!(h.ended());
        assert_eq!(h.lookup_positions(), vec![7]);
    }

    /// A scan limit ahead of the last resolved watermark resolves its
    /// frontier exactly once, after the watermarks before it.
    #[tokio::test]
    async fn scan_limit_resolves_only_the_frontier() {
        let mut h = ResolverHarness::new();
        h.item(1);
        h.watermark(5);
        h.scan_limit(8);
        h.settle().await;
        assert_eq!(h.lookup_positions(), vec![5]);

        h.reply(0, Ok(Some(50)));
        h.settle().await;
        assert_eq!(h.lookup_positions(), vec![5, 8]);

        h.reply(1, Ok(Some(80)));
        h.settle().await;
        assert_eq!(
            h.frames(),
            vec![
                Frame::Item(1),
                Frame::Beacon(5, 50),
                Frame::ScanLimit(8, Some(80))
            ]
        );
        assert!(h.ended());
        assert_eq!(h.lookup_positions(), vec![5, 8]);
    }

    /// A missing checkpoint can be a successful resolution at an absolute
    /// numeric edge. Preserve `Ok(None)` so the endpoint can either construct
    /// an edge cursor or reject a non-edge position.
    #[tokio::test]
    async fn scan_limit_preserves_successful_none_resolution() {
        let mut h = ResolverHarness::new();
        h.scan_limit(0);
        h.settle().await;
        h.reply(0, Ok(None));
        h.settle().await;
        assert_eq!(h.frames(), vec![Frame::ScanLimit(0, None)]);
    }

    /// A failed lookup ends the stream with that failure; frames queued
    /// behind it are not emitted.
    #[tokio::test]
    async fn lookup_failure_ends_the_stream() {
        let mut h = ResolverHarness::new();
        h.watermark(5);
        h.item(9);
        h.settle().await;
        h.reply(0, Err(ScanStop::Fault(anyhow::anyhow!("lookup failed"))));
        h.settle().await;
        assert_eq!(h.frames(), vec![Frame::Fault("lookup failed".to_owned())]);
        assert!(h.ended());
    }

    /// A fault after a watermark: the watermark's beacon, the client's best
    /// resume point, is emitted before the fault.
    #[tokio::test]
    async fn fault_after_watermark_emits_beacon_first() {
        let mut h = ResolverHarness::new();
        h.item(1);
        h.watermark(5);
        h.fault("boom");
        h.settle().await;
        assert_eq!(h.lookup_positions(), vec![5]);

        h.reply(0, Ok(Some(50)));
        h.settle().await;
        assert_eq!(
            h.frames(),
            vec![
                Frame::Item(1),
                Frame::Beacon(5, 50),
                Frame::Fault("boom".to_owned())
            ]
        );
        assert!(h.ended());
    }

    /// A cancellation after a watermark: the watermark's beacon is emitted
    /// before the cancellation.
    #[tokio::test]
    async fn cancel_after_watermark_emits_beacon_first() {
        let mut h = ResolverHarness::new();
        h.watermark(5);
        h.cancel();
        h.settle().await;
        assert_eq!(h.lookup_positions(), vec![5]);

        h.reply(0, Ok(Some(50)));
        h.settle().await;
        assert_eq!(h.frames(), vec![Frame::Beacon(5, 50), Frame::Cancelled]);
        assert!(h.ended());
    }
}
