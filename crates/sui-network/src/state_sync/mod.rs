// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Peer-to-peer data synchronization of checkpoints.
//!
//! This StateSync module is responsible for the synchronization and dissemination of checkpoints
//! and the transactions, and their effects, contained within. This module is *not* responsible for
//! the execution of the transactions included in a checkpoint, that process is left to another
//! component in the system.
//!
//! # High-level Overview of StateSync
//!
//! StateSync discovers new checkpoints via a few different sources:
//! 1. If this node is a Validator, checkpoints will be produced via consensus at which point
//!    consensus can notify state-sync of the new checkpoint via [Handle::send_checkpoint].
//! 2. A peer notifies us of the latest checkpoint which they have synchronized. State-Sync will
//!    also periodically query its peers to discover what their latest checkpoint is.
//!
//! We keep track of two different watermarks:
//! * highest_verified_checkpoint - This is the highest checkpoint header that we've locally
//!   verified. This indicated that we have in our persistent store (and have verified) all
//!   checkpoint headers up to and including this value.
//! * highest_synced_checkpoint - This is the highest checkpoint that we've fully synchronized,
//!   meaning we've downloaded and have in our persistent stores all of the transactions, and their
//!   effects (but not the objects), for all checkpoints up to and including this point. This is
//!   the watermark that is shared with other peers, either via notification or when they query for
//!   our latest checkpoint, and is intended to be used as a guarantee of data availability.
//!
//! The `PeerHeights` struct is used to track the highest_synced_checkpoint watermark for all of
//! our peers.
//!
//! When a new checkpoint is discovered, and we've determined that it is higher than our
//! highest_verified_checkpoint, then StateSync will kick off a task to synchronize and verify all
//! checkpoints between our highest_synced_checkpoint and the newly discovered checkpoint. This
//! process is done by querying one of our peers for the checkpoints we're missing (using the
//! `PeerHeights` struct as a way to intelligently select which peers have the data available for
//! us to query) at which point we will locally verify the signatures on the checkpoint header with
//! the appropriate committee (based on the epoch). As checkpoints are verified, the
//! highest_synced_checkpoint watermark will be ratcheted up.
//!
//! Once we've ratcheted up our highest_verified_checkpoint, and if it is higher than
//! highest_synced_checkpoint, StateSync will then kick off a task to synchronize the contents of
//! all of the checkpoints from highest_synced_checkpoint..=highest_verified_checkpoint. After the
//! contents of each checkpoint is fully downloaded, StateSync will update our
//! highest_synced_checkpoint watermark and send out a notification on a broadcast channel
//! indicating that a new checkpoint has been fully downloaded. Notifications on this broadcast
//! channel will always be made in order. StateSync will also send out a notification to its peers
//! of the newly synchronized checkpoint so that it can help other peers synchronize.

use anemo::{PeerId, Request, Response, Result, types::PeerEvent};
use futures::{
    FutureExt, StreamExt,
    stream::{FuturesOrdered, FuturesUnordered},
};
use rand::Rng;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use sui_config::p2p::StateSyncConfig;
use sui_types::{
    committee::Committee,
    digests::CheckpointDigest,
    messages_checkpoint::{
        CertifiedCheckpointSummary as Checkpoint, CheckpointSequenceNumber, EndOfEpochData,
        VerifiedCheckpoint, VerifiedCheckpointContents, VersionedFullCheckpointContents,
    },
    storage::WriteStore,
};
use tap::Pipe;
use tokio::sync::oneshot;
use tokio::{
    sync::{broadcast, mpsc, watch},
    task::{AbortHandle, JoinSet},
};
use tracing::{debug, info, instrument, trace, warn};

#[allow(clippy::result_large_err)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/sui.StateSync.rs"));
}
mod builder;
mod metrics;
mod server;
#[cfg(test)]
mod tests;
mod worker;

use self::{
    metrics::{ContentRequestOutcome, HedgeEvent, Metrics},
    server::CheckpointContentsDownloadLimitLayer,
};
use crate::state_sync::worker::process_archive_checkpoint;
pub use builder::{Builder, UnstartedStateSync};
pub use generated::{
    state_sync_client::StateSyncClient,
    state_sync_server::{StateSync, StateSyncServer},
};
pub use server::GetCheckpointAvailabilityResponse;
pub use server::GetCheckpointSummaryRequest;
use sui_config::node::ArchiveReaderConfig;
use sui_storage::object_store::util::{build_object_store, fetch_checkpoint};
use sui_storage::verify_checkpoint;

/// A handle to the StateSync subsystem.
///
/// This handle can be cloned and shared. Once all copies of a StateSync system's Handle have been
/// dropped, the StateSync system will be gracefully shutdown.
#[derive(Clone, Debug)]
pub struct Handle {
    sender: mpsc::Sender<StateSyncMessage>,
    checkpoint_event_sender: broadcast::Sender<VerifiedCheckpoint>,
    metrics: Metrics,
}

impl Handle {
    /// Send a newly minted checkpoint from Consensus to StateSync so that it can be disseminated
    /// to other nodes on the network.
    ///
    /// # Invariant
    ///
    /// Consensus must only notify StateSync of new checkpoints that have been fully committed to
    /// persistent storage. This includes CheckpointContents and all Transactions and
    /// TransactionEffects included therein.
    pub async fn send_checkpoint(&self, checkpoint: VerifiedCheckpoint) {
        self.sender
            .send(StateSyncMessage::VerifiedCheckpoint(Box::new(checkpoint)))
            .await
            .unwrap()
    }

    /// Subscribe to the stream of checkpoints that have been fully synchronized and downloaded.
    pub fn subscribe_to_synced_checkpoints(&self) -> broadcast::Receiver<VerifiedCheckpoint> {
        self.checkpoint_event_sender.subscribe()
    }

    /// Returns the number of peers reported due to consistent state sync failures.
    pub fn get_peers_reported_for_failure(&self) -> u64 {
        self.metrics.get_peers_reported_for_failure()
    }
}

pub(super) fn compute_adaptive_timeout(
    tx_count: u64,
    min_timeout: Duration,
    max_timeout: Duration,
) -> Duration {
    const MAX_TRANSACTIONS_PER_CHECKPOINT: u64 = 10_000;
    const JITTER_FRACTION: f64 = 0.1;

    let max_timeout = max_timeout.max(min_timeout);
    let ratio = (tx_count as f64 / MAX_TRANSACTIONS_PER_CHECKPOINT as f64).min(1.0);
    let extra = Duration::from_secs_f64((max_timeout - min_timeout).as_secs_f64() * ratio);
    let base = min_timeout + extra;

    let jitter_range = base.as_secs_f64() * JITTER_FRACTION;
    let jitter = rand::thread_rng().gen_range(-jitter_range..jitter_range);
    Duration::from_secs_f64((base.as_secs_f64() + jitter).max(min_timeout.as_secs_f64()))
}

/// How long one checkpoint contents request to a peer took, as far as is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ContentResponseTime {
    /// The peer answered after this long.
    Answered(Duration),
    /// The request was cancelled after running this long because another peer answered first,
    /// so the peer would have taken longer.
    LongerThan(Duration),
}

/// Near the release frontier, how a contents request that is not a hedge relates to its
/// peer's slow flag when it is sent (see [`SLOW_PEER_PROBE_INTERVAL`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SlowPeerRequest {
    /// The peer was not slow from this node.
    PeerNotSlow,
    /// The peer was slow, and the request is a slow-peer probe.
    Probe,
    /// The peer was slow, and the request is not a probe.
    NotProbe,
}

impl SlowPeerRequest {
    /// Whether the request's outcome is recorded as a response-time sample (see
    /// `PeerScore::record_content_response_time`). While a peer is slow, only probes are.
    /// Other requests to it are hedged right away once a peer that isn't slow has the
    /// checkpoint, so their slow outcomes are cut short before the peer's slow-response
    /// threshold while their fast answers still arrive. Recording them would make the peer look
    /// less slow than it is.
    pub(super) fn records_response_time(self) -> bool {
        self != Self::NotProbe
    }
}

#[cfg_attr(test, derive(Debug))]
pub(super) struct PeerScore {
    successes: VecDeque<(Instant, u64 /* response_size_bytes */, Duration)>,
    /// Requests cancelled because another peer delivered the same contents first, with the time
    /// they had run. That time is a lower bound on the response time, so a sample is kept only
    /// if it lowers the throughput estimate (see `record_cancelled`). These samples count as
    /// neither successes nor failures.
    cancelled: VecDeque<(Instant, u64 /* response_size_bytes */, Duration)>,
    /// How long recent checkpoint contents requests that were not hedges took, oldest first.
    /// Used to decide whether requests to this peer should be hedged sooner (see
    /// `slow_peer_hedge_delay`).
    content_response_times: VecDeque<(Instant, ContentResponseTime)>,
    /// Contents requests that were not hedges sent to this peer while it was slow since the last
    /// slow-peer probe (see [`SLOW_PEER_PROBE_INTERVAL`]).
    requests_since_probe: u32,
    failures: VecDeque<Instant>,
    window: Duration,
    failure_rate: f64,
    failing_since: Option<Instant>,
}

impl PeerScore {
    const MAX_SAMPLES: usize = 20;
    const MIN_SAMPLES_FOR_FAILURE: usize = 10;
    /// A contents response is slow when it takes longer than the peer's fastest recent response
    /// times this factor, or plus [`Self::SLOW_RESPONSE_MIN_MARGIN`], whichever is more.
    const SLOW_RESPONSE_FACTOR: f64 = 1.25;
    const SLOW_RESPONSE_MIN_MARGIN: Duration = Duration::from_millis(20);
    /// A peer is slow when at least this share of its recent contents responses were slow.
    const SLOW_PEER_SHARE: f64 = 0.25;
    const MIN_SAMPLES_FOR_SLOW: usize = 10;

    pub(super) fn new(window: Duration, failure_rate: f64) -> Self {
        Self {
            successes: VecDeque::new(),
            cancelled: VecDeque::new(),
            content_response_times: VecDeque::new(),
            requests_since_probe: 0,
            failures: VecDeque::new(),
            window,
            failure_rate,
            failing_since: None,
        }
    }

    pub(super) fn record_success(&mut self, size: u64, response_time: Duration) {
        let now = Instant::now();
        self.successes.push_back((now, size, response_time));
        while self.successes.len() > Self::MAX_SAMPLES {
            self.successes.pop_front();
        }
        // Note: a single success does NOT clear `failing_since`. Whether the peer is
        // "consistently failing" is driven by the windowed failure RATE (see
        // `update_failing_state`), so an occasional success while the rate stays high (e.g.
        // a brief reconnect during connection churn) must not keep resetting the clock and
        // prevent the peer from ever being reported.
    }

    /// Records a request that was cancelled after `elapsed` because another peer delivered `size`
    /// bytes first. `size / elapsed` is only an upper bound on this peer's throughput, so the
    /// sample is kept only when it is below the current estimate, which it then lowers. Without
    /// an estimate the sample carries no usable information and is dropped.
    pub(super) fn record_cancelled(&mut self, size: u64, elapsed: Duration) {
        let Some(current) = self.effective_throughput() else {
            return;
        };
        if elapsed.is_zero() || size as f64 / elapsed.as_secs_f64() >= current {
            return;
        }
        self.cancelled.push_back((Instant::now(), size, elapsed));
        while self.cancelled.len() > Self::MAX_SAMPLES {
            self.cancelled.pop_front();
        }
    }

    /// Records how long a checkpoint contents request that was not a hedge took. Hedge requests
    /// are left out: a hedge is cancelled whenever the request it hedged answers first, however
    /// short a time it has run, so how often hedges are cut short says nothing about the peer.
    /// Near the release frontier, requests sent while the peer was slow that were not slow-peer
    /// probes are left out too (see [`SlowPeerRequest::records_response_time`]).
    /// A request that was not a hedge and is cancelled after its peer's slow-response threshold
    /// was slow. One cancelled sooner, for example by a hedge to a closer peer, might still have
    /// answered in time, so `slow_peer_hedge_delay` counts it neither way. With
    /// `drop_uncountable`, such a sample is dropped when the peer already has a slow-response
    /// threshold, so that it doesn't take the place of a sample that counts; without a threshold
    /// yet, it is kept, because the threshold it is compared against is only known once the peer
    /// answers.
    pub(super) fn record_content_response_time(
        &mut self,
        response_time: ContentResponseTime,
        drop_uncountable: bool,
    ) {
        if drop_uncountable
            && let ContentResponseTime::LongerThan(elapsed) = response_time
            && self
                .slow_response_threshold()
                .is_some_and(|threshold| elapsed < threshold)
        {
            return;
        }
        self.content_response_times
            .push_back((Instant::now(), response_time));
        while self.content_response_times.len() > Self::MAX_SAMPLES {
            self.content_response_times.pop_front();
        }
    }

    /// The fastest answer among this peer's recent contents requests that were not hedges.
    pub(super) fn fastest_recent_content_answer(&self) -> Option<Duration> {
        self.recent_content_response_times()
            .filter_map(|response_time| match response_time {
                ContentResponseTime::Answered(elapsed) => Some(elapsed),
                ContentResponseTime::LongerThan(_) => None,
            })
            .min()
    }

