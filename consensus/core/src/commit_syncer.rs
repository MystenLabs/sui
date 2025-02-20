// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! CommitSyncer implements efficient synchronization of committed data.
//!
//! During the operation of a committee of authorities for consensus, one or more authorities
//! can fall behind the quorum in their received and accepted blocks. This can happen due to
//! network disruptions, host crash, or other reasons. Authorities fell behind need to catch up to
//! the quorum to be able to vote on the latest leaders. So efficient synchronization is necessary
//! to minimize the impact of temporary disruptions and maintain smooth operations of the network.
//!  
//! CommitSyncer achieves efficient synchronization by relying on the following: when blocks
//! are included in commits with >= 2f+1 certifiers by stake, these blocks must have passed
//! verifications on some honest validators, so re-verifying them is unnecessary. In fact, the
//! quorum certified commits themselves can be trusted to be sent to Sui directly, but for
//! simplicity this is not done. Blocks from trusted commits still go through Core and committer.
//!
//! Another way CommitSyncer improves the efficiency of synchronization is parallel fetching:
//! commits have a simple dependency graph (linear), so it is easy to fetch ranges of commits
//! in parallel.
//!
//! Commit synchronization is an expensive operation, involving transferring large amount of data via
//! the network. And it is not on the critical path of block processing. So the heuristics for
//! synchronization, including triggers and retries, should be chosen to favor throughput and
//! efficient resource usage, over faster reactions.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use futures::{stream::FuturesOrdered, StreamExt as _};
use itertools::Itertools as _;
use mysten_metrics::spawn_logged_monitored_task;
use parking_lot::RwLock;
use rand::{prelude::SliceRandom as _, rngs::ThreadRng};
use tokio::{
    runtime::Handle,
    sync::oneshot,
    task::{JoinHandle, JoinSet},
    time::{sleep, MissedTickBehavior},
};
use tracing::{debug, info, warn};

use crate::{
    block::{BlockAPI, BlockRef, SignedBlock, VerifiedBlock},
    block_verifier::BlockVerifier,
    commit::{Commit, CommitAPI as _, CommitDigest, CommitRange, CommitRef, TrustedCommit},
    commit_vote_monitor::CommitVoteMonitor,
    context::Context,
    core_thread::CoreThreadDispatcher,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    network::NetworkClient,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    CommitConsumerMonitor, CommitIndex,
};

// Handle to stop the CommitSyncer loop.
pub(crate) struct CommitSyncerHandle {
    schedule_task: JoinHandle<()>,
    tx_shutdown: oneshot::Sender<()>,
}

impl CommitSyncerHandle {
    pub(crate) async fn stop(self) {
        let _ = self.tx_shutdown.send(());
        // Do not abort schedule task, which waits for fetches to shut down.
        if let Err(e) = self.schedule_task.await {
            if e.is_panic() {
                std::panic::resume_unwind(e.into_panic());
            }
        }
    }
}

pub(crate) struct CommitSyncer<C: NetworkClient> {
    // States shared by scheduler and fetch tasks.

    // Shared components wrapper.
    inner: Arc<Inner<C>>,

    // States only used by the scheduler.

    // Inflight requests to fetch commits from different authorities.
    inflight_fetches: JoinSet<(u32, Vec<TrustedCommit>, Vec<VerifiedBlock>)>,
    // Additional ranges of commits to fetch.
    pending_fetches: BTreeSet<CommitRange>,
    // Fetched commits and blocks by commit range.
    fetched_ranges: BTreeMap<CommitRange, Vec<VerifiedBlock>>,
    // Highest commit index among inflight and pending fetches.
    // Used to determine the start of new ranges to be fetched.
    highest_scheduled_index: Option<CommitIndex>,
    // Highest index among fetched commits, after commits and blocks are verified.
    // Used for metrics.
    highest_fetched_commit_index: CommitIndex,
    // The commit index that is the max of highest local commit index and commit index inflight to Core.
    // Used to determine if fetched blocks can be sent to Core without gaps.
    synced_commit_index: CommitIndex,
}

