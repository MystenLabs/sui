// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A shared service that fetches historical checkpoints **forward** for pipeline subscribers that
//! are catching up to the network. It serves every pipeline type — both concurrent and sequential
//! pipelines subscribe over their own historical range before handing off to live ingestion.
//!
//! Each subscriber gets a [`BackfillHandle`] over a half-open range `[start, end)`. A handle walks
//! the range upward in fixed, grid-aligned chunks ([`BackfillHandle::next_chunk`]); when its range
//! is exhausted (bounded subscriptions) or it has caught up to within
//! [`BackfillConfig::handoff_threshold`] of the network tip (live subscriptions), `next_chunk`
//! returns `None`, signalling the caller to hand off to live ingestion.
//!
//! Overlapping subscriptions share fetch work through a chunk-aligned cache. Because chunks are
//! always grid-aligned and (for live subscriptions) full-size, the fetched range for a given
//! `chunk_lo` is independent of when the entry was created — which is what makes the cache safe to
//! reuse even though the network tip advances over the lifetime of the service.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Bound;
use std::ops::Range;
use std::ops::RangeBounds;
use std::sync::Arc;
use std::time::Duration;

use mysten_metrics::monitored_mpsc;
use sui_futures::service::Service;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio::time::Instant;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::ingestion::ingestion_client::CheckpointEnvelope;
use crate::ingestion::ingestion_client::IngestionClient;

/// Bound on the number of in-flight handle requests, applied as a semaphore so a misbehaving caller
/// cannot grow the unbounded request channel without limit.
const BACKFILL_REQUEST_CHANNEL_SIZE: usize = 1024;

/// Configuration for [`BackfillService`].
#[derive(Clone)]
pub(crate) struct BackfillConfig {
    /// Bucket size, and the maximum number of checkpoints any single [`BackfillHandle::next_chunk`]
    /// call returns.
    pub chunk_size: u64,

    /// Maximum distance below a chunk's lower bound for a handle to be considered an expected
    /// consumer of that chunk. This bounds cache retention to consumers likely to share the same
    /// completed fetch.
    pub expected_consumer_max_distance: u64,

    /// How long to keep a completed cache entry around for expected consumers.
    pub expected_consumer_wait_duration: Duration,

    /// Polling interval to retry fetching checkpoints that do not exist yet.
    pub retry_interval: Duration,

    /// A live (unbounded) subscription stops backfilling and hands off to live ingestion once the
    /// gap between its low watermark and the network tip is at most this many checkpoints. Must be
    /// at least `chunk_size`, which guarantees a live chunk never reaches above the tip (so a fetch
    /// never blocks on a not-yet-existent checkpoint, and chunks stay full-size and cacheable).
    pub handoff_threshold: u64,

    /// How often the service re-polls the network tip so live subscriptions keep backfilling toward
    /// the advancing tip.
    pub tip_refresh_interval: Duration,
}

/// A source of checkpoint data for forward backfills.
pub(crate) struct BackfillService {
    backfill_config: BackfillConfig,
    ingestion_client: IngestionClient,
    /// The network tip captured at startup; the service re-polls and advances it from here.
    initial_tip: u64,
    pipeline_ranges: HashMap<BackfillHandleId, Subscription>,
    request_semaphore: Arc<Semaphore>,
    request_tx: monitored_mpsc::UnboundedSender<BackfillRequest>,
    request_rx: monitored_mpsc::UnboundedReceiver<BackfillRequest>,
}

/// Per-pipeline subscription handle returned by [`BackfillService::subscribe`].
pub(crate) struct BackfillHandle {
    backfill_handle_id: BackfillHandleId,
    request_semaphore: Arc<Semaphore>,
    request_tx: monitored_mpsc::UnboundedSender<BackfillRequest>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct BackfillHandleId(usize);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CacheEntryId(usize);

/// A handle's remaining backfill range. `end` is `None` for live subscriptions that track the
/// advancing network tip, and `Some(exclusive_end)` for bounded subscriptions.
#[derive(Clone, Copy)]
struct Subscription {
    start: u64,
    end: Option<u64>,
}

enum BackfillRequest {
    NextChunk {
        backfill_handle_id: BackfillHandleId,
        reply: oneshot::Sender<Option<Vec<CheckpointEnvelope>>>,
        _permit: OwnedSemaphorePermit,
    },
    DropHandle {
        backfill_handle_id: BackfillHandleId,
    },
}

struct CacheEntry {
    /// Generation ID for this `chunk_lo`, used to ignore stale fetch completions after eviction.
    cache_entry_id: CacheEntryId,
    /// Expected consumers keep a completed chunk warm without blocking forever on distant handles.
    remaining_backfill_handle_ids: HashSet<BackfillHandleId>,
    /// Requests wait here so concurrent consumers share one fetch fan-out.
    pending_handles: HashMap<BackfillHandleId, PendingHandle>,
    /// Ordered storage lets a shared cache entry serve per-handle sub-ranges deterministically.
    checkpoint_envelopes: BTreeMap<u64, CheckpointEnvelope>,
    /// A count avoids assuming fetch completions arrive contiguously or in order.
    expected_checkpoint_envelope_count: usize,
}

struct PendingHandle {
    reply: oneshot::Sender<Option<Vec<CheckpointEnvelope>>>,
    /// Each handle may need only the overlap between its subscription and the cached chunk.
    response_range: Range<u64>,
}

struct BackfillState {
    backfill_config: BackfillConfig,
    ingestion_client: IngestionClient,
    pipeline_ranges: HashMap<BackfillHandleId, Subscription>,
    /// The latest known network tip. Advances as the tip-refresh poll completes; only ever
    /// increases. Used to decide when a live subscription has caught up.
    tip: u64,
    /// Fixed upper bound for bounded subscriptions (the maximum `end`), or `None` when any live
    /// subscription is present (in which case chunks are full-size). Captured at startup so a chunk
    /// for a given `chunk_lo` always covers the same range.
    bounded_cap: Option<u64>,
    /// Cache key is the chunk's inclusive lower bound.
    cache: HashMap<u64, CacheEntry>,
    /// Handles can be pending on or expected by many cache entries; this avoids scanning all cache
    /// entries when a handle closes.
    cache_chunks_by_backfill_handle_id: HashMap<BackfillHandleId, HashSet<u64>>,
    /// Cache keys can repeat after eviction, so stale fetches need a generation discriminator.
    next_cache_entry_id: usize,
    /// Tasks run independently so one slow checkpoint or eviction timer does not block requests.
    tasks: JoinSet<BackfillTask>,
}

enum BackfillTask {
    Fetch {
        cache_entry_id: CacheEntryId,
        chunk_lo: u64,
        checkpoint_envelope: CheckpointEnvelope,
    },
    Evict {
        chunk_lo: u64,
        cache_entry_id: CacheEntryId,
    },
}

impl BackfillService {
    pub(crate) fn new(
        backfill_config: BackfillConfig,
        ingestion_client: IngestionClient,
        initial_tip: u64,
    ) -> anyhow::Result<Self> {
        // Range alignment relies on a non-zero chunk, and zero-sized chunks would make no progress.
        anyhow::ensure!(
            backfill_config.chunk_size > 0,
            "backfill chunk_size must be greater than zero"
        );

        // A live chunk is `[chunk_lo, chunk_lo + chunk_size)`; the handoff threshold keeps a live
        // subscription from requesting a chunk that reaches above the tip only if it is at least a
        // chunk wide.
        anyhow::ensure!(
            backfill_config.handoff_threshold >= backfill_config.chunk_size,
            "backfill handoff_threshold must be at least chunk_size"
        );

        let (request_tx, request_rx) = monitored_mpsc::unbounded_channel("backfill_requests");
        Ok(Self {
            backfill_config,
            ingestion_client,
            initial_tip,
            pipeline_ranges: HashMap::new(),
            request_semaphore: Arc::new(Semaphore::new(BACKFILL_REQUEST_CHANNEL_SIZE)),
            request_tx,
            request_rx,
        })
    }

