// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Event-driven recovery for minimal blocks that cannot be inflated at receipt.
//!
//! [`HintCache`] holds digests learned at receipt time, before verification or
//! acceptance: a full block's digest is hashed from its bytes, a minimal block carries
//! its claimed digest. Feeding these to the inflation resolver lets a child reconstruct
//! the moment its ancestor's bytes exist locally. Hints are untrusted and bounded; the
//! child's claimed-digest check remains the cryptographic gate.
//!
//! [`RecoveryManager`] is an actor that parks un-inflatable minimal blocks keyed by
//! their first missing slot, re-attempts inflation when that slot is heard from (a
//! receipt-time hint, or Core's accepted-block broadcast for blocks that arrive via
//! sync), and escalates stragglers to a digest-verified sender fetch after a deadline.
//! Wakes are event-driven because arrival lag is wide (measured p50 ≈ 167 ms /
//! p95 ≈ 473 ms): a fixed schedule either fetches too much or waits too long. All
//! parking state is owned by the actor task: a hint is published to the cache before
//! its wake is sent, and a park re-checks its missing slot after registering, so a
//! hint racing the park is always seen by one side or the other.

use std::{
    collections::{BTreeMap, BinaryHeap, HashMap, VecDeque},
    sync::{Arc, Weak},
    time::Duration,
};

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockDigest, BlockRef, Round};
use parking_lot::Mutex;
use tokio::{
    sync::{Semaphore, broadcast, mpsc},
    time::Instant,
};
use tracing::debug;

use crate::{
    block::{BlockAPI as _, Slot, VerifiedBlock},
    block_inflater::BlockInflater,
    context::Context,
    minimal_block::{FallbackReason, InflateError},
    network::{ExtendedSerializedBlock, ValidatorNetworkClient, ValidatorNetworkService},
};

/// Candidates retained per slot; mirrors the codec's per-slot bound.
const MAX_HINTS_PER_SLOT: usize = 3;
/// Hints retained per hinted authority, across all slots. An authority can only hint
/// its own slots (identity is validated against the authenticated stream peer), so this
/// bounds how much cache one misbehaving peer can occupy.
const MAX_HINTS_PER_AUTHORITY: usize = 1024;
/// Ceiling on the derived global hint capacity (memory guard: digests are 32 B, so this
/// is ~8 MiB at the ceiling).
const MAX_HINTS_TOTAL_CEILING: usize = 262_144;
/// Parked entries, globally and per sending peer.
const MAX_PARKED_ENTRIES: usize = 4096;
const MAX_PARKED_ENTRIES_PER_PEER: usize = 256;
/// Parked bytes (retained minimal encodings), globally and per sending peer.
const MAX_PARKED_BYTES: usize = 16 << 20;
const MAX_PARKED_BYTES_PER_PEER: usize = 2 << 20;
/// Concurrent digest-verified sender fetches.
const MAX_FETCHES: usize = 64;
const MAX_FETCHES_PER_PEER: usize = 6;
/// Bounded queue of refs waiting for a fetch permit (refs only — no block bytes — so
/// the bound is about actor memory hygiene, not payload). Sized to the parking table:
/// every escalated entry fits, so intents are dropped (with a metric) only in genuine
/// overload beyond everything else being full.
const MAX_FETCH_INTENTS: usize = MAX_PARKED_ENTRIES;
/// Recovered serializations waiting for a submission permit.
const MAX_PENDING_SUBMISSIONS: usize = 1024;
/// Concurrent recovered-block submissions into the verification pipeline. Submission
/// must not serialize behind one slow verify; it must also not stampede the core queue.
const MAX_CONCURRENT_SUBMISSIONS: usize = 16;
/// Parked entries escalate to the sender fetch after this long, plus per-entry jitter.
/// Generous: arrival p99 is ~726 ms, and fetching earlier does not make a block
/// acceptable earlier — acceptance still waits on the same ancestors.
const FETCH_DEADLINE: Duration = Duration::from_millis(1000);
const FETCH_DEADLINE_JITTER_MS: u64 = 250;
/// In-flight parks buffered ahead of the actor. Sized to absorb bursts (~0.5 s at the
/// worst observed park rate) while bounding channel-held bytes; a full channel
/// backpressures the sending stream rather than dropping.
pub(crate) const PARK_CHANNEL_CAPACITY: usize = 1024;
/// In-flight hint wakes; overflow is shed (counted) — wakes are covered elsewhere.
pub(crate) const HINT_CHANNEL_CAPACITY: usize = 1024;
/// Deferred fetch/submission dispatch is retried on every task completion (a completed
/// task has provably released its permits); this tick is only the safety net between
/// completions, so queued work can never wait long on a missed edge.
const DRAIN_FALLBACK_TICK: Duration = Duration::from_millis(25);
/// Timeout for one sender fetch (off-stream; nothing waits on it).
pub(crate) const RECOVERY_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Receipt-time digest hints: slot -> ordered digests, untrusted, bounded.
///
/// Callers must validate hint identity BEFORE insertion: the hinted author must equal
/// the authenticated stream peer and the epoch must be current, so a peer can only ever
/// hint its own slots. The cache never advances its eviction horizon from received
/// (unverified) rounds — only from accepted rounds — so a Byzantine future-round block
/// cannot flush it.
/// Outcome of a hint insertion, labeling the rejection counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HintInsert {
    Inserted,
    Duplicate,
    BelowHorizon,
    AboveHorizon,
    SlotFull,
    AuthorityFull,
    GlobalFull,
}

