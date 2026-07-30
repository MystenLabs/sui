// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Streamed commit catch-up for observer nodes.
//!
//! When an observer node falls behind its peer's commits (or starts mid-epoch), the
//! pull-based CommitSyncer catches up with parallel range fetches. Against the single
//! upstream peer of an observer, that parallelism provides no bandwidth benefit while
//! adding request round trips, scheduling latency and redundant transfers (blocks also
//! arrive via the live block stream and get rejected while commit-lagging).
//!
//! `CommitStreamSyncer` instead opens a single `stream_commits` server stream: the peer
//! streams consecutive certified commits together with their referenced blocks, in
//! verification windows delimited by certifier vote blocks. The client verifies each
//! window (digest-chained commits, quorum votes on the window's last commit, and blocks
//! digest-matched against the certified commits) and feeds it to Core in order.
//! Backpressure is applied by not polling the stream while the commit consumer is too
//! far behind, which propagates to the server via HTTP/2 flow control.

use std::{
    sync::{
        Arc, Weak,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use consensus_types::block::{BlockRef, TransactionIndex};
use futures::StreamExt as _;
use mysten_common::ZipDebugEqIteratorExt;
use mysten_metrics::spawn_monitored_task;
use parking_lot::{Mutex, RwLock};
use tokio::{
    runtime::Handle,
    sync::oneshot,
    task::JoinHandle,
    time::{MissedTickBehavior, sleep},
};
use tracing::{debug, info, warn};

use crate::{
    CommitConsumerMonitor, CommitIndex,
    block::{BlockAPI as _, ExtendedBlock, SignedBlock, VerifiedBlock},
    block_verifier::BlockVerifier,
    commit::{CertifiedCommit, CertifiedCommits, CommitAPI as _, CommitDigest, TrustedCommit},
    commit_syncer::{CommitSyncerHandle, verify_commit_sequence},
    commit_vote_monitor::{CommitVoteMonitor, is_commit_lagging},
    context::Context,
    core_thread::CoreThreadDispatcher,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    network::{ObserverNetworkClient, ObserverNetworkService, PeerId},
    observer_subscriber::ObserverSubscriber,
    round_tracker::RoundTracker,
    task::join_and_propagate_panic,
    transaction_vote_tracker::TransactionVoteTracker,
};

/// Number of commit-sync batches of lag below which catch-up is considered complete and
/// the live block stream can be (re)subscribed. Catch-up is entered at
/// `COMMIT_LAG_MULTIPLIER` (5) batches of lag; the gap between the two thresholds is the
/// hysteresis band preventing rapid flapping between the two modes.
pub(crate) const RESUBSCRIBE_LAG_BATCHES: u32 = 1;

/// Number of consecutive catch-up sessions without any commit progress after which
/// streamed catch-up is abandoned in favor of the pull-based CommitSyncer.
const MAX_FAILED_SESSIONS: u32 = 5;

/// The outcome of a streamed catch-up episode.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CatchupOutcome {
    /// The local commit index caught up with the peer's certified commit tip.
    CaughtUp,
    /// Repeated sessions made no progress; the caller should fall back to pull-based
    /// commit sync.
    Failed,
    /// Shutdown was requested.
    Shutdown,
}

enum SessionResult {
    /// The stream ended cleanly, i.e. the peer reached its certified commit tip.
    StreamEnded { made_progress: bool },
    /// The stream failed or returned invalid data.
    Error { made_progress: bool },
    /// Shutdown was requested.
    Shutdown,
}

/// Catches up an observer node by streaming certified commits from a peer.
pub(crate) struct CommitStreamSyncer<C: ObserverNetworkClient> {
    context: Arc<Context>,
    network_client: Arc<C>,
    core_dispatcher: Arc<dyn CoreThreadDispatcher>,
    commit_vote_monitor: Arc<CommitVoteMonitor>,
    commit_consumer_monitor: Arc<CommitConsumerMonitor>,
    block_verifier: Arc<dyn BlockVerifier>,
    transaction_vote_tracker: TransactionVoteTracker,
    round_tracker: Arc<RwLock<RoundTracker>>,
    // Held weakly so a catch-up task racing shutdown cannot keep DagState alive.
    dag_state: Weak<RwLock<DagState>>,
}

impl<C: ObserverNetworkClient> CommitStreamSyncer<C> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        context: Arc<Context>,
        network_client: Arc<C>,
        core_dispatcher: Arc<dyn CoreThreadDispatcher>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        commit_consumer_monitor: Arc<CommitConsumerMonitor>,
        block_verifier: Arc<dyn BlockVerifier>,
        transaction_vote_tracker: TransactionVoteTracker,
        round_tracker: Arc<RwLock<RoundTracker>>,
        dag_state: Weak<RwLock<DagState>>,
    ) -> Self {
        Self {
            context,
            network_client,
            core_dispatcher,
            commit_vote_monitor,
            commit_consumer_monitor,
            block_verifier,
            transaction_vote_tracker,
            round_tracker,
            dag_state,
        }
    }

    /// Whether the local commit index is close enough to the quorum commit index for the
    /// live block stream to keep up from here.
    pub(crate) fn is_caught_up(&self) -> bool {
        let Some(dag_state) = self.dag_state.upgrade() else {
            return false;
        };
        let local_commit_index = dag_state.read().last_commit_index();
        let quorum_commit_index = self.commit_vote_monitor.quorum_commit_index();
        quorum_commit_index.saturating_sub(local_commit_index)
            <= self.context.parameters.commit_sync_batch_size * RESUBSCRIBE_LAG_BATCHES
    }

    /// Runs commit-stream sessions against `peer` until the node is caught up, the
    /// sessions repeatedly fail without progress, or shutdown is requested.
    pub(crate) async fn run_catchup(
        &self,
        peer: PeerId,
        shutdown: &mut oneshot::Receiver<()>,
    ) -> CatchupOutcome {
        let node_metrics = &self.context.metrics.node_metrics;
        let _catchup_timer = node_metrics.commit_stream_catchup_duration.start_timer();
        let mut backoff = mysten_common::backoff::ExponentialBackoff::new(
            Duration::from_millis(100),
            Duration::from_secs(10),
        );
        let mut failed_sessions: u32 = 0;

        loop {
            let result = self.run_session(peer.clone(), shutdown).await;
            let (outcome_label, made_progress) = match &result {
                SessionResult::StreamEnded { made_progress } => ("ended", *made_progress),
                SessionResult::Error { made_progress } => ("error", *made_progress),
                SessionResult::Shutdown => ("shutdown", false),
            };
            node_metrics
                .commit_stream_session_result
                .with_label_values(&[outcome_label])
                .inc();

            match result {
                SessionResult::Shutdown => return CatchupOutcome::Shutdown,
                SessionResult::StreamEnded { .. } => {
                    // The peer served everything it can certify. If the remaining lag to
                    // the quorum commit index is small enough, the block stream can take
                    // over from here.
                    if self.is_caught_up() {
                        node_metrics
                            .commit_stream_session_result
                            .with_label_values(&["caught_up"])
                            .inc();
                        info!("Commit stream catch-up from {} complete", peer);
                        return CatchupOutcome::CaughtUp;
                    }
                }
                SessionResult::Error { .. } => {}
            }

            if made_progress {
                failed_sessions = 0;
                backoff.reset();
            } else {
                failed_sessions += 1;
                if failed_sessions >= MAX_FAILED_SESSIONS {
                    warn!(
                        "Commit stream catch-up from {} made no progress after {} sessions",
                        peer, failed_sessions
                    );
                    return CatchupOutcome::Failed;
                }
            }

            let delay = backoff.next().unwrap();
            tokio::select! {
                _ = &mut *shutdown => return CatchupOutcome::Shutdown,
                _ = sleep(delay) => {}
            }
        }
    }

    /// Runs a single commit-stream session: opens the stream from the local commit tip
    /// and processes verification windows until the stream ends or fails.
    async fn run_session(
        &self,
        peer: PeerId,
        shutdown: &mut oneshot::Receiver<()>,
    ) -> SessionResult {
        let node_metrics = &self.context.metrics.node_metrics;

        // Derive the start index from local state on every (re)connect, so a session
        // always requests exactly the next commit Core can accept.
        let Some(dag_state) = self.dag_state.upgrade() else {
            return SessionResult::Shutdown;
        };
        let start = dag_state.read().last_commit_index() + 1;
        drop(dag_state);

        let mut stream = match self
            .network_client
            .stream_commits(
                peer.clone(),
                start,
                self.context.parameters.commit_sync_request_timeout,
            )
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                debug!("Failed to open commit stream to {}: {}", peer, e);
                return SessionResult::Error {
                    made_progress: false,
                };
            }
        };

        let mut made_progress = false;
        // The next commit index expected from the stream, and the digest of the last
        // verified commit to chain the next window to.
        let mut expected_next = start;
        let mut previous_digest: Option<CommitDigest> = None;
        // Accumulated window contents, across stream items until certifier blocks arrive.
        let mut window_commits: Vec<Bytes> = vec![];
        let mut window_block_counts: Vec<u32> = vec![];
        let mut window_blocks: Vec<Bytes> = vec![];

        let unhandled_commits_threshold = self.context.parameters.commit_sync_batch_size
            * self.context.parameters.commit_sync_batches_ahead as u32;

        loop {
            // Backpressure: do not poll the stream while the commit consumer is too far
            // behind what has already been fed to Core. Not polling propagates to the
            // server via HTTP/2 flow control.
            let wait_index = expected_next.saturating_sub(unhandled_commits_threshold);
            tokio::select! {
                _ = &mut *shutdown => return SessionResult::Shutdown,
                _ = self.commit_consumer_monitor.wait_for_handled_commit(wait_index) => {}
            }

            let item = tokio::select! {
                _ = &mut *shutdown => return SessionResult::Shutdown,
                item = stream.next() => item,
            };
            let Some(item) = item else {
                return SessionResult::StreamEnded { made_progress };
            };

            // Validate the item's internal consistency before accumulating.
            if item.block_counts.len() != item.commits.len()
                || item.block_counts.iter().map(|c| *c as usize).sum::<usize>() != item.blocks.len()
            {
                info!(
                    "Invalid commit stream item from {}: mismatched commit and block counts",
                    peer
                );
                return SessionResult::Error { made_progress };
            }

            let item_bytes: usize = item
                .commits
                .iter()
                .chain(item.blocks.iter())
                .chain(item.certifier_blocks.iter())
                .map(|b| b.len())
                .sum();
            node_metrics
                .commit_stream_received_commits
                .inc_by(item.commits.len() as u64);
            node_metrics
                .commit_stream_received_blocks
                .inc_by(item.blocks.len() as u64);
            node_metrics
                .commit_stream_received_bytes
                .inc_by(item_bytes as u64);

            window_commits.extend(item.commits);
            window_block_counts.extend(item.block_counts);
            window_blocks.extend(item.blocks);

            // Certifier blocks delimit a verification window.
            if item.certifier_blocks.is_empty() {
                continue;
            }
            if window_commits.is_empty() {
                info!("Invalid commit stream window from {}: no commits", peer);
                return SessionResult::Error { made_progress };
            }

            match self
                .process_window(
                    peer.clone(),
                    expected_next,
                    previous_digest,
                    std::mem::take(&mut window_commits),
                    std::mem::take(&mut window_block_counts),
                    std::mem::take(&mut window_blocks),
                    item.certifier_blocks,
                )
                .await
            {
                Ok((last_index, last_digest)) => {
                    expected_next = last_index + 1;
                    previous_digest = Some(last_digest);
                    made_progress = true;
                }
                Err(ConsensusError::Shutdown) => return SessionResult::Shutdown,
                Err(e) => {
                    info!(
                        "Failed to process commit stream window from {}: {}",
                        peer, e
                    );
                    return SessionResult::Error { made_progress };
                }
            }
        }
    }

    /// Verifies a complete window and feeds it to Core. Returns the index and digest of
    /// the last verified commit.
    async fn process_window(
        &self,
        peer: PeerId,
        start_index: CommitIndex,
        previous_digest: Option<CommitDigest>,
        serialized_commits: Vec<Bytes>,
        block_counts: Vec<u32>,
        serialized_blocks: Vec<Bytes>,
        serialized_certifier_blocks: Vec<Bytes>,
    ) -> ConsensusResult<(CommitIndex, CommitDigest)> {
        // Verify the window on the blocking pool: signature verification of certifier
        // blocks and digest computations are CPU heavy.
        let (commits, blocks_per_commit, certifier_blocks_and_votes) = Handle::current()
            .spawn_blocking({
                let context = self.context.clone();
                let block_verifier = self.block_verifier.clone();
                let peer = peer.clone();
                move || {
                    verify_commit_window(
                        &context,
                        block_verifier.as_ref(),
                        peer,
                        start_index,
                        previous_digest,
                        serialized_commits,
                        block_counts,
                        serialized_blocks,
                        serialized_certifier_blocks,
                    )
                }
            })
            .await
            .expect("Spawn blocking should not fail")?;

        let last_commit = commits.last().expect("Verified window is never empty");
        let (last_index, last_digest) = (last_commit.index(), last_commit.digest());

        // Cheap clones of the certifier blocks for everything but the vote tracker.
        let certifier_blocks: Vec<_> = certifier_blocks_and_votes
            .iter()
            .map(|(block, _)| block.clone())
            .collect();

        // Guard against feeding Core a gap, which is fatal (see Core::add_certified_commits).
        // The local commit index may have advanced past parts of the window; dropping the
        // already-committed prefix is safe since blocks are idempotent in the DAG.
        let Some(dag_state) = self.dag_state.upgrade() else {
            return Err(ConsensusError::Shutdown);
        };
        let last_commit_index = dag_state.read().last_commit_index();
        drop(dag_state);
        let filtered: Vec<_> = commits
            .into_iter()
            .zip_debug_eq(blocks_per_commit)
            .filter(|(commit, _)| commit.index() > last_commit_index)
            .collect();
        if let Some((first, _)) = filtered.first()
            && first.index() != last_commit_index + 1
        {
            return Err(ConsensusError::UnexpectedStartCommit {
                peer,
                start: last_commit_index + 1,
                commit: Box::new((**first).clone()),
            });
        }
        let certified_commits: Vec<_> = filtered
            .into_iter()
            .map(|(commit, blocks)| CertifiedCommit::new_certified(commit, blocks))
            .collect();

        // Track votes carried by the received blocks, mirroring the pull-based
        // commit sync handling.
        if self.context.protocol_config.transaction_voting_enabled() {
            // Only reject votes matter from certifier blocks; committed blocks record no
            // extra votes.
            self.transaction_vote_tracker
                .add_voted_blocks(certifier_blocks_and_votes);
            for commit in &certified_commits {
                for block in commit.blocks() {
                    self.transaction_vote_tracker
                        .add_voted_blocks(vec![(block.clone(), vec![])]);
                }
            }
        }

        // Record commit votes from all received blocks. This is what keeps the quorum
        // commit index advancing while the block stream subscription is gated during
        // catch-up.
        for commit in &certified_commits {
            for block in commit.blocks() {
                self.commit_vote_monitor.observe_block(block);
            }
        }
        for block in &certifier_blocks {
            self.commit_vote_monitor.observe_block(block);
        }

        // Update the round tracker from received blocks. Excluded ancestors are not
        // available for streamed blocks, same as for fetched blocks.
        {
            let mut round_tracker = self.round_tracker.write();
            for commit in &certified_commits {
                for block in commit.blocks() {
                    round_tracker.update_from_verified_block(&ExtendedBlock {
                        block: block.clone(),
                        excluded_ancestors: vec![],
                    });
                }
            }
            for block in &certifier_blocks {
                round_tracker.update_from_verified_block(&ExtendedBlock {
                    block: block.clone(),
                    excluded_ancestors: vec![],
                });
            }
        }

        if !certified_commits.is_empty() {
            // Waiting for Core here is the Core-side backpressure on the stream.
            self.core_dispatcher
                .add_certified_commits(CertifiedCommits::new(certified_commits, certifier_blocks))
                .await
                .map_err(|_| ConsensusError::Shutdown)?;
        }

        Ok((last_index, last_digest))
    }
}