    /// Subscribe to a backfill range. A bounded `end` backfills exactly that range; an unbounded
    /// end (e.g. `start..`) tracks the advancing network tip and hands off near it. Subscriptions
    /// must be created before [`Self::run`].
    pub(crate) fn subscribe(&mut self, range: impl RangeBounds<u64>) -> BackfillHandle {
        let start = match range.start_bound() {
            Bound::Included(&start) => start,
            Bound::Excluded(&start) => start.saturating_add(1),
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&end) => Some(end.saturating_add(1)),
            Bound::Excluded(&end) => Some(end),
            Bound::Unbounded => None,
        };

        // Subscriptions are fixed before `run`, so a dense local ID is enough to correlate requests.
        let id = BackfillHandleId(self.pipeline_ranges.len());
        self.pipeline_ranges.insert(id, Subscription { start, end });
        BackfillHandle {
            backfill_handle_id: id,
            request_semaphore: self.request_semaphore.clone(),
            request_tx: self.request_tx.clone(),
        }
    }

    pub(crate) fn run(self) -> Service {
        Service::new().spawn_aborting(async move {
            let BackfillService {
                backfill_config,
                ingestion_client,
                initial_tip,
                pipeline_ranges,
                request_semaphore: _,
                request_tx: _,
                mut request_rx,
            } = self;

            let Some(mut state) =
                BackfillState::new(backfill_config, ingestion_client, initial_tip, pipeline_ranges)
            else {
                info!("No backfill subscriptions, stopping backfill service");
                return Ok(());
            };

            // Re-poll the network tip in a continuously-running task so live subscriptions keep
            // chasing it. The sequential sleep→poll loop cannot overlap itself, and the first poll
            // lands one interval in, so the seeded `initial_tip` is used until then.
            let (tip_tx, mut tip_rx) = mpsc::channel(1);
            let tip_client = state.ingestion_client.clone();
            let tip_refresh_interval = state.backfill_config.tip_refresh_interval;
            state.tasks.spawn(async move {
                loop {
                    tokio::time::sleep(tip_refresh_interval).await;
                    if let Ok(tip) = tip_client
                        .latest_checkpoint_number()
                        .await
                        .inspect_err(|e| warn!("failed to refresh network tip during backfill: {e}"))
                    {
                        // The receiver is only dropped on shutdown, which aborts this task.
                        let _ = tip_tx.send(tip).await;
                    }
                }
            });

            loop {
                tokio::select! {
                    request = request_rx.recv() => {
                        let Some(request) = request else {
                            // All handles have been dropped; pending tasks no longer have consumers.
                            break;
                        };
                        match request {
                            BackfillRequest::NextChunk { backfill_handle_id, reply, .. } => state.handle_next_chunk(backfill_handle_id, reply),
                            BackfillRequest::DropHandle { backfill_handle_id } => state.close_handle(backfill_handle_id),
                        }
                    }

                    Some(task) = state.tasks.join_next() => {
                        // Task completions are driven even when no handle sends another request.
                        match task? {
                            BackfillTask::Fetch { cache_entry_id, chunk_lo, checkpoint_envelope } => {
                                state.handle_fetch(cache_entry_id, chunk_lo, checkpoint_envelope);
                            }
                            BackfillTask::Evict { chunk_lo, cache_entry_id } => {
                                state.handle_evict(chunk_lo, cache_entry_id);
                            }
                        }
                    }

                    Some(tip) = tip_rx.recv() => {
                        state.handle_refresh_tip(tip);
                    }

                    else => {
                        break;
                    }
                }
            }

            state.close_pending_handles();

            Ok(())
        })
    }
}