impl HintInsert {
    pub(crate) fn label(self) -> &'static str {
        match self {
            HintInsert::Inserted => "inserted",
            HintInsert::Duplicate => "duplicate",
            HintInsert::BelowHorizon => "below_horizon",
            HintInsert::AboveHorizon => "above_horizon",
            HintInsert::SlotFull => "slot_full",
            HintInsert::AuthorityFull => "authority_full",
            HintInsert::GlobalFull => "global_full",
        }
    }
}

pub(crate) struct HintCache {
    inner: Mutex<HintCacheInner>,
    /// Slots older than `horizon - retained_rounds` are evicted.
    retained_rounds: Round,
    /// Derived from retention x committee size so the cap can never be smaller than
    /// the retention window's working set; a flat cap below that starves fresh hints
    /// behind stale ones. Overflow evicts oldest-first for the same reason.
    capacity: usize,
}

struct HintCacheInner {
    hints: BTreeMap<Slot, Vec<BlockDigest>>,
    per_authority: HashMap<AuthorityIndex, usize>,
    total: usize,
    horizon: Round,
}

impl HintCache {
    pub(crate) fn new(retained_rounds: Round, committee_size: usize) -> Self {
        let capacity = (retained_rounds as usize)
            .saturating_mul(committee_size.max(1))
            .clamp(1024, MAX_HINTS_TOTAL_CEILING);
        Self {
            inner: Mutex::new(HintCacheInner {
                hints: BTreeMap::new(),
                per_authority: HashMap::new(),
                total: 0,
                horizon: 0,
            }),
            retained_rounds,
            capacity,
        }
    }

    /// Inserts a validated hint, evicting the OLDEST slots when full: a fresh hint is
    /// always worth more than the oldest retained one (children reference recent
    /// rounds; old slots are in accepted DAG state anyway). Returns the insertion
    /// outcome for the rejection counters.
    pub(crate) fn insert(&self, slot: Slot, digest: BlockDigest) -> HintInsert {
        let mut inner = self.inner.lock();
        // Preflight every rejection with read-only lookups BEFORE any eviction: a hint
        // that is going to be rejected must not be able to grind valid entries out of
        // a full cache (e.g. a duplicate storm evicting the working set).
        if slot.round.saturating_add(self.retained_rounds) < inner.horizon {
            return HintInsert::BelowHorizon;
        }
        // Symmetric future bound: oldest-first eviction never reaches a far-future
        // slot, so without this a peer could park its per-authority quota there
        // permanently. The slack covers legitimate pipeline lag. The horizon==0
        // exemption covers startup; the first advance_horizon prunes anything a
        // bounded insert would have refused.
        if inner.horizon > 0 && slot.round > inner.horizon.saturating_add(self.retained_rounds) {
            return HintInsert::AboveHorizon;
        }
        if let Some(digests) = inner.hints.get(&slot) {
            if digests.contains(&digest) {
                return HintInsert::Duplicate;
            }
            if digests.len() >= MAX_HINTS_PER_SLOT {
                return HintInsert::SlotFull;
            }
        }
        // Conservative pre-eviction check: eviction can only DECREASE this count, so a
        // preflight pass here remains valid afterwards.
        if inner
            .per_authority
            .get(&slot.authority)
            .copied()
            .unwrap_or(0)
            >= MAX_HINTS_PER_AUTHORITY
        {
            return HintInsert::AuthorityFull;
        }
        while inner.total >= self.capacity {
            let Some((oldest, _)) = inner.hints.first_key_value() else {
                break;
            };
            // Evict only strictly older slots; an incoming hint that cannot displace
            // anything strictly older than itself is the one refused.
            if oldest.round >= slot.round {
                return HintInsert::GlobalFull;
            }
            let (evicted_slot, evicted) = inner.hints.pop_first().expect("non-empty");
            inner.total = inner.total.saturating_sub(evicted.len());
            if let Some(count) = inner.per_authority.get_mut(&evicted_slot.authority) {
                *count = count.saturating_sub(evicted.len());
            }
        }
        *inner.per_authority.entry(slot.authority).or_insert(0) += 1;
        inner.hints.entry(slot).or_default().push(digest);
        inner.total += 1;
        HintInsert::Inserted
    }

    pub(crate) fn candidates(&self, slot: Slot) -> Vec<BlockDigest> {
        self.inner
            .lock()
            .hints
            .get(&slot)
            .cloned()
            .unwrap_or_default()
    }