/// Verifies a window of streamed commits: the commit chain and quorum certification via
/// `verify_commit_sequence`, then the streamed blocks against the block refs of the now
/// certified commits. Matching a block's computed digest against a certified commit's
/// block ref verifies the block without checking its signature.
#[allow(clippy::too_many_arguments)]
fn verify_commit_window(
    context: &Context,
    block_verifier: &dyn BlockVerifier,
    peer: PeerId,
    start_index: CommitIndex,
    previous_digest: Option<CommitDigest>,
    serialized_commits: Vec<Bytes>,
    block_counts: Vec<u32>,
    serialized_blocks: Vec<Bytes>,
    serialized_certifier_blocks: Vec<Bytes>,
) -> ConsensusResult<(
    Vec<TrustedCommit>,
    Vec<Vec<VerifiedBlock>>,
    Vec<(VerifiedBlock, Vec<TransactionIndex>)>,
)> {
    let (commits, certifier_blocks_and_votes) = verify_commit_sequence(
        context,
        block_verifier,
        peer.clone(),
        (start_index..=CommitIndex::MAX).into(),
        previous_digest,
        serialized_commits,
        serialized_certifier_blocks,
    )?;

    if commits.len() != block_counts.len() {
        return Err(ConsensusError::InvalidCommitStreamWindow {
            peer,
            reason: format!(
                "{} commits verified but block counts for {}",
                commits.len(),
                block_counts.len()
            ),
        });
    }

    let mut blocks_iter = serialized_blocks.into_iter();
    let mut blocks_per_commit = Vec::with_capacity(commits.len());
    for (commit, block_count) in commits.iter().zip_debug_eq(block_counts) {
        if commit.blocks().len() != block_count as usize {
            return Err(ConsensusError::InvalidCommitStreamWindow {
                peer,
                reason: format!(
                    "Commit {} references {} blocks but {} were sent",
                    commit.index(),
                    commit.blocks().len(),
                    block_count
                ),
            });
        }
        let mut commit_blocks = Vec::with_capacity(block_count as usize);
        for expected_ref in commit.blocks() {
            let serialized =
                blocks_iter
                    .next()
                    .ok_or_else(|| ConsensusError::InvalidCommitStreamWindow {
                        peer: peer.clone(),
                        reason: "Fewer blocks than referenced by commits".to_string(),
                    })?;
            let signed_block: SignedBlock =
                bcs::from_bytes(&serialized).map_err(ConsensusError::MalformedBlock)?;
            let received_ref = BlockRef::new(
                signed_block.round(),
                signed_block.author(),
                VerifiedBlock::compute_digest(&serialized),
            );
            if *expected_ref != received_ref {
                return Err(ConsensusError::UnexpectedBlockForCommit {
                    peer: peer.clone(),
                    requested: *expected_ref,
                    received: received_ref,
                });
            }
            commit_blocks.push(VerifiedBlock::new_verified(signed_block, serialized));
        }
        blocks_per_commit.push(commit_blocks);
    }

    Ok((commits, blocks_per_commit, certifier_blocks_and_votes))
}

