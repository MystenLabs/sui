// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Event-driven recovery for minimal blocks that cannot be inflated at receipt.
//!
//! Two cooperating pieces close the gap between minimal-block inflation and the
//! baseline full-block pipeline:
//!
//! - [`HintCache`]: digests learned at *receipt time*, before verification or
//!   acceptance. A full-form block's digest is a hash of the bytes that just arrived; a
//!   minimal block carries its own claimed digest. Feeding these to the inflation
//!   resolver lets a child reconstruct the moment its ancestor's bytes exist locally —
//!   the same wall-clock point at which the baseline design could first make use of the
//!   ancestor — and verify in parallel with the ancestor's own verification. Hints are
//!   untrusted: the child's claimed-digest check remains the cryptographic gate, so a
//!   wrong hint costs a bounded reconstruction attempt, never a wrong acceptance.
//!
//! - [`RecoveryManager`]: an actor that parks un-inflatable minimal blocks keyed by
//!   their first missing slot and re-attempts inflation when that slot is heard from —
//!   either a receipt-time hint (subscription streams) or Core's accepted-block
//!   broadcast (blocks that arrive via sync/fetch and never cross our streams). A
//!   deadline escalates stragglers to the digest-verified sender fetch, the liveness
//!   floor that converts slot-level knowledge into an exact, fetchable ref. All parking
//!   state is owned by the actor task, so hint-drain and park ordering cannot race: a
//!   hint is published to the cache *before* its wake command is sent, and a park
//!   re-attempts inflation *after* registering, on the same task that drains waiters.
//!
//! Measured motivation (private testnet, ~125 validators): ~48% of received minimal
//! blocks race an in-flight ancestor; natural arrival lag is p50 ≈ 167 ms / p95 ≈ 473 ms.
//! Timer-based recovery either fetches too much (bandwidth tax) or waits past arrival
//! (round-rate tax); waking on the actual arrival events is the only schedule that
//! matches the distribution.

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
/// Hard global bound on retained hints, independent of committee size.
const MAX_HINTS_TOTAL: usize = 32_768;
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
/// Timeout for one sender fetch (off-stream; nothing waits on it).
pub(crate) const RECOVERY_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Receipt-time digest hints: slot -> ordered digests, untrusted, bounded.
///
/// Callers must validate hint identity BEFORE insertion: the hinted author must equal
/// the authenticated stream peer and the epoch must be current, so a peer can only ever
/// hint its own slots. The cache never advances its eviction horizon from received
/// (unverified) rounds — only from accepted rounds — so a Byzantine future-round block
/// cannot flush it.
pub(crate) struct HintCache {
    inner: Mutex<HintCacheInner>,
    /// Slots older than `horizon - retained_rounds` are evicted.
    retained_rounds: Round,
}

struct HintCacheInner {
    hints: BTreeMap<Slot, Vec<BlockDigest>>,
    per_authority: HashMap<AuthorityIndex, usize>,
    total: usize,
    horizon: Round,
}

impl HintCache {
    pub(crate) fn new(retained_rounds: Round) -> Self {
        Self {
            inner: Mutex::new(HintCacheInner {
                hints: BTreeMap::new(),
                per_authority: HashMap::new(),
                total: 0,
                horizon: 0,
            }),
            retained_rounds,
        }
    }