impl BackfillState {
    fn new(
        backfill_config: BackfillConfig,
        ingestion_client: IngestionClient,
        initial_tip: u64,
        mut pipeline_ranges: HashMap<BackfillHandleId, Subscription>,
    ) -> Option<Self> {
        // Subscriptions that are already caught up at startup have no backfill work; dropping them
        // lets the service stop when nothing remains.
        pipeline_ranges.retain(|_, subscription| {
            !subscription.is_caught_up(initial_tip, backfill_config.handoff_threshold)
        });

        if pipeline_ranges.is_empty() {
            return None;
        }

        // If every remaining subscription is bounded, chunks are clamped to the largest `end` so the
        // service never fetches checkpoints no subscriber asked for. If any subscription is live,
        // chunks are full-size (the handoff threshold keeps them at or below the tip).
        let bounded_cap = pipeline_ranges
            .values()
            .map(|subscription| subscription.end)
            .try_fold(0, |max, end| end.map(|end| max.max(end)));

        let state = Self {
            backfill_config,
            ingestion_client,
            pipeline_ranges,
            tip: initial_tip,
            bounded_cap,
            cache: HashMap::new(),
            cache_chunks_by_backfill_handle_id: HashMap::new(),
            next_cache_entry_id: 0,
            tasks: JoinSet::new(),
        };
        Some(state)
    }

    fn handle_next_chunk(
        &mut self,
        backfill_handle_id: BackfillHandleId,
        reply: oneshot::Sender<Option<Vec<CheckpointEnvelope>>>,
    ) {
        let handoff_threshold = self.backfill_config.handoff_threshold;
        let tip = self.tip;

        let Some(subscription) = self.pipeline_ranges.get_mut(&backfill_handle_id) else {
            // This handle already completed or was filtered out at startup.
            let _ = reply.send(None);
            return;
        };

        if subscription.is_caught_up(tip, handoff_threshold) {
            // Bounded subscription is exhausted, or a live one has reached the tip: hand off.
            self.pipeline_ranges.remove(&backfill_handle_id);
            let _ = reply.send(None);
            return;
        }

        let BackfillConfig {
            chunk_size,
            expected_consumer_max_distance,
            retry_interval,
            ..
        } = self.backfill_config;

        // Align the low end down to the chunk grid so overlapping subscriptions converge on the same
        // shared chunk.
        let chunk_lo = subscription.start - subscription.start % chunk_size;
        // Extend up one full chunk, clamped to the bounded cap when there is one. A live chunk is
        // never clamped to the tip (the handoff threshold keeps it at or below the tip), so a given
        // `chunk_lo` always maps to the same `[chunk_lo, chunk_hi)` regardless of the moving tip.
        let chunk_hi = match self.bounded_cap {
            Some(cap) => chunk_lo.saturating_add(chunk_size).min(cap),
            None => chunk_lo.saturating_add(chunk_size),
        };

        // A cache entry holds a full chunk while each caller receives only its requested overlap.
        let response_lo = subscription.start;
        let response_hi = chunk_hi.min(subscription.end.unwrap_or(u64::MAX));

        subscription.start = response_hi;
        // A bounded subscription that ended mid-chunk (`response_hi == end < chunk_hi`) must be
        // dropped so it is not counted as an expected consumer of the chunk it just consumed. A live
        // subscription is excluded automatically because its `start` advanced to `chunk_hi`, and it
        // must NOT be removed on the current tip since the tip advances and it may backfill more.
        if matches!(subscription.end, Some(end) if subscription.start >= end) {
            self.pipeline_ranges.remove(&backfill_handle_id);
        }

        let pending_handle = PendingHandle {
            reply,
            response_range: response_lo..response_hi,
        };

        if !self.cache.contains_key(&chunk_lo) {
            // Fetch completions can arrive after this entry is evicted and recreated.
            let cache_entry_id = CacheEntryId(self.next_cache_entry_id);
            self.next_cache_entry_id += 1;

            for checkpoint in chunk_lo..chunk_hi {
                let ingestion_client = self.ingestion_client.clone();
                self.tasks.spawn(async move {
                    let checkpoint_envelope = ingestion_client
                        .wait_for(checkpoint, retry_interval)
                        .await
                        .unwrap_or_else(|e| {
                            panic!("checkpoint {checkpoint} fetch failed during backfill: {e}")
                        });

                    BackfillTask::Fetch {
                        cache_entry_id,
                        chunk_lo,
                        checkpoint_envelope,
                    }
                });
            }

            // Keep the completed chunk warm only for consumers close enough below it to plausibly
            // request this same chunk next.
            let remaining_backfill_handle_ids = self
                .pipeline_ranges
                .iter()
                .filter_map(|(&handle_id, subscription)| {
                    let start = subscription.start;
                    let end = subscription.end.unwrap_or(u64::MAX);
                    // Range is after chunk.
                    if start >= chunk_hi
                        // Range is before chunk.
                        || end <= chunk_lo
                        // Range overlaps, but its start is too far below the chunk.
                        || chunk_lo.saturating_sub(start) > expected_consumer_max_distance
                    {
                        return None;
                    }

                    Some(handle_id)
                })
                .collect::<HashSet<_>>();

            self.insert_cache_entry(
                chunk_lo,
                CacheEntry {
                    cache_entry_id,
                    remaining_backfill_handle_ids,
                    pending_handles: HashMap::new(),
                    checkpoint_envelopes: BTreeMap::new(),
                    expected_checkpoint_envelope_count: (chunk_hi - chunk_lo) as usize,
                },
            );
        }

        if self.insert_pending_handle(chunk_lo, backfill_handle_id, pending_handle) {
            self.dispatch_pending(chunk_lo);
        }
    }

    fn handle_fetch(
        &mut self,
        cache_entry_id: CacheEntryId,
        chunk_lo: u64,
        checkpoint_envelope: CheckpointEnvelope,
    ) {
        let Some(cache_entry) = self.cache.get_mut(&chunk_lo) else {
            // The cache entry was evicted before this fetch completed.
            return;
        };
        if cache_entry.cache_entry_id != cache_entry_id {
            // The chunk key was reused after this fetch was spawned.
            return;
        }

        let sequence_number = *checkpoint_envelope.checkpoint.summary.sequence_number();
        // Duplicate completion would mean two tasks raced to fill the same checkpoint slot.
        assert!(
            cache_entry
                .checkpoint_envelopes
                .insert(sequence_number, checkpoint_envelope)
                .is_none(),
            "checkpoint {sequence_number} fetch slot must not be completed twice"
        );

        if cache_entry.has_all_checkpoint_envelopes() {
            if cache_entry.remaining_backfill_handle_ids.is_empty() {
                self.dispatch_pending(chunk_lo);
            } else {
                // Expected consumers can reduce duplicate fetches, but they must not block current
                // waiters beyond the configured grace period.
                let evict_at =
                    Instant::now() + self.backfill_config.expected_consumer_wait_duration;
                let cache_entry_id = cache_entry.cache_entry_id;
                self.tasks.spawn(async move {
                    tokio::time::sleep_until(evict_at).await;
                    BackfillTask::Evict {
                        chunk_lo,
                        cache_entry_id,
                    }
                });
            }
        }
    }