impl<C: NetworkClient> CommitSyncer<C> {
    pub(crate) fn new(
        context: Arc<Context>,
        core_thread_dispatcher: Arc<dyn CoreThreadDispatcher>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        commit_consumer_monitor: Arc<CommitConsumerMonitor>,
        network_client: Arc<C>,
        block_verifier: Arc<dyn BlockVerifier>,
        dag_state: Arc<RwLock<DagState>>,
    ) -> Self {
        let inner = Arc::new(Inner {
            context,
            core_thread_dispatcher,
            commit_vote_monitor,
            commit_consumer_monitor,
            network_client,
            block_verifier,
            dag_state,
        });
        let synced_commit_index = inner.dag_state.read().last_commit_index();
        CommitSyncer {
            inner,
            inflight_fetches: JoinSet::new(),
            pending_fetches: BTreeSet::new(),
            fetched_ranges: BTreeMap::new(),
            highest_scheduled_index: None,
            highest_fetched_commit_index: 0,
            synced_commit_index,
        }
    }

    pub(crate) fn start(self) -> CommitSyncerHandle {
        let (tx_shutdown, rx_shutdown) = oneshot::channel();
        let schedule_task = spawn_logged_monitored_task!(self.schedule_loop(rx_shutdown,));
        CommitSyncerHandle {
            schedule_task,
            tx_shutdown,
        }
    }

    async fn schedule_loop(mut self, mut rx_shutdown: oneshot::Receiver<()>) {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                // Periodically, schedule new fetches if the node is falling behind.
                _ = interval.tick() => {
                    self.try_schedule_once();
                }
                // Handles results from fetch tasks.
                Some(result) = self.inflight_fetches.join_next(), if !self.inflight_fetches.is_empty() => {
                    if let Err(e) = result {
                        if e.is_panic() {
                            std::panic::resume_unwind(e.into_panic());
                        }
                        warn!("Fetch cancelled. CommitSyncer shutting down: {}", e);
                        // If any fetch is cancelled or panicked, try to shutdown and exit the loop.
                        self.inflight_fetches.shutdown().await;
                        return;
                    }
                    let (target_end, commits, blocks) = result.unwrap();
                    self.handle_fetch_result(target_end, commits, blocks).await;
                }
                _ = &mut rx_shutdown => {
                    // Shutdown requested.
                    info!("CommitSyncer shutting down ...");
                    self.inflight_fetches.shutdown().await;
                    return;
                }
            }