    /// Advances the eviction horizon from an ACCEPTED round (never a received one).
    pub(crate) fn advance_horizon(&self, accepted_round: Round) {
        let mut inner = self.inner.lock();
        if accepted_round <= inner.horizon {
            return;
        }
        let first_horizon = inner.horizon == 0;
        inner.horizon = accepted_round;
        let cutoff = accepted_round.saturating_sub(self.retained_rounds);
        let retained = inner.hints.split_off(&Slot::new_for_test(cutoff, 0));
        let evicted = std::mem::replace(&mut inner.hints, retained);
        Self::discount(&mut inner, evicted);
        if first_horizon {
            // Startup inserts had no horizon to bound them above; anything beyond the
            // bound `insert` enforces from now on would otherwise never be evicted (it
            // is never the oldest) and could pin quota forever.
            if let Some(bound) = accepted_round
                .saturating_add(self.retained_rounds)
                .checked_add(1)
            {
                let above = inner.hints.split_off(&Slot::new_for_test(bound, 0));
                Self::discount(&mut inner, above);
            }
        }
    }

    fn discount(inner: &mut HintCacheInner, evicted: BTreeMap<Slot, Vec<BlockDigest>>) {
        for (slot, digests) in evicted {
            inner.total = inner.total.saturating_sub(digests.len());
            if let Some(count) = inner.per_authority.get_mut(&slot.authority) {
                *count = count.saturating_sub(digests.len());
            }
        }
    }
}

/// Commands from subscription streams to the manager actor.
/// A minimal block that failed inflation at receipt: park it.
///
/// Parks are lossless: the stream side blocks (backpressuring that one peer) rather
/// than drop when the channel is full, because a lost park is a block no recovery path
/// owns — its descendants wedge in `block_manager` until sync refetches it. Hint wakes
/// travel on a separate lossy channel; they have three covers (at-park recheck, the
/// accepted broadcast, the deadline) and may be shed freely.
pub(crate) struct ParkCommand {
    pub(crate) peer: AuthorityIndex,
    pub(crate) block_ref: BlockRef,
    pub(crate) minimal: Bytes,
    /// The slot whose arrival could make the block inflatable, or `None` when waiting
    /// cannot repair it (ambiguity, digest mismatch) and only the fetch can.
    pub(crate) missing: Option<Slot>,
}

struct ParkedEntry {
    peer: AuthorityIndex,
    minimal: Bytes,
    missing: Slot,
    // Assigned once at park; identifies this entry's lifetime so a deadline left in
    // the heap by a removed entry cannot expire a later re-park of the same ref.
    generation: u64,
    parked_at: Instant,
    deadline: Instant,
}

/// A fetch awaiting a permit: the claimed ref is the fetch key; no payload is retained.
struct FetchIntent {
    block_ref: BlockRef,
    peer: AuthorityIndex,
    parked_at: Instant,
}

/// A recovered serialization awaiting a submission permit.
struct PendingSubmission {
    block_ref: BlockRef,
    peer: AuthorityIndex,
    serialized: Bytes,
    result: &'static str,
    attempt: &'static str,
    parked_at: Instant,
}

/// Outcome of a detached task, returned through the `JoinSet`. Results ride the join
/// itself so a task's permits are provably released before its outcome is observed —
/// which is what makes join-driven queue draining exact.
enum TaskResult {
    /// The fetch finished; `serialized` is the digest-verified block, or `None` on a
    /// failed fetch or claimed-digest mismatch (both already counted by the task).
    FetchDone {
        block_ref: BlockRef,
        peer: AuthorityIndex,
        serialized: Option<Bytes>,
        parked_at: Instant,
    },
    /// The submission finished; the ref's recovery-dedup slot can be released.
    SubmitDone(BlockRef),
}

/// Why a parked entry is being re-attempted; becomes the metric label on success.
#[derive(Clone, Copy)]
enum WakeSource {
    /// At park time or on a receipt-time hint: resolution from bytes-arrival state.
    Hint,
    /// Core's accepted-block broadcast.
    Accepted,
}

impl WakeSource {
    fn label(self) -> &'static str {
        match self {
            WakeSource::Hint => "hint_inflated",
            WakeSource::Accepted => "inflated_late",
        }
    }
}

/// The parking actor. All parking state lives on this struct and is touched only by
/// `run`'s single task, which is what makes hint-drain vs park ordering race-free.
pub(crate) struct RecoveryManager<C: ValidatorNetworkClient, S: ValidatorNetworkService> {
    context: Arc<Context>,
    block_inflater: Arc<BlockInflater>,
    hint_cache: Arc<HintCache>,
    network_client: Arc<C>,
    authority_service: Weak<S>,
    entries: HashMap<BlockRef, ParkedEntry>,
    waiters: HashMap<Slot, Vec<BlockRef>>,
    // Min-heap on deadline via Reverse ordering; stale generations are skipped on pop.
    deadlines: BinaryHeap<std::cmp::Reverse<(Instant, BlockRef, u64)>>,
    // Refs (no bytes) awaiting a fetch permit; drained on fetch completions.
    fetch_intents: VecDeque<FetchIntent>,
    // Recovered serializations awaiting a submission permit; drained on completions.
    pending_submissions: VecDeque<PendingSubmission>,
    // Every claimed ref currently anywhere in recovery — parked, awaiting a fetch
    // permit, or fetching — so duplicate receipts cannot multiply fetches.
    in_recovery: std::collections::HashSet<BlockRef>,
    // Detached work (fetches, submissions) lives here so aborting the actor aborts it
    // all: nothing outlives `stop()`.
    tasks: tokio::task::JoinSet<TaskResult>,
    generation: u64,
    parked_bytes: usize,
    parked_bytes_per_peer: HashMap<AuthorityIndex, usize>,
    parked_entries_per_peer: HashMap<AuthorityIndex, usize>,
    fetch_permits: Arc<Semaphore>,
    per_peer_fetch_permits: HashMap<AuthorityIndex, Arc<Semaphore>>,
    submission_permits: Arc<Semaphore>,
}