    fn recent_content_response_times(&self) -> impl Iterator<Item = ContentResponseTime> + '_ {
        let now = Instant::now();
        self.content_response_times
            .iter()
            .filter(move |(ts, _)| now.duration_since(*ts) < self.window)
            .map(|(_, response_time)| *response_time)
    }

    /// The peer's fastest recent contents response times [`Self::SLOW_RESPONSE_FACTOR`], or plus
    /// [`Self::SLOW_RESPONSE_MIN_MARGIN`], whichever is more. Responses that take longer are
    /// slow. `None` if the peer has no recent answers.
    fn slow_response_threshold(&self) -> Option<Duration> {
        let fastest = self.fastest_recent_content_answer()?;
        Some(
            fastest
                .mul_f64(Self::SLOW_RESPONSE_FACTOR)
                .max(fastest + Self::SLOW_RESPONSE_MIN_MARGIN),
        )
    }

    /// If this peer is slow, its slow-response threshold (see `slow_response_threshold`), after
    /// which requests to it should be hedged. The peer is slow when at least
    /// [`Self::SLOW_PEER_SHARE`] of its recent responses took longer than the threshold. Returns
    /// `None` if the peer isn't slow, or if it has too few recent samples to tell.
    pub(super) fn slow_peer_hedge_delay(&self) -> Option<Duration> {
        let threshold = self.slow_response_threshold()?;
        let mut slow = 0;
        let mut known = 0;
        for response_time in self.recent_content_response_times() {
            match response_time {
                ContentResponseTime::Answered(elapsed) => {
                    known += 1;
                    if elapsed > threshold {
                        slow += 1;
                    }
                }
                ContentResponseTime::LongerThan(elapsed) if elapsed >= threshold => {
                    known += 1;
                    slow += 1;
                }
                // Cancelled before the threshold: whether the response would have been slow is
                // unknown. Near the release frontier, such samples are only stored while the
                // peer has no threshold yet, or under an earlier, lower threshold.
                ContentResponseTime::LongerThan(_) => {}
            }
        }
        (known >= Self::MIN_SAMPLES_FOR_SLOW && slow as f64 >= Self::SLOW_PEER_SHARE * known as f64)
            .then_some(threshold)
    }

    /// Called when a contents request that is not a hedge is about to be sent to this peer near
    /// the release frontier. Every [`SLOW_PEER_PROBE_INTERVAL`]th such request sent while
    /// the peer is slow is a slow-peer probe. Requests sent while the peer isn't slow are not
    /// counted.
    pub(super) fn classify_new_request(&mut self) -> SlowPeerRequest {
        if self.slow_peer_hedge_delay().is_none() {
            return SlowPeerRequest::PeerNotSlow;
        }
        self.requests_since_probe += 1;
        if self.requests_since_probe < SLOW_PEER_PROBE_INTERVAL {
            return SlowPeerRequest::NotProbe;
        }
        self.requests_since_probe = 0;
        SlowPeerRequest::Probe
    }

    pub(super) fn record_failure(&mut self) {
        let now = Instant::now();
        self.failures.push_back(now);
        while self.failures.len() > Self::MAX_SAMPLES {
            self.failures.pop_front();
        }
    }

    pub(super) fn is_failing(&self) -> bool {
        let now = Instant::now();
        let recent_failures = self
            .failures
            .iter()
            .filter(|ts| now.duration_since(**ts) < self.window)
            .count();
        let recent_successes = self
            .successes
            .iter()
            .filter(|(ts, _, _)| now.duration_since(*ts) < self.window)
            .count();

        let total = recent_failures + recent_successes;
        if total < Self::MIN_SAMPLES_FOR_FAILURE {
            return false;
        }

        let rate = recent_failures as f64 / total as f64;
        rate >= self.failure_rate
    }

    pub(super) fn update_failing_state(&mut self) {
        if self.is_failing() {
            // Start the consistently-failing clock when the windowed failure rate first
            // crosses the threshold, and let it accumulate while the rate stays high.
            if self.failing_since.is_none() {
                self.failing_since = Some(Instant::now());
            }
        } else {
            // The windowed failure rate dropped below the threshold: the peer is no longer
            // failing, so clear the clock. Only an actual drop in the rate resets it.
            self.failing_since = None;
        }
    }

    pub(super) fn reset_failing_since(&mut self) {
        self.failing_since = None;
    }

    pub(super) fn consistently_failing(&self, threshold: Duration) -> bool {
        match self.failing_since {
            Some(since) => since.elapsed() >= threshold,
            None => false,
        }
    }

    pub(super) fn effective_throughput(&self) -> Option<f64> {
        let now = Instant::now();
        let (total_size, total_time) = self
            .successes
            .iter()
            .chain(&self.cancelled)
            .filter(|(ts, _, _)| now.duration_since(*ts) < self.window)
            .fold((0u64, Duration::ZERO), |(size, time), (_, s, d)| {
                (size + s, time + *d)
            });

        if total_size == 0 {
            return None;
        }

        if total_time.is_zero() {
            return None;
        }

        Some(total_size as f64 / total_time.as_secs_f64())
    }
}

struct PeerHeights {
    /// Table used to track the highest checkpoint for each of our peers.
    peers: HashMap<PeerId, PeerStateSyncInfo>,
    unprocessed_checkpoints: HashMap<CheckpointDigest, Checkpoint>,
    sequence_number_to_digest: HashMap<CheckpointSequenceNumber, CheckpointDigest>,
    scores: HashMap<PeerId, PeerScore>,

    wait_interval_when_no_peer_to_sync_content: Duration,
    peer_scoring_window: Duration,
    peer_failure_rate: f64,
    checkpoint_content_timeout_min: Duration,
    checkpoint_content_timeout_max: Duration,
    exploration_probability: f64,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct PeerStateSyncInfo {
    /// The digest of the Peer's genesis checkpoint.
    genesis_checkpoint_digest: CheckpointDigest,
    /// Indicates if this Peer is on the same chain as us.
    on_same_chain_as_us: bool,
    /// Highest checkpoint sequence number we know of for this Peer.
    height: CheckpointSequenceNumber,
    /// lowest available checkpoint watermark for this Peer.
    /// This defaults to 0 for now.
    lowest: CheckpointSequenceNumber,
}

impl PeerStateSyncInfo {
    /// Whether the peer has announced `sequence_number` and still has its contents.
    fn has_contents(&self, sequence_number: CheckpointSequenceNumber) -> bool {
        self.height >= sequence_number && self.lowest <= sequence_number
    }
}

impl PeerHeights {
    pub fn highest_known_checkpoint_sequence_number(&self) -> Option<CheckpointSequenceNumber> {
        self.peers
            .values()
            .filter_map(|info| info.on_same_chain_as_us.then_some(info.height))
            .max()
    }

    pub fn peers_on_same_chain(&self) -> impl Iterator<Item = (&PeerId, &PeerStateSyncInfo)> {
        self.peers
            .iter()
            .filter(|(_peer_id, info)| info.on_same_chain_as_us)
    }

    // Returns a bool that indicates if the update was done successfully.
    //
    // This will return false if the given peer doesn't have an entry or is not on the same chain
    // as us
    #[instrument(level = "debug", skip_all, fields(peer_id=?peer_id, checkpoint=?checkpoint.sequence_number()))]
    pub fn update_peer_info(
        &mut self,
        peer_id: PeerId,
        checkpoint: Checkpoint,
        low_watermark: Option<CheckpointSequenceNumber>,
    ) -> bool {
        debug!("Update peer info");

        let info = match self.peers.get_mut(&peer_id) {
            Some(info) if info.on_same_chain_as_us => info,
            _ => return false,
        };

        info.height = std::cmp::max(*checkpoint.sequence_number(), info.height);
        if let Some(low_watermark) = low_watermark {
            info.lowest = low_watermark;
        }
        self.insert_checkpoint(checkpoint);

        true
    }