    fn handle_evict(&mut self, chunk_lo: u64, cache_entry_id: CacheEntryId) {
        if self
            .cache
            .get(&chunk_lo)
            .is_some_and(|cache_entry| cache_entry.cache_entry_id == cache_entry_id)
        {
            // Expired retention must still answer current waiters; otherwise they can hang after
            // the cache entry leaves the map.
            self.dispatch_pending(chunk_lo);
        }
    }

    fn handle_refresh_tip(&mut self, tip: u64) {
        // The network tip only increases; a higher tip lets live subscriptions keep backfilling
        // past where they would otherwise have handed off. Handles pull on demand, so no parked
        // waiters need waking here.
        self.tip = self.tip.max(tip);
    }

    fn close_handle(&mut self, backfill_handle_id: BackfillHandleId) {
        self.pipeline_ranges.remove(&backfill_handle_id);

        let cache_chunks = self
            .cache_chunks_by_backfill_handle_id
            .get(&backfill_handle_id)
            .cloned()
            .unwrap_or_default();

        let mut ready_chunks = Vec::new();
        for chunk_lo in cache_chunks {
            let Some((pending_handle, is_ready)) =
                self.remove_handle_from_cache_entry(chunk_lo, backfill_handle_id)
            else {
                continue;
            };

            if let Some(pending_handle) = pending_handle {
                let PendingHandle { reply, .. } = pending_handle;
                let _ = reply.send(None);
            }

            if is_ready {
                ready_chunks.push(chunk_lo);
            }
        }

        for chunk_lo in ready_chunks {
            self.dispatch_pending(chunk_lo);
        }
    }

    fn dispatch_pending(&mut self, chunk_lo: u64) {
        let Some(mut cache_entry) = self.remove_cache_entry(chunk_lo) else {
            return;
        };

        for (_, pending_handle) in cache_entry.pending_handles.drain() {
            let PendingHandle {
                reply,
                response_range,
            } = pending_handle;
            let checkpoint_envelopes = cache_entry
                .checkpoint_envelopes
                .range(response_range)
                .map(|(_, v)| v.clone())
                .collect();
            // A dropped receiver means the caller no longer needs the backfill result.
            let _ = reply.send(Some(checkpoint_envelopes));
        }
    }

    fn close_pending_handles(mut self) {
        while let Some(chunk_lo) = self.cache.keys().next().copied() {
            let Some(cache_entry) = self.remove_cache_entry(chunk_lo) else {
                continue;
            };
            for pending_handle in cache_entry.pending_handles.into_values() {
                let PendingHandle { reply, .. } = pending_handle;
                // Shutdown should resolve callers instead of leaving their oneshot receivers open.
                let _ = reply.send(None);
            }
        }
    }

    fn insert_cache_entry(&mut self, chunk_lo: u64, cache_entry: CacheEntry) {
        assert!(
            !self.cache.contains_key(&chunk_lo),
            "cache entry for chunk {chunk_lo} must not be replaced"
        );

        self.index_cache_entry(chunk_lo, &cache_entry);
        self.cache.insert(chunk_lo, cache_entry);
    }

    fn remove_cache_entry(&mut self, chunk_lo: u64) -> Option<CacheEntry> {
        let cache_entry = self.cache.remove(&chunk_lo)?;
        for &handle_id in &cache_entry.remaining_backfill_handle_ids {
            self.unindex_cache_chunk(handle_id, chunk_lo);
        }

        for &handle_id in cache_entry.pending_handles.keys() {
            self.unindex_cache_chunk(handle_id, chunk_lo);
        }

        Some(cache_entry)
    }

    fn insert_pending_handle(
        &mut self,
        chunk_lo: u64,
        backfill_handle_id: BackfillHandleId,
        pending_handle: PendingHandle,
    ) -> bool {
        self.index_cache_chunk(backfill_handle_id, chunk_lo);
        let cache_entry = self
            .cache
            .get_mut(&chunk_lo)
            .expect("cache entry must exist before adding a pending handle");
        cache_entry
            .remaining_backfill_handle_ids
            .remove(&backfill_handle_id);
        cache_entry
            .pending_handles
            .insert(backfill_handle_id, pending_handle);

        cache_entry.has_all_checkpoint_envelopes()
            && cache_entry.remaining_backfill_handle_ids.is_empty()
    }

    fn remove_handle_from_cache_entry(
        &mut self,
        chunk_lo: u64,
        backfill_handle_id: BackfillHandleId,
    ) -> Option<(Option<PendingHandle>, bool)> {
        self.unindex_cache_chunk(backfill_handle_id, chunk_lo);
        let cache_entry = self.cache.get_mut(&chunk_lo)?;
        cache_entry
            .remaining_backfill_handle_ids
            .remove(&backfill_handle_id);
        let pending_handle = cache_entry.pending_handles.remove(&backfill_handle_id);
        let is_ready = cache_entry.has_all_checkpoint_envelopes()
            && cache_entry.remaining_backfill_handle_ids.is_empty();

        Some((pending_handle, is_ready))
    }

    fn index_cache_entry(&mut self, chunk_lo: u64, cache_entry: &CacheEntry) {
        for &handle_id in &cache_entry.remaining_backfill_handle_ids {
            self.index_cache_chunk(handle_id, chunk_lo);
        }

        for &handle_id in cache_entry.pending_handles.keys() {
            self.index_cache_chunk(handle_id, chunk_lo);
        }
    }