    /// Inserts a validated hint. Returns false when capacity or horizon rejected it.
    pub(crate) fn insert(&self, slot: Slot, digest: BlockDigest) -> bool {
        let mut inner = self.inner.lock();
        if slot.round.saturating_add(self.retained_rounds) < inner.horizon
            || inner.total >= MAX_HINTS_TOTAL
        {
            return false;
        }
        let count = inner.per_authority.entry(slot.authority).or_insert(0);
        if *count >= MAX_HINTS_PER_AUTHORITY {
            return false;
        }
        *count += 1;
        let count_rollback = slot.authority;
        let digests = inner.hints.entry(slot).or_default();
        if digests.len() >= MAX_HINTS_PER_SLOT || digests.contains(&digest) {
            *inner
                .per_authority
                .get_mut(&count_rollback)
                .expect("just inserted") -= 1;
            return false;
        }
        digests.push(digest);
        inner.total += 1;
        true
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
        inner.horizon = accepted_round;
        let cutoff = accepted_round.saturating_sub(self.retained_rounds);
        let retained = inner.hints.split_off(&Slot::new_for_test(cutoff, 0));
        let evicted = std::mem::replace(&mut inner.hints, retained);
        for (slot, digests) in evicted {
            inner.total = inner.total.saturating_sub(digests.len());
            if let Some(count) = inner.per_authority.get_mut(&slot.authority) {
                *count = count.saturating_sub(digests.len());
            }
        }
    }
}

/// Commands from subscription streams to the manager actor.
pub(crate) enum RecoveryCommand {
    /// A minimal block that failed inflation at receipt: park it.
    Park {
        peer: AuthorityIndex,
        block_ref: BlockRef,
        minimal: Bytes,
        missing: Slot,
    },
    /// A hint for `slot` was just published to the cache: wake its waiters.
    SlotHeard(Slot),
}