// Observer sync modes, reported via the `observer_sync_mode` metric.
const MODE_BLOCK_STREAM: u8 = 0;
const MODE_COMMIT_STREAM: u8 = 1;
const MODE_PULL_FALLBACK: u8 = 2;

/// How often the supervisor evaluates commit lag while in block stream mode.
const LAG_CHECK_INTERVAL: Duration = Duration::from_secs(2);

/// Starts the pull-based CommitSyncer if streamed catch-up is abandoned. Type-erases the
/// CommitSyncer's network client generics.
pub(crate) type FallbackCommitSyncerStarter =
    Box<dyn FnOnce() -> CommitSyncerHandle + Send + 'static>;

/// Supervises how an observer node stays in sync with its peer, alternating between:
/// - **commit stream catch-up**: while commit-lagging, the block stream subscription is
///   torn down (its blocks would be rejected anyway) and `CommitStreamSyncer` catches up
///   over a single `stream_commits` stream;
/// - **block stream**: once caught up, the live block stream is subscribed and commit lag
///   is monitored to decide when to re-enter catch-up, with hysteresis between the two
///   thresholds (see `RESUBSCRIBE_LAG_BATCHES`);
/// - **pull fallback**: if streamed catch-up repeatedly fails without progress, the
///   pull-based CommitSyncer is started and the block stream stays subscribed, matching
///   the pre-streaming behavior.
pub(crate) struct ObserverSyncSupervisor<C: ObserverNetworkClient, S: ObserverNetworkService> {
    inner: Arc<SupervisorInner<C, S>>,
    task: Mutex<Option<(oneshot::Sender<()>, JoinHandle<()>)>>,
}