    fn index_cache_chunk(&mut self, backfill_handle_id: BackfillHandleId, chunk_lo: u64) {
        self.cache_chunks_by_backfill_handle_id
            .entry(backfill_handle_id)
            .or_default()
            .insert(chunk_lo);
    }

    fn unindex_cache_chunk(&mut self, backfill_handle_id: BackfillHandleId, chunk_lo: u64) {
        let remove_entry = if let Some(chunks) = self
            .cache_chunks_by_backfill_handle_id
            .get_mut(&backfill_handle_id)
        {
            chunks.remove(&chunk_lo);
            chunks.is_empty()
        } else {
            false
        };

        if remove_entry {
            self.cache_chunks_by_backfill_handle_id
                .remove(&backfill_handle_id);
        }
    }
}

impl BackfillHandle {
    pub(crate) async fn next_chunk(&mut self) -> anyhow::Result<Option<Vec<CheckpointEnvelope>>> {
        let permit = self
            .request_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "backfill request semaphore closed for handle {:?}",
                    self.backfill_handle_id
                )
            })?;
        let (reply, receive) = oneshot::channel();
        if self
            .request_tx
            .send(BackfillRequest::NextChunk {
                backfill_handle_id: self.backfill_handle_id,
                reply,
                _permit: permit,
            })
            .is_err()
        {
            // The service has stopped, so no chunk can be returned.
            return Ok(None);
        }

        let Ok(Some(chunk)) = receive.await else {
            // The service closed the pending reply or returned no chunk.
            return Ok(None);
        };

        Ok(Some(chunk))
    }
}

impl Default for BackfillConfig {
    fn default() -> Self {
        Self {
            chunk_size: 100,
            expected_consumer_max_distance: 100,
            expected_consumer_wait_duration: Duration::from_secs(5),
            retry_interval: Duration::from_millis(200),
            handoff_threshold: 100,
            tip_refresh_interval: Duration::from_secs(1),
        }
    }
}

impl Drop for BackfillHandle {
    fn drop(&mut self) {
        let _ = self.request_tx.send(BackfillRequest::DropHandle {
            backfill_handle_id: self.backfill_handle_id,
        });
    }
}

impl CacheEntry {
    fn has_all_checkpoint_envelopes(&self) -> bool {
        self.checkpoint_envelopes.len() == self.expected_checkpoint_envelope_count
    }
}

impl Subscription {
    /// A bounded subscription is caught up once its low watermark reaches its end; a live one once
    /// the gap to the network tip is within the handoff threshold.
    fn is_caught_up(&self, tip: u64, handoff_threshold: u64) -> bool {
        match self.end {
            Some(end) => self.start >= end,
            None => tip.saturating_sub(self.start) <= handoff_threshold,
        }
    }
}