struct ParkedEntry {
    peer: AuthorityIndex,
    minimal: Bytes,
    missing: Slot,
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

/// Why a parked entry is being re-attempted; becomes the metric label on success.
#[derive(Clone, Copy)]
enum WakeSource {
    /// At park time or on a receipt-time hint: resolution from bytes-arrival state.
    Hint,
    /// Core's accepted-block broadcast or a lag-triggered rescan.
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
    tasks: tokio::task::JoinSet<()>,
    // Completions from detached tasks; bounded in practice by the permit counts.
    completions_tx: mpsc::UnboundedSender<BlockRef>,
    completions_rx: mpsc::UnboundedReceiver<BlockRef>,
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
        let completions = mpsc::unbounded_channel();
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
            completions_tx: completions.0,
            completions_rx: completions.1,
            generation: 0,
            parked_bytes: 0,
            parked_bytes_per_peer: HashMap::new(),
            parked_entries_per_peer: HashMap::new(),
            fetch_permits: Arc::new(Semaphore::new(MAX_FETCHES)),
            per_peer_fetch_permits: HashMap::new(),
            submission_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_SUBMISSIONS)),
        }
    }

    /// Runs the actor until the command channel closes (subscriber stop) or the task is
    /// aborted.
    pub(crate) async fn run(
        mut self,
        mut commands: mpsc::Receiver<RecoveryCommand>,
        mut accepted_blocks: broadcast::Receiver<VerifiedBlock>,
    ) {
        loop {
            // Deadline heap drives fetch escalation; queued intents/submissions add a
            // near-term fallback wake so freed permits are never waited on for long
            // even if a completion message is missed.
            let mut next_deadline = self
                .deadlines
                .peek()
                .map(|std::cmp::Reverse((at, _, _))| *at)
                .unwrap_or_else(|| Instant::now() + Duration::from_secs(3600));
            if !self.fetch_intents.is_empty() || !self.pending_submissions.is_empty() {
                next_deadline = next_deadline.min(Instant::now() + Duration::from_millis(200));
            }
            tokio::select! {
                command = commands.recv() => {
                    let Some(command) = command else {
                        return;
                    };
                    match command {
                        RecoveryCommand::Park { peer, block_ref, minimal, missing } => {
                            self.handle_park(peer, block_ref, minimal, missing);
                        }
                        RecoveryCommand::SlotHeard(slot) => {
                            self.wake_slot(slot, WakeSource::Hint);
                        }
                    }
                }
                accepted = accepted_blocks.recv() => {
                    match accepted {
                        Ok(block) => {
                            self.hint_cache.advance_horizon(block.round());
                            self.wake_slot(
                                Slot::new(block.round(), block.author()),
                                WakeSource::Accepted,
                            );
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            debug!(
                                "Recovery manager lagged {skipped} accepted blocks; rescanning"
                            );
                            self.context
                                .metrics
                                .node_metrics
                                .minimal_block_recovery_rescans
                                .inc();
                            let refs: Vec<BlockRef> = self.entries.keys().cloned().collect();
                            for block_ref in refs {
                                self.reattempt(block_ref, WakeSource::Accepted);
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            return;
                        }
                    }
                }
                completed = self.completions_rx.recv() => {
                    if let Some(block_ref) = completed {
                        self.in_recovery.remove(&block_ref);
                    }
                }
                // Reap finished detached tasks so the JoinSet stays bounded by the
                // permit counts; a panic in one is a bug and is propagated, matching
                // the crate's task conventions.
                Some(result) = self.tasks.join_next(), if !self.tasks.is_empty() => {
                    if let Err(e) = result
                        && e.is_panic()
                    {
                        std::panic::resume_unwind(e.into_panic());
                    }
                }
                _ = tokio::time::sleep_until(next_deadline) => {
                    self.expire_deadlines();
                }
            }
            // Deferred work gets one dispatch attempt after EVERY event (a cheap no-op
            // when the queues are empty), so sustained traffic cannot postpone it and
            // completion messages need no special-cased draining.
            self.drain_queues();
        }
    }

    /// Parks a new entry — registering first, then re-attempting inflation on this same
    /// task, so a hint published between the stream's failed inflation and this park
    /// cannot be lost — or resolves it immediately if local state already suffices.
    fn handle_park(
        &mut self,
        peer: AuthorityIndex,
        block_ref: BlockRef,
        minimal: Bytes,
        missing: Slot,
    ) {
        if self.in_recovery.contains(&block_ref) {
            self.context
                .metrics
                .node_metrics
                .minimal_block_recovery_duplicates
                .inc();
            return;
        }
        self.in_recovery.insert(block_ref);
        let over_capacity = self.entries.len() >= MAX_PARKED_ENTRIES
            || self.parked_bytes + minimal.len() > MAX_PARKED_BYTES
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
        self.reattempt(block_ref, WakeSource::Hint);
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
                let generation = &mut self.generation;
                let deadlines = &mut self.deadlines;
                let entry = self.entries.get_mut(&block_ref).expect("present above");
                if entry.missing != next_missing {
                    // Re-key: retire the old-slot registration so rescans cannot
                    // accumulate stale waiter references.
                    let old_slot = entry.missing;
                    entry.missing = next_missing;
                    *generation += 1;
                    entry.generation = *generation;
                    deadlines.push(std::cmp::Reverse((
                        entry.deadline,
                        block_ref,
                        entry.generation,
                    )));
                    if let Some(waiting) = self.waiters.get_mut(&old_slot) {
                        waiting.retain(|r| *r != block_ref);
                        if waiting.is_empty() {
                            self.waiters.remove(&old_slot);
                        }
                    }
                }
                // Push-if-absent: the at-park reattempt and lag rescans revisit entries
                // whose registration already exists.
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
    /// head can never spin the actor.
    fn drain_queues(&mut self) {
        let intents = std::mem::take(&mut self.fetch_intents);
        for intent in intents {
            self.start_fetch(intent);
        }
        let submissions = std::mem::take(&mut self.pending_submissions);
        for submission in submissions {
            self.dispatch_submission(submission);
        }
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
                    .minimal_block_recovery_intents_dropped
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
        let completions = self.completions_tx.clone();
        self.tasks.spawn(async move {
            let _permit = permit;
            let block_ref = submission.block_ref;
            let Some(authority_service) = authority_service.upgrade() else {
                let _ = completions.send(block_ref);
                return;
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
            let _ = completions.send(block_ref);
        });
    }

    /// Fetches the full block from its author (digest-verified) and submits it; bounded
    /// by global and per-peer permits, with a bounded byte-free intent queue when
    /// permits are dry. Completion is reported back to the actor, which releases the
    /// ref's recovery-dedup slot and drains deferred work.
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
                    .minimal_block_recovery_intents_dropped
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
        let authority_service = self.authority_service.clone();
        let submission_permits = self.submission_permits.clone();
        let completions = self.completions_tx.clone();
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
            let result = network_client
                .fetch_blocks(peer, vec![block_ref], vec![], false, RECOVERY_FETCH_TIMEOUT)
                .await;
            let serialized = match result {
                Ok(blocks) => blocks
                    .into_iter()
                    .find(|bytes| VerifiedBlock::compute_digest(bytes) == block_ref.digest),
                Err(e) => {
                    debug!(
                        "Recovery fetch of {} from peer {} failed: {}",
                        block_ref, peer, e
                    );
                    recovery_metric
                        .with_label_values(&[peer_hostname, "fetch_failed", "fetch"])
                        .inc();
                    let _ = completions.send(block_ref);
                    return;
                }
            };
            let Some(serialized) = serialized else {
                // The author could not produce its own claimed block: a peer fault.
                recovery_metric
                    .with_label_values(&[peer_hostname, "digest_mismatch", "fetch"])
                    .inc();
                let _ = completions.send(block_ref);
                return;
            };
            let submission = submission_permits.acquire_owned().await;
            if let (Ok(_permit), Some(authority_service)) =
                (submission, authority_service.upgrade())
            {
                let block = ExtendedSerializedBlock {
                    block: serialized,
                    excluded_ancestors: vec![],
                    minimal: None,
                };
                let outcome = match authority_service.handle_send_block(peer, block).await {
                    Ok(()) => "fetch_recovered",
                    Err(_) => "rejected",
                };
                let node_metrics = &context.metrics.node_metrics;
                node_metrics
                    .minimal_block_recovery
                    .with_label_values(&[peer_hostname, outcome, "fetch"])
                    .inc();
                node_metrics
                    .minimal_block_recovery_latency
                    .observe(parked_at.elapsed().as_secs_f64());
            }
            let _ = completions.send(block_ref);
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
        let cache = HintCache::new(512);
        assert!(cache.insert(slot(5, 0), digest(1)));
        assert!(!cache.insert(slot(5, 0), digest(1)), "duplicate rejected");
        assert!(cache.insert(slot(5, 0), digest(2)));
        assert!(cache.insert(slot(5, 0), digest(3)));
        assert!(
            !cache.insert(slot(5, 0), digest(4)),
            "per-slot cap enforced"
        );
        assert_eq!(cache.candidates(slot(5, 0)).len(), MAX_HINTS_PER_SLOT);
    }

    #[tokio::test]
    async fn hint_cache_caps_per_authority() {
        let cache = HintCache::new(1 << 20);
        for round in 0..MAX_HINTS_PER_AUTHORITY as Round {
            assert!(cache.insert(slot(round, 1), digest((round % 251) as u8)));
        }
        assert!(
            !cache.insert(slot(1 << 19, 1), digest(9)),
            "per-authority cap enforced"
        );
        // A different authority is unaffected.
        assert!(cache.insert(slot(7, 2), digest(9)));
    }

    #[tokio::test]
    async fn hint_cache_horizon_advances_only_from_accepted_rounds() {
        let cache = HintCache::new(100);
        assert!(cache.insert(slot(10, 0), digest(1)));
        // A far-future hint (e.g. from a Byzantine block) is stored but must not evict
        // the useful horizon — eviction advances only via advance_horizon (accepted).
        assert!(cache.insert(slot(1_000_000, 0), digest(2)));
        assert_eq!(cache.candidates(slot(10, 0)), vec![digest(1)]);
        // Accepted progress evicts slots below the retained window and frees their
        // per-authority budget.
        cache.advance_horizon(200);
        assert!(cache.candidates(slot(10, 0)).is_empty());
        assert_eq!(cache.candidates(slot(1_000_000, 0)), vec![digest(2)]);
        // Hints below the accepted horizon window are refused outright.
        assert!(!cache.insert(slot(50, 0), digest(3)));
        assert!(cache.insert(slot(150, 0), digest(3)));
    }
}