struct SupervisorInner<C: ObserverNetworkClient, S: ObserverNetworkService> {
    context: Arc<Context>,
    observer_subscriber: Arc<ObserverSubscriber<C, S>>,
    syncer: CommitStreamSyncer<C>,
    commit_vote_monitor: Arc<CommitVoteMonitor>,
    dag_state: Weak<RwLock<DagState>>,
    mode: AtomicU8,
    fallback_starter: Mutex<Option<FallbackCommitSyncerStarter>>,
    fallback_handle: Mutex<Option<CommitSyncerHandle>>,
}

impl<C: ObserverNetworkClient, S: ObserverNetworkService> ObserverSyncSupervisor<C, S> {
    pub(crate) fn start(
        context: Arc<Context>,
        observer_subscriber: Arc<ObserverSubscriber<C, S>>,
        syncer: CommitStreamSyncer<C>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        dag_state: Weak<RwLock<DagState>>,
        fallback_starter: FallbackCommitSyncerStarter,
        peer: PeerId,
    ) -> Self {
        let inner = Arc::new(SupervisorInner {
            context,
            observer_subscriber,
            syncer,
            commit_vote_monitor,
            dag_state,
            mode: AtomicU8::new(MODE_COMMIT_STREAM),
            fallback_starter: Mutex::new(Some(fallback_starter)),
            fallback_handle: Mutex::new(None),
        });
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let task_inner = inner.clone();
        let task = spawn_monitored_task!(Self::run(task_inner, peer, shutdown_receiver));
        Self {
            inner,
            task: Mutex::new(Some((shutdown_sender, task))),
        }
    }