    #[instrument(level = "debug", skip_all, fields(peer_id=?peer_id, lowest = ?info.lowest, height = ?info.height))]
    pub fn insert_peer_info(&mut self, peer_id: PeerId, info: PeerStateSyncInfo) {
        use std::collections::hash_map::Entry;
        debug!("Insert peer info");

        match self.peers.entry(peer_id) {
            Entry::Occupied(mut entry) => {
                // If there's already an entry and the genesis checkpoint digests match then update
                // the maximum height. Otherwise we'll use the more recent one
                let entry = entry.get_mut();
                if entry.genesis_checkpoint_digest == info.genesis_checkpoint_digest {
                    entry.height = std::cmp::max(entry.height, info.height);
                } else {
                    *entry = info;
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(info);
            }
        }
    }

    pub fn mark_peer_as_not_on_same_chain(&mut self, peer_id: PeerId) {
        if let Some(info) = self.peers.get_mut(&peer_id) {
            info.on_same_chain_as_us = false;
        }
        self.scores.remove(&peer_id);
    }

    /// Updates the peer's height without storing any checkpoint data.
    /// Returns false if the peer doesn't exist or is not on the same chain.
    pub fn update_peer_height(
        &mut self,
        peer_id: PeerId,
        height: CheckpointSequenceNumber,
        low_watermark: Option<CheckpointSequenceNumber>,
    ) -> bool {
        let info = match self.peers.get_mut(&peer_id) {
            Some(info) if info.on_same_chain_as_us => info,
            _ => return false,
        };

        info.height = std::cmp::max(height, info.height);
        if let Some(low_watermark) = low_watermark {
            info.lowest = low_watermark;
        }

        true
    }

    pub fn cleanup_old_checkpoints(&mut self, sequence_number: CheckpointSequenceNumber) {
        self.unprocessed_checkpoints
            .retain(|_digest, checkpoint| *checkpoint.sequence_number() > sequence_number);
        self.sequence_number_to_digest
            .retain(|&s, _digest| s > sequence_number);
    }

    /// Inserts a checkpoint into the unprocessed checkpoints store.
    /// Only one checkpoint per sequence number is stored. If a checkpoint with the same
    /// sequence number but different digest already exists, the new one is dropped.
    // TODO: also record who gives this checkpoint info for peer quality measurement?
    pub fn insert_checkpoint(&mut self, checkpoint: Checkpoint) {
        let digest = *checkpoint.digest();
        let sequence_number = *checkpoint.sequence_number();

        // Check if we already have a checkpoint for this sequence number.
        if let Some(existing_digest) = self.sequence_number_to_digest.get(&sequence_number) {
            if *existing_digest == digest {
                // Same checkpoint, nothing to do.
                return;
            }
            tracing::info!(
                ?sequence_number,
                ?existing_digest,
                ?digest,
                "received checkpoint with same sequence number but different digest, dropping new checkpoint"
            );
            return;
        }

        self.unprocessed_checkpoints.insert(digest, checkpoint);
        self.sequence_number_to_digest
            .insert(sequence_number, digest);
    }

    pub fn remove_checkpoint(&mut self, digest: &CheckpointDigest) {
        if let Some(checkpoint) = self.unprocessed_checkpoints.remove(digest) {
            self.sequence_number_to_digest
                .remove(checkpoint.sequence_number());
        }
    }

    pub fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<&Checkpoint> {
        self.sequence_number_to_digest
            .get(&sequence_number)
            .and_then(|digest| self.get_checkpoint_by_digest(digest))
    }

    pub fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<&Checkpoint> {
        self.unprocessed_checkpoints.get(digest)
    }

    #[cfg(test)]
    pub fn set_wait_interval_when_no_peer_to_sync_content(&mut self, duration: Duration) {
        self.wait_interval_when_no_peer_to_sync_content = duration;
    }

    pub fn wait_interval_when_no_peer_to_sync_content(&self) -> Duration {
        self.wait_interval_when_no_peer_to_sync_content
    }

    pub fn record_success(&mut self, peer_id: PeerId, size: u64, response_time: Duration) {
        if !self.peers.contains_key(&peer_id) {
            return;
        }
        self.scores
            .entry(peer_id)
            .or_insert_with(|| PeerScore::new(self.peer_scoring_window, self.peer_failure_rate))
            .record_success(size, response_time);
    }

    /// Records a contents request that was cancelled after `elapsed` because another peer
    /// delivered the same `size` bytes first. It can only lower the peer's throughput estimate
    /// (see `PeerScore::record_cancelled`) and counts as neither success nor failure.
    pub fn record_cancelled_request(&mut self, peer_id: PeerId, size: u64, elapsed: Duration) {
        if let Some(score) = self.scores.get_mut(&peer_id) {
            score.record_cancelled(size, elapsed);
        }
    }

    /// Records how long a checkpoint contents request to `peer_id` that was not a hedge took
    /// (see `PeerScore::record_content_response_time`).
    pub fn record_content_response_time(
        &mut self,
        peer_id: PeerId,
        response_time: ContentResponseTime,
        drop_uncountable: bool,
    ) {
        if !self.peers.contains_key(&peer_id) {
            return;
        }
        self.scores
            .entry(peer_id)
            .or_insert_with(|| PeerScore::new(self.peer_scoring_window, self.peer_failure_rate))
            .record_content_response_time(response_time, drop_uncountable);
    }

    /// If `peer_id` is slow from this node, the delay after which a contents request to it
    /// should be hedged (see `PeerScore::slow_peer_hedge_delay`).
    pub fn slow_peer_hedge_delay(&self, peer_id: &PeerId) -> Option<Duration> {
        self.scores
            .get(peer_id)
            .and_then(|score| score.slow_peer_hedge_delay())
    }

    /// Called when a contents request that is not a hedge is about to be sent to `peer_id` near
    /// the release frontier (see `PeerScore::classify_new_request`).
    pub fn classify_new_request(&mut self, peer_id: &PeerId) -> SlowPeerRequest {
        self.scores
            .get_mut(peer_id)
            .map_or(SlowPeerRequest::PeerNotSlow, |score| {
                score.classify_new_request()
            })
    }

    /// The fastest answer among `peer_id`'s recent contents requests that were not hedges (see
    /// `PeerScore::fastest_recent_content_answer`).
    pub fn fastest_recent_content_answer(&self, peer_id: &PeerId) -> Option<Duration> {
        self.scores
            .get(peer_id)
            .and_then(|score| score.fastest_recent_content_answer())
    }

    pub fn record_failure(&mut self, peer_id: PeerId) {
        if !self.peers.contains_key(&peer_id) {
            return;
        }
        self.scores
            .entry(peer_id)
            .or_insert_with(|| PeerScore::new(self.peer_scoring_window, self.peer_failure_rate))
            .record_failure();
    }

    pub fn get_throughput(&self, peer_id: &PeerId) -> Option<f64> {
        self.scores
            .get(peer_id)
            .and_then(|s| s.effective_throughput())
    }

    pub fn is_failing(&self, peer_id: &PeerId) -> bool {
        self.scores
            .get(peer_id)
            .map(|s| s.is_failing())
            .unwrap_or(false)
    }

    pub fn update_failing_states(&mut self) {
        for score in self.scores.values_mut() {
            score.update_failing_state();
        }
    }

    pub fn find_peer_to_report_for_failure(&self, threshold: Duration) -> Option<PeerId> {
        self.scores
            .iter()
            .filter(|(peer_id, score)| {
                score.consistently_failing(threshold)
                    && self
                        .peers
                        .get(peer_id)
                        .is_some_and(|info| info.on_same_chain_as_us)
            })
            .max_by_key(|(_, score)| {
                score
                    .failing_since
                    .map(|since| since.elapsed())
                    .unwrap_or(Duration::ZERO)
            })
            .map(|(peer_id, _)| *peer_id)
    }
}

// PeerBalancer selects peers using weighted random selection:
// - Most of the time: select from known peers, weighted by throughput
// - With configured probability: select from unknown peers (to explore)
// - Failing peers: only as last resort
#[derive(Clone)]
struct PeerBalancer {
    known_peers: Vec<(anemo::Peer, PeerStateSyncInfo, f64)>,
    unknown_peers: Vec<(anemo::Peer, PeerStateSyncInfo, f64)>,
    failing_peers: Vec<(anemo::Peer, PeerStateSyncInfo)>,
    requested_checkpoint: Option<CheckpointSequenceNumber>,
    request_type: PeerCheckpointRequestType,
    exploration_probability: f64,
}

#[derive(Clone)]
enum PeerCheckpointRequestType {
    Summary,
    Content,
}

impl PeerBalancer {
    pub fn new(
        network: &anemo::Network,
        peer_heights: Arc<RwLock<PeerHeights>>,
        request_type: PeerCheckpointRequestType,
    ) -> Self {
        let peer_heights_guard = peer_heights.read().unwrap();

        let mut known_peers = Vec::new();
        let mut unknown_peers = Vec::new();
        let mut failing_peers = Vec::new();
        let exploration_probability = peer_heights_guard.exploration_probability;

        for (peer_id, info) in peer_heights_guard.peers_on_same_chain() {
            let Some(peer) = network.peer(*peer_id) else {
                continue;
            };
            let rtt_secs = peer.connection_rtt().as_secs_f64();

            if peer_heights_guard.is_failing(peer_id) {
                failing_peers.push((peer, *info));
            } else if let Some(throughput) = peer_heights_guard.get_throughput(peer_id) {
                known_peers.push((peer, *info, throughput));
            } else {
                unknown_peers.push((peer, *info, rtt_secs));
            }
        }
        drop(peer_heights_guard);

        unknown_peers.sort_by(|(_, _, rtt_a), (_, _, rtt_b)| {
            rtt_a
                .partial_cmp(rtt_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Self {
            known_peers,
            unknown_peers,
            failing_peers,
            requested_checkpoint: None,
            request_type,
            exploration_probability,
        }
    }

    pub fn with_checkpoint(mut self, checkpoint: CheckpointSequenceNumber) -> Self {
        self.requested_checkpoint = Some(checkpoint);
        self
    }

    /// Removes `peers` from the candidates.
    pub fn excluding(mut self, peers: &HashSet<PeerId>) -> Self {
        self.known_peers
            .retain(|(peer, _, _)| !peers.contains(&peer.peer_id()));
        self.unknown_peers
            .retain(|(peer, _, _)| !peers.contains(&peer.peer_id()));
        self.failing_peers
            .retain(|(peer, _)| !peers.contains(&peer.peer_id()));
        self
    }

    fn is_eligible(&self, info: &PeerStateSyncInfo) -> bool {
        let requested = self.requested_checkpoint.unwrap_or(0);
        match &self.request_type {
            PeerCheckpointRequestType::Summary => info.height >= requested,
            PeerCheckpointRequestType::Content => info.has_contents(requested),
        }
    }

    fn select_by_throughput(&mut self) -> Option<(anemo::Peer, PeerStateSyncInfo)> {
        let eligible: Vec<_> = self
            .known_peers
            .iter()
            .enumerate()
            .filter(|(_, (_, info, _))| self.is_eligible(info))
            .map(|(i, (_, _, t))| (i, *t))
            .collect();

        if eligible.is_empty() {
            return None;
        }

        let total: f64 = eligible.iter().map(|(_, t)| t).sum();
        let mut pick = rand::thread_rng().gen_range(0.0..total);

        for (idx, throughput) in &eligible {
            pick -= throughput;
            if pick <= 0.0 {
                let (peer, info, _) = self.known_peers.remove(*idx);
                return Some((peer, info));
            }
        }

        let (idx, _) = eligible.last().unwrap();
        let (peer, info, _) = self.known_peers.remove(*idx);
        Some((peer, info))
    }

    fn select_by_rtt(&mut self) -> Option<(anemo::Peer, PeerStateSyncInfo)> {
        let pos = self
            .unknown_peers
            .iter()
            .position(|(_, info, _)| self.is_eligible(info))?;
        let (peer, info, _) = self.unknown_peers.remove(pos);
        Some((peer, info))
    }

    fn select_failing(&mut self) -> Option<(anemo::Peer, PeerStateSyncInfo)> {
        let pos = self
            .failing_peers
            .iter()
            .position(|(_, info)| self.is_eligible(info))?;
        Some(self.failing_peers.remove(pos))
    }
}

impl Iterator for PeerBalancer {
    type Item = StateSyncClient<anemo::Peer>;

    fn next(&mut self) -> Option<Self::Item> {
        let has_eligible_known = self
            .known_peers
            .iter()
            .any(|(_, info, _)| self.is_eligible(info));
        let has_eligible_unknown = self
            .unknown_peers
            .iter()
            .any(|(_, info, _)| self.is_eligible(info));

        let explore = has_eligible_unknown
            && (!has_eligible_known || rand::thread_rng().gen_bool(self.exploration_probability));

        if explore && let Some((peer, _)) = self.select_by_rtt() {
            return Some(StateSyncClient::new(peer));
        }

        if has_eligible_known && let Some((peer, _)) = self.select_by_throughput() {
            return Some(StateSyncClient::new(peer));
        }

        if let Some((peer, _)) = self.select_failing() {
            return Some(StateSyncClient::new(peer));
        }

        None
    }
}

#[derive(Clone, Debug)]
enum StateSyncMessage {
    StartSyncJob,
    // Validators will send this to the StateSyncEventLoop in order to kick off notifying our peers
    // of the new checkpoint.
    VerifiedCheckpoint(Box<VerifiedCheckpoint>),
    // Notification that the checkpoint content sync task will send to the event loop in the event
    // it was able to successfully sync a checkpoint's contents. If multiple checkpoints were
    // synced at the same time, only the highest checkpoint is sent.
    SyncedCheckpoint(Box<VerifiedCheckpoint>),
}

struct StateSyncEventLoop<S> {
    config: StateSyncConfig,

    mailbox: mpsc::Receiver<StateSyncMessage>,
    /// Weak reference to our own mailbox
    weak_sender: mpsc::WeakSender<StateSyncMessage>,

    tasks: JoinSet<()>,
    sync_checkpoint_summaries_task: Option<AbortHandle>,
    sync_checkpoint_contents_task: Option<AbortHandle>,
    download_limit_layer: Option<CheckpointContentsDownloadLimitLayer>,

    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
    checkpoint_event_sender: broadcast::Sender<VerifiedCheckpoint>,
    network: anemo::Network,
    metrics: Metrics,

    sync_checkpoint_from_archive_task: Option<AbortHandle>,
    archive_config: Option<ArchiveReaderConfig>,
    discovery_sender: Option<crate::discovery::Sender>,
}

impl<S> StateSyncEventLoop<S>
where
    S: WriteStore + Clone + Send + Sync + 'static,
{
    // Note: A great deal of care is taken to ensure that all event handlers are non-asynchronous
    // and that the only "await" points are from the select macro picking which event to handle.
    // This ensures that the event loop is able to process events at a high speed and reduce the
    // chance for building up a backlog of events to process.
    pub async fn start(mut self) {
        info!("State-Synchronizer started");

        self.config.pinned_checkpoints.sort();

        let mut interval = tokio::time::interval(self.config.interval_period());
        let mut peer_events = {
            let (subscriber, peers) = self.network.subscribe().unwrap();
            for peer_id in peers {
                self.spawn_get_latest_from_peer(peer_id);
            }
            subscriber
        };
        let (
            target_checkpoint_contents_sequence_sender,
            target_checkpoint_contents_sequence_receiver,
        ) = watch::channel(0);

        // Spawn tokio task to update metrics periodically in the background
        let (_sender, receiver) = oneshot::channel();
        tokio::spawn(update_checkpoint_watermark_metrics(
            receiver,
            self.store.clone(),
            self.metrics.clone(),
        ));

        // Start checkpoint contents sync loop.
        let task = sync_checkpoint_contents(
            self.network.clone(),
            self.store.clone(),
            self.peer_heights.clone(),
            self.weak_sender.clone(),
            self.checkpoint_event_sender.clone(),
            self.config.checkpoint_content_download_concurrency(),
            self.config.checkpoint_content_download_tx_concurrency(),
            self.config.use_get_checkpoint_contents_v2(),
            self.config.checkpoint_content_hedge_delay(),
            self.metrics.clone(),
            target_checkpoint_contents_sequence_receiver,
        );
        let task_handle = self.tasks.spawn(task);
        self.sync_checkpoint_contents_task = Some(task_handle);

        // Start archive based checkpoint content sync loop.
        // TODO: Consider switching to sync from archive only on startup.
        // Right now because the peer set is fixed at startup, a node may eventually
        // end up with peers who have all purged their local state. In such a scenario it will be
        // stuck until restart when it ends up with a different set of peers. Once the discovery
        // mechanism can dynamically identify and connect to other peers on the network, we will rely
        // on sync from archive as a fall back.
        let task = sync_checkpoint_contents_from_archive(
            self.network.clone(),
            self.archive_config.clone(),
            self.store.clone(),
            self.peer_heights.clone(),
            self.metrics.clone(),
        );
        let task_handle = self.tasks.spawn(task);
        self.sync_checkpoint_from_archive_task = Some(task_handle);

        // Start main loop.
        loop {
            tokio::select! {
                now = interval.tick() => {
                    self.handle_tick(now.into_std());
                },
                maybe_message = self.mailbox.recv() => {
                    // Once all handles to our mailbox have been dropped this
                    // will yield `None` and we can terminate the event loop
                    if let Some(message) = maybe_message {
                        self.handle_message(message);
                    } else {
                        break;
                    }
                },
                peer_event = peer_events.recv() => {
                    self.handle_peer_event(peer_event);
                },
                Some(task_result) = self.tasks.join_next() => {
                    match task_result {
                        Ok(()) => {},
                        Err(e) => {
                            if e.is_cancelled() {
                                // avoid crashing on ungraceful shutdown
                            } else if e.is_panic() {
                                // propagate panics.
                                std::panic::resume_unwind(e.into_panic());
                            } else {
                                panic!("task failed: {e}");
                            }
                        },
                    };

                    if matches!(&self.sync_checkpoint_contents_task, Some(t) if t.is_finished()) {
                        panic!("sync_checkpoint_contents task unexpectedly terminated")
                    }

                    if matches!(&self.sync_checkpoint_summaries_task, Some(t) if t.is_finished()) {
                        self.sync_checkpoint_summaries_task = None;
                    }

                    if matches!(&self.sync_checkpoint_from_archive_task, Some(t) if t.is_finished()) {
                        panic!("sync_checkpoint_from_archive task unexpectedly terminated")
                    }
                },
            }

            self.maybe_start_checkpoint_summary_sync_task();
            self.maybe_trigger_checkpoint_contents_sync_task(
                &target_checkpoint_contents_sequence_sender,
            );
        }

        info!("State-Synchronizer ended");
    }

    fn handle_message(&mut self, message: StateSyncMessage) {
        debug!("Received message: {:?}", message);
        match message {
            StateSyncMessage::StartSyncJob => self.maybe_start_checkpoint_summary_sync_task(),
            StateSyncMessage::VerifiedCheckpoint(checkpoint) => {
                self.handle_checkpoint_from_consensus(checkpoint)
            }
            // After we've successfully synced a checkpoint we can notify our peers
            StateSyncMessage::SyncedCheckpoint(checkpoint) => {
                self.spawn_notify_peers_of_checkpoint(*checkpoint)
            }
        }
    }

    // Handle a checkpoint that we received from consensus
    #[instrument(level = "debug", skip_all)]
    fn handle_checkpoint_from_consensus(&mut self, checkpoint: Box<VerifiedCheckpoint>) {
        // Always check previous_digest matches in case there is a gap between
        // state sync and consensus.
        let prev_digest = *self.store.get_checkpoint_by_sequence_number(checkpoint.sequence_number() - 1)
            .unwrap_or_else(|| panic!("Got checkpoint {} from consensus but cannot find checkpoint {} in certified_checkpoints", checkpoint.sequence_number(), checkpoint.sequence_number() - 1))
            .digest();
        if checkpoint.previous_digest != Some(prev_digest) {
            panic!(
                "Checkpoint {} from consensus has mismatched previous_digest, expected: {:?}, actual: {:?}",
                checkpoint.sequence_number(),
                Some(prev_digest),
                checkpoint.previous_digest
            );
        }

        let latest_checkpoint = self
            .store
            .get_highest_verified_checkpoint()
            .expect("store operation should not fail");

        // If this is an older checkpoint, just ignore it
        if latest_checkpoint.sequence_number() >= checkpoint.sequence_number() {
            return;
        }

        let checkpoint = *checkpoint;
        let next_sequence_number = latest_checkpoint.sequence_number().checked_add(1).unwrap();
        if *checkpoint.sequence_number() > next_sequence_number {
            debug!(
                "consensus sent too new of a checkpoint, expecting: {}, got: {}",
                next_sequence_number,
                checkpoint.sequence_number()
            );
        }

        // Because checkpoint from consensus sends in order, when we have checkpoint n,
        // we must have all of the checkpoints before n from either state sync or consensus.
        #[cfg(debug_assertions)]
        {
            let _ = (next_sequence_number..=*checkpoint.sequence_number())
                .map(|n| {
                    let checkpoint = self
                        .store
                        .get_checkpoint_by_sequence_number(n)
                        .unwrap_or_else(|| panic!("store should contain checkpoint {n}"));
                    self.store
                        .get_full_checkpoint_contents(Some(n), &checkpoint.content_digest)
                        .unwrap_or_else(|| {
                            panic!(
                                "store should contain checkpoint contents for {:?}",
                                checkpoint.content_digest
                            )
                        });
                })
                .collect::<Vec<_>>();
        }

        // If this is the last checkpoint of a epoch, we need to make sure
        // new committee is in store before we verify newer checkpoints in next epoch.
        // This could happen before this validator's reconfiguration finishes, because
        // state sync does not reconfig.
        // TODO: make CheckpointAggregator use WriteStore so we don't need to do this
        // committee insertion in two places (only in `insert_checkpoint`).
        if let Some(EndOfEpochData {
            next_epoch_committee,
            ..
        }) = checkpoint.end_of_epoch_data.as_ref()
        {
            let next_committee = next_epoch_committee.iter().cloned().collect();
            let committee =
                Committee::new(checkpoint.epoch().checked_add(1).unwrap(), next_committee);
            self.store
                .insert_committee(committee)
                .expect("store operation should not fail");
        }

        self.store
            .update_highest_verified_checkpoint(&checkpoint)
            .expect("store operation should not fail");
        self.store
            .update_highest_synced_checkpoint(&checkpoint)
            .expect("store operation should not fail");

        // We don't care if no one is listening as this is a broadcast channel
        let _ = self.checkpoint_event_sender.send(checkpoint.clone());

        self.spawn_notify_peers_of_checkpoint(checkpoint);
    }

    fn handle_peer_event(
        &mut self,
        peer_event: Result<PeerEvent, tokio::sync::broadcast::error::RecvError>,
    ) {
        use tokio::sync::broadcast::error::RecvError;

        match peer_event {
            Ok(PeerEvent::NewPeer(peer_id)) => {
                self.spawn_get_latest_from_peer(peer_id);
            }
            Ok(PeerEvent::LostPeer(peer_id, _)) => {
                let mut heights = self.peer_heights.write().unwrap();
                heights.peers.remove(&peer_id);
                heights.scores.remove(&peer_id);
            }

            Err(RecvError::Closed) => {
                panic!("PeerEvent channel shouldn't be able to be closed");
            }

            Err(RecvError::Lagged(_)) => {
                trace!("State-Sync fell behind processing PeerEvents");
            }
        }
    }

    fn spawn_get_latest_from_peer(&mut self, peer_id: PeerId) {
        if let Some(peer) = self.network.peer(peer_id) {
            let genesis_checkpoint_digest = *self
                .store
                .get_checkpoint_by_sequence_number(0)
                .expect("store should contain genesis checkpoint")
                .digest();
            let task = get_latest_from_peer(
                genesis_checkpoint_digest,
                peer,
                self.peer_heights.clone(),
                self.config.timeout(),
            );
            self.tasks.spawn(task);
        }
    }

    fn handle_tick(&mut self, _now: std::time::Instant) {
        let task = query_peers_for_their_latest_checkpoint(
            self.network.clone(),
            self.peer_heights.clone(),
            self.weak_sender.clone(),
            self.config.timeout(),
        );
        self.tasks.spawn(task);

        if let Some(layer) = self.download_limit_layer.as_ref() {
            layer.maybe_prune_map();
        }

        self.maybe_report_failing_peer();
    }

    fn maybe_report_failing_peer(&mut self) {
        let threshold = self.config.peer_disconnect_threshold();
        if threshold.is_zero() {
            return;
        }

        let mut peer_heights = self.peer_heights.write().unwrap();
        peer_heights.update_failing_states();

        let Some(peer_id) = peer_heights.find_peer_to_report_for_failure(threshold) else {
            return;
        };
        // Reset the failing clock so this peer isn't re-reported on the next tick.
        // If the peer is still failing, update_failing_states will restart the clock
        // and the peer must fail for another full threshold before being reported again.
        // Discovery may decline to disconnect (e.g. too few peers), so this also
        // prevents spamming reports every tick in that case.
        if let Some(score) = peer_heights.scores.get_mut(&peer_id) {
            score.reset_failing_since();
        }
        drop(peer_heights);

        info!("reporting peer {peer_id} for consistent state sync failures");
        if let Some(sender) = &self.discovery_sender {
            sender.report_peer_failure(peer_id);
        } else {
            let _ = self.network.disconnect(peer_id);
        }
        self.metrics.inc_peers_reported_for_failure();
    }

    fn maybe_start_checkpoint_summary_sync_task(&mut self) {
        // Only run one sync task at a time
        if self.sync_checkpoint_summaries_task.is_some() {
            return;
        }

        let highest_processed_checkpoint = self
            .store
            .get_highest_verified_checkpoint()
            .expect("store operation should not fail");

        let highest_known_sequence_number = self
            .peer_heights
            .read()
            .unwrap()
            .highest_known_checkpoint_sequence_number();

        if let Some(target_seq) = highest_known_sequence_number
            && *highest_processed_checkpoint.sequence_number() < target_seq
        {
            // Limit the per-sync batch size according to config.
            let max_batch_size = self.config.max_checkpoint_sync_batch_size();
            let limited_target = std::cmp::min(
                target_seq,
                highest_processed_checkpoint
                    .sequence_number()
                    .saturating_add(max_batch_size),
            );
            let was_limited = limited_target < target_seq;

            // Start sync job.
            let weak_sender = self.weak_sender.clone();
            let task = sync_to_checkpoint(
                self.network.clone(),
                self.store.clone(),
                self.peer_heights.clone(),
                self.metrics.clone(),
                self.config.pinned_checkpoints.clone(),
                self.config.checkpoint_header_download_concurrency(),
                self.config.timeout(),
                limited_target,
            )
            .map(move |result| match result {
                Ok(()) => {
                    // If we limited the sync range, immediately trigger
                    // another sync to continue catching up.
                    if was_limited && let Some(sender) = weak_sender.upgrade() {
                        let _ = sender.try_send(StateSyncMessage::StartSyncJob);
                    }
                }
                Err(e) => {
                    debug!("error syncing checkpoint {e}");
                }
            });
            let task_handle = self.tasks.spawn(task);
            self.sync_checkpoint_summaries_task = Some(task_handle);
        }
    }

    fn maybe_trigger_checkpoint_contents_sync_task(
        &mut self,
        target_sequence_channel: &watch::Sender<CheckpointSequenceNumber>,
    ) {
        let highest_verified_checkpoint = self
            .store
            .get_highest_verified_checkpoint()
            .expect("store operation should not fail");
        let highest_synced_checkpoint = self
            .store
            .get_highest_synced_checkpoint()
            .expect("store operation should not fail");

        if highest_verified_checkpoint.sequence_number()
            > highest_synced_checkpoint.sequence_number()
            // skip if we aren't connected to any peers that can help
            && self
                .peer_heights
                .read()
                .unwrap()
                .highest_known_checkpoint_sequence_number()
                > Some(*highest_synced_checkpoint.sequence_number())
        {
            let _ = target_sequence_channel.send_if_modified(|num| {
                let new_num = *highest_verified_checkpoint.sequence_number();
                if *num == new_num {
                    return false;
                }
                *num = new_num;
                true
            });
        }
    }

    fn spawn_notify_peers_of_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        let task = notify_peers_of_checkpoint(
            self.network.clone(),
            self.peer_heights.clone(),
            checkpoint,
            self.config.timeout(),
        );
        self.tasks.spawn(task);
    }
}

async fn notify_peers_of_checkpoint(
    network: anemo::Network,
    peer_heights: Arc<RwLock<PeerHeights>>,
    checkpoint: VerifiedCheckpoint,
    timeout: Duration,
) {
    let futs = peer_heights
        .read()
        .unwrap()
        .peers_on_same_chain()
        // Filter out any peers who we know already have a checkpoint higher than this one
        .filter_map(|(peer_id, info)| {
            (*checkpoint.sequence_number() > info.height).then_some(peer_id)
        })
        // Filter out any peers who we aren't connected with
        .flat_map(|peer_id| network.peer(*peer_id))
        .map(StateSyncClient::new)
        .map(|mut client| {
            let request = Request::new(checkpoint.inner().clone()).with_timeout(timeout);
            async move { client.push_checkpoint_summary(request).await }
        })
        .collect::<Vec<_>>();
    futures::future::join_all(futs).await;
}

async fn get_latest_from_peer(
    our_genesis_checkpoint_digest: CheckpointDigest,
    peer: anemo::Peer,
    peer_heights: Arc<RwLock<PeerHeights>>,
    timeout: Duration,
) {
    let peer_id = peer.peer_id();
    let mut client = StateSyncClient::new(peer);

    let info = {
        let maybe_info = peer_heights.read().unwrap().peers.get(&peer_id).copied();

        if let Some(info) = maybe_info {
            info
        } else {
            // TODO do we want to create a new API just for querying a node's chainid?
            //
            // We need to query this node's genesis checkpoint to see if they're on the same chain
            // as us
            let request = Request::new(GetCheckpointSummaryRequest::BySequenceNumber(0))
                .with_timeout(timeout);
            let response = client
                .get_checkpoint_summary(request)
                .await
                .map(Response::into_inner);

            let info = match response {
                Ok(Some(checkpoint)) => {
                    let digest = *checkpoint.digest();
                    PeerStateSyncInfo {
                        genesis_checkpoint_digest: digest,
                        on_same_chain_as_us: our_genesis_checkpoint_digest == digest,
                        height: *checkpoint.sequence_number(),
                        lowest: CheckpointSequenceNumber::default(),
                    }
                }
                Ok(None) => PeerStateSyncInfo {
                    genesis_checkpoint_digest: CheckpointDigest::default(),
                    on_same_chain_as_us: false,
                    height: CheckpointSequenceNumber::default(),
                    lowest: CheckpointSequenceNumber::default(),
                },
                Err(status) => {
                    trace!("get_latest_checkpoint_summary request failed: {status:?}");
                    return;
                }
            };
            peer_heights
                .write()
                .unwrap()
                .insert_peer_info(peer_id, info);
            info
        }
    };

    // Bail early if this node isn't on the same chain as us
    if !info.on_same_chain_as_us {
        trace!(?info, "Peer {peer_id} not on same chain as us");
        return;
    }
    let Ok(Some((highest_checkpoint, low_watermark))) =
        query_peer_for_latest_info(&mut client, timeout).await
    else {
        return;
    };
    peer_heights
        .write()
        .unwrap()
        .update_peer_info(peer_id, highest_checkpoint, low_watermark);
}

/// Queries a peer for their highest_synced_checkpoint and low checkpoint watermark.
///
/// Distinguishes three outcomes so callers can score the peer:
/// - `Ok(Some(..))`: the peer answered with usable latest-checkpoint info.
/// - `Ok(None)`: the peer answered but had nothing useful to report (e.g. a pre-upgrade peer
///   that returns no latest summary). This is not a failure.
/// - `Err(())`: the request itself failed (timeout / transport error). This is a failure: a
///   peer that consistently fails these liveness queries is consistently unreachable.
async fn query_peer_for_latest_info(
    client: &mut StateSyncClient<anemo::Peer>,
    timeout: Duration,
) -> Result<Option<(Checkpoint, Option<CheckpointSequenceNumber>)>, ()> {
    let request = Request::new(()).with_timeout(timeout);
    let response = client
        .get_checkpoint_availability(request)
        .await
        .map(Response::into_inner);
    match response {
        Ok(GetCheckpointAvailabilityResponse {
            highest_synced_checkpoint,
            lowest_available_checkpoint,
        }) => {
            return Ok(Some((
                highest_synced_checkpoint,
                Some(lowest_available_checkpoint),
            )));
        }
        Err(status) => {
            // If peer hasn't upgraded they would return 404 NotFound error
            if status.status() != anemo::types::response::StatusCode::NotFound {
                trace!("get_checkpoint_availability request failed: {status:?}");
                return Err(());
            }
        }
    };

    // Then we try the old query
    // TODO: remove this once the new feature stabilizes
    let request = Request::new(GetCheckpointSummaryRequest::Latest).with_timeout(timeout);
    let response = client
        .get_checkpoint_summary(request)
        .await
        .map(Response::into_inner);
    match response {
        Ok(Some(checkpoint)) => Ok(Some((checkpoint, None))),
        Ok(None) => Ok(None),
        Err(status) => {
            trace!("get_checkpoint_summary (latest) request failed: {status:?}");
            Err(())
        }
    }
}

#[instrument(level = "debug", skip_all)]
async fn query_peers_for_their_latest_checkpoint(
    network: anemo::Network,
    peer_heights: Arc<RwLock<PeerHeights>>,
    sender: mpsc::WeakSender<StateSyncMessage>,
    timeout: Duration,
) {
    let peer_heights = &peer_heights;
    let futs = peer_heights
        .read()
        .unwrap()
        .peers_on_same_chain()
        // Filter out any peers who we aren't connected with
        .flat_map(|(peer_id, _info)| network.peer(*peer_id))
        .map(|peer| {
            let peer_id = peer.peer_id();
            let mut client = StateSyncClient::new(peer);

            async move {
                let start = Instant::now();
                let response = query_peer_for_latest_info(&mut client, timeout).await;
                let elapsed = start.elapsed();
                match response {
                    Ok(Some((highest_checkpoint, low_watermark))) => {
                        // A peer that answers our periodic liveness/discovery query is healthy;
                        // record it so its success rate offsets occasional failures.
                        let size = bcs::serialized_size(&highest_checkpoint).unwrap_or(0) as u64;
                        let mut peer_heights = peer_heights.write().unwrap();
                        peer_heights.record_success(peer_id, size, elapsed);
                        peer_heights
                            .update_peer_info(peer_id, highest_checkpoint.clone(), low_watermark)
                            .then_some(highest_checkpoint)
                    }
                    // Peer responded but had nothing useful; neutral, do not score.
                    Ok(None) => None,
                    // Request failed (timeout / transport). A peer that consistently fails
                    // these liveness queries is consistently unreachable, so record the
                    // failure to feed the scorer even when we are not actively syncing from
                    // it. Without this, an unreachable peer is only scored while there is
                    // active sync traffic, so it can evade the consistently-failing-peer
                    // disconnect logic indefinitely.
                    Err(()) => {
                        peer_heights.write().unwrap().record_failure(peer_id);
                        None
                    }
                }
            }
        })
        .collect::<Vec<_>>();

    debug!("Query {} peers for latest checkpoint", futs.len());

    let checkpoints = futures::future::join_all(futs).await.into_iter().flatten();

    let highest_checkpoint_seq = checkpoints
        .map(|checkpoint| *checkpoint.sequence_number())
        .max();

    let our_highest_seq = peer_heights
        .read()
        .unwrap()
        .highest_known_checkpoint_sequence_number();

    debug!(
        "Our highest checkpoint {our_highest_seq:?}, peers' highest checkpoint {highest_checkpoint_seq:?}"
    );

    let _new_checkpoint = match (highest_checkpoint_seq, our_highest_seq) {
        (Some(theirs), None) => theirs,
        (Some(theirs), Some(ours)) if theirs > ours => theirs,
        _ => return,
    };

    if let Some(sender) = sender.upgrade() {
        let _ = sender.send(StateSyncMessage::StartSyncJob).await;
    }
}

async fn sync_to_checkpoint<S>(
    network: anemo::Network,
    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
    metrics: Metrics,
    pinned_checkpoints: Vec<(CheckpointSequenceNumber, CheckpointDigest)>,
    checkpoint_header_download_concurrency: usize,
    timeout: Duration,
    target_sequence_number: CheckpointSequenceNumber,
) -> Result<()>
where
    S: WriteStore,
{
    metrics.set_highest_known_checkpoint(target_sequence_number);

    let mut current = store
        .get_highest_verified_checkpoint()
        .expect("store operation should not fail");
    if *current.sequence_number() >= target_sequence_number {
        return Err(anyhow::anyhow!(
            "target checkpoint {} is older than highest verified checkpoint {}",
            target_sequence_number,
            current.sequence_number(),
        ));
    }

    let peer_balancer = PeerBalancer::new(
        &network,
        peer_heights.clone(),
        PeerCheckpointRequestType::Summary,
    );
    // range of the next sequence_numbers to fetch
    let mut request_stream = (current.sequence_number().checked_add(1).unwrap()
        ..=target_sequence_number)
        .map(|next| {
            let peers = peer_balancer.clone().with_checkpoint(next);
            let peer_heights = peer_heights.clone();
            let pinned_checkpoints = &pinned_checkpoints;
            async move {
                if let Some(checkpoint) = peer_heights
                    .read()
                    .unwrap()
                    .get_checkpoint_by_sequence_number(next)
                {
                    return (Some(checkpoint.to_owned()), next, None);
                }

                // Iterate through peers trying each one in turn until we're able to
                // successfully get the target checkpoint.
                for mut peer in peers {
                    let peer_id = peer.inner().peer_id();
                    let request = Request::new(GetCheckpointSummaryRequest::BySequenceNumber(next))
                        .with_timeout(timeout);
                    let start = Instant::now();
                    let result = peer.get_checkpoint_summary(request).await;
                    let elapsed = start.elapsed();

                    let checkpoint = match result {
                        Ok(response) => match response.into_inner() {
                            Some(cp) => Some(cp),
                            None => {
                                trace!("peer unable to help sync");
                                peer_heights.write().unwrap().record_failure(peer_id);
                                None
                            }
                        },
                        Err(e) => {
                            trace!("{e:?}");
                            peer_heights.write().unwrap().record_failure(peer_id);
                            None
                        }
                    };

                    let Some(checkpoint) = checkpoint else {
                        continue;
                    };

                    let size = bcs::serialized_size(&checkpoint).expect("serialization should not fail") as u64;
                    peer_heights.write().unwrap().record_success(peer_id, size, elapsed);

                    // peer didn't give us a checkpoint with the height that we requested
                    if *checkpoint.sequence_number() != next {
                        tracing::debug!(
                            "peer returned checkpoint with wrong sequence number: expected {next}, got {}",
                            checkpoint.sequence_number()
                        );
                        peer_heights
                            .write()
                            .unwrap()
                            .mark_peer_as_not_on_same_chain(peer_id);
                        continue;
                    }

                    // peer gave us a checkpoint whose digest does not match pinned digest
                    let checkpoint_digest = checkpoint.digest();
                    if let Ok(pinned_digest_index) = pinned_checkpoints.binary_search_by_key(
                        checkpoint.sequence_number(),
                        |(seq_num, _digest)| *seq_num
                    )
                        && pinned_checkpoints[pinned_digest_index].1 != *checkpoint_digest
                    {
                        tracing::debug!(
                            "peer returned checkpoint with digest that does not match pinned digest: expected {:?}, got {:?}",
                            pinned_checkpoints[pinned_digest_index].1,
                            checkpoint_digest
                        );
                        peer_heights
                            .write()
                            .unwrap()
                            .mark_peer_as_not_on_same_chain(peer.inner().peer_id());
                        continue;
                    }

                    // Insert in our store in the event that things fail and we need to retry
                    peer_heights
                        .write()
                        .unwrap()
                        .insert_checkpoint(checkpoint.clone());
                    return (Some(checkpoint), next, Some(peer_id));
                }
                (None, next, None)
            }
        })
        .pipe(futures::stream::iter)
        .buffered(checkpoint_header_download_concurrency);

    while let Some((maybe_checkpoint, next, maybe_peer_id)) = request_stream.next().await {
        assert_eq!(
            current
                .sequence_number()
                .checked_add(1)
                .expect("exhausted u64"),
            next
        );

        // Verify the checkpoint
        let checkpoint = 'cp: {
            let checkpoint = maybe_checkpoint.ok_or_else(|| {
                anyhow::anyhow!("no peers were able to help sync checkpoint {next}")
            })?;
            // Skip verification for manually pinned checkpoints.
            if pinned_checkpoints
                .binary_search_by_key(checkpoint.sequence_number(), |(seq_num, _digest)| *seq_num)
                .is_ok()
            {
                break 'cp VerifiedCheckpoint::new_unchecked(checkpoint);
            }
            match verify_checkpoint(&current, &store, checkpoint) {
                Ok(verified_checkpoint) => verified_checkpoint,
                Err(checkpoint) => {
                    let mut peer_heights = peer_heights.write().unwrap();
                    // Remove the checkpoint from our temporary store so that we can try querying
                    // another peer for a different one
                    peer_heights.remove_checkpoint(checkpoint.digest());

                    // Mark peer as not on the same chain as us
                    if let Some(peer_id) = maybe_peer_id {
                        peer_heights.mark_peer_as_not_on_same_chain(peer_id);
                    }

                    return Err(anyhow::anyhow!(
                        "unable to verify checkpoint {checkpoint:?}"
                    ));
                }
            }
        };

        debug!(checkpoint_seq = ?checkpoint.sequence_number(), "verified checkpoint summary");
        if let Some((checkpoint_summary_age_metric, checkpoint_summary_age_metric_deprecated)) =
            metrics.checkpoint_summary_age_metrics()
        {
            checkpoint.report_checkpoint_age(
                checkpoint_summary_age_metric,
                checkpoint_summary_age_metric_deprecated,
            );
        }

        // Insert the newly verified checkpoint into our store, which will bump our highest
        // verified checkpoint watermark as well.
        store
            .insert_checkpoint(&checkpoint)
            .expect("store operation should not fail");

        current = checkpoint;
    }

    peer_heights
        .write()
        .unwrap()
        .cleanup_old_checkpoints(*current.sequence_number());

    Ok(())
}

async fn sync_checkpoint_contents_from_archive<S>(
    network: anemo::Network,
    archive_config: Option<ArchiveReaderConfig>,
    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
    metrics: Metrics,
) where
    S: WriteStore + Clone + Send + Sync + 'static,
{
    loop {
        sync_checkpoint_contents_from_archive_iteration(
            &network,
            &archive_config,
            store.clone(),
            peer_heights.clone(),
            metrics.clone(),
        )
        .await;
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn sync_checkpoint_contents_from_archive_iteration<S>(
    network: &anemo::Network,
    archive_config: &Option<ArchiveReaderConfig>,
    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
    metrics: Metrics,
) where
    S: WriteStore + Clone + Send + Sync + 'static,
{
    let peers: Vec<_> = peer_heights
        .read()
        .unwrap()
        .peers_on_same_chain()
        // Filter out any peers who we aren't connected with.
        .filter_map(|(peer_id, info)| network.peer(*peer_id).map(|peer| (peer, *info)))
        .collect();
    let lowest_checkpoint_on_peers = peers
        .iter()
        .map(|(_p, state_sync_info)| state_sync_info.lowest)
        .min();
    let highest_synced = store
        .get_highest_synced_checkpoint()
        .expect("store operation should not fail")
        .sequence_number;
    let sync_from_archive = if let Some(lowest_checkpoint_on_peers) = lowest_checkpoint_on_peers {
        highest_synced < lowest_checkpoint_on_peers
    } else {
        false
    };
    debug!(
        "Syncing checkpoint contents from archive: {sync_from_archive},  highest_synced: {highest_synced},  lowest_checkpoint_on_peers: {}",
        lowest_checkpoint_on_peers.map_or_else(|| "None".to_string(), |l| l.to_string())
    );
    if sync_from_archive {
        let start = highest_synced
            .checked_add(1)
            .expect("Checkpoint seq num overflow");
        let end = lowest_checkpoint_on_peers.unwrap();

        let Some(archive_config) = archive_config else {
            warn!("Failed to find an archive reader to complete the state sync request");
            return;
        };
        let Some(ingestion_url) = &archive_config.ingestion_url else {
            warn!("Archival ingestion url for state sync is not configured");
            return;
        };
        if ingestion_url.contains("checkpoints.mainnet.sui.io") {
            warn!("{} can't be used as an archival fallback", ingestion_url);
            return;
        }
        let obj_store = build_object_store(
            ingestion_url,
            archive_config.remote_store_options.clone(),
            archive_config.remote_store_headers.clone(),
        );
        let mut checkpoint_stream = futures::stream::iter(start..=end)
            .map(|seq| {
                let obj_store = obj_store.clone();
                async move { (seq, fetch_checkpoint(&obj_store, seq).await) }
            })
            .buffered(archive_config.download_concurrency.get());

        while let Some((seq, result)) = checkpoint_stream.next().await {
            let checkpoint = match result {
                Ok(checkpoint) => checkpoint,
                Err(err) => {
                    warn!("State sync from archive failed fetching checkpoint {seq}: {err}");
                    return;
                }
            };
            if let Err(err) = process_archive_checkpoint(&store, &checkpoint, &metrics) {
                warn!("State sync from archive failed processing checkpoint {seq}: {err}");
                return;
            }
        }
    }
}

async fn sync_checkpoint_contents<S>(
    network: anemo::Network,
    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
    sender: mpsc::WeakSender<StateSyncMessage>,
    checkpoint_event_sender: broadcast::Sender<VerifiedCheckpoint>,
    checkpoint_content_download_concurrency: usize,
    checkpoint_content_download_tx_concurrency: u64,
    use_get_checkpoint_contents_v2: bool,
    checkpoint_content_hedge_delay: Option<Duration>,
    metrics: Metrics,
    mut target_sequence_channel: watch::Receiver<CheckpointSequenceNumber>,
) where
    S: WriteStore + Clone,
{
    let mut highest_synced = store
        .get_highest_synced_checkpoint()
        .expect("store operation should not fail");

    let mut current_sequence = highest_synced.sequence_number().checked_add(1).unwrap();
    let mut target_sequence_cursor = 0;
    let mut highest_started_network_total_transactions = highest_synced.network_total_transactions;
    let mut checkpoint_contents_tasks = FuturesOrdered::new();

    let mut tx_concurrency_remaining = checkpoint_content_download_tx_concurrency;

    let hedge = checkpoint_content_hedge_delay.map(|delay| ContentHedge {
        delay,
        highest_synced: Arc::new(AtomicU64::new(*highest_synced.sequence_number())),
    });

    loop {
        tokio::select! {
            result = target_sequence_channel.changed() => {
                match result {
                    Ok(()) => {
                        target_sequence_cursor = (*target_sequence_channel.borrow_and_update()).checked_add(1).unwrap();
                    }
                    Err(_) => {
                        // Watch channel is closed, exit loop.
                        return
                    }
                }
            },
            Some(maybe_checkpoint) = checkpoint_contents_tasks.next() => {
                match maybe_checkpoint {
                    Ok(checkpoint) => {
                        let _: &VerifiedCheckpoint = &checkpoint;  // type hint

                        store
                            .update_highest_synced_checkpoint(&checkpoint)
                            .expect("store operation should not fail");
                        // We don't care if no one is listening as this is a broadcast channel
                        let _ = checkpoint_event_sender.send(checkpoint.clone());
                        tx_concurrency_remaining += checkpoint.network_total_transactions - highest_synced.network_total_transactions;
                        highest_synced = checkpoint;
                        if let Some(hedge) = &hedge {
                            hedge
                                .highest_synced
                                .store(*highest_synced.sequence_number(), Ordering::Relaxed);
                        }

                    }
                    Err(checkpoint) => {
                        let _: &VerifiedCheckpoint = &checkpoint;  // type hint
                        if let Some(lowest_peer_checkpoint) =
                            peer_heights.read().ok().and_then(|x| x.peers.values().map(|state_sync_info| state_sync_info.lowest).min()) {
                            if checkpoint.sequence_number() >= &lowest_peer_checkpoint {
                                info!("unable to sync contents of checkpoint through state sync {} with lowest peer checkpoint: {}", checkpoint.sequence_number(), lowest_peer_checkpoint);
                            }
                        } else {
                            info!("unable to sync contents of checkpoint through state sync {}", checkpoint.sequence_number());

                        }
                        // Calculate tx_count for retry by getting previous checkpoint
                        let retry_tx_count = if *checkpoint.sequence_number() == 0 {
                            checkpoint.network_total_transactions
                        } else {
                            let prev = store
                                .get_checkpoint_by_sequence_number(checkpoint.sequence_number() - 1)
                                .expect("previous checkpoint must exist")
                                .network_total_transactions;
                            checkpoint.network_total_transactions - prev
                        };
                        // Retry contents sync on failure.
                        checkpoint_contents_tasks.push_front(sync_one_checkpoint_contents(
                            network.clone(),
                            &store,
                            peer_heights.clone(),
                            use_get_checkpoint_contents_v2,
                            hedge.clone(),
                            metrics.clone(),
                            retry_tx_count,
                            checkpoint,
                        ));
                    }
                }
            },
        }

        // Start new tasks up to configured concurrency limits.
        while current_sequence < target_sequence_cursor
            && checkpoint_contents_tasks.len() < checkpoint_content_download_concurrency
        {
            let next_checkpoint = store
                .get_checkpoint_by_sequence_number(current_sequence)
                .unwrap_or_else(|| panic!(
                    "BUG: store should have all checkpoints older than highest_verified_checkpoint (checkpoint {})",
                    current_sequence
                ));

            // Enforce transaction count concurrency limit.
            let tx_count = next_checkpoint.network_total_transactions
                - highest_started_network_total_transactions;
            if tx_count > tx_concurrency_remaining {
                break;
            }
            tx_concurrency_remaining -= tx_count;

            highest_started_network_total_transactions = next_checkpoint.network_total_transactions;
            current_sequence += 1;
            checkpoint_contents_tasks.push_back(sync_one_checkpoint_contents(
                network.clone(),
                &store,
                peer_heights.clone(),
                use_get_checkpoint_contents_v2,
                hedge.clone(),
                metrics.clone(),
                tx_count,
                next_checkpoint,
            ));
        }

        if highest_synced
            .sequence_number()
            .is_multiple_of(checkpoint_content_download_concurrency as u64)
            || checkpoint_contents_tasks.is_empty()
        {
            // Periodically notify event loop to notify our peers that we've synced to a new checkpoint height
            if let Some(sender) = sender.upgrade() {
                let message = StateSyncMessage::SyncedCheckpoint(Box::new(highest_synced.clone()));
                let _ = sender.send(message).await;
            }
        }
    }
}

// Err hands the checkpoint back so the caller can retry it; Ok carries the same type, so boxing
// the error would not shrink the Result.
#[allow(clippy::result_large_err)]
#[instrument(level = "debug", skip_all, fields(sequence_number = ?checkpoint.sequence_number()))]
async fn sync_one_checkpoint_contents<S>(
    network: anemo::Network,
    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
    use_get_checkpoint_contents_v2: bool,
    hedge: Option<ContentHedge>,
    metrics: Metrics,
    tx_count: u64,
    checkpoint: VerifiedCheckpoint,
) -> Result<VerifiedCheckpoint, VerifiedCheckpoint>
where
    S: WriteStore + Clone,
{
    let (timeout_min, timeout_max) = {
        let ph = peer_heights.read().unwrap();
        (
            ph.checkpoint_content_timeout_min,
            ph.checkpoint_content_timeout_max,
        )
    };
    let timeout = compute_adaptive_timeout(tx_count, timeout_min, timeout_max);
    debug!(
        "syncing checkpoint contents with adaptive timeout {:?} for {} txns",
        timeout, tx_count
    );

    // Check if we already have produced this checkpoint locally. If so, we don't need
    // to get it from peers anymore.
    if store
        .get_highest_synced_checkpoint()
        .expect("store operation should not fail")
        .sequence_number()
        >= checkpoint.sequence_number()
    {
        debug!("checkpoint was already created via consensus output");
        return Ok(checkpoint);
    }

    // Request checkpoint contents from peers.
    let now = tokio::time::Instant::now();
    let fetch = ContentFetch {
        network: &network,
        peer_heights: &peer_heights,
        checkpoint: &checkpoint,
        use_get_checkpoint_contents_v2,
        timeout,
        metrics: &metrics,
    };
    let Some(_contents) = get_full_checkpoint_contents(fetch, &store, hedge.as_ref()).await else {
        // Delay completion in case of error so we don't hammer the network with retries.
        let duration = peer_heights
            .read()
            .unwrap()
            .wait_interval_when_no_peer_to_sync_content();
        if now.elapsed() < duration {
            let duration = duration - now.elapsed();
            info!("retrying checkpoint sync after {:?}", duration);
            tokio::time::sleep(duration).await;
        }
        return Err(checkpoint);
    };
    debug!("completed checkpoint contents sync");
    Ok(checkpoint)
}

/// In hedged mode, the most requests for one checkpoint's contents that are in flight at once.
const MAX_CONCURRENT_CONTENT_REQUESTS: usize = 2;

/// In hedged mode, only checkpoints at most this far past the highest synced checkpoint are
/// hedged, or keep looking for a peer after every peer that had the checkpoint failed.
/// Checkpoints are released in order, so a later checkpoint can't be released before the ones
/// ahead of it. Extra requests for it would add load (for example during catch-up, with hundreds
/// of downloads in flight) without reducing how long it takes to release.
const CONTENT_HEDGE_WINDOW: CheckpointSequenceNumber = 8;

/// How often a download near the release frontier checks which peers have announced the
/// checkpoint: while a hedge is due or every request failed, and, for measuring when peers
/// announce it and for early hedges, for the whole download after its first request.
const CONTENT_CANDIDATE_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// How much sooner a peer that hasn't been asked yet must be expected to finish than the request
/// in flight for a hedge to go out to it right away. Keeps small differences in expected response
/// times, which are within their measurement noise, from sending extra requests.
const CLOSER_PEER_MIN_GAIN: Duration = Duration::from_millis(50);

/// Near the release frontier, one in this many requests that are not hedges sent to a peer
/// while it is slow from this node is a slow-peer probe. Requests to a slow peer are otherwise
/// hedged right away once a peer that isn't slow has the checkpoint, so their slow outcomes are
/// cut short before the slow peer's slow-response threshold, where they don't count (see
/// `PeerScore::slow_peer_hedge_delay`), while their fast answers still arrive. Recording those
/// would make the peer look less slow than it is, so while a peer is slow, only probes add
/// response-time samples (see `SlowPeerRequest::records_response_time`). Samples from before it
/// became slow still count until newer ones replace them or they age out.
/// A probe is only hedged after its hedge delay (see `ContentFetch::hedge_delay_for`) or to a
/// peer expected to finish at least [`CLOSER_PEER_MIN_GAIN`] sooner, so it usually ends in a
/// sample that counts: an answer, or a cancellation at or after the threshold. Probes keep the
/// slow flag measurable and re-check the peer every so often, so that it stops being treated as
/// slow once it answers in time again.
const SLOW_PEER_PROBE_INTERVAL: u32 = 4;

/// Settings for hedged checkpoint contents downloads.
#[derive(Clone)]
struct ContentHedge {
    /// How long a request to a peer that is not slow from this node may run before another peer
    /// is also asked. Requests to a slow peer are hedged after the peer's slow-response threshold
    /// if that is shorter (see [`ContentFetch::hedge_delay_for`]).
    delay: Duration,
    /// Sequence number of the highest checkpoint whose contents are synced and released.
    highest_synced: Arc<AtomicU64>,
}

impl ContentHedge {
    fn covers(&self, sequence_number: CheckpointSequenceNumber) -> bool {
        sequence_number
            <= self
                .highest_synced
                .load(Ordering::Relaxed)
                .saturating_add(CONTENT_HEDGE_WINDOW)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContentRequestKind {
    /// The first request for a checkpoint's contents.
    First,
    /// A request sent while an earlier request for the same checkpoint was still running.
    Hedge,
    /// A request sent after every earlier request for the same checkpoint failed.
    Fallback,
}

/// A request for one checkpoint's contents that is still in flight.
#[derive(Clone, Copy)]
struct InFlightContentRequest {
    start: Instant,
    kind: ContentRequestKind,
    /// For a request that is not a hedge, near the release frontier, whether its peer was slow
    /// from this node when it was sent and whether it is a slow-peer probe. A probe is not hedged
    /// right away just because its peer is slow, and while a peer is slow, only probes record
    /// response-time samples (see [`SLOW_PEER_PROBE_INTERVAL`]). `PeerNotSlow` for hedges and for
    /// downloads that started outside the hedge window.
    slow_peer: SlowPeerRequest,
}

/// Peers that could be asked for a checkpoint's contents, each with its
/// [`expected_response_time`], by whether they are slow from this node.
#[derive(Default)]
struct UnaskedPeers {
    not_slow: Vec<(anemo::Peer, Duration)>,
    slow: Vec<(anemo::Peer, Duration)>,
}

impl UnaskedPeers {
    /// The peers that are not slow, or the slow ones if every peer is slow.
    fn preferred(self) -> Vec<(anemo::Peer, Duration)> {
        if self.not_slow.is_empty() {
            self.slow
        } else {
            self.not_slow
        }
    }
}

/// A finished request for one checkpoint's contents to one peer.
struct ContentResponse {
    peer_id: PeerId,
    kind: ContentRequestKind,
    elapsed: Duration,
    /// `None` if the request failed or timed out, or the peer had no contents.
    contents: Option<VersionedFullCheckpointContents>,
}

/// Downloads the contents of one checkpoint from peers.
struct ContentFetch<'a> {
    network: &'a anemo::Network,
    peer_heights: &'a Arc<RwLock<PeerHeights>>,
    checkpoint: &'a VerifiedCheckpoint,
    use_get_checkpoint_contents_v2: bool,
    timeout: Duration,
    metrics: &'a Metrics,
}

impl ContentFetch<'_> {
    /// Peers that, as of now, have announced the checkpoint and still have its contents.
    fn candidates(&self) -> PeerBalancer {
        PeerBalancer::new(
            self.network,
            self.peer_heights.clone(),
            PeerCheckpointRequestType::Content,
        )
        .with_checkpoint(*self.checkpoint.sequence_number())
    }

    /// Whether a connected peer not in `asked` has announced the checkpoint and still has its
    /// contents. Cheaper than [`Self::candidates`], for frequent checks.
    fn has_unasked_candidate(&self, asked: &HashSet<PeerId>) -> bool {
        let sequence_number = *self.checkpoint.sequence_number();
        self.peer_heights
            .read()
            .unwrap()
            .peers_on_same_chain()
            .any(|(peer_id, info)| {
                info.has_contents(sequence_number)
                    && !asked.contains(peer_id)
                    && self.network.peer(*peer_id).is_some()
            })
    }

    /// The next peer to ask among the peers not in `asked` that have the checkpoint now.
    fn next_candidate(&self, asked: &HashSet<PeerId>) -> Option<StateSyncClient<anemo::Peer>> {
        if !self.has_unasked_candidate(asked) {
            return None;
        }
        self.candidates().excluding(asked).next()
    }

    /// Connected peers on the same chain that, as of now, have announced the checkpoint and
    /// still have its contents, whether or not they were asked.
    fn peers_with_checkpoint(&self) -> Vec<anemo::Peer> {
        let sequence_number = *self.checkpoint.sequence_number();
        self.peer_heights
            .read()
            .unwrap()
            .peers_on_same_chain()
            .filter(|(_, info)| info.has_contents(sequence_number))
            .filter_map(|(peer_id, _)| self.network.peer(*peer_id))
            .collect()
    }

    /// The peers in `peers` that are not in `asked` and have no recent failures, each with its
    /// [`expected_response_time`], split by whether they are slow from this node (see
    /// `PeerHeights::slow_peer_hedge_delay`).
    fn unasked_peers_with_expected_response_times(
        &self,
        peers: &[anemo::Peer],
        asked: &HashSet<PeerId>,
    ) -> UnaskedPeers {
        let peer_heights = self.peer_heights.read().unwrap();
        let mut unasked = UnaskedPeers::default();
        for peer in peers {
            let peer_id = peer.peer_id();
            if asked.contains(&peer_id) || peer_heights.is_failing(&peer_id) {
                continue;
            }
            let candidate = (peer.clone(), expected_response_time(peer, &peer_heights));
            if peer_heights.slow_peer_hedge_delay(&peer_id).is_some() {
                unasked.slow.push(candidate);
            } else {
                unasked.not_slow.push(candidate);
            }
        }
        unasked
    }

    /// Among the peers not in `asked` that have the checkpoint now and have no recent failures,
    /// the one with the lowest [`expected_response_time`], preferring peers that are not slow
    /// from this node.
    fn closest_candidate(&self, asked: &HashSet<PeerId>) -> Option<StateSyncClient<anemo::Peer>> {
        self.unasked_peers_with_expected_response_times(&self.peers_with_checkpoint(), asked)
            .preferred()
            .into_iter()
            .min_by_key(|(_, expected)| *expected)
            .map(|(peer, _)| StateSyncClient::new(peer))
    }

    /// The peer to send a request to other than an early hedge: with `prefer_closest`,
    /// [`Self::closest_candidate`], or [`Self::next_candidate`] if that finds no peer; otherwise
    /// [`Self::next_candidate`].
    fn candidate(
        &self,
        asked: &HashSet<PeerId>,
        prefer_closest: bool,
    ) -> Option<StateSyncClient<anemo::Peer>> {
        if prefer_closest {
            self.closest_candidate(asked)
                .or_else(|| self.next_candidate(asked))
        } else {
            self.next_candidate(asked)
        }
    }

    /// With one request in flight, the peer to hedge to right away, if any, and the event that
    /// counts the hedge. `peers` are the peers that have the checkpoint now. If the request in
    /// flight is to a peer that is slow from this node and is not a slow-peer probe, this is the
    /// peer that is not slow with the lowest [`expected_response_time`]
    /// ([`HedgeEvent::SentAroundSlowPeer`]). Otherwise, or if there is no such peer, it is the
    /// peer expected to finish at least [`CLOSER_PEER_MIN_GAIN`] before the request in flight
    /// (see [`closer_peer_to_hedge`]), preferring peers that are not slow
    /// ([`HedgeEvent::SentToCloserPeer`]). Returns `None` if the in-flight request's peer is no
    /// longer connected.
    fn early_hedge(
        &self,
        peers: &[anemo::Peer],
        asked: &HashSet<PeerId>,
        in_flight_peer_id: PeerId,
        in_flight: &InFlightContentRequest,
    ) -> Option<(StateSyncClient<anemo::Peer>, HedgeEvent)> {
        let in_flight_peer = self.network.peer(in_flight_peer_id)?;
        let (in_flight_expected, in_flight_peer_slow) = {
            let peer_heights = self.peer_heights.read().unwrap();
            (
                expected_response_time(&in_flight_peer, &peer_heights),
                peer_heights
                    .slow_peer_hedge_delay(&in_flight_peer_id)
                    .is_some(),
            )
        };
        let unasked = self.unasked_peers_with_expected_response_times(peers, asked);
        if in_flight_peer_slow
            && in_flight.slow_peer != SlowPeerRequest::Probe
            && let Some((peer, _)) = unasked
                .not_slow
                .iter()
                .min_by_key(|(_, expected)| *expected)
        {
            return Some((
                StateSyncClient::new(peer.clone()),
                HedgeEvent::SentAroundSlowPeer,
            ));
        }
        closer_peer_to_hedge(
            Instant::now(),
            in_flight.start,
            in_flight_expected,
            unasked.preferred(),
        )
        .map(|peer| (StateSyncClient::new(peer), HedgeEvent::SentToCloserPeer))
    }

    /// How long a request to `peer_id` may run before it is hedged: `hedge.delay`, or the peer's
    /// slow-response threshold if the peer is slow from this node and the threshold is shorter.
    fn hedge_delay_for(&self, hedge: &ContentHedge, peer_id: PeerId) -> Duration {
        let slow_peer_delay = self
            .peer_heights
            .read()
            .unwrap()
            .slow_peer_hedge_delay(&peer_id);
        match slow_peer_delay {
            Some(delay) if delay < hedge.delay => {
                self.metrics
                    .record_checkpoint_content_slow_peer_hedge_delay(
                        &content_metrics_peer_label(self.network, peer_id),
                        delay,
                    );
                delay
            }
            _ => hedge.delay,
        }
    }

    /// Called when a request that is not a hedge is about to be sent to `peer_id` near the
    /// release frontier: whether the peer is slow and the request is a slow-peer probe (see
    /// [`SLOW_PEER_PROBE_INTERVAL`]). Counts probes in
    /// `state_sync_checkpoint_content_slow_peer_probes`.
    fn classify_new_request(&self, peer_id: PeerId) -> SlowPeerRequest {
        let slow_peer = self
            .peer_heights
            .write()
            .unwrap()
            .classify_new_request(&peer_id);
        if slow_peer == SlowPeerRequest::Probe {
            self.metrics
                .inc_checkpoint_content_slow_peer_probe(&content_metrics_peer_label(
                    self.network,
                    peer_id,
                ));
        }
        slow_peer
    }

    fn request(
        &self,
        mut peer: StateSyncClient<anemo::Peer>,
        kind: ContentRequestKind,
    ) -> impl Future<Output = ContentResponse> {
        let peer_id = peer.inner().peer_id();
        let digest = self.checkpoint.content_digest;
        let timeout = self.timeout;
        let use_get_checkpoint_contents_v2 = self.use_get_checkpoint_contents_v2;
        debug!(
            ?timeout,
            ?kind,
            "requesting checkpoint contents from {}",
            peer_id
        );
        async move {
            let request = Request::new(digest).with_timeout(timeout);
            let start = Instant::now();
            let result = if use_get_checkpoint_contents_v2 {
                peer.get_checkpoint_contents_v2(request).await
            } else {
                peer.get_checkpoint_contents(request)
                    .await
                    .map(|r| r.map(|c| c.map(VersionedFullCheckpointContents::V1)))
            };
            let elapsed = start.elapsed();
            let contents = match result {
                Ok(response) => {
                    let contents = response.into_inner();
                    if contents.is_none() {
                        trace!("peer unable to help sync");
                    }
                    contents
                }
                Err(e) => {
                    trace!("{e:?}");
                    None
                }
            };
            ContentResponse {
                peer_id,
                kind,
                elapsed,
                contents,
            }
        }
    }

    /// Scores the peer and records metrics for a finished request. Returns the contents and
    /// their serialized size if they match the checkpoint's content digest.
    fn record_response(
        &self,
        response: ContentResponse,
    ) -> Option<(VersionedFullCheckpointContents, u64)> {
        let ContentResponse {
            peer_id,
            elapsed,
            contents,
            ..
        } = response;
        let peer_label = content_metrics_peer_label(self.network, peer_id);
        let Some(contents) = contents else {
            self.peer_heights.write().unwrap().record_failure(peer_id);
            self.metrics.record_checkpoint_content_peer_request(
                &peer_label,
                ContentRequestOutcome::Failure,
                elapsed,
            );
            return None;
        };
        if contents
            .verify_digests(self.checkpoint.content_digest)
            .is_err()
        {
            self.metrics.record_checkpoint_content_peer_request(
                &peer_label,
                ContentRequestOutcome::Failure,
                elapsed,
            );
            return None;
        }
        let size = bcs::serialized_size(&contents).expect("serialization should not fail") as u64;
        self.peer_heights
            .write()
            .unwrap()
            .record_success(peer_id, size, elapsed);
        self.metrics.record_checkpoint_content_peer_request(
            &peer_label,
            ContentRequestOutcome::Success,
            elapsed,
        );
        Some((contents, size))
    }

    /// Tries the peers that had the checkpoint when the download started, one at a time.
    async fn one_peer_at_a_time(&self) -> Option<VersionedFullCheckpointContents> {
        let mut kind = ContentRequestKind::First;
        for peer in self.candidates() {
            if kind == ContentRequestKind::Fallback {
                self.metrics.inc_checkpoint_content_fallback_requests();
            }
            let response = self.request(peer, kind).await;
            if let Some((contents, _size)) = self.record_response(response) {
                return Some(contents);
            }
            kind = ContentRequestKind::Fallback;
        }
        None
    }

    /// Sends a hedge request to another peer when the latest request has run for its peer's
    /// hedge delay (see [`Self::hedge_delay_for`]) without a valid response, with at most
    /// [`MAX_CONCURRENT_CONTENT_REQUESTS`] requests in flight. If no other peer has the
    /// checkpoint at that point, checks again every [`CONTENT_CANDIDATE_POLL_INTERVAL`]. When
    /// every request has failed, asks another peer right away. Peers are picked from the peers
    /// that have announced the checkpoint at the time each request is sent, not when the download
    /// started. The first valid response wins, and the other request is cancelled.
    ///
    /// A download near the release frontier when it starts sends its first request, hedges after
    /// the hedge delay, and requests after every request failed to the peer with the lowest
    /// [`expected_response_time`], preferring peers that are not slow from this node (see
    /// [`Self::closest_candidate`]). From its first request until it ends, it checks every
    /// [`CONTENT_CANDIDATE_POLL_INTERVAL`] which peers have the checkpoint (see
    /// [`PeerAnnouncements`]). At each check while only one request is in flight, it hedges right
    /// away to a peer that hasn't been asked yet if the request in flight is to a slow peer and is
    /// not a slow-peer probe (see [`SLOW_PEER_PROBE_INTERVAL`]) and a peer that isn't slow has the
    /// checkpoint, or if a peer is expected to finish at least [`CLOSER_PEER_MIN_GAIN`] before the
    /// request in flight (see [`Self::early_hedge`]).
    async fn hedged(&self, hedge: &ContentHedge) -> Option<VersionedFullCheckpointContents> {
        let sequence_number = *self.checkpoint.sequence_number();
        let near_frontier = hedge.covers(sequence_number);
        let wait_interval = self
            .peer_heights
            .read()
            .unwrap()
            .wait_interval_when_no_peer_to_sync_content();
        let download_start = tokio::time::Instant::now();
        let mut asked = HashSet::new();
        let mut in_flight = FuturesUnordered::new();
        // Each request still in flight, by peer.
        let mut in_flight_requests: HashMap<PeerId, InFlightContentRequest> = HashMap::new();
        let mut no_hedge_candidate_reported = false;
        let mut announcements = PeerAnnouncements::new(near_frontier);
        let next_hedge = tokio::time::sleep(hedge.delay);
        tokio::pin!(next_hedge);
        let next_other_peer_check = tokio::time::sleep(CONTENT_CANDIDATE_POLL_INTERVAL);
        tokio::pin!(next_other_peer_check);

        loop {
            if in_flight.is_empty() {
                // Either nothing was sent yet or every request failed. Ask the next peer that has
                // the checkpoint now instead of waiting for the retry interval.
                let peer = self.candidate(&asked, near_frontier);
                if let Some(peer) = peer {
                    let kind = if asked.is_empty() {
                        ContentRequestKind::First
                    } else {
                        self.metrics.inc_checkpoint_content_fallback_requests();
                        ContentRequestKind::Fallback
                    };
                    let peer_id = peer.inner().peer_id();
                    asked.insert(peer_id);
                    let slow_peer = if near_frontier {
                        self.classify_new_request(peer_id)
                    } else {
                        SlowPeerRequest::PeerNotSlow
                    };
                    in_flight_requests.insert(
                        peer_id,
                        InFlightContentRequest {
                            start: Instant::now(),
                            kind,
                            slow_peer,
                        },
                    );
                    in_flight.push(self.request(peer, kind));
                    next_hedge
                        .as_mut()
                        .reset(tokio::time::Instant::now() + self.hedge_delay_for(hedge, peer_id));
                    if kind == ContentRequestKind::First {
                        announcements.first_request_sent();
                        next_other_peer_check
                            .as_mut()
                            .reset(tokio::time::Instant::now() + CONTENT_CANDIDATE_POLL_INTERVAL);
                        if announcements.is_measuring() {
                            announcements.observe(
                                &self.peers_with_checkpoint(),
                                self.network,
                                self.metrics,
                            );
                        }
                    }
                } else if hedge.covers(sequence_number) && download_start.elapsed() < wait_interval
                {
                    // No peer that hasn't been asked has the checkpoint yet. Near the release
                    // frontier, check again after a short poll interval, or when the retry
                    // interval ends if that is sooner.
                    let next_check = tokio::time::Instant::now() + CONTENT_CANDIDATE_POLL_INTERVAL;
                    let wake =
                        tokio::time::sleep_until(next_check.min(download_start + wait_interval));
                    tokio::pin!(wake);
                    // Keep watching which peers have the checkpoint until then.
                    loop {
                        tokio::select! {
                            biased;
                            () = wake.as_mut() => break,
                            () = next_other_peer_check.as_mut(), if announcements.is_measuring() => {
                                next_other_peer_check.as_mut().reset(
                                    tokio::time::Instant::now() + CONTENT_CANDIDATE_POLL_INTERVAL,
                                );
                                announcements.observe(
                                    &self.peers_with_checkpoint(),
                                    self.network,
                                    self.metrics,
                                );
                            }
                        }
                    }
                    continue;
                } else {
                    return None;
                }
            }

            tokio::select! {
                biased;
                Some(response) = in_flight.next() => {
                    let finished_request = in_flight_requests.remove(&response.peer_id);
                    let (peer_id, kind, elapsed) = (response.peer_id, response.kind, response.elapsed);
                    let Some((contents, size)) = self.record_response(response) else {
                        continue;
                    };
                    if kind == ContentRequestKind::Hedge {
                        self.metrics.inc_checkpoint_content_hedge(HedgeEvent::Won);
                    } else if finished_request
                        .is_none_or(|request| request.slow_peer.records_response_time())
                    {
                        self.peer_heights.write().unwrap().record_content_response_time(
                            peer_id,
                            ContentResponseTime::Answered(elapsed),
                            false,
                        );
                    }
                    // Dropping `in_flight` on return cancels the requests that lost.
                    for (peer_id, request) in in_flight_requests {
                        let cancelled_after = request.start.elapsed();
                        self.metrics.record_checkpoint_content_peer_request(
                            &content_metrics_peer_label(self.network, peer_id),
                            ContentRequestOutcome::Cancelled,
                            cancelled_after,
                        );
                        let mut peer_heights = self.peer_heights.write().unwrap();
                        peer_heights.record_cancelled_request(peer_id, size, cancelled_after);
                        if request.kind != ContentRequestKind::Hedge
                            && request.slow_peer.records_response_time()
                        {
                            peer_heights.record_content_response_time(
                                peer_id,
                                ContentResponseTime::LongerThan(cancelled_after),
                                near_frontier,
                            );
                        }
                    }
                    return Some(contents);
                }
                () = next_hedge.as_mut(), if in_flight.len() < MAX_CONCURRENT_CONTENT_REQUESTS => {
                    if !hedge.covers(sequence_number) {
                        next_hedge
                            .as_mut()
                            .reset(tokio::time::Instant::now() + hedge.delay);
                        continue;
                    }
                    match self.candidate(&asked, near_frontier) {
                        Some(peer) => {
                            self.metrics.inc_checkpoint_content_hedge(HedgeEvent::Sent);
                            let peer_id = peer.inner().peer_id();
                            asked.insert(peer_id);
                            in_flight_requests.insert(
                                peer_id,
                                InFlightContentRequest {
                                    start: Instant::now(),
                                    kind: ContentRequestKind::Hedge,
                                    slow_peer: SlowPeerRequest::PeerNotSlow,
                                },
                            );
                            in_flight.push(self.request(peer, ContentRequestKind::Hedge));
                            next_hedge.as_mut().reset(
                                tokio::time::Instant::now() + self.hedge_delay_for(hedge, peer_id),
                            );
                        }
                        None => {
                            if !no_hedge_candidate_reported {
                                no_hedge_candidate_reported = true;
                                self.metrics
                                    .inc_checkpoint_content_hedge(HedgeEvent::NoCandidate);
                            }
                            next_hedge
                                .as_mut()
                                .reset(tokio::time::Instant::now() + CONTENT_CANDIDATE_POLL_INTERVAL);
                        }
                    }
                }
                () = next_other_peer_check.as_mut(), if announcements.is_measuring() => {
                    next_other_peer_check
                        .as_mut()
                        .reset(tokio::time::Instant::now() + CONTENT_CANDIDATE_POLL_INTERVAL);
                    let peers = self.peers_with_checkpoint();
                    announcements.observe(&peers, self.network, self.metrics);
                    if in_flight.len() >= MAX_CONCURRENT_CONTENT_REQUESTS {
                        continue;
                    }
                    let in_flight_request = in_flight_requests
                        .iter()
                        .next()
                        .map(|(peer_id, request)| (*peer_id, *request));
                    if let Some((in_flight_peer_id, in_flight_request)) = in_flight_request
                        && let Some((peer, event)) = self.early_hedge(
                            &peers,
                            &asked,
                            in_flight_peer_id,
                            &in_flight_request,
                        )
                    {
                        self.metrics.inc_checkpoint_content_hedge(HedgeEvent::Sent);
                        self.metrics.inc_checkpoint_content_hedge(event);
                        let peer_id = peer.inner().peer_id();
                        asked.insert(peer_id);
                        in_flight_requests.insert(
                            peer_id,
                            InFlightContentRequest {
                                start: Instant::now(),
                                kind: ContentRequestKind::Hedge,
                                slow_peer: SlowPeerRequest::PeerNotSlow,
                            },
                        );
                        in_flight.push(self.request(peer, ContentRequestKind::Hedge));
                        next_hedge.as_mut().reset(
                            tokio::time::Instant::now() + self.hedge_delay_for(hedge, peer_id),
                        );
                    }
                }
            }
        }
    }
}

/// For a download near the release frontier, tracks from its first request which peers have the
/// checkpoint, and records for each peer how long after the first request a check first saw it
/// having the checkpoint (`state_sync_checkpoint_content_peer_announce_delay`). Once the first
/// request is out, [`Self::is_measuring`] also turns on the download's checks for early hedges.
struct PeerAnnouncements {
    /// Whether the download is measured.
    measured: bool,
    /// When the first request went out, once it has, if the download is measured.
    first_request_at: Option<Instant>,
    /// Peers seen having the checkpoint since the first request.
    seen: HashSet<PeerId>,
}

impl PeerAnnouncements {
    fn new(measured: bool) -> Self {
        Self {
            measured,
            first_request_at: None,
            seen: HashSet::new(),
        }
    }

    fn first_request_sent(&mut self) {
        if self.measured && self.first_request_at.is_none() {
            self.first_request_at = Some(Instant::now());
        }
    }

    /// Whether the download is measured and its first request has gone out.
    fn is_measuring(&self) -> bool {
        self.first_request_at.is_some()
    }

    /// Records the announce delay of each peer in `peers`, the peers that have the checkpoint
    /// now, that wasn't seen having it before.
    fn observe(&mut self, peers: &[anemo::Peer], network: &anemo::Network, metrics: &Metrics) {
        let Some(first_request_at) = self.first_request_at else {
            return;
        };
        let delay = first_request_at.elapsed();
        for peer in peers {
            let peer_id = peer.peer_id();
            if self.seen.insert(peer_id) {
                metrics.record_checkpoint_content_peer_announce_delay(
                    &content_metrics_peer_label(network, peer_id),
                    delay,
                );
            }
        }
    }
}

/// How long a contents request to `peer` is expected to take: the longer of the connection's
/// round-trip time and the peer's fastest recent answer (see
/// `PeerHeights::fastest_recent_content_answer`), or the round-trip time alone if the peer has
/// no recent answers. A request needs at least one round trip, and a peer that answered in some
/// time recently is not expected to answer faster.
fn expected_response_time(peer: &anemo::Peer, peer_heights: &PeerHeights) -> Duration {
    let rtt = peer.connection_rtt();
    peer_heights
        .fastest_recent_content_answer(&peer.peer_id())
        .map_or(rtt, |fastest| fastest.max(rtt))
}

/// Among `candidates`, each with its expected response time, the one expected to finish first
/// if a request to it were sent at `now`, provided it is expected to finish at least
/// [`CLOSER_PEER_MIN_GAIN`] before the request in flight. That request was sent at
/// `in_flight_start` to a peer with expected response time `in_flight_expected`.
fn closer_peer_to_hedge<T>(
    now: Instant,
    in_flight_start: Instant,
    in_flight_expected: Duration,
    candidates: impl IntoIterator<Item = (T, Duration)>,
) -> Option<T> {
    let (candidate, expected) = candidates
        .into_iter()
        .min_by_key(|(_, expected)| *expected)?;
    (now + expected + CLOSER_PEER_MIN_GAIN <= in_flight_start + in_flight_expected)
        .then_some(candidate)
}

/// Label for per-peer content request metrics. Only peers in the network's known-peer set get
/// their own label, which bounds the label cardinality.
fn content_metrics_peer_label(network: &anemo::Network, peer_id: PeerId) -> String {
    if network.known_peers().get(&peer_id).is_some() {
        peer_id.to_string()
    } else {
        "other".to_owned()
    }
}

#[instrument(level = "debug", skip_all)]
async fn get_full_checkpoint_contents<S>(
    fetch: ContentFetch<'_>,
    store: S,
    hedge: Option<&ContentHedge>,
) -> Option<VersionedFullCheckpointContents>
where
    S: WriteStore,
{
    let checkpoint = fetch.checkpoint;
    let sequence_number = checkpoint.sequence_number;
    let digest = checkpoint.content_digest;
    if let Some(contents) = store.get_full_checkpoint_contents(Some(sequence_number), &digest) {
        debug!("store already contains checkpoint contents");
        return Some(contents);
    }

    let start = Instant::now();
    let contents = match hedge {
        None => fetch.one_peer_at_a_time().await,
        Some(hedge) => fetch.hedged(hedge).await,
    };
    let Some(contents) = contents else {
        debug!("no peers had checkpoint contents");
        return None;
    };
    fetch
        .metrics
        .record_checkpoint_content_download(start.elapsed());
    let verified_contents = VerifiedCheckpointContents::new_unchecked(contents.clone());
    store
        .insert_checkpoint_contents(checkpoint, verified_contents)
        .expect("store operation should not fail");
    Some(contents)
}

async fn update_checkpoint_watermark_metrics<S>(
    mut recv: oneshot::Receiver<()>,
    store: S,
    metrics: Metrics,
) -> Result<()>
where
    S: WriteStore + Clone + Send + Sync,
{
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
             _now = interval.tick() => {
                let highest_verified_checkpoint = store.get_highest_verified_checkpoint()
                    .expect("store operation should not fail");
                metrics.set_highest_verified_checkpoint(highest_verified_checkpoint.sequence_number);
                let highest_synced_checkpoint = store.get_highest_synced_checkpoint()
                    .expect("store operation should not fail");
                metrics.set_highest_synced_checkpoint(highest_synced_checkpoint.sequence_number);
             },
            _ = &mut recv => break,
        }
    }
    Ok(())
}