            self.try_start_fetches();
        }
    }

    fn try_schedule_once(&mut self) {
        let quorum_commit_index = self.inner.commit_vote_monitor.quorum_commit_index();
        let local_commit_index = self.inner.dag_state.read().last_commit_index();
        let metrics = &self.inner.context.metrics.node_metrics;
        metrics
            .commit_sync_quorum_index
            .set(quorum_commit_index as i64);
        metrics
            .commit_sync_local_index
            .set(local_commit_index as i64);
        let highest_handled_index = self.inner.commit_consumer_monitor.highest_handled_commit();
        let highest_scheduled_index = self.highest_scheduled_index.unwrap_or(0);
        // Update synced_commit_index periodically to make sure it is no smaller than
        // local commit index.
        self.synced_commit_index = self.synced_commit_index.max(local_commit_index);
        let unhandled_commits_threshold = self.unhandled_commits_threshold();
        info!(
            "Checking to schedule fetches: synced_commit_index={}, highest_handled_index={}, highest_scheduled_index={}, quorum_commit_index={}, unhandled_commits_threshold={}",
            self.synced_commit_index, highest_handled_index, highest_scheduled_index, quorum_commit_index, unhandled_commits_threshold,
        );

        // TODO: cleanup inflight fetches that are no longer needed.
        let fetch_after_index = self
            .synced_commit_index
            .max(self.highest_scheduled_index.unwrap_or(0));
        // When the node is falling behind, schedule pending fetches which will be executed on later.
        for prev_end in (fetch_after_index..=quorum_commit_index)
            .step_by(self.inner.context.parameters.commit_sync_batch_size as usize)
        {
            // Create range with inclusive start and end.
            let range_start = prev_end + 1;
            let range_end = prev_end + self.inner.context.parameters.commit_sync_batch_size;
            // Commit range is not fetched when [range_start, range_end] contains less number of commits
            // than the target batch size. This is to avoid the cost of processing more and smaller batches.
            // Block broadcast, subscription and synchronization will help the node catchup.
            if quorum_commit_index < range_end {
                break;
            }
            // Pause scheduling new fetches when handling of commits is lagging.
            if highest_handled_index + unhandled_commits_threshold < range_end {
                warn!("Skip scheduling new commit fetches: consensus handler is lagging. highest_handled_index={}, highest_scheduled_index={}", highest_handled_index, highest_scheduled_index);
                break;
            }
            self.pending_fetches
                .insert((range_start..=range_end).into());
            // quorum_commit_index should be non-decreasing, so highest_scheduled_index should not
            // decrease either.
            self.highest_scheduled_index = Some(range_end);
        }
    }

    async fn handle_fetch_result(
        &mut self,
        target_end: CommitIndex,
        commits: Vec<TrustedCommit>,
        blocks: Vec<VerifiedBlock>,
    ) {
        assert!(!commits.is_empty());
        let metrics = &self.inner.context.metrics.node_metrics;
        metrics
            .commit_sync_fetched_commits
            .inc_by(commits.len() as u64);
        metrics
            .commit_sync_fetched_blocks
            .inc_by(blocks.len() as u64);
        metrics.commit_sync_total_fetched_blocks_size.inc_by(
            blocks
                .iter()
                .map(|b| b.serialized().len() as u64)
                .sum::<u64>(),
        );

        let (commit_start, commit_end) = (
            commits.first().unwrap().index(),
            commits.last().unwrap().index(),
        );
        self.highest_fetched_commit_index = self.highest_fetched_commit_index.max(commit_end);
        metrics
            .commit_sync_highest_fetched_index
            .set(self.highest_fetched_commit_index as i64);

        // Allow returning partial results, and try fetching the rest separately.
        if commit_end < target_end {
            self.pending_fetches
                .insert((commit_end + 1..=target_end).into());
        }
        // Make sure synced_commit_index is up to date.
        self.synced_commit_index = self
            .synced_commit_index
            .max(self.inner.dag_state.read().last_commit_index());
        // Only add new blocks if at least some of them are not already synced.
        if self.synced_commit_index < commit_end {
            self.fetched_ranges
                .insert((commit_start..=commit_end).into(), blocks);
        }
        // Try to process as many fetched blocks as possible.
        while let Some((fetched_commit_range, _blocks)) = self.fetched_ranges.first_key_value() {
            // Only pop fetched_ranges if there is no gap with blocks already synced.
            // Note: start, end and synced_commit_index are all inclusive.
            let (fetched_commit_range, blocks) =
                if fetched_commit_range.start() <= self.synced_commit_index + 1 {
                    self.fetched_ranges.pop_first().unwrap()
                } else {
                    // Found gap between earliest fetched block and latest synced block,
                    // so not sending additional blocks to Core.
                    metrics.commit_sync_gap_on_processing.inc();
                    break;
                };
            // Avoid sending to Core a whole batch of already synced blocks.
            if fetched_commit_range.end() <= self.synced_commit_index {
                continue;
            }

            debug!(
                "Fetched certified blocks for commit range {:?}: {}",
                fetched_commit_range,
                blocks.iter().map(|b| b.reference().to_string()).join(","),
            );
            // If core thread cannot handle the incoming blocks, it is ok to block here.
            // Also it is possible to have missing ancestors because an equivocating validator
            // may produce blocks that are not included in commits but are ancestors to other blocks.
            // Synchronizer is needed to fill in the missing ancestors in this case.
            match self.inner.core_thread_dispatcher.add_blocks(blocks).await {
                Ok(missing) => {
                    if !missing.is_empty() {
                        warn!(
                            "Fetched blocks have missing ancestors: {:?} for commit range {:?}",
                            missing, fetched_commit_range
                        );
                    }
                    for block_ref in missing {
                        let hostname = &self
                            .inner
                            .context
                            .committee
                            .authority(block_ref.author)
                            .hostname;
                        metrics
                            .commit_sync_fetch_missing_blocks
                            .with_label_values(&[hostname])
                            .inc();
                    }
                }
                Err(e) => {
                    info!("Failed to add blocks, shutting down: {}", e);
                    return;
                }
            };
            // Once commits and blocks are sent to Core, ratchet up synced_commit_index
            self.synced_commit_index = self.synced_commit_index.max(fetched_commit_range.end());
        }

        metrics
            .commit_sync_inflight_fetches
            .set(self.inflight_fetches.len() as i64);
        metrics
            .commit_sync_pending_fetches
            .set(self.pending_fetches.len() as i64);
        metrics
            .commit_sync_highest_synced_index
            .set(self.synced_commit_index as i64);
    }

    fn try_start_fetches(&mut self) {
        // Cap parallel fetches based on configured limit and committee size, to avoid overloading the network.
        // Also when there are too many fetched blocks that cannot be sent to Core before an earlier fetch
        // has not finished, reduce parallelism so the earlier fetch can retry on a better host and succeed.
        let target_parallel_fetches = self
            .inner
            .context
            .parameters
            .commit_sync_parallel_fetches
            .min(self.inner.context.committee.size() * 2 / 3)
            .min(
                self.inner
                    .context
                    .parameters
                    .commit_sync_batches_ahead
                    .saturating_sub(self.fetched_ranges.len()),
            )
            .max(1);
        // Start new fetches if there are pending batches and available slots.
        loop {
            if self.inflight_fetches.len() >= target_parallel_fetches {
                break;
            }
            let Some(commit_range) = self.pending_fetches.pop_first() else {
                break;
            };
            self.inflight_fetches
                .spawn(Self::fetch_loop(self.inner.clone(), commit_range));
        }

        let metrics = &self.inner.context.metrics.node_metrics;
        metrics
            .commit_sync_inflight_fetches
            .set(self.inflight_fetches.len() as i64);
        metrics
            .commit_sync_pending_fetches
            .set(self.pending_fetches.len() as i64);
        metrics
            .commit_sync_highest_synced_index
            .set(self.synced_commit_index as i64);
    }

    // Retries fetching commits and blocks from available authorities, until a request succeeds
    // where at least a prefix of the commit range is fetched.
    // Returns the fetched commits and blocks referenced by the commits.
    async fn fetch_loop(
        inner: Arc<Inner<C>>,
        commit_range: CommitRange,
    ) -> (CommitIndex, Vec<TrustedCommit>, Vec<VerifiedBlock>) {
        // Individual request base timeout.
        const TIMEOUT: Duration = Duration::from_secs(10);
        // Max per-request timeout will be base timeout times a multiplier.
        // At the extreme, this means there will be 120s timeout to fetch max_blocks_per_fetch blocks.
        const MAX_TIMEOUT_MULTIPLIER: u32 = 12;
        // timeout * max number of targets should be reasonably small, so the
        // system can adjust to slow network or large data sizes quickly.
        const MAX_NUM_TARGETS: usize = 24;
        let mut timeout_multiplier = 0;
        let _timer = inner
            .context
            .metrics
            .node_metrics
            .commit_sync_fetch_loop_latency
            .start_timer();
        info!("Starting to fetch commits in {commit_range:?} ...",);
        loop {
            // Attempt to fetch commits and blocks through min(committee size, MAX_NUM_TARGETS) peers.
            let mut target_authorities = inner
                .context
                .committee
                .authorities()
                .filter_map(|(i, _)| {
                    if i != inner.context.own_index {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect_vec();
            target_authorities.shuffle(&mut ThreadRng::default());
            target_authorities.truncate(MAX_NUM_TARGETS);
            // Increase timeout multiplier for each loop until MAX_TIMEOUT_MULTIPLIER.
            timeout_multiplier = (timeout_multiplier + 1).min(MAX_TIMEOUT_MULTIPLIER);
            let request_timeout = TIMEOUT * timeout_multiplier;
            // Give enough overall timeout for fetching commits and blocks.
            // - Timeout for fetching commits and commit certifying blocks.
            // - Timeout for fetching blocks referenced by the commits.
            // - Time spent on pipelining requests to fetch blocks.
            // - Another headroom to allow fetch_once() to timeout gracefully if possible.
            let fetch_timeout = request_timeout * 4;
            // Try fetching from selected target authority.
            for authority in target_authorities {
                match tokio::time::timeout(
                    fetch_timeout,
                    Self::fetch_once(
                        inner.clone(),
                        authority,
                        commit_range.clone(),
                        request_timeout,
                    ),
                )
                .await
                {
                    Ok(Ok((commits, blocks))) => {
                        info!("Finished fetching commits in {commit_range:?}",);
                        return (commit_range.end(), commits, blocks);
                    }
                    Ok(Err(e)) => {
                        let hostname = inner
                            .context
                            .committee
                            .authority(authority)
                            .hostname
                            .clone();
                        warn!("Failed to fetch {commit_range:?} from {hostname}: {}", e);
                        inner
                            .context
                            .metrics
                            .node_metrics
                            .commit_sync_fetch_once_errors
                            .with_label_values(&[&hostname, e.name()])
                            .inc();
                    }
                    Err(_) => {
                        let hostname = inner
                            .context
                            .committee
                            .authority(authority)
                            .hostname
                            .clone();
                        warn!("Timed out fetching {commit_range:?} from {authority}",);
                        inner
                            .context
                            .metrics
                            .node_metrics
                            .commit_sync_fetch_once_errors
                            .with_label_values(&[&hostname, "FetchTimeout"])
                            .inc();
                    }
                }
            }
            // Avoid busy looping, by waiting for a while before retrying.
            sleep(TIMEOUT).await;
        }
    }

    // Fetches commits and blocks from a single authority. At a high level, first the commits are
    // fetched and verified. After that, blocks referenced in the certified commits are fetched
    // and sent to Core for processing.
    async fn fetch_once(
        inner: Arc<Inner<C>>,
        target_authority: AuthorityIndex,
        commit_range: CommitRange,
        timeout: Duration,
    ) -> ConsensusResult<(Vec<TrustedCommit>, Vec<VerifiedBlock>)> {
        let _timer = inner
            .context
            .metrics
            .node_metrics
            .commit_sync_fetch_once_latency
            .start_timer();

        // 1. Fetch commits in the commit range from the target authority.
        let (serialized_commits, serialized_blocks) = inner
            .network_client
            .fetch_commits(target_authority, commit_range.clone(), timeout)
            .await?;

        // 2. Verify the response contains blocks that can certify the last returned commit,
        // and the returned commits are chained by digest, so earlier commits are certified
        // as well.
        let commits = Handle::current()
            .spawn_blocking({
                let inner = inner.clone();
                move || {
                    inner.verify_commits(
                        target_authority,
                        commit_range,
                        serialized_commits,
                        serialized_blocks,
                    )
                }
            })
            .await
            .expect("Spawn blocking should not fail")?;

        // 3. Fetch blocks referenced by the commits, from the same authority.
        let block_refs: Vec<_> = commits.iter().flat_map(|c| c.blocks()).cloned().collect();
        let num_chunks = block_refs
            .len()
            .div_ceil(inner.context.parameters.max_blocks_per_fetch)
            as u32;
        let mut requests: FuturesOrdered<_> = block_refs
            .chunks(inner.context.parameters.max_blocks_per_fetch)
            .enumerate()
            .map(|(i, request_block_refs)| {
                let inner = inner.clone();
                async move {
                    // 4. Send out pipelined fetch requests to avoid overloading the target authority.
                    sleep(timeout * i as u32 / num_chunks).await;
                    // TODO: add some retries.
                    let serialized_blocks = inner
                        .network_client
                        .fetch_blocks(
                            target_authority,
                            request_block_refs.to_vec(),
                            vec![],
                            timeout,
                        )
                        .await?;
                    // 5. Verify the same number of blocks are returned as requested.
                    if request_block_refs.len() != serialized_blocks.len() {
                        return Err(ConsensusError::UnexpectedNumberOfBlocksFetched {
                            authority: target_authority,
                            requested: request_block_refs.len(),
                            received: serialized_blocks.len(),
                        });
                    }
                    // 6. Verify returned blocks have valid formats.
                    let signed_blocks = serialized_blocks
                        .iter()
                        .map(|serialized| {
                            let block: SignedBlock = bcs::from_bytes(serialized)
                                .map_err(ConsensusError::MalformedBlock)?;
                            Ok(block)
                        })
                        .collect::<ConsensusResult<Vec<_>>>()?;
                    // 7. Verify the returned blocks match the requested block refs.
                    // If they do match, the returned blocks can be considered verified as well.
                    let mut blocks = Vec::new();
                    for ((requested_block_ref, signed_block), serialized) in request_block_refs
                        .iter()
                        .zip(signed_blocks.into_iter())
                        .zip(serialized_blocks.into_iter())
                    {
                        let signed_block_digest = VerifiedBlock::compute_digest(&serialized);
                        let received_block_ref = BlockRef::new(
                            signed_block.round(),
                            signed_block.author(),
                            signed_block_digest,
                        );
                        if *requested_block_ref != received_block_ref {
                            return Err(ConsensusError::UnexpectedBlockForCommit {
                                peer: target_authority,
                                requested: *requested_block_ref,
                                received: received_block_ref,
                            });
                        }
                        blocks.push(VerifiedBlock::new_verified(signed_block, serialized));
                    }
                    Ok(blocks)
                }
            })
            .collect();

        let mut fetched_blocks = Vec::new();
        while let Some(result) = requests.next().await {
            fetched_blocks.extend(result?);
        }

        // 8. Make sure fetched block timestamps are lower than current time.
        for block in &fetched_blocks {
            let now_ms = inner.context.clock.timestamp_utc_ms();
            let forward_drift = block.timestamp_ms().saturating_sub(now_ms);
            if forward_drift == 0 {
                continue;
            };
            let peer_hostname = &inner.context.committee.authority(target_authority).hostname;
            inner
                .context
                .metrics
                .node_metrics
                .block_timestamp_drift_wait_ms
                .with_label_values(&[peer_hostname, "commit_syncer"])
                .inc_by(forward_drift);
            let forward_drift = Duration::from_millis(forward_drift);
            if forward_drift >= inner.context.parameters.max_forward_time_drift {
                warn!(
                    "Local clock is behind a quorum of peers: local ts {}, certified block ts {}",
                    now_ms,
                    block.timestamp_ms()
                );
            }
            sleep(forward_drift).await;
        }

        Ok((commits, fetched_blocks))
    }

    fn unhandled_commits_threshold(&self) -> CommitIndex {
        self.inner.context.parameters.commit_sync_batch_size
            * (self.inner.context.parameters.commit_sync_batches_ahead as u32)
    }

    #[cfg(test)]
    fn pending_fetches(&self) -> BTreeSet<CommitRange> {
        self.pending_fetches.clone()
    }

    #[cfg(test)]
    fn fetched_ranges(&self) -> BTreeMap<CommitRange, Vec<VerifiedBlock>> {
        self.fetched_ranges.clone()
    }

    #[cfg(test)]
    fn highest_scheduled_index(&self) -> Option<CommitIndex> {
        self.highest_scheduled_index
    }

    #[cfg(test)]
    fn highest_fetched_commit_index(&self) -> CommitIndex {
        self.highest_fetched_commit_index
    }

    #[cfg(test)]
    fn synced_commit_index(&self) -> CommitIndex {
        self.synced_commit_index
    }
}

struct Inner<C: NetworkClient> {
    context: Arc<Context>,
    core_thread_dispatcher: Arc<dyn CoreThreadDispatcher>,
    commit_vote_monitor: Arc<CommitVoteMonitor>,
    commit_consumer_monitor: Arc<CommitConsumerMonitor>,
    network_client: Arc<C>,
    block_verifier: Arc<dyn BlockVerifier>,
    dag_state: Arc<RwLock<DagState>>,
}

impl<C: NetworkClient> Inner<C> {
    fn verify_commits(
        &self,
        peer: AuthorityIndex,
        commit_range: CommitRange,
        serialized_commits: Vec<Bytes>,
        serialized_blocks: Vec<Bytes>,
    ) -> ConsensusResult<Vec<TrustedCommit>> {
        // Parse and verify commits.
        let mut commits = Vec::new();
        for serialized in &serialized_commits {
            let commit: Commit =
                bcs::from_bytes(serialized).map_err(ConsensusError::MalformedCommit)?;
            let digest = TrustedCommit::compute_digest(serialized);
            if commits.is_empty() {
                // start is inclusive, so first commit must be at the start index.
                if commit.index() != commit_range.start() {
                    return Err(ConsensusError::UnexpectedStartCommit {
                        peer,
                        start: commit_range.start(),
                        commit: Box::new(commit),
                    });
                }
            } else {
                // Verify next commit increments index and references the previous digest.
                let (last_commit_digest, last_commit): &(CommitDigest, Commit) =
                    commits.last().unwrap();
                if commit.index() != last_commit.index() + 1
                    || &commit.previous_digest() != last_commit_digest
                {
                    return Err(ConsensusError::UnexpectedCommitSequence {
                        peer,
                        prev_commit: Box::new(last_commit.clone()),
                        curr_commit: Box::new(commit),
                    });
                }
            }
            // Do not process more commits past the end index.
            if commit.index() > commit_range.end() {
                break;
            }
            commits.push((digest, commit));
        }
        let Some((end_commit_digest, end_commit)) = commits.last() else {
            return Err(ConsensusError::NoCommitReceived { peer });
        };

        // Parse and verify blocks. Then accumulate votes on the end commit.
        let end_commit_ref = CommitRef::new(end_commit.index(), *end_commit_digest);
        let mut stake_aggregator = StakeAggregator::<QuorumThreshold>::new();
        for serialized in serialized_blocks {
            let block: SignedBlock =
                bcs::from_bytes(&serialized).map_err(ConsensusError::MalformedBlock)?;
            // Only block signatures need to be verified, to verify commit votes.
            // But the blocks will be sent to Core, so they need to be fully verified.
            self.block_verifier.verify_and_vote(&block)?;
            for vote in block.commit_votes() {
                if *vote == end_commit_ref {
                    stake_aggregator.add(block.author(), &self.context.committee);
                }
            }
        }

        // Check if the end commit has enough votes.
        if !stake_aggregator.reached_threshold(&self.context.committee) {
            return Err(ConsensusError::NotEnoughCommitVotes {
                stake: stake_aggregator.stake(),
                peer,
                commit: Box::new(end_commit.clone()),
            });
        }

        Ok(commits
            .into_iter()
            .zip(serialized_commits)
            .map(|((_d, c), s)| TrustedCommit::new_trusted(c, s))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use bytes::Bytes;
    use consensus_config::{AuthorityIndex, Parameters};
    use parking_lot::RwLock;

    use crate::{
        block::{BlockRef, TestBlock, VerifiedBlock},
        block_verifier::NoopBlockVerifier,
        commit::CommitRange,
        commit_syncer::CommitSyncer,
        commit_vote_monitor::CommitVoteMonitor,
        context::Context,
        core_thread::MockCoreThreadDispatcher,
        dag_state::DagState,
        error::ConsensusResult,
        network::{BlockStream, NetworkClient},
        storage::mem_store::MemStore,
        CommitConsumerMonitor, CommitDigest, CommitRef, Round,
    };

    #[derive(Default)]
    struct FakeNetworkClient {}

    #[async_trait::async_trait]
    impl NetworkClient for FakeNetworkClient {
        const SUPPORT_STREAMING: bool = true;

        async fn send_block(
            &self,
            _peer: AuthorityIndex,
            _serialized_block: &VerifiedBlock,
            _timeout: Duration,
        ) -> ConsensusResult<()> {
            unimplemented!("Unimplemented")
        }

        async fn subscribe_blocks(
            &self,
            _peer: AuthorityIndex,
            _last_received: Round,
            _timeout: Duration,
        ) -> ConsensusResult<BlockStream> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_blocks(
            &self,
            _peer: AuthorityIndex,
            _block_refs: Vec<BlockRef>,
            _highest_accepted_rounds: Vec<Round>,
            _timeout: Duration,
        ) -> ConsensusResult<Vec<Bytes>> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_commits(
            &self,
            _peer: AuthorityIndex,
            _commit_range: CommitRange,
            _timeout: Duration,
        ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_latest_blocks(
            &self,
            _peer: AuthorityIndex,
            _authorities: Vec<AuthorityIndex>,
            _timeout: Duration,
        ) -> ConsensusResult<Vec<Bytes>> {
            unimplemented!("Unimplemented")
        }

        async fn get_latest_rounds(
            &self,
            _peer: AuthorityIndex,
            _timeout: Duration,
        ) -> ConsensusResult<(Vec<Round>, Vec<Round>)> {
            unimplemented!("Unimplemented")
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn commit_syncer_start_and_pause_scheduling() {
        // SETUP
        let (context, _) = Context::new_for_test(4);
        // Use smaller batches and fetch limits for testing.
        let context = Context {
            own_index: AuthorityIndex::new_for_test(3),
            parameters: Parameters {
                commit_sync_batch_size: 5,
                commit_sync_batches_ahead: 5,
                commit_sync_parallel_fetches: 5,
                max_blocks_per_fetch: 5,
                ..context.parameters
            },
            ..context
        };
        let context = Arc::new(context);
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let core_thread_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let network_client = Arc::new(FakeNetworkClient::default());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let commit_consumer_monitor = Arc::new(CommitConsumerMonitor::new(0));
        let mut commit_syncer = CommitSyncer::new(
            context,
            core_thread_dispatcher,
            commit_vote_monitor.clone(),
            commit_consumer_monitor.clone(),
            network_client,
            block_verifier,
            dag_state,
        );

        // Check initial state.
        assert!(commit_syncer.pending_fetches().is_empty());
        assert!(commit_syncer.fetched_ranges().is_empty());
        assert!(commit_syncer.highest_scheduled_index().is_none());
        assert_eq!(commit_syncer.highest_fetched_commit_index(), 0);
        assert_eq!(commit_syncer.synced_commit_index(), 0);

        // Observe round 15 blocks voting for commit 10 from authorities 0 to 2 in CommitVoteMonitor
        for i in 0..3 {
            let test_block = TestBlock::new(15, i)
                .set_commit_votes(vec![CommitRef::new(10, CommitDigest::MIN)])
                .build();
            let block = VerifiedBlock::new_for_test(test_block);
            commit_vote_monitor.observe_block(&block);
        }

        // Fetches should be scheduled after seeing progress of other validators.
        commit_syncer.try_schedule_once();

        // Verify state.
        assert_eq!(commit_syncer.pending_fetches().len(), 2);
        assert!(commit_syncer.fetched_ranges().is_empty());
        assert_eq!(commit_syncer.highest_scheduled_index(), Some(10));
        assert_eq!(commit_syncer.highest_fetched_commit_index(), 0);
        assert_eq!(commit_syncer.synced_commit_index(), 0);

        // Observe round 40 blocks voting for commit 35 from authorities 0 to 2 in CommitVoteMonitor
        for i in 0..3 {
            let test_block = TestBlock::new(40, i)
                .set_commit_votes(vec![CommitRef::new(35, CommitDigest::MIN)])
                .build();
            let block = VerifiedBlock::new_for_test(test_block);
            commit_vote_monitor.observe_block(&block);
        }

        // Fetches should be scheduled until the unhandled commits threshold.
        commit_syncer.try_schedule_once();

        // Verify commit syncer is paused after scheduling 15 commits to index 25.
        assert_eq!(commit_syncer.unhandled_commits_threshold(), 25);
        assert_eq!(commit_syncer.highest_scheduled_index(), Some(25));
        let pending_fetches = commit_syncer.pending_fetches();
        assert_eq!(pending_fetches.len(), 5);

        // Indicate commit index 25 is consumed, and try to schedule again.
        commit_consumer_monitor.set_highest_handled_commit(25);
        commit_syncer.try_schedule_once();

        // Verify commit syncer schedules fetches up to index 35.
        assert_eq!(commit_syncer.highest_scheduled_index(), Some(35));
        let pending_fetches = commit_syncer.pending_fetches();
        assert_eq!(pending_fetches.len(), 7);

        // Verify contiguous ranges are scheduled.
        for (range, start) in pending_fetches.iter().zip((1..35).step_by(5)) {
            assert_eq!(range.start(), start);
            assert_eq!(range.end(), start + 4);
        }
    }
}