    async fn run(
        inner: Arc<SupervisorInner<C, S>>,
        peer: PeerId,
        mut shutdown: oneshot::Receiver<()>,
    ) {
        loop {
            // Catch up via the commit stream. On bootstrap there is no quorum commit
            // information yet, so catch-up always runs first: if the node is already at
            // the peer's tip, the stream ends immediately and this is a single cheap RPC.
            Self::set_mode(&inner, MODE_COMMIT_STREAM);
            match inner.syncer.run_catchup(peer.clone(), &mut shutdown).await {
                CatchupOutcome::Shutdown => return,
                CatchupOutcome::CaughtUp => {}
                CatchupOutcome::Failed => {
                    warn!(
                        "Streamed commit catch-up from {} failed, falling back to pull-based commit sync",
                        peer
                    );
                    let starter = inner.fallback_starter.lock().take();
                    if let Some(starter) = starter {
                        inner.fallback_handle.lock().replace(starter());
                    }
                    Self::set_mode(&inner, MODE_PULL_FALLBACK);
                    inner.observer_subscriber.subscribe(peer);
                    // Remain in fallback mode until shutdown.
                    let _ = shutdown.await;
                    return;
                }
            }

            // Caught up: subscribe to the live block stream, and monitor commit lag to
            // decide when to gate it and re-enter catch-up.
            Self::set_mode(&inner, MODE_BLOCK_STREAM);
            inner.observer_subscriber.subscribe(peer.clone());
            let mut interval = tokio::time::interval(LAG_CHECK_INTERVAL);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = &mut shutdown => return,
                    _ = interval.tick() => {}
                }
                let Some(dag_state) = inner.dag_state.upgrade() else {
                    return;
                };
                let local_commit_index = dag_state.read().last_commit_index();
                drop(dag_state);
                if is_commit_lagging(
                    &inner.context,
                    local_commit_index,
                    inner.commit_vote_monitor.quorum_commit_index(),
                ) {
                    info!(
                        "Commit lagging behind quorum, gating block stream to catch up via commit stream"
                    );
                    inner.observer_subscriber.stop().await;
                    break;
                }
            }
        }
    }

    fn set_mode(inner: &SupervisorInner<C, S>, mode: u8) {
        inner.mode.store(mode, Ordering::Relaxed);
        inner
            .context
            .metrics
            .node_metrics
            .observer_sync_mode
            .set(mode as i64);
    }

    /// Forces a re-subscription to the peer, e.g. after its address changed. While in
    /// commit-stream mode this is unnecessary: the catch-up session reconnects through
    /// the channel pool on its own retries.
    pub(crate) fn resubscribe(&self, peer: PeerId) {
        if self.inner.mode.load(Ordering::Relaxed) != MODE_COMMIT_STREAM {
            self.inner.observer_subscriber.subscribe(peer);
        }
    }

    pub(crate) async fn stop(&self) {
        let task = self.task.lock().take();
        if let Some((shutdown_sender, task)) = task {
            let _ = shutdown_sender.send(());
            join_and_propagate_panic(task).await;
        }
        self.inner.observer_subscriber.stop().await;
        let fallback_handle = self.inner.fallback_handle.lock().take();
        if let Some(fallback_handle) = fallback_handle {
            fallback_handle.stop().await;
        }
        // Drop the unstarted fallback CommitSyncer (if any), releasing its strong
        // references, including DagState.
        drop(self.inner.fallback_starter.lock().take());
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use consensus_config::{AuthorityIndex, Parameters};
    use consensus_types::block::Round;
    use futures::stream;
    use parking_lot::Mutex;
    use tokio::time::timeout;

    use super::*;
    use crate::{
        CommitConsumerMonitor,
        block::TestBlock,
        block_verifier::NoopBlockVerifier,
        commit::{CommitRange, CommitRef},
        core_thread::CoreError,
        network::{CommitStreamItem, ObserverBlockStream, ObserverCommitStream},
        storage::mem_store::MemStore,
        transaction_vote_tracker::TransactionVoteTracker,
    };

    // Applies certified commits to DagState so the syncer's contiguity guard sees the
    // local commit index advance, mimicking Core.
    struct TestCoreDispatcher {
        dag_state: Arc<RwLock<DagState>>,
        received_commit_indices: Mutex<Vec<CommitIndex>>,
    }

    #[async_trait]
    impl CoreThreadDispatcher for TestCoreDispatcher {
        async fn add_blocks(
            &self,
            _blocks: Vec<VerifiedBlock>,
        ) -> Result<BTreeSet<BlockRef>, CoreError> {
            unimplemented!("Unimplemented")
        }

        async fn check_block_refs(
            &self,
            _block_refs: Vec<BlockRef>,
        ) -> Result<BTreeSet<BlockRef>, CoreError> {
            unimplemented!("Unimplemented")
        }

        async fn add_certified_commits(
            &self,
            commits: CertifiedCommits,
        ) -> Result<BTreeSet<BlockRef>, CoreError> {
            let mut dag_state = self.dag_state.write();
            let mut received = self.received_commit_indices.lock();
            for commit in commits.commits() {
                received.push(commit.index());
                dag_state.set_last_commit((**commit).clone());
            }
            Ok(BTreeSet::new())
        }

        async fn new_block(&self, _round: Round, _force: bool) -> Result<(), CoreError> {
            unimplemented!("Unimplemented")
        }

        async fn get_missing_blocks(&self) -> Result<BTreeSet<BlockRef>, CoreError> {
            unimplemented!("Unimplemented")
        }

        fn set_propagation_delay(&self, _delay: Round) -> Result<(), CoreError> {
            unimplemented!("Unimplemented")
        }

        fn set_last_known_proposed_round(&self, _round: Round) -> Result<(), CoreError> {
            unimplemented!("Unimplemented")
        }
    }

    // Serves a scripted list of commit stream items on the first stream_commits call,
    // and empty streams afterwards. Records requested start indices.
    struct MockStreamClient {
        items: Mutex<Option<Vec<CommitStreamItem>>>,
        requested_starts: Mutex<Vec<CommitIndex>>,
        fail_connections: bool,
    }

    impl MockStreamClient {
        fn new(items: Vec<CommitStreamItem>) -> Self {
            Self {
                items: Mutex::new(Some(items)),
                requested_starts: Mutex::new(vec![]),
                fail_connections: false,
            }
        }

        fn new_failing() -> Self {
            Self {
                items: Mutex::new(None),
                requested_starts: Mutex::new(vec![]),
                fail_connections: true,
            }
        }
    }

    #[async_trait]
    impl ObserverNetworkClient for MockStreamClient {
        async fn stream_blocks(
            &self,
            _peer: PeerId,
            _highest_round_per_authority: Vec<Round>,
            _timeout: Duration,
        ) -> ConsensusResult<ObserverBlockStream> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_blocks(
            &self,
            _peer: PeerId,
            _block_refs: Vec<BlockRef>,
            _fetch_after_rounds: Vec<Round>,
            _fetch_missing_ancestors: bool,
            _timeout: Duration,
        ) -> ConsensusResult<Vec<Bytes>> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_commits(
            &self,
            _peer: PeerId,
            _commit_range: CommitRange,
            _timeout: Duration,
        ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
            unimplemented!("Unimplemented")
        }

        async fn stream_commits(
            &self,
            _peer: PeerId,
            start: CommitIndex,
            _timeout: Duration,
        ) -> ConsensusResult<ObserverCommitStream> {
            self.requested_starts.lock().push(start);
            if self.fail_connections {
                return Err(ConsensusError::NetworkRequest(
                    "connection failed".to_string(),
                ));
            }
            let items = self.items.lock().take().unwrap_or_default();
            Ok(Box::pin(stream::iter(items)))
        }
    }

    struct TestFixture {
        syncer: CommitStreamSyncer<MockStreamClient>,
        network_client: Arc<MockStreamClient>,
        dispatcher: Arc<TestCoreDispatcher>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        // Keeps DagState alive, since the syncer holds it weakly.
        _dag_state: Arc<RwLock<DagState>>,
    }

    fn setup(network_client: Arc<MockStreamClient>) -> TestFixture {
        let (mut context, _keys) = Context::new_for_test(4);
        context.parameters = Parameters {
            commit_sync_batch_size: 3,
            ..context.parameters
        };
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let dispatcher = Arc::new(TestCoreDispatcher {
            dag_state: dag_state.clone(),
            received_commit_indices: Mutex::new(vec![]),
        });
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let commit_consumer_monitor = Arc::new(CommitConsumerMonitor::new(0, 0));
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());
        let round_tracker = Arc::new(RwLock::new(RoundTracker::new(context.clone(), vec![])));

        let syncer = CommitStreamSyncer::new(
            context,
            network_client.clone(),
            dispatcher.clone(),
            commit_vote_monitor.clone(),
            commit_consumer_monitor.clone(),
            block_verifier,
            transaction_vote_tracker,
            round_tracker,
            Arc::downgrade(&dag_state),
        );
        TestFixture {
            syncer,
            network_client,
            dispatcher,
            commit_vote_monitor,
            _dag_state: dag_state,
        }
    }

    // Builds stream items for commits 1..=num_commits, one block per commit, with
    // certifier blocks from a quorum every `window_size` commits.
    fn build_stream_items(num_commits: u32, window_size: u32) -> Vec<CommitStreamItem> {
        let mut items = vec![];
        let mut item = empty_item();
        let mut previous_digest = CommitDigest::MIN;
        for index in 1..=num_commits {
            let block = VerifiedBlock::new_for_test(TestBlock::new(index, 0).build());
            let commit = TrustedCommit::new_for_test(
                index,
                previous_digest,
                0,
                block.reference(),
                vec![block.reference()],
            );
            previous_digest = commit.digest();
            item.commits.push(commit.serialized().clone());
            item.block_counts.push(1);
            item.blocks.push(block.serialized().clone());
            if index % window_size == 0 || index == num_commits {
                item.certifier_blocks = (0..3)
                    .map(|author| {
                        VerifiedBlock::new_for_test(
                            TestBlock::new(index + 1, author)
                                .set_commit_votes(vec![CommitRef::new(index, commit.digest())])
                                .build(),
                        )
                        .serialized()
                        .clone()
                    })
                    .collect();
                items.push(std::mem::replace(&mut item, empty_item()));
            }
        }
        items
    }

    fn empty_item() -> CommitStreamItem {
        CommitStreamItem {
            commits: vec![],
            block_counts: vec![],
            blocks: vec![],
            certifier_blocks: vec![],
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_catchup_happy_path() {
        telemetry_subscribers::init_for_testing();
        let client = Arc::new(MockStreamClient::new(build_stream_items(6, 3)));
        let fixture = setup(client);
        let peer = PeerId::Validator(AuthorityIndex::new_for_test(0));

        let (_tx, mut shutdown) = tokio::sync::oneshot::channel();
        let outcome = fixture.syncer.run_catchup(peer, &mut shutdown).await;
        assert_eq!(outcome, CatchupOutcome::CaughtUp);

        // All commits fed to Core contiguously.
        assert_eq!(
            *fixture.dispatcher.received_commit_indices.lock(),
            vec![1, 2, 3, 4, 5, 6]
        );
        // Start derived from the (initially empty) local state.
        assert_eq!(*fixture.network_client.requested_starts.lock(), vec![1]);
        // Commit votes from streamed blocks were observed.
        assert_eq!(fixture.commit_vote_monitor.quorum_commit_index(), 6);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_catchup_invalid_window_not_fed_to_core() {
        telemetry_subscribers::init_for_testing();
        // Window 2 (commits 4..6) has certifier blocks below quorum stake.
        let mut items = build_stream_items(6, 3);
        items[1].certifier_blocks.truncate(1);
        let client = Arc::new(MockStreamClient::new(items));
        let fixture = setup(client);
        let peer = PeerId::Validator(AuthorityIndex::new_for_test(0));

        let (_tx, mut shutdown) = tokio::sync::oneshot::channel();
        let outcome = fixture.syncer.run_catchup(peer, &mut shutdown).await;
        // The first window succeeded and covered the observed quorum index (3), so the
        // syncer considers itself caught up after retrying against an ended stream.
        assert_eq!(outcome, CatchupOutcome::CaughtUp);

        // Only the valid first window reached Core.
        assert_eq!(
            *fixture.dispatcher.received_commit_indices.lock(),
            vec![1, 2, 3]
        );
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_catchup_broken_commit_chain_not_fed_to_core() {
        telemetry_subscribers::init_for_testing();
        // Corrupt the second window by replacing its first commit with one that does not
        // chain to the first window.
        let mut items = build_stream_items(6, 3);
        let bogus_block = VerifiedBlock::new_for_test(TestBlock::new(4, 0).build());
        let bogus_commit = TrustedCommit::new_for_test(
            4,
            CommitDigest::MIN,
            0,
            bogus_block.reference(),
            vec![bogus_block.reference()],
        );
        items[1].commits[0] = bogus_commit.serialized().clone();
        let client = Arc::new(MockStreamClient::new(items));
        let fixture = setup(client);
        let peer = PeerId::Validator(AuthorityIndex::new_for_test(0));

        let (_tx, mut shutdown) = tokio::sync::oneshot::channel();
        let outcome = fixture.syncer.run_catchup(peer, &mut shutdown).await;
        assert_eq!(outcome, CatchupOutcome::CaughtUp);
        assert_eq!(
            *fixture.dispatcher.received_commit_indices.lock(),
            vec![1, 2, 3]
        );
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_catchup_fails_without_progress() {
        telemetry_subscribers::init_for_testing();
        let client = Arc::new(MockStreamClient::new_failing());
        let fixture = setup(client);
        let peer = PeerId::Validator(AuthorityIndex::new_for_test(0));

        let (_tx, mut shutdown) = tokio::sync::oneshot::channel();
        let outcome = fixture.syncer.run_catchup(peer, &mut shutdown).await;
        assert_eq!(outcome, CatchupOutcome::Failed);
        assert_eq!(
            fixture.network_client.requested_starts.lock().len(),
            MAX_FAILED_SESSIONS as usize
        );
        assert!(fixture.dispatcher.received_commit_indices.lock().is_empty());
    }

    #[tokio::test]
    async fn test_catchup_shutdown() {
        telemetry_subscribers::init_for_testing();
        // With connections failing, the catch-up loop reaches its retry sleep, where the
        // already-fired shutdown signal must win.
        let client = Arc::new(MockStreamClient::new_failing());
        let fixture = setup(client);
        let peer = PeerId::Validator(AuthorityIndex::new_for_test(0));

        let (tx, mut shutdown) = tokio::sync::oneshot::channel();
        tx.send(()).unwrap();
        let outcome = timeout(
            Duration::from_secs(5),
            fixture.syncer.run_catchup(peer, &mut shutdown),
        )
        .await
        .expect("Catch-up should return promptly on shutdown");
        assert_eq!(outcome, CatchupOutcome::Shutdown);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_backpressure_waits_for_consumer() {
        telemetry_subscribers::init_for_testing();
        let (mut context, _keys) = Context::new_for_test(4);
        // Tight thresholds: at most 1 commit ahead of the consumer.
        context.parameters = Parameters {
            commit_sync_batch_size: 1,
            commit_sync_batches_ahead: 1,
            ..context.parameters
        };
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let dispatcher = Arc::new(TestCoreDispatcher {
            dag_state: dag_state.clone(),
            received_commit_indices: Mutex::new(vec![]),
        });
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let commit_consumer_monitor = Arc::new(CommitConsumerMonitor::new(0, 0));
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());
        let round_tracker = Arc::new(RwLock::new(RoundTracker::new(context.clone(), vec![])));
        let client = Arc::new(MockStreamClient::new(build_stream_items(4, 1)));
        let syncer = CommitStreamSyncer::new(
            context,
            client.clone(),
            dispatcher.clone(),
            commit_vote_monitor,
            commit_consumer_monitor.clone(),
            block_verifier,
            transaction_vote_tracker,
            round_tracker,
            Arc::downgrade(&dag_state),
        );
        let peer = PeerId::Validator(AuthorityIndex::new_for_test(0));

        let (_tx, mut shutdown) = tokio::sync::oneshot::channel();
        let mut catchup = std::pin::pin!(syncer.run_catchup(peer, &mut shutdown));

        // Without consumer progress, the syncer stalls after feeding commit 1: polling
        // for commit 2 would put it more than 1 commit ahead of the consumer.
        assert!(
            timeout(Duration::from_secs(30), &mut catchup)
                .await
                .is_err(),
            "Catch-up should stall on consumer backpressure"
        );
        assert_eq!(*dispatcher.received_commit_indices.lock(), vec![1]);

        // Consumer progress unblocks the rest.
        commit_consumer_monitor.set_highest_handled_commit(4);
        let outcome = timeout(Duration::from_secs(30), catchup)
            .await
            .expect("Catch-up should finish after consumer progress");
        assert_eq!(outcome, CatchupOutcome::CaughtUp);
        assert_eq!(*dispatcher.received_commit_indices.lock(), vec![1, 2, 3, 4]);
    }

    // Supervisor tests.

    use std::sync::atomic::AtomicBool;

    use crate::{
        block::TestBlock as SupTestBlock, commit::TrustedCommit as SupTrustedCommit,
        network::NodeId, network::ObserverStreamItem,
    };

    // Mock client for supervisor tests: commit streams are scripted per session, block
    // streams never yield and are recorded.
    struct SupervisorMockClient {
        commit_stream_sessions: Mutex<std::collections::VecDeque<Vec<CommitStreamItem>>>,
        stream_commits_calls: Mutex<Vec<CommitIndex>>,
        stream_blocks_calls: Mutex<Vec<PeerId>>,
    }

    impl SupervisorMockClient {
        fn new(sessions: Vec<Vec<CommitStreamItem>>) -> Self {
            Self {
                commit_stream_sessions: Mutex::new(sessions.into()),
                stream_commits_calls: Mutex::new(vec![]),
                stream_blocks_calls: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl ObserverNetworkClient for SupervisorMockClient {
        async fn stream_blocks(
            &self,
            peer: PeerId,
            _highest_round_per_authority: Vec<Round>,
            _timeout: Duration,
        ) -> ConsensusResult<ObserverBlockStream> {
            self.stream_blocks_calls.lock().push(peer);
            Ok(Box::pin(stream::pending::<ObserverStreamItem>()))
        }

        async fn fetch_blocks(
            &self,
            _peer: PeerId,
            _block_refs: Vec<BlockRef>,
            _fetch_after_rounds: Vec<Round>,
            _fetch_missing_ancestors: bool,
            _timeout: Duration,
        ) -> ConsensusResult<Vec<Bytes>> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_commits(
            &self,
            _peer: PeerId,
            _commit_range: CommitRange,
            _timeout: Duration,
        ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
            unimplemented!("Unimplemented")
        }

        async fn stream_commits(
            &self,
            _peer: PeerId,
            start: CommitIndex,
            _timeout: Duration,
        ) -> ConsensusResult<ObserverCommitStream> {
            self.stream_commits_calls.lock().push(start);
            let items = self
                .commit_stream_sessions
                .lock()
                .pop_front()
                .unwrap_or_default();
            Ok(Box::pin(stream::iter(items)))
        }
    }

    struct SupervisorMockService {}

    #[async_trait]
    impl crate::network::ObserverNetworkService for SupervisorMockService {
        async fn handle_block(&self, _peer: PeerId, _block: Bytes) -> ConsensusResult<()> {
            Ok(())
        }

        async fn handle_stream_blocks(
            &self,
            _peer: NodeId,
            _highest_round_per_authority: Vec<Round>,
        ) -> ConsensusResult<ObserverBlockStream> {
            unimplemented!("Unimplemented")
        }

        async fn handle_fetch_blocks(
            &self,
            _peer: NodeId,
            _block_refs: Vec<BlockRef>,
            _fetch_after_rounds: Vec<Round>,
            _fetch_missing_ancestors: bool,
        ) -> ConsensusResult<Vec<Bytes>> {
            unimplemented!("Unimplemented")
        }

        async fn handle_fetch_commits(
            &self,
            _peer: NodeId,
            _commit_range: CommitRange,
        ) -> ConsensusResult<(Vec<SupTrustedCommit>, Vec<VerifiedBlock>)> {
            unimplemented!("Unimplemented")
        }

        async fn handle_stream_commits(
            &self,
            _peer: NodeId,
            _start: CommitIndex,
        ) -> ConsensusResult<ObserverCommitStream> {
            unimplemented!("Unimplemented")
        }
    }

    struct SupervisorFixture {
        supervisor: ObserverSyncSupervisor<SupervisorMockClient, SupervisorMockService>,
        client: Arc<SupervisorMockClient>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        fallback_started: Arc<AtomicBool>,
        observer_service: Arc<SupervisorMockService>,
        dag_state: Arc<RwLock<DagState>>,
    }

    fn setup_supervisor(sessions: Vec<Vec<CommitStreamItem>>) -> SupervisorFixture {
        let (mut context, _keys) = Context::new_for_test(4);
        context.parameters = Parameters {
            commit_sync_batch_size: 3,
            ..context.parameters
        };
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let dispatcher = Arc::new(TestCoreDispatcher {
            dag_state: dag_state.clone(),
            received_commit_indices: Mutex::new(vec![]),
        });
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let commit_consumer_monitor = Arc::new(CommitConsumerMonitor::new(0, 0));
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());
        let round_tracker = Arc::new(RwLock::new(RoundTracker::new(context.clone(), vec![])));
        let client = Arc::new(SupervisorMockClient::new(sessions));
        let observer_service = Arc::new(SupervisorMockService {});

        let syncer = CommitStreamSyncer::new(
            context.clone(),
            client.clone(),
            dispatcher,
            commit_vote_monitor.clone(),
            commit_consumer_monitor,
            block_verifier,
            transaction_vote_tracker,
            round_tracker,
            Arc::downgrade(&dag_state),
        );
        let observer_subscriber = Arc::new(ObserverSubscriber::new(
            context.clone(),
            client.clone(),
            observer_service.clone(),
            commit_vote_monitor.clone(),
            dag_state.clone(),
            None,
        ));
        let fallback_started = Arc::new(AtomicBool::new(false));
        let starter_flag = fallback_started.clone();
        let supervisor = ObserverSyncSupervisor::start(
            context,
            observer_subscriber,
            syncer,
            commit_vote_monitor.clone(),
            Arc::downgrade(&dag_state),
            Box::new(move || {
                starter_flag.store(true, Ordering::Relaxed);
                CommitSyncerHandle::new_for_test()
            }),
            PeerId::Validator(AuthorityIndex::new_for_test(0)),
        );
        SupervisorFixture {
            supervisor,
            client,
            commit_vote_monitor,
            fallback_started,
            observer_service,
            dag_state,
        }
    }

    async fn wait_until(condition: impl Fn() -> bool) {
        for _ in 0..300 {
            if condition() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("Condition not reached in time");
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_supervisor_bootstrap_to_block_stream() {
        telemetry_subscribers::init_for_testing();
        // Session 1: nothing to stream, observer is at the peer's tip.
        let fixture = setup_supervisor(vec![vec![]]);

        // Catch-up ends immediately and the block stream gets subscribed.
        wait_until(|| !fixture.client.stream_blocks_calls.lock().is_empty()).await;
        assert_eq!(fixture.client.stream_commits_calls.lock().len(), 1);
        assert!(!fixture.fallback_started.load(Ordering::Relaxed));

        fixture.supervisor.stop().await;
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_supervisor_lag_reenters_catchup_then_fallback() {
        telemetry_subscribers::init_for_testing();
        let fixture = setup_supervisor(vec![vec![]]);
        wait_until(|| !fixture.client.stream_blocks_calls.lock().is_empty()).await;

        // Inject commit lag: a quorum votes for commit 100 while local index is 0,
        // exceeding the batch_size (3) * COMMIT_LAG_MULTIPLIER (5) threshold.
        for author in 0..3 {
            let block = VerifiedBlock::new_for_test(
                SupTestBlock::new(10, author)
                    .set_commit_votes(vec![CommitRef::new(100, CommitDigest::MIN)])
                    .build(),
            );
            fixture.commit_vote_monitor.observe_block(&block);
        }

        // The supervisor gates the block stream and re-enters catch-up. All further
        // sessions serve empty streams while the node still lags, so streamed catch-up
        // eventually gives up and starts the pull-based fallback.
        wait_until(|| fixture.fallback_started.load(Ordering::Relaxed)).await;
        assert!(fixture.client.stream_commits_calls.lock().len() > 1);
        // In fallback mode the block stream is subscribed again.
        wait_until(|| fixture.client.stream_blocks_calls.lock().len() >= 2).await;

        fixture.supervisor.stop().await;
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_supervisor_stop_releases_references() {
        telemetry_subscribers::init_for_testing();
        let fixture = setup_supervisor(vec![vec![]]);
        wait_until(|| !fixture.client.stream_blocks_calls.lock().is_empty()).await;

        fixture.supervisor.stop().await;
        drop(fixture.supervisor);

        // Only the fixture's own references remain: the supervisor task, subscriber and
        // fallback syncer have all released theirs.
        wait_until(|| Arc::strong_count(&fixture.dag_state) == 1).await;
        wait_until(|| Arc::strong_count(&fixture.observer_service) == 1).await;
    }

    // Streams commits 1..=3 in one window for supervisor catch-up tests.
    fn supervisor_catchup_session() -> Vec<CommitStreamItem> {
        build_stream_items(3, 3)
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_supervisor_catchup_then_block_stream() {
        telemetry_subscribers::init_for_testing();
        // Session 1 streams commits 1..=3 and ends; the observed certifier votes put the
        // quorum index at 3, matching the local index, so the node is caught up.
        let fixture = setup_supervisor(vec![supervisor_catchup_session()]);

        wait_until(|| !fixture.client.stream_blocks_calls.lock().is_empty()).await;
        assert_eq!(fixture.client.stream_commits_calls.lock().len(), 1);
        assert_eq!(fixture.commit_vote_monitor.quorum_commit_index(), 3);
        assert_eq!(fixture.dag_state.read().last_commit_index(), 3);
        assert!(!fixture.fallback_started.load(Ordering::Relaxed));

        fixture.supervisor.stop().await;
    }
}