/// Bridge a single pipeline between its forward backfill and live ingestion. The returned service
/// drives `handle` to drain backfill chunks (ascending) into `pipeline_tx`, then forwards the live
/// subscription `live_rx` once backfill hands off — skipping any checkpoint backfill already
/// delivered. The pipeline reads the other end of `pipeline_tx` as its single checkpoint stream, so
/// it sees one contiguous `[start, ..)` sequence with no gap at the handoff boundary.
pub(crate) fn backfill_adapter(
    pipeline: &'static str,
    start: u64,
    mut handle: BackfillHandle,
    mut live_rx: mpsc::Receiver<Arc<CheckpointEnvelope>>,
    pipeline_tx: mpsc::Sender<Arc<CheckpointEnvelope>>,
) -> Service {
    Service::new().spawn_aborting(async move {
        // Backfill phase: deliver `[start, handoff)` in ascending order, tracking the boundary.
        let mut next_expected = start;
        while let Some(chunk) = handle.next_chunk().await? {
            for envelope in chunk {
                next_expected = envelope
                    .checkpoint
                    .summary
                    .sequence_number()
                    .saturating_add(1);
                if pipeline_tx.send(Arc::new(envelope)).await.is_err() {
                    // The pipeline stopped consuming; nothing more to do.
                    return Ok(());
                }
            }
        }

        debug!(
            pipeline,
            next_expected, "Backfill caught up; switching to live ingestion"
        );

        // Live phase: forward `[handoff, ..)`, skipping checkpoints backfill already delivered (the
        // overlap the live broadcaster re-fetches because it starts at the boot tip).
        while let Some(envelope) = live_rx.recv().await {
            if *envelope.checkpoint.summary.sequence_number() < next_expected {
                continue;
            }
            if pipeline_tx.send(envelope).await.is_err() {
                return Ok(());
            }
        }

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use sui_futures::service::Service;
    use sui_types::digests::CheckpointDigest;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
    use tokio::task::JoinHandle;
    use tokio::time::timeout;

    use crate::ingestion::ingestion_client::tests::MockIngestionClient;
    use crate::metrics::tests::test_ingestion_metrics;

    use super::*;

    const CHUNK_SIZE: u64 = 3;
    const EXPECTED_CONSUMER_MAX_DISTANCE: u64 = 1;
    const EXPECTED_CONSUMER_WAIT_DURATION: Duration = Duration::from_secs(60);
    const HANDOFF_THRESHOLD: u64 = CHUNK_SIZE;
    const RETRY_INTERVAL: Duration = Duration::from_millis(1);
    // Large enough that the periodic tip refresh never fires on its own during a test; tests that
    // exercise refresh use `start_paused` and advance time explicitly.
    const TIP_REFRESH_INTERVAL: Duration = Duration::from_secs(3600);
    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    fn checkpoint_envelope_with_timestamp(
        sequence_number: u64,
        timestamp_ms: u64,
    ) -> CheckpointEnvelope {
        CheckpointEnvelope {
            checkpoint: Arc::new(
                TestCheckpointBuilder::new(sequence_number)
                    .with_timestamp_ms(timestamp_ms)
                    .build_checkpoint(),
            ),
            chain_id: CheckpointDigest::new([1; 32]).into(),
        }
    }

    fn checkpoint_envelope(sequence_number: u64) -> CheckpointEnvelope {
        checkpoint_envelope_with_timestamp(sequence_number, 1_000 + sequence_number)
    }

    fn checkpoint_sequences(checkpoints: &[CheckpointEnvelope]) -> Vec<u64> {
        checkpoints
            .iter()
            .map(|envelope| *envelope.checkpoint.summary.sequence_number())
            .collect()
    }

    fn checkpoint_timestamps(checkpoints: &[CheckpointEnvelope]) -> Vec<u64> {
        checkpoints
            .iter()
            .map(|envelope| envelope.checkpoint.summary.timestamp_ms)
            .collect()
    }

    fn backfill_config() -> BackfillConfig {
        BackfillConfig {
            chunk_size: CHUNK_SIZE,
            expected_consumer_max_distance: EXPECTED_CONSUMER_MAX_DISTANCE,
            expected_consumer_wait_duration: EXPECTED_CONSUMER_WAIT_DURATION,
            retry_interval: RETRY_INTERVAL,
            handoff_threshold: HANDOFF_THRESHOLD,
            tip_refresh_interval: TIP_REFRESH_INTERVAL,
        }
    }

    fn backfill_service(initial_tip: u64) -> (BackfillService, Arc<MockIngestionClient>) {
        backfill_service_with_config(backfill_config(), initial_tip)
    }

    fn backfill_service_with_config(
        config: BackfillConfig,
        initial_tip: u64,
    ) -> (BackfillService, Arc<MockIngestionClient>) {
        let mock_client = Arc::new(MockIngestionClient {
            latest_checkpoint: initial_tip.into(),
            ..Default::default()
        });
        let ingestion_client =
            IngestionClient::from_trait(mock_client.clone(), test_ingestion_metrics());

        let service = BackfillService::new(config, ingestion_client, initial_tip)
            .expect("test backfill config must be valid");

        (service, mock_client)
    }

    fn insert_mock_checkpoints(mock_client: &MockIngestionClient, checkpoints: Range<u64>) {
        for checkpoint in checkpoints {
            mock_client.checkpoints.insert(
                checkpoint,
                checkpoint_envelope(checkpoint).checkpoint.as_ref().clone(),
            );
        }
    }

    fn insert_mock_checkpoints_with_timestamp_base(
        mock_client: &MockIngestionClient,
        checkpoints: Range<u64>,
        timestamp_base: u64,
    ) {
        for checkpoint in checkpoints {
            mock_client.checkpoints.insert(
                checkpoint,
                checkpoint_envelope_with_timestamp(checkpoint, timestamp_base + checkpoint)
                    .checkpoint
                    .as_ref()
                    .clone(),
            );
        }
    }

    /// Build a backfill service with `N` subscriptions, returning the handles as an array in
    /// subscription order (callers destructure, e.g. `let (service, [handle], client) = ...`).
    fn test_service<const N: usize>(
        initial_tip: u64,
        pipeline_ranges: [(u64, Option<u64>); N],
    ) -> (Service, [BackfillHandle; N], Arc<MockIngestionClient>) {
        let (mut backfill_service, mock_client) = backfill_service(initial_tip);
        let handles = pipeline_ranges.map(|(start, end)| match end {
            Some(end) => backfill_service.subscribe(start..end),
            None => backfill_service.subscribe(start..),
        });
        let service = backfill_service.run();

        (service, handles, mock_client)
    }

    async fn expect_next_chunk(
        handle_name: &str,
        handle: &mut BackfillHandle,
    ) -> Vec<CheckpointEnvelope> {
        timeout(TEST_TIMEOUT, handle.next_chunk())
            .await
            .unwrap_or_else(|_| panic!("{handle_name} next_chunk should return"))
            .unwrap_or_else(|error| panic!("{handle_name} next_chunk should not fail: {error}"))
            .unwrap_or_else(|| panic!("{handle_name} chunk should be returned"))
    }

    async fn expect_next_chunk_sequences(
        handle_name: &str,
        handle: &mut BackfillHandle,
    ) -> Vec<u64> {
        checkpoint_sequences(&expect_next_chunk(handle_name, handle).await)
    }

    async fn expect_no_next_chunk(handle_name: &str, handle: &mut BackfillHandle) {
        let chunk = timeout(TEST_TIMEOUT, handle.next_chunk())
            .await
            .unwrap_or_else(|_| panic!("{handle_name} next_chunk should return"))
            .unwrap_or_else(|error| panic!("{handle_name} next_chunk should not fail: {error}"));

        assert!(
            chunk.is_none(),
            "{handle_name} chunk should not be returned"
        );
    }

    async fn expect_spawned_next_chunk_result(
        task_name: &str,
        task: JoinHandle<anyhow::Result<Option<Vec<CheckpointEnvelope>>>>,
    ) -> Option<Vec<CheckpointEnvelope>> {
        timeout(TEST_TIMEOUT, task)
            .await
            .unwrap_or_else(|_| panic!("{task_name} handle should finish"))
            .unwrap_or_else(|error| panic!("{task_name} task should not panic: {error}"))
            .unwrap_or_else(|error| panic!("{task_name} next_chunk should not fail: {error}"))
    }

    async fn expect_spawned_next_chunk(
        task_name: &str,
        task: JoinHandle<anyhow::Result<Option<Vec<CheckpointEnvelope>>>>,
    ) -> Vec<CheckpointEnvelope> {
        expect_spawned_next_chunk_result(task_name, task)
            .await
            .unwrap_or_else(|| panic!("{task_name} chunk should be returned"))
    }

    async fn expect_service_join(mut service: Service) {
        timeout(TEST_TIMEOUT, service.join())
            .await
            .expect("service should stop")
            .expect("service should not fail");
    }

    async fn expect_service_shutdown(service: Service) {
        service.shutdown().await.expect("service should shut down");
    }

    async fn yield_to_service() {
        // Some tests need the first request parked in the service before mutating mock state.
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }
    }

    fn spawn_next_chunk(
        mut handle: BackfillHandle,
    ) -> JoinHandle<anyhow::Result<Option<Vec<CheckpointEnvelope>>>> {
        tokio::spawn(async move { handle.next_chunk().await })
    }

    fn expect_new_err(config: BackfillConfig) -> anyhow::Error {
        let mock_client = Arc::new(MockIngestionClient::default());
        let ingestion_client = IngestionClient::from_trait(mock_client, test_ingestion_metrics());

        BackfillService::new(config, ingestion_client, 0)
            .err()
            .expect("config must be rejected")
    }

    #[test]
    fn new_rejects_zero_chunk_size() {
        let error = expect_new_err(BackfillConfig {
            chunk_size: 0,
            ..backfill_config()
        });
        assert!(error.to_string().contains("chunk_size"));
    }

    #[test]
    fn new_rejects_handoff_threshold_below_chunk_size() {
        let error = expect_new_err(BackfillConfig {
            chunk_size: 10,
            handoff_threshold: 9,
            ..backfill_config()
        });
        assert!(error.to_string().contains("handoff_threshold"));
    }

    #[tokio::test]
    async fn run_without_subscriptions_exits() {
        let (backfill_service, _) = backfill_service(0);
        let service = backfill_service.run();

        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn next_chunk_returns_none_after_service_stops() {
        let (service, [mut handle], _) = test_service(10, [(0, Some(CHUNK_SIZE))]);

        expect_service_shutdown(service).await;

        expect_no_next_chunk("closed handle", &mut handle).await;
    }

    #[tokio::test]
    async fn next_chunk_returns_none_for_empty_subscription() {
        let (service, [mut handle], _) = test_service(10, [(1, Some(1))]);

        expect_no_next_chunk("empty handle", &mut handle).await;

        expect_service_shutdown(service).await;
    }

    #[tokio::test]
    async fn bounded_next_chunk_fetches_chunk_through_service() {
        let (service, [mut handle], mock_client) = test_service(10, [(0, Some(CHUNK_SIZE))]);
        insert_mock_checkpoints(&mock_client, 0..CHUNK_SIZE);

        let sequences = expect_next_chunk_sequences("handle", &mut handle).await;

        assert_eq!(sequences, vec![0, 1, 2]);

        drop(handle);
        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn bounded_next_chunk_advances_subscription_range_until_empty() {
        let (service, [mut handle], mock_client) = test_service(10, [(0, Some(CHUNK_SIZE * 2))]);
        insert_mock_checkpoints(&mock_client, 0..CHUNK_SIZE * 2);

        let first_sequences = expect_next_chunk_sequences("first", &mut handle).await;
        let second_sequences = expect_next_chunk_sequences("second", &mut handle).await;

        assert_eq!(first_sequences, vec![0, 1, 2]);
        assert_eq!(second_sequences, vec![3, 4, 5]);

        expect_no_next_chunk("completed handle", &mut handle).await;

        drop(handle);
        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn bounded_next_chunk_filters_first_chunk_by_pipeline_lo() {
        let (service, [mut handle], mock_client) = test_service(10, [(2, Some(4))]);
        insert_mock_checkpoints(&mock_client, 0..6);

        let first = expect_next_chunk_sequences("first", &mut handle).await;
        let second = expect_next_chunk_sequences("second", &mut handle).await;

        assert_eq!(first, vec![2]);
        assert_eq!(second, vec![3]);

        drop(handle);
        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn bounded_final_chunk_is_clamped_to_cap() {
        // With a bounded cap of 2, the chunk grid `[0, 3)` is clamped to `[0, 2)`, so the service
        // never fetches checkpoint 2 (which is not inserted).
        let (service, [mut handle], mock_client) = test_service(10, [(0, Some(2))]);
        insert_mock_checkpoints(&mock_client, 0..2);

        let sequences = expect_next_chunk_sequences("handle", &mut handle).await;

        assert_eq!(sequences, vec![0, 1]);

        drop(handle);
        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn concurrent_handles_receive_same_pending_chunk() {
        let (service, [first_handle, second_handle], mock_client) =
            test_service(10, [(0, Some(CHUNK_SIZE)), (0, Some(CHUNK_SIZE))]);

        let first = spawn_next_chunk(first_handle);
        let second = spawn_next_chunk(second_handle);

        tokio::task::yield_now().await;
        insert_mock_checkpoints(&mock_client, 0..CHUNK_SIZE);

        let first = expect_spawned_next_chunk("first", first).await;
        let second = expect_spawned_next_chunk("second", second).await;

        assert_eq!(checkpoint_sequences(&first), vec![0, 1, 2]);
        assert_eq!(checkpoint_sequences(&second), vec![0, 1, 2]);

        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn concurrent_handles_receive_filtered_pending_chunk() {
        // Both handles share chunk `[0, 3)`; the narrower handle's `end` clips its response.
        let (service, [narrow_handle, wide_handle], mock_client) =
            test_service(10, [(0, Some(2)), (0, Some(CHUNK_SIZE))]);

        let narrow = spawn_next_chunk(narrow_handle);
        let wide = spawn_next_chunk(wide_handle);

        tokio::task::yield_now().await;
        insert_mock_checkpoints(&mock_client, 0..CHUNK_SIZE);

        let narrow = expect_spawned_next_chunk("narrow", narrow).await;
        let wide = expect_spawned_next_chunk("wide", wide).await;

        assert_eq!(checkpoint_sequences(&narrow), vec![0, 1]);
        assert_eq!(checkpoint_sequences(&wide), vec![0, 1, 2]);

        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn next_chunk_returns_none_when_service_drops_pending_reply() {
        let (service, [handle], _) = test_service(10, [(0, Some(CHUNK_SIZE))]);
        let pending = spawn_next_chunk(handle);

        tokio::task::yield_now().await;
        expect_service_shutdown(service).await;

        let chunk = expect_spawned_next_chunk_result("pending", pending).await;

        assert!(chunk.is_none());
    }

    #[tokio::test]
    async fn service_stops_when_last_handle_drops_with_pending_fetches() {
        let (service, [handle], mock_client) = test_service(10, [(0, Some(CHUNK_SIZE))]);
        mock_client.not_found_failures.insert(0, usize::MAX);
        let pending = spawn_next_chunk(handle);

        timeout(TEST_TIMEOUT, async {
            loop {
                if mock_client
                    .not_found_failures
                    .get(&0)
                    .is_some_and(|remaining| *remaining < usize::MAX)
                {
                    // Wait until the fetch task is parked in the service before aborting the
                    // caller, otherwise the test can pass without exercising shutdown cleanup.
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("fetch should start");

        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());

        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn dropped_expected_consumer_releases_pending_chunk() {
        let (service, [first_handle, expected_handle], mock_client) =
            test_service(10, [(0, Some(CHUNK_SIZE)), (0, Some(CHUNK_SIZE))]);

        let first = spawn_next_chunk(first_handle);
        yield_to_service().await;
        // Dropping the expected consumer removes it from the chunk's expected set, so the chunk is
        // released to `first` on completion instead of waiting out the retention timer.
        drop(expected_handle);
        insert_mock_checkpoints(&mock_client, 0..CHUNK_SIZE);

        let first_chunk = expect_spawned_next_chunk("first", first).await;

        assert_eq!(checkpoint_sequences(&first_chunk), vec![0, 1, 2]);

        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn completed_chunks_remain_cached_during_expected_consumer_wait_duration() {
        let (service, [first_handle, mut expected_handle], mock_client) =
            test_service(10, [(0, Some(CHUNK_SIZE)), (0, Some(CHUNK_SIZE))]);

        insert_mock_checkpoints(&mock_client, 0..CHUNK_SIZE);
        let first = spawn_next_chunk(first_handle);
        yield_to_service().await;

        // Distinct timestamps let the test distinguish cache reuse from a fresh fetch.
        insert_mock_checkpoints_with_timestamp_base(&mock_client, 0..CHUNK_SIZE, 10_000);
        let cached_chunk = expect_next_chunk("expected consumer", &mut expected_handle).await;
        let first_chunk = expect_spawned_next_chunk("first", first).await;

        assert_eq!(
            checkpoint_timestamps(&first_chunk),
            vec![1_000, 1_001, 1_002]
        );
        assert_eq!(
            checkpoint_timestamps(&cached_chunk),
            vec![1_000, 1_001, 1_002]
        );

        drop(expected_handle);
        expect_service_join(service).await;
    }

    #[tokio::test(start_paused = true)]
    async fn completed_chunks_are_evicted_after_expected_consumer_wait_duration() {
        let config = BackfillConfig {
            expected_consumer_wait_duration: Duration::from_millis(1),
            ..backfill_config()
        };
        let (mut backfill_service, mock_client) = backfill_service_with_config(config, 10);
        let mut expected_handle = backfill_service.subscribe(0..CHUNK_SIZE);
        let first_handle = backfill_service.subscribe(0..CHUNK_SIZE);
        let service = backfill_service.run();

        insert_mock_checkpoints(&mock_client, 0..CHUNK_SIZE);
        let first = spawn_next_chunk(first_handle);
        yield_to_service().await;

        tokio::time::advance(Duration::from_millis(10)).await;
        // The first waiter only completes after the retention timer has released the cached entry.
        let first_chunk = expect_spawned_next_chunk("first", first).await;

        insert_mock_checkpoints_with_timestamp_base(&mock_client, 0..CHUNK_SIZE, 10_000);
        let refreshed_chunk = expect_next_chunk("expected consumer", &mut expected_handle).await;
        assert_eq!(
            checkpoint_timestamps(&first_chunk),
            vec![1_000, 1_001, 1_002]
        );
        assert_eq!(
            checkpoint_timestamps(&refreshed_chunk),
            vec![10_000, 10_001, 10_002]
        );

        drop(expected_handle);
        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn live_subscription_hands_off_within_threshold_of_tip() {
        // tip = 5, threshold = 3: the handle backfills `[0, 3)`, then hands off because the gap to
        // the tip (5 - 3 = 2) is within the threshold.
        let (service, [mut handle], mock_client) = test_service(5, [(0, None)]);
        insert_mock_checkpoints(&mock_client, 0..CHUNK_SIZE);

        let sequences = expect_next_chunk_sequences("live", &mut handle).await;
        assert_eq!(sequences, vec![0, 1, 2]);

        expect_no_next_chunk("caught-up live", &mut handle).await;

        drop(handle);
        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn live_subscription_already_caught_up_at_startup() {
        // start = 4, tip = 5, threshold = 3: nothing to backfill, so the service has no work and
        // the handle is told to hand off immediately.
        let (service, [mut handle], _) = test_service(5, [(4, None)]);

        expect_no_next_chunk("caught-up live", &mut handle).await;

        expect_service_join(service).await;
    }

    #[tokio::test(start_paused = true)]
    async fn live_subscription_chases_advancing_tip() {
        let (mut backfill_service, mock_client) = backfill_service(5);
        let mut handle = backfill_service.subscribe(0..);
        let service = backfill_service.run();

        insert_mock_checkpoints(&mock_client, 0..9);

        // With tip = 5 the handle backfills `[0, 3)` and would hand off next (gap 5 - 3 = 2).
        let first = expect_next_chunk_sequences("first", &mut handle).await;
        assert_eq!(first, vec![0, 1, 2]);

        // Advance the network tip and let the periodic refresh pick it up.
        mock_client.latest_checkpoint.store(20, Ordering::Relaxed);
        tokio::time::advance(TIP_REFRESH_INTERVAL).await;
        yield_to_service().await;

        // The raised ceiling lets the handle keep backfilling past the original tip.
        let second = expect_next_chunk_sequences("second", &mut handle).await;
        let third = expect_next_chunk_sequences("third", &mut handle).await;
        assert_eq!(second, vec![3, 4, 5]);
        assert_eq!(third, vec![6, 7, 8]);

        drop(handle);
        expect_service_join(service).await;
    }
}