impl<C: ValidatorNetworkClient, S: ValidatorNetworkService> RecoveryManager<C, S> {
    pub(crate) fn new(
        context: Arc<Context>,
        block_inflater: Arc<BlockInflater>,
        hint_cache: Arc<HintCache>,
        network_client: Arc<C>,
        authority_service: Weak<S>,
    ) -> Self {
        Self {
            context,
            block_inflater,
            hint_cache,
            network_client,
            authority_service,
            entries: HashMap::new(),
            waiters: HashMap::new(),
            deadlines: BinaryHeap::new(),
            fetch_intents: VecDeque::new(),
            pending_submissions: VecDeque::new(),
            in_recovery: std::collections::HashSet::new(),
            tasks: tokio::task::JoinSet::new(),
            generation: 0,
            parked_bytes: 0,
            parked_bytes_per_peer: HashMap::new(),
            parked_entries_per_peer: HashMap::new(),
            fetch_permits: Arc::new(Semaphore::new(MAX_FETCHES)),
            per_peer_fetch_permits: HashMap::new(),
            submission_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_SUBMISSIONS)),
        }
    }

    /// Runs the actor until a command channel closes (subscriber stop) or the task is
    /// aborted.
    pub(crate) async fn run(
        mut self,
        mut parks: mpsc::Receiver<ParkCommand>,
        mut hints: mpsc::Receiver<Slot>,
        mut accepted_blocks: broadcast::Receiver<VerifiedBlock>,
    ) {
        let mut drain_tick = tokio::time::interval(DRAIN_FALLBACK_TICK);
        drain_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let busy = self
            .context
            .metrics
            .node_metrics
            .minimal_block_recovery_actor_busy
            .clone();
        loop {
            let next_deadline = self
                .deadlines
                .peek()
                .map(|std::cmp::Reverse((at, _, _))| *at)
                .unwrap_or_else(|| Instant::now() + Duration::from_secs(3600));
            tokio::select! {
                // The gate is the backpressure point for lossless parks: while the
                // table is full, parks wait in the channel (and ultimately on the
                // sending streams) until wakes and deadlines free entries — within
                // the deadline bound, so the arm always reopens.
                park = parks.recv(), if self.entries.len() < MAX_PARKED_ENTRIES => {
                    let Some(park) = park else {
                        return;
                    };
                    let timer = busy.with_label_values(&["park"]).start_timer();
                    self.handle_park(park);
                    timer.observe_duration();
                }
                slot = hints.recv() => {
                    let Some(slot) = slot else {
                        return;
                    };
                    let timer = busy.with_label_values(&["slot_heard"]).start_timer();
                    self.wake_slot(slot, WakeSource::Hint);
                    timer.observe_duration();
                }
                accepted = accepted_blocks.recv() => {
                    match accepted {
                        Ok(block) => {
                            let timer = busy.with_label_values(&["accepted"]).start_timer();
                            self.hint_cache.advance_horizon(block.round());
                            self.wake_slot(
                                Slot::new(block.round(), block.author()),
                                WakeSource::Accepted,
                            );
                            timer.observe_duration();
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            // Skipped wakes are covered by the parked-entry deadline.
                            // Resubscribing jumps to the head: once lagged, a receiver
                            // is pinned at the channel's retention edge and would
                            // otherwise crawl through stale blocks, re-lagging on
                            // every send.
                            debug!("Recovery manager lagged {skipped} accepted blocks");
                            self.context
                                .metrics
                                .node_metrics
                                .minimal_block_recovery_accepted_lag
                                .inc();
                            accepted_blocks = accepted_blocks.resubscribe();
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            return;
                        }
                    }
                }
                Some(result) = self.tasks.join_next(), if !self.tasks.is_empty() => {
                    let timer = busy.with_label_values(&["task"]).start_timer();
                    match result {
                        Ok(TaskResult::FetchDone { block_ref, peer, serialized, parked_at }) => {
                            match serialized {
                                Some(serialized) => self.dispatch_submission(PendingSubmission {
                                    block_ref,
                                    peer,
                                    serialized,
                                    result: "fetch_recovered",
                                    attempt: "fetch",
                                    parked_at,
                                }),
                                None => {
                                    self.in_recovery.remove(&block_ref);
                                }
                            }
                        }
                        Ok(TaskResult::SubmitDone(block_ref)) => {
                            self.in_recovery.remove(&block_ref);
                        }
                        // A panic in a detached task is a bug and is propagated,
                        // matching the crate's task conventions. Cancellation only
                        // happens when the JoinSet is dropped, i.e. at shutdown.
                        Err(e) => {
                            if e.is_panic() {
                                std::panic::resume_unwind(e.into_panic());
                            }
                        }
                    }
                    // The finished task has provably released its permits (results
                    // ride the join), so this is the exact moment deferred dispatch
                    // can succeed.
                    self.drain_queues();
                    timer.observe_duration();
                }
                _ = drain_tick.tick(), if !self.fetch_intents.is_empty()
                    || !self.pending_submissions.is_empty() => {
                    let timer = busy.with_label_values(&["tick"]).start_timer();
                    self.drain_queues();
                    timer.observe_duration();
                }
                _ = tokio::time::sleep_until(next_deadline) => {
                    let timer = busy.with_label_values(&["deadline"]).start_timer();
                    self.expire_deadlines();
                    timer.observe_duration();
                }
            }
        }
    }

    /// Parks a new entry — registering first, then re-attempting inflation on this same
    /// task, so a hint published between the stream's failed inflation and this park
    /// cannot be lost — or resolves it immediately if local state already suffices.
    ///
    /// The global entry cap is enforced by gating the park channel arm in `run`, so
    /// only the byte and per-peer caps divert here.
    fn handle_park(&mut self, park: ParkCommand) {
        let ParkCommand {
            peer,
            block_ref,
            minimal,
            missing,
        } = park;
        if self.in_recovery.contains(&block_ref) {
            self.context
                .metrics
                .node_metrics
                .minimal_block_recovery_duplicates
                .inc();
            return;
        }
        self.in_recovery.insert(block_ref);
        let Some(missing) = missing else {
            // Nothing to wait for: only the digest-verified sender fetch can repair it.
            self.start_fetch(FetchIntent {
                block_ref,
                peer,
                parked_at: Instant::now(),
            });
            return;
        };
        let over_capacity = self.parked_bytes + minimal.len() > MAX_PARKED_BYTES
            || self
                .parked_entries_per_peer
                .get(&peer)
                .copied()
                .unwrap_or(0)
                >= MAX_PARKED_ENTRIES_PER_PEER
            || self.parked_bytes_per_peer.get(&peer).copied().unwrap_or(0) + minimal.len()
                > MAX_PARKED_BYTES_PER_PEER;
        self.generation += 1;
        let now = Instant::now();
        // Deterministic spread is all the jitter needs to achieve: avoid synchronized
        // deadline herds across entries.
        let jitter = Duration::from_millis(self.generation % FETCH_DEADLINE_JITTER_MS);
        let entry = ParkedEntry {
            peer,
            minimal,
            missing,
            generation: self.generation,
            parked_at: now,
            deadline: now + FETCH_DEADLINE + jitter,
        };
        if over_capacity {
            // The table is protecting memory; the fetch path (itself bounded, with a
            // bounded intent queue) takes over rather than dropping silently.
            self.context
                .metrics
                .node_metrics
                .minimal_block_recovery_overflow
                .inc();
            self.start_fetch(FetchIntent {
                block_ref,
                peer: entry.peer,
                parked_at: entry.parked_at,
            });
            return;
        }
        self.parked_bytes += entry.minimal.len();
        *self.parked_bytes_per_peer.entry(peer).or_insert(0) += entry.minimal.len();
        *self.parked_entries_per_peer.entry(peer).or_insert(0) += 1;
        self.deadlines.push(std::cmp::Reverse((
            entry.deadline,
            block_ref,
            entry.generation,
        )));
        self.waiters.entry(missing).or_default().push(block_ref);
        self.entries.insert(block_ref, entry);
        self.publish_parked_gauge();
        // Same-task re-check: any hint published before this point is visible now.
        // Gated on candidate presence — without one, inflation is guaranteed to fail
        // at the same slot, and an unconditional decode per park was a measured actor
        // saturation source. A candidate that appears later wakes the entry normally.
        if self.block_inflater.can_resolve(missing) {
            self.reattempt(block_ref, WakeSource::Hint);
        }
    }

    /// Wakes every entry parked on `slot`.
    fn wake_slot(&mut self, slot: Slot, source: WakeSource) {
        let Some(waiting) = self.waiters.remove(&slot) else {
            return;
        };
        for block_ref in waiting {
            self.reattempt(block_ref, source);
        }
    }

    /// One inflation attempt for a parked entry: submit on success, re-key on a new
    /// missing slot, escalate to fetch on ambiguity/mismatch, drop on malformed.
    fn reattempt(&mut self, block_ref: BlockRef, source: WakeSource) {
        let Some(entry) = self.entries.get(&block_ref) else {
            return;
        };
        let peer = entry.peer;
        self.context
            .metrics
            .node_metrics
            .minimal_block_recovery_attempts
            .inc();
        match self.block_inflater.inflate(&entry.minimal, peer) {
            Ok((_signed_block, serialized)) => {
                let entry = self.remove_entry(&block_ref).expect("present above");
                self.context
                    .metrics
                    .node_metrics
                    .minimal_blocks_received_bytes_saved
                    .with_label_values(&[self.context.committee.authority(peer).hostname.as_str()])
                    .inc_by(serialized.len().saturating_sub(entry.minimal.len()) as u64);
                self.publish_parked_gauge();
                // The ref stays in `in_recovery` until the submission task completes,
                // so duplicate receipts during the submission window are suppressed.
                self.submit(block_ref, peer, serialized, source.label(), entry.parked_at);
            }
            Err(InflateError::NeedFullBlock {
                reason: FallbackReason::MissingAncestor(next_missing),
                ..
            }) => {
                let entry = self.entries.get_mut(&block_ref).expect("present above");
                if entry.missing != next_missing {
                    // Re-key: retire the old-slot registration so it cannot
                    // accumulate stale waiter references. The deadline (and hence
                    // the entry's generation and heap entry) is unchanged — it
                    // belongs to the parked block, not to the slot it waits on.
                    let old_slot = entry.missing;
                    entry.missing = next_missing;
                    if let Some(waiting) = self.waiters.get_mut(&old_slot) {
                        waiting.retain(|r| *r != block_ref);
                        if waiting.is_empty() {
                            self.waiters.remove(&old_slot);
                        }
                    }
                }
                // Push-if-absent: the at-park reattempt revisits an entry whose
                // registration already exists.
                let waiting = self.waiters.entry(next_missing).or_default();
                if !waiting.contains(&block_ref) {
                    waiting.push(block_ref);
                }
            }
            Err(InflateError::NeedFullBlock { .. }) => {
                // Ambiguity or digest mismatch: waiting cannot repair these; the full
                // block resolves them.
                let entry = self.remove_entry(&block_ref).expect("present above");
                self.publish_parked_gauge();
                self.start_fetch(FetchIntent {
                    block_ref,
                    peer: entry.peer,
                    parked_at: entry.parked_at,
                });
            }
            Err(InflateError::Malformed(_)) => {
                // Structurally invalid bytes cannot become valid; the stream path
                // already counted the peer fault.
                self.remove_entry(&block_ref);
                self.in_recovery.remove(&block_ref);
                self.publish_parked_gauge();
            }
        }
    }

    fn expire_deadlines(&mut self) {
        while let Some(std::cmp::Reverse((at, block_ref, entry_generation))) =
            self.deadlines.peek().cloned()
        {
            if at > Instant::now() {
                break;
            }
            self.deadlines.pop();
            let stale = self
                .entries
                .get(&block_ref)
                .map(|e| e.generation != entry_generation)
                .unwrap_or(true);
            if stale {
                continue;
            }
            let entry = self.remove_entry(&block_ref).expect("checked above");
            self.publish_parked_gauge();
            self.start_fetch(FetchIntent {
                block_ref,
                peer: entry.peer,
                parked_at: entry.parked_at,
            });
        }
    }

    /// One pass over deferred work: each queued intent/submission gets exactly one
    /// dispatch attempt (failures re-queue onto the fresh queue), so a permit-starved
    /// head can never spin the actor. Runs on task completions and the fallback tick
    /// only — never per inbound event — so its cost scales with dispatch opportunities,
    /// not with traffic.
    fn drain_queues(&mut self) {
        let intents = std::mem::take(&mut self.fetch_intents);
        for intent in intents {
            self.start_fetch(intent);
        }
        let submissions = std::mem::take(&mut self.pending_submissions);
        for submission in submissions {
            self.dispatch_submission(submission);
        }
        let queued = &self
            .context
            .metrics
            .node_metrics
            .minimal_block_recovery_queued;
        queued
            .with_label_values(&["fetch_intents"])
            .set(self.fetch_intents.len() as i64);
        queued
            .with_label_values(&["pending_submissions"])
            .set(self.pending_submissions.len() as i64);
    }

    fn remove_entry(&mut self, block_ref: &BlockRef) -> Option<ParkedEntry> {
        let entry = self.entries.remove(block_ref)?;
        self.parked_bytes = self.parked_bytes.saturating_sub(entry.minimal.len());
        if let Some(bytes) = self.parked_bytes_per_peer.get_mut(&entry.peer) {
            *bytes = bytes.saturating_sub(entry.minimal.len());
        }
        if let Some(count) = self.parked_entries_per_peer.get_mut(&entry.peer) {
            *count = count.saturating_sub(1);
        }
        if let Some(waiting) = self.waiters.get_mut(&entry.missing) {
            waiting.retain(|r| r != block_ref);
            if waiting.is_empty() {
                self.waiters.remove(&entry.missing);
            }
        }
        Some(entry)
    }

    fn publish_parked_gauge(&self) {
        self.context
            .metrics
            .node_metrics
            .minimal_block_recovery_parked
            .set(self.entries.len() as i64);
    }

    /// Queues a recovered serialization for submission through the normal
    /// handle_send_block path. Dispatch is bounded (permit-gated) and pending work is
    /// bounded (queue cap): recovery can neither serialize behind one slow verification
    /// nor accumulate unbounded waiting tasks.
    fn submit(
        &mut self,
        block_ref: BlockRef,
        peer: AuthorityIndex,
        serialized: Bytes,
        result: &'static str,
        parked_at: Instant,
    ) {
        self.dispatch_submission(PendingSubmission {
            block_ref,
            peer,
            serialized,
            result,
            attempt: "event",
            parked_at,
        });
    }

    fn dispatch_submission(&mut self, submission: PendingSubmission) {
        let Ok(permit) = self.submission_permits.clone().try_acquire_owned() else {
            if self.pending_submissions.len() >= MAX_PENDING_SUBMISSIONS {
                self.context
                    .metrics
                    .node_metrics
                    .minimal_block_recovery_work_dropped
                    .inc();
                if let Some(shed) = self.pending_submissions.pop_front() {
                    self.in_recovery.remove(&shed.block_ref);
                }
            }
            self.pending_submissions.push_back(submission);
            return;
        };
        let context = self.context.clone();
        let authority_service = self.authority_service.clone();
        self.tasks.spawn(async move {
            let _permit = permit;
            let block_ref = submission.block_ref;
            let Some(authority_service) = authority_service.upgrade() else {
                return TaskResult::SubmitDone(block_ref);
            };
            let peer_hostname = context
                .committee
                .authority(submission.peer)
                .hostname
                .as_str();
            let block = ExtendedSerializedBlock {
                block: submission.serialized,
                excluded_ancestors: vec![],
                minimal: None,
            };
            let outcome = match authority_service
                .handle_send_block(submission.peer, block)
                .await
            {
                Ok(()) => submission.result,
                Err(_) => "rejected",
            };
            let node_metrics = &context.metrics.node_metrics;
            node_metrics
                .minimal_block_recovery
                .with_label_values(&[peer_hostname, outcome, submission.attempt])
                .inc();
            node_metrics
                .minimal_block_recovery_latency
                .observe(submission.parked_at.elapsed().as_secs_f64());
            TaskResult::SubmitDone(block_ref)
        });
    }

    /// Fetches the full block from its author and verifies it against the claimed
    /// digest; bounded by global and per-peer permits, with a bounded byte-free intent
    /// queue when permits are dry. The task ends at verification — its permits bound
    /// network occupancy only — and the actor dispatches the result through the
    /// separately bounded submission stage.
    fn start_fetch(&mut self, intent: FetchIntent) {
        let peer_permits = self
            .per_peer_fetch_permits
            .entry(intent.peer)
            .or_insert_with(|| Arc::new(Semaphore::new(MAX_FETCHES_PER_PEER)))
            .clone();
        let (Ok(global), Ok(per_peer)) = (
            self.fetch_permits.clone().try_acquire_owned(),
            peer_permits.try_acquire_owned(),
        ) else {
            if self.fetch_intents.len() >= MAX_FETCH_INTENTS {
                self.context
                    .metrics
                    .node_metrics
                    .minimal_block_recovery_work_dropped
                    .inc();
                if let Some(dropped) = self.fetch_intents.pop_front() {
                    self.in_recovery.remove(&dropped.block_ref);
                }
            }
            self.fetch_intents.push_back(intent);
            return;
        };
        let context = self.context.clone();
        let network_client = self.network_client.clone();
        self.tasks.spawn(async move {
            let _global = global;
            let _per_peer = per_peer;
            let FetchIntent {
                block_ref,
                peer,
                parked_at,
            } = intent;
            let peer_hostname = context.committee.authority(peer).hostname.as_str();
            let recovery_metric = &context.metrics.node_metrics.minimal_block_recovery;
            let serialized = match network_client
                .fetch_blocks(peer, vec![block_ref], vec![], false, RECOVERY_FETCH_TIMEOUT)
                .await
            {
                Ok(blocks) => {
                    let found = blocks
                        .into_iter()
                        .find(|bytes| VerifiedBlock::compute_digest(bytes) == block_ref.digest);
                    if found.is_none() {
                        // The author could not produce its own claimed block: a peer
                        // fault.
                        recovery_metric
                            .with_label_values(&[peer_hostname, "digest_mismatch", "fetch"])
                            .inc();
                    }
                    found
                }
                Err(e) => {
                    debug!(
                        "Recovery fetch of {} from peer {} failed: {}",
                        block_ref, peer, e
                    );
                    recovery_metric
                        .with_label_values(&[peer_hostname, "fetch_failed", "fetch"])
                        .inc();
                    None
                }
            };
            TaskResult::FetchDone {
                block_ref,
                peer,
                serialized,
                parked_at,
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use consensus_config::AuthorityIndex;
    use consensus_types::block::BlockDigest;

    use super::*;

    fn digest(byte: u8) -> BlockDigest {
        BlockDigest([byte; 32])
    }

    fn slot(round: Round, authority: u32) -> Slot {
        Slot::new(round, AuthorityIndex::new_for_test(authority))
    }

    #[tokio::test]
    async fn hint_cache_caps_per_slot_and_deduplicates() {
        let cache = HintCache::new(512, 4);
        assert_eq!(cache.insert(slot(5, 0), digest(1)), HintInsert::Inserted);
        assert_eq!(cache.insert(slot(5, 0), digest(1)), HintInsert::Duplicate);
        assert_eq!(cache.insert(slot(5, 0), digest(2)), HintInsert::Inserted);
        assert_eq!(cache.insert(slot(5, 0), digest(3)), HintInsert::Inserted);
        assert_eq!(cache.insert(slot(5, 0), digest(4)), HintInsert::SlotFull);
        assert_eq!(cache.candidates(slot(5, 0)).len(), MAX_HINTS_PER_SLOT);
    }

    #[tokio::test]
    async fn hint_cache_caps_per_authority() {
        let cache = HintCache::new(1 << 20, 4);
        for round in 0..MAX_HINTS_PER_AUTHORITY as Round {
            assert_eq!(
                cache.insert(slot(round, 1), digest((round % 251) as u8)),
                HintInsert::Inserted
            );
        }
        assert_eq!(
            cache.insert(slot(1 << 19, 1), digest(9)),
            HintInsert::AuthorityFull
        );
        // A different authority is unaffected.
        assert_eq!(cache.insert(slot(7, 2), digest(9)), HintInsert::Inserted);
    }

    #[tokio::test]
    async fn hint_cache_horizon_advances_only_from_accepted_rounds() {
        let cache = HintCache::new(100, 4);
        assert_eq!(cache.insert(slot(10, 0), digest(1)), HintInsert::Inserted);
        // A received (unverified) future round must not move the horizon; only
        // advance_horizon (accepted) evicts.
        assert_eq!(cache.insert(slot(250, 0), digest(2)), HintInsert::Inserted);
        assert_eq!(cache.candidates(slot(10, 0)), vec![digest(1)]);
        // Accepted progress evicts slots below the retained window and frees their
        // per-authority budget; hints within the window survive.
        cache.advance_horizon(200);
        assert!(cache.candidates(slot(10, 0)).is_empty());
        assert_eq!(cache.candidates(slot(250, 0)), vec![digest(2)]);
        // Hints below the accepted horizon window are refused outright.
        assert_eq!(
            cache.insert(slot(50, 0), digest(3)),
            HintInsert::BelowHorizon
        );
        assert_eq!(cache.insert(slot(150, 0), digest(3)), HintInsert::Inserted);
    }

    /// A peer must not be able to park its per-authority hint quota in the far future,
    /// where oldest-first eviction never reaches it: inserts beyond the horizon's
    /// future bound are refused, and startup inserts (accepted while no horizon
    /// exists) are pruned to the same bound when the first horizon is established.
    #[tokio::test]
    async fn hint_cache_rejects_far_future_hints() {
        let cache = HintCache::new(100, 4);
        // Startup: no horizon yet — fill the authority's ENTIRE quota far in the
        // future, past what any horizon will reach.
        for round in 0..MAX_HINTS_PER_AUTHORITY as Round {
            assert_eq!(
                cache.insert(slot(1_000_000 + round, 1), digest((round % 251) as u8)),
                HintInsert::Inserted
            );
        }
        assert_eq!(
            cache.insert(slot(999_999, 1), digest(9)),
            HintInsert::AuthorityFull
        );
        cache.advance_horizon(500);
        // The first horizon prunes what insert would now refuse AND frees the quota;
        // a leaked count would leave this authority pinned at AuthorityFull forever.
        assert!(cache.candidates(slot(1_000_000, 1)).is_empty());
        // Within horizon + retained slack: legitimate pipeline lag.
        assert_eq!(cache.insert(slot(600, 1), digest(2)), HintInsert::Inserted);
        // Beyond it: refused, and nothing already retained is disturbed.
        assert_eq!(
            cache.insert(slot(601, 1), digest(3)),
            HintInsert::AboveHorizon
        );
        assert_eq!(cache.candidates(slot(600, 1)), vec![digest(2)]);
    }

    /// A full cache must admit FRESH hints by evicting its oldest slots, never reject
    /// the new in favor of the stale (measured live as decaying hint wakes).
    #[tokio::test]
    async fn hint_cache_full_evicts_oldest_not_newest() {
        // retained_rounds x committee = 4 x 1 -> capacity clamps to the 1024 floor.
        // Fill across two authorities (512 each) so the per-authority cap stays clear
        // and the GLOBAL eviction path is what this test exercises.
        let cache = HintCache::new(4, 1);
        for round in 0..1024u32 {
            assert_eq!(
                cache.insert(slot(round, round % 2), digest((round % 251) as u8)),
                HintInsert::Inserted,
                "round {round}"
            );
        }
        // Full. A FRESH hint evicts the oldest slot and lands.
        assert_eq!(cache.insert(slot(5000, 0), digest(7)), HintInsert::Inserted);
        assert!(cache.candidates(slot(0, 0)).is_empty(), "oldest evicted");
        assert_eq!(cache.candidates(slot(5000, 0)), vec![digest(7)]);
        // An incoming hint OLDER than everything retained is the one refused.
        assert_eq!(cache.insert(slot(0, 0), digest(9)), HintInsert::GlobalFull);
        // A rejected hint must not evict: a duplicate of a retained entry, sent while
        // the cache is full, leaves the working set untouched (adversarial grind guard).
        let surviving_oldest = slot(1, 1);
        let retained = cache.candidates(surviving_oldest);
        assert!(!retained.is_empty());
        assert_eq!(
            cache.insert(slot(5000, 0), digest(7)),
            HintInsert::Duplicate
        );
        assert_eq!(cache.candidates(surviving_oldest), retained);
    }
}
