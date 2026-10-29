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
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use consensus_types::block::{BlockRef, TransactionIndex};
use futures::{StreamExt as _, stream::FuturesUnordered};
use itertools::Itertools as _;
use mysten_common::ZipDebugEqIteratorExt;
use mysten_metrics::spawn_logged_monitored_task;
use parking_lot::{Mutex, RwLock};
use rand::{prelude::SliceRandom as _, rngs::ThreadRng};
use tokio::{
    sync::oneshot,
    task::{JoinHandle, JoinSet},
    time::{Instant, MissedTickBehavior, sleep},
};
use tracing::{debug, info, warn};

use crate::{
    CommitConsumerMonitor, CommitIndex,
    block::{BlockAPI, ExtendedBlock, SignedBlock, VerifiedBlock},
    block_verifier::BlockVerifier,
    commit::{
        CertifiedCommit, CertifiedCommits, Commit, CommitAPI as _, CommitDigest, CommitRange,
        CommitRef, TrustedCommit,
    },
    commit_vote_monitor::CommitVoteMonitor,
    context::Context,
    core_thread::CoreThreadDispatcher,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    network::{CommitSyncerClient, ObserverNetworkClient, PeerId, ValidatorNetworkClient},
    peers_pool::PeersPool,
    round_tracker::RoundTracker,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    task::{join_and_propagate_panic, shutdown_join_set, spawn_blocking},
    transaction_vote_tracker::TransactionVoteTracker,
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
        join_and_propagate_panic(self.schedule_task).await;
    }
}

pub(crate) struct CommitSyncer<VC: ValidatorNetworkClient, OC: ObserverNetworkClient> {
    // States shared by scheduler and fetch tasks.

    // Shared components wrapper.
    inner: Arc<Inner<VC, OC>>,

    // States only used by the scheduler.

    // Inflight requests to fetch commits from different authorities.
    inflight_fetches: JoinSet<(u32, CertifiedCommits)>,
    // Additional ranges of commits to fetch.
    pending_fetches: BTreeSet<CommitRange>,
    // Fetched commits and blocks by commit range, with the serialized bytes of the blocks.
    fetched_ranges: BTreeMap<CommitRange, (CertifiedCommits, u64)>,
    // Total serialized block bytes held in fetched_ranges.
    fetched_bytes: u64,
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

impl<VC, OC> CommitSyncer<VC, OC>
where
    VC: ValidatorNetworkClient,
    OC: ObserverNetworkClient,
{
    pub(crate) fn new(
        context: Arc<Context>,
        core_thread_dispatcher: Arc<dyn CoreThreadDispatcher>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        commit_consumer_monitor: Arc<CommitConsumerMonitor>,
        block_verifier: Arc<dyn BlockVerifier>,
        transaction_vote_tracker: TransactionVoteTracker,
        round_tracker: Arc<RwLock<RoundTracker>>,
        network_client: Arc<CommitSyncerClient<VC, OC>>,
        dag_state: Arc<RwLock<DagState>>,
        peers_pool: Arc<PeersPool>,
    ) -> Self {
        let size_estimate =
            SizeEstimate::new(context.protocol_config.max_transactions_in_block_bytes() * 2);
        let pacer =
            context
                .parameters
                .commit_sync_ingress_bytes_per_second
                .map(|bytes_per_second| {
                    IngressPacer::new(
                        bytes_per_second,
                        context.parameters.commit_sync_chunk_bytes as u64,
                    )
                });
        let inner = Arc::new(Inner {
            context,
            core_thread_dispatcher,
            commit_vote_monitor,
            commit_consumer_monitor,
            block_verifier,
            transaction_vote_tracker,
            round_tracker,
            network_client,
            dag_state,
            peers_pool,
            size_estimate,
            pacer,
            inflight_bytes: AtomicU64::new(0),
            active_fetches: AtomicUsize::new(0),
        });
        let synced_commit_index = inner.dag_state.read().last_commit_index();
        CommitSyncer {
            inner,
            inflight_fetches: JoinSet::new(),
            pending_fetches: BTreeSet::new(),
            fetched_ranges: BTreeMap::new(),
            fetched_bytes: 0,
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
                        shutdown_join_set(&mut self.inflight_fetches).await;
                        return;
                    }
                    let (target_end, commits) = result.unwrap();
                    self.handle_fetch_result(target_end, commits).await;
                }
                _ = &mut rx_shutdown => {
                    // Shutdown requested.
                    info!("CommitSyncer shutting down ...");
                    shutdown_join_set(&mut self.inflight_fetches).await;
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
        metrics
            .commit_sync_block_bytes_estimate
            .set(self.inner.size_estimate.block_bytes() as i64);
        metrics
            .commit_sync_stream_bytes_per_second_estimate
            .set(self.inner.size_estimate.stream_bytes_per_second() as i64);
        let highest_handled_index = self.inner.commit_consumer_monitor.highest_handled_commit();
        let highest_scheduled_index = self.highest_scheduled_index.unwrap_or(0);
        // Update synced_commit_index periodically to make sure it is no smaller than
        // local commit index.
        self.synced_commit_index = self.synced_commit_index.max(local_commit_index);
        let unhandled_commits_threshold = self.unhandled_commits_threshold();
        info!(
            "Checking to schedule fetches: synced_commit_index={}, highest_handled_index={}, highest_scheduled_index={}, quorum_commit_index={}, unhandled_commits_threshold={}",
            self.synced_commit_index,
            highest_handled_index,
            highest_scheduled_index,
            quorum_commit_index,
            unhandled_commits_threshold,
        );

        // TODO: cleanup inflight fetches that are no longer needed.
        let fetch_after_index = self
            .synced_commit_index
            .max(self.highest_scheduled_index.unwrap_or(0));
        let parameters = &self.inner.context.parameters;
        // Ranges are shortened below the batch size when the estimated block bytes of a full
        // batch exceed the range byte budget, so each fetch stays bounded in bytes when blocks
        // are large.
        let range_size = commits_per_range(
            parameters.commit_sync_batch_size,
            parameters.commit_sync_range_bytes,
            self.inner.size_estimate.commit_bytes(),
        );
        // When the node is falling behind, schedule pending fetches which will be executed on later.
        let mut range_start = fetch_after_index + 1;
        loop {
            // Create range with inclusive start and end.
            let range_end = range_start + range_size - 1;
            // Commit range is not fetched when [range_start, range_end] contains less number of commits
            // than the target batch size. This is to avoid the cost of processing more and smaller batches.
            // Block broadcast, subscription and synchronization will help the node catchup.
            if quorum_commit_index < range_end {
                break;
            }
            // Pause scheduling new fetches when handling of commits is lagging.
            if highest_handled_index + unhandled_commits_threshold < range_end {
                warn!(
                    "Skip scheduling new commit fetches: consensus handler is lagging. highest_handled_index={}, highest_scheduled_index={}",
                    highest_handled_index, highest_scheduled_index
                );
                break;
            }
            self.pending_fetches
                .insert((range_start..=range_end).into());
            // quorum_commit_index should be non-decreasing, so highest_scheduled_index should not
            // decrease either.
            self.highest_scheduled_index = Some(range_end);
            range_start = range_end + 1;
        }
    }

    async fn handle_fetch_result(
        &mut self,
        target_end: CommitIndex,
        certified_commits: CertifiedCommits,
    ) {
        assert!(!certified_commits.commits().is_empty());

        let (total_blocks_fetched, total_blocks_size_bytes) = certified_commits
            .commits()
            .iter()
            .fold((0, 0), |(blocks, bytes), c| {
                (
                    blocks + c.blocks().len(),
                    bytes
                        + c.blocks()
                            .iter()
                            .map(|b| b.serialized().len())
                            .sum::<usize>() as u64,
                )
            });
        self.inner.size_estimate.observe_commits(
            certified_commits.commits().len() as u64,
            total_blocks_fetched as u64,
        );

        let metrics = &self.inner.context.metrics.node_metrics;
        metrics
            .commit_sync_fetched_commits
            .inc_by(certified_commits.commits().len() as u64);
        metrics
            .commit_sync_fetched_blocks
            .inc_by(total_blocks_fetched as u64);
        metrics
            .commit_sync_total_fetched_blocks_size
            .inc_by(total_blocks_size_bytes);

        let (commit_start, commit_end) = (
            certified_commits.commits().first().unwrap().index(),
            certified_commits.commits().last().unwrap().index(),
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
            self.fetched_ranges.insert(
                (commit_start..=commit_end).into(),
                (certified_commits, total_blocks_size_bytes),
            );
            self.fetched_bytes += total_blocks_size_bytes;
        }
        // Try to process as many fetched blocks as possible.
        while let Some((fetched_commit_range, _)) = self.fetched_ranges.first_key_value() {
            // Only pop fetched_ranges if there is no gap with blocks already synced.
            // Note: start, end and synced_commit_index are all inclusive.
            let (fetched_commit_range, (commits, bytes)) =
                if fetched_commit_range.start() <= self.synced_commit_index + 1 {
                    self.fetched_ranges.pop_first().unwrap()
                } else {
                    // Found gap between earliest fetched block and latest synced block,
                    // so not sending additional blocks to Core.
                    metrics.commit_sync_gap_on_processing.inc();
                    break;
                };
            self.fetched_bytes = self.fetched_bytes.saturating_sub(bytes);
            // Avoid sending to Core a whole batch of already synced blocks.
            if fetched_commit_range.end() <= self.synced_commit_index {
                continue;
            }

            debug!(
                "Fetched blocks for commit range {:?}: {}",
                fetched_commit_range,
                commits
                    .commits()
                    .iter()
                    .flat_map(|c| c.blocks())
                    .map(|b| b.reference().to_string())
                    .join(","),
            );

            // If core thread cannot handle the incoming blocks, it is ok to block here
            // to slow down the commit syncer.
            match self
                .inner
                .core_thread_dispatcher
                .add_certified_commits(commits)
                .await
            {
                // Missing ancestors are possible from certification blocks, but
                // it is unnecessary to try to sync their causal history. If they are required
                // for the progress of the DAG, they will be included in a future commit.
                Ok(missing) => {
                    if !missing.is_empty() {
                        info!(
                            "Certification blocks have missing ancestors: {} for commit range {:?}",
                            missing.iter().map(|b| b.to_string()).join(","),
                            fetched_commit_range,
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

        self.update_metrics();
    }

    fn try_start_fetches(&mut self) {
        // Cap parallel fetches based on configured limit and known peers, to avoid overloading the network.
        // Also when there are too many fetched blocks that cannot be sent to Core before an earlier fetch
        // has not finished, reduce parallelism so the earlier fetch can retry on a better host and succeed.
        // For validators, use committee size for the calculation. For observers, don't apply the 2/3 limit.
        let known_peers_count = self.inner.peers_pool.get_known_peers().len();
        let target_parallel_fetches = if self.inner.context.is_validator() {
            self.inner
                .context
                .parameters
                .commit_sync_parallel_fetches
                .min(known_peers_count * 2 / 3)
                .min(
                    self.inner
                        .context
                        .parameters
                        .commit_sync_batches_ahead
                        .saturating_sub(self.fetched_ranges.len()),
                )
                .max(1)
        } else {
            // For observers, currently the node is probably connected only to another peer. In the future probably more observer peers might be available
            // to sync from. That's why we do not cap the number of parallel fetches by the number of known peers.
            self.inner
                .context
                .parameters
                .commit_sync_parallel_fetches
                .min(
                    self.inner
                        .context
                        .parameters
                        .commit_sync_batches_ahead
                        .saturating_sub(self.fetched_ranges.len()),
                )
                .max(1)
        };
        let inflight_bytes_budget = self.inner.context.parameters.commit_sync_inflight_bytes as u64;
        // Start new fetches if there are pending batches and available slots.
        loop {
            if self.inflight_fetches.len() >= target_parallel_fetches {
                break;
            }
            // Bound the estimated bytes held by inflight and fetched ranges. The first fetch is
            // always allowed so a single oversized estimate cannot stall syncing.
            if !self.inflight_fetches.is_empty()
                && self.estimated_inflight_bytes() >= inflight_bytes_budget
            {
                break;
            }
            let Some(commit_range) = self.pending_fetches.pop_first() else {
                break;
            };
            // The charge starts at the estimated bytes of the whole range and is lowered by the
            // fetch task as chunks arrive, so the budget reflects bytes still to be fetched.
            let estimated_bytes = self
                .inner
                .size_estimate
                .commit_bytes()
                .map_or(0, |bytes| bytes.saturating_mul(commit_range.size() as u64));
            let charge = InflightCharge::new(self.inner.clone(), estimated_bytes);
            self.inflight_fetches
                .spawn(Self::fetch_loop(self.inner.clone(), commit_range, charge));
        }

        self.update_metrics();
    }

    fn estimated_inflight_bytes(&self) -> u64 {
        self.inner.inflight_bytes.load(Ordering::Relaxed) + self.fetched_bytes
    }

    fn update_metrics(&self) {
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
        metrics
            .commit_sync_inflight_bytes
            .set(self.estimated_inflight_bytes() as i64);
    }

    // Retries fetching commits and blocks from available authorities, until a request succeeds
    // where at least a prefix of the commit range is fetched.
    // Returns the fetched commits and blocks referenced by the commits.
    async fn fetch_loop(
        inner: Arc<Inner<VC, OC>>,
        commit_range: CommitRange,
        mut charge: InflightCharge<VC, OC>,
    ) -> (CommitIndex, CertifiedCommits) {
        let base_timeout = inner.context.parameters.commit_sync_request_timeout;
        let _timer = inner
            .context
            .metrics
            .node_metrics
            .commit_sync_fetch_loop_latency
            .start_timer();
        info!("Starting to fetch commits in {commit_range:?} ...",);
        loop {
            // Attempt to fetch commits and blocks through min(available peers count, MAX_NUM_TARGETS) peers.
            let mut target_peers = inner.peers_pool.get_known_peers();
            target_peers.shuffle(&mut ThreadRng::default());
            target_peers.truncate(MAX_NUM_TARGETS);
            // Try fetching from selected target peers. Every network request inside fetch_once()
            // carries its own deadline, so no overall timeout is applied here and blocks fetched
            // before a failure are kept for the retry.
            for peer in target_peers {
                match Self::fetch_once(
                    inner.clone(),
                    peer.clone(),
                    commit_range.clone(),
                    &mut charge,
                )
                .await
                {
                    Ok(commits) => {
                        info!("Finished fetching commits in {commit_range:?}",);
                        return (commit_range.end(), commits);
                    }
                    Err(e) => {
                        warn!(
                            "Failed to fetch {commit_range:?} from {}: {}",
                            peer.hostname(&inner.context),
                            e
                        );
                        inner
                            .context
                            .metrics
                            .node_metrics
                            .commit_sync_fetch_once_errors
                            .with_label_values(&[peer.labelname(&inner.context).as_str(), e.name()])
                            .inc();
                    }
                }
            }
            // Avoid busy looping, by waiting for a while before retrying.
            sleep(base_timeout).await;
        }
    }

    // Fetches commits and blocks starting from a single peer. At a high level, first the commits
    // are fetched and verified. After that, blocks referenced in the certified commits are fetched
    // in byte-budgeted chunks, from the same peer and others on failures, and sent to Core for
    // processing.
    async fn fetch_once(
        inner: Arc<Inner<VC, OC>>,
        target_peer: PeerId,
        commit_range: CommitRange,
        charge: &mut InflightCharge<VC, OC>,
    ) -> ConsensusResult<CertifiedCommits> {
        let _timer = inner
            .context
            .metrics
            .node_metrics
            .commit_sync_fetch_once_latency
            .start_timer();
        let parameters = &inner.context.parameters;
        let request_timeout = parameters.commit_sync_request_timeout;
        let probe_timeout = parameters.commit_sync_probe_timeout;

        // 0. Probe the target to check reachability before committing to the full fetch.
        // This skips unreachable and slow peers quickly.
        // 1. Fetch commits in the commit range from the target authority.
        // The request timeout header is only enforced by the server, so the client bounds both
        // steps with a deadline of its own.
        let (serialized_commits, serialized_blocks) =
            tokio::time::timeout(probe_timeout + request_timeout, async {
                inner
                    .network_client
                    .probe_connectivity(target_peer.clone(), probe_timeout)
                    .await?;
                inner
                    .network_client
                    .fetch_commits(target_peer.clone(), commit_range.clone(), request_timeout)
                    .await
            })
            .await
            .map_err(|_| {
                ConsensusError::NetworkRequestTimeout(format!(
                    "fetch_commits for {commit_range:?} from {} timed out",
                    target_peer.hostname(&inner.context)
                ))
            })??;

        // 2. Verify the response contains blocks that can certify the last returned commit,
        // and the returned commits are chained by digests, so earlier commits are certified
        // as well.
        let (mut commits, commit_certifying_blocks_and_votes) = spawn_blocking({
            let context = inner.context.clone();
            let block_verifier = inner.block_verifier.clone();
            let peer = target_peer.clone();
            let commit_range = commit_range.clone();
            move || {
                Inner::<VC, OC>::verify_commits(
                    &context,
                    block_verifier.as_ref(),
                    peer,
                    commit_range,
                    serialized_commits,
                    serialized_blocks,
                )
            }
        })
        .await??;

        // 3. Keep only the prefix of commits whose estimated block bytes fit the range budget.
        // Commits in the prefix are certified by the digest chain to the verified last commit.
        // The remainder is rescheduled by the caller as a partial result.
        let kept = commits_within_budget(
            &commits,
            parameters.commit_sync_range_bytes,
            inner.size_estimate.block_bytes(),
        );
        if kept < commits.len() {
            info!(
                "Processing {} of {} fetched commits in {commit_range:?} to stay within the range byte budget",
                kept,
                commits.len()
            );
            inner
                .context
                .metrics
                .node_metrics
                .commit_sync_truncated_ranges
                .inc();
            commits.truncate(kept);
        }

        // Only the vote tracker needs the reject votes, so move them into it without cloning.
        // Cheap clones of the blocks are enough for the rest of the fetch handling.
        let commit_certifying_blocks: Vec<_> = commit_certifying_blocks_and_votes
            .iter()
            .map(|(block, _)| block.clone())
            .collect();
        if inner.context.protocol_config.transaction_voting_enabled() {
            inner
                .transaction_vote_tracker
                .add_voted_blocks(commit_certifying_blocks_and_votes);
        }

        // 4. Fetch blocks referenced by the commits in byte-budgeted chunks, starting from the
        // peer where commits were fetched and moving to other peers on failures.
        let block_refs: BTreeSet<BlockRef> =
            commits.iter().flat_map(|c| c.blocks()).cloned().collect();
        charge.set(block_refs.len() as u64 * inner.size_estimate.block_bytes());
        let mut fetched_blocks =
            Self::fetch_blocks_in_chunks(inner.clone(), target_peer.clone(), block_refs, charge)
                .await?;

        // 5. Check if the block timestamps are lower than current time - this is for metrics only.
        for block in fetched_blocks
            .values()
            .chain(commit_certifying_blocks.iter())
        {
            let now_ms = inner.context.clock.timestamp_utc_ms();
            let forward_drift = block.timestamp_ms().saturating_sub(now_ms);
            if forward_drift == 0 {
                continue;
            };
            // Extract hostname based on peer type
            inner
                .context
                .metrics
                .node_metrics
                .block_timestamp_drift_ms
                .with_label_values(&[
                    target_peer.labelname(&inner.context).as_str(),
                    "commit_syncer",
                ])
                .inc_by(forward_drift);
        }

        // 6. Now create certified commits by assigning the blocks to each commit.
        let mut certified_commits = Vec::new();
        for commit in &commits {
            let blocks = commit
                .blocks()
                .iter()
                .map(|block_ref| {
                    fetched_blocks
                        .remove(block_ref)
                        .expect("Block should exist")
                })
                .collect::<Vec<_>>();
            certified_commits.push(CertifiedCommit::new_certified(commit.clone(), blocks));
        }

        // 7. Add blocks in certified commits to the transaction vote tracker.
        for commit in &certified_commits {
            for block in commit.blocks() {
                // Only account for reject votes in the block, since they may vote on uncommitted
                // blocks or transactions. It is unnecessary to vote on the committed blocks
                // themselves.
                if inner.context.protocol_config.transaction_voting_enabled() {
                    inner
                        .transaction_vote_tracker
                        .add_voted_blocks(vec![(block.clone(), vec![])]);
                }
            }
        }

        // 8. Record commit votes from the fetched blocks.
        for commit in &certified_commits {
            for block in commit.blocks() {
                inner.commit_vote_monitor.observe_block(block);
            }
        }
        for block in &commit_certifying_blocks {
            inner.commit_vote_monitor.observe_block(block);
        }

        // 9. Update round tracker from the fetched blocks. For fetched blocks,
        // excluded_ancestors are not available so we use an empty vector.
        {
            let mut tracker = inner.round_tracker.write();
            // Update from commit blocks
            for commit in &certified_commits {
                for block in commit.blocks() {
                    tracker.update_from_verified_block(&ExtendedBlock {
                        block: block.clone(),
                        excluded_ancestors: vec![],
                    });
                }
            }
            // Update from vote blocks
            for block in &commit_certifying_blocks {
                tracker.update_from_verified_block(&ExtendedBlock {
                    block: block.clone(),
                    excluded_ancestors: vec![],
                });
            }
        }

        Ok(CertifiedCommits::new(
            certified_commits,
            commit_certifying_blocks,
        ))
    }

    // Fetches the given blocks in chunks sized by the block size estimate. The number of requests
    // in flight is derived from the measured throughput per request stream, so that the active
    // ranges together approach commit_sync_target_bytes_per_second, within the configured bounds.
    // Requests go to the target peer first. A failed request moves subsequent requests to the
    // next known peer, while blocks already fetched are kept. Fails after MAX_NUM_TARGETS failed
    // requests.
    async fn fetch_blocks_in_chunks(
        inner: Arc<Inner<VC, OC>>,
        target_peer: PeerId,
        mut missing: BTreeSet<BlockRef>,
        charge: &mut InflightCharge<VC, OC>,
    ) -> ConsensusResult<BTreeMap<BlockRef, VerifiedBlock>> {
        let parameters = &inner.context.parameters;
        let mut fetched = BTreeMap::new();
        if missing.is_empty() {
            return Ok(fetched);
        }

        let mut peers = inner.peers_pool.get_known_peers();
        peers.retain(|peer| *peer != target_peer);
        peers.shuffle(&mut ThreadRng::default());
        peers.insert(0, target_peer);
        peers.truncate(MAX_NUM_TARGETS);
        // Index of the peer that new chunk requests are sent to.
        let mut cursor = 0;
        let mut failures = 0;
        // Block refs that are in flight in a chunk request.
        let mut requested = BTreeSet::new();
        let mut inflight = FuturesUnordered::new();

        loop {
            let parallelism = chunk_parallelism(
                parameters.commit_sync_target_bytes_per_second,
                inner.active_fetches.load(Ordering::Relaxed),
                inner.size_estimate.stream_bytes_per_second(),
                parameters.commit_sync_chunk_parallelism,
                parameters.commit_sync_max_chunk_parallelism,
            );
            while inflight.len() < parallelism {
                let chunk_size = blocks_per_chunk(
                    parameters.commit_sync_chunk_bytes,
                    inner.size_estimate.block_bytes(),
                    parameters.max_blocks_per_fetch,
                );
                let chunk: Vec<BlockRef> = missing
                    .difference(&requested)
                    .take(chunk_size)
                    .cloned()
                    .collect();
                if chunk.is_empty() {
                    break;
                }
                requested.extend(chunk.iter().cloned());
                inflight.push(Self::fetch_chunk(
                    inner.clone(),
                    cursor,
                    peers[cursor].clone(),
                    chunk,
                ));
            }
            let Some((peer_index, chunk, result)) = inflight.next().await else {
                break;
            };
            for block_ref in &chunk {
                requested.remove(block_ref);
            }
            match result {
                Ok(blocks) => {
                    // A prefix of the request still carries real blocks, so the size and
                    // throughput estimates were updated from them in fetch_chunk().
                    for block in blocks {
                        missing.remove(&block.reference());
                        fetched.insert(block.reference(), block);
                    }
                }
                Err(e) => {
                    failures += 1;
                    inner
                        .context
                        .metrics
                        .node_metrics
                        .commit_sync_chunk_errors
                        .with_label_values(&[
                            peers[peer_index].labelname(&inner.context).as_str(),
                            e.name(),
                        ])
                        .inc();
                    warn!(
                        "Failed to fetch {} blocks from {}: {}",
                        chunk.len(),
                        peers[peer_index].hostname(&inner.context),
                        e
                    );
                    if failures >= MAX_NUM_TARGETS {
                        return Err(e);
                    }
                    // Nothing came back, so the request was too large for the deadline or the
                    // peer is unusable. Shrink subsequent chunks and move to the next peer.
                    inner.size_estimate.inflate_block_bytes();
                    if peer_index == cursor {
                        cursor = (cursor + 1) % peers.len();
                    }
                }
            }
            charge.set(missing.len() as u64 * inner.size_estimate.block_bytes());
            if missing.is_empty() && inflight.is_empty() {
                break;
            }
        }
        Ok(fetched)
    }

    // Fetches one chunk of blocks from a peer with a deadline derived from the expected bytes.
    // Returns the blocks that match the requested refs, which can be a prefix of the request.
    async fn fetch_chunk(
        inner: Arc<Inner<VC, OC>>,
        peer_index: usize,
        peer: PeerId,
        block_refs: Vec<BlockRef>,
    ) -> (usize, Vec<BlockRef>, ConsensusResult<Vec<VerifiedBlock>>) {
        let parameters = &inner.context.parameters;
        let expected_bytes = block_refs.len() as u64 * inner.size_estimate.block_bytes();
        if let Some(pacer) = &inner.pacer {
            pacer.acquire(expected_bytes).await;
        }
        let deadline = parameters.commit_sync_chunk_base_timeout
            + Duration::from_secs_f64(
                expected_bytes as f64
                    / parameters.commit_sync_min_fetch_bytes_per_second.max(1) as f64,
            );
        let start = Instant::now();
        let result = async {
            // The network client applies the deadline to the stream and returns blocks received
            // before it expires. The outer timeout only guards clients that do not.
            let serialized_blocks = tokio::time::timeout(
                deadline + parameters.commit_sync_chunk_base_timeout,
                inner.network_client.fetch_blocks(
                    peer.clone(),
                    block_refs.clone(),
                    vec![],
                    false,
                    deadline,
                ),
            )
            .await
            .map_err(|_| {
                ConsensusError::NetworkRequestTimeout(format!(
                    "fetch_blocks from {} timed out after {:?}",
                    peer.hostname(&inner.context),
                    deadline
                ))
            })??;
            if serialized_blocks.is_empty() {
                return Err(ConsensusError::UnexpectedNumberOfBlocksFetched {
                    peer: peer.clone(),
                    requested: block_refs.len(),
                    received: 0,
                });
            }
            let total_bytes = serialized_blocks
                .iter()
                .map(|b| b.len() as u64)
                .sum::<u64>();
            inner
                .context
                .metrics
                .node_metrics
                .commit_sync_received_blocks_size
                .inc_by(total_bytes);
            // Verify returned blocks have valid formats.
            let signed_blocks = serialized_blocks
                .iter()
                .map(|serialized| {
                    let block: SignedBlock =
                        bcs::from_bytes(serialized).map_err(ConsensusError::MalformedBlock)?;
                    Ok(block)
                })
                .collect::<ConsensusResult<Vec<_>>>()?;
            // Verify the returned blocks match the requested block refs in order. Peers return
            // blocks in request order, so a short response is a prefix of the request.
            // If they do match, the returned blocks can be considered verified as well.
            if signed_blocks.len() > block_refs.len() {
                return Err(ConsensusError::UnexpectedNumberOfBlocksFetched {
                    peer: peer.clone(),
                    requested: block_refs.len(),
                    received: signed_blocks.len(),
                });
            }
            let mut blocks = Vec::with_capacity(signed_blocks.len());
            for ((requested_block_ref, signed_block), serialized) in block_refs
                .iter()
                .take(signed_blocks.len())
                .zip_debug_eq(signed_blocks)
                .zip_debug_eq(serialized_blocks)
            {
                let signed_block_digest = VerifiedBlock::compute_digest(&serialized);
                let received_block_ref = BlockRef::new(
                    signed_block.round(),
                    signed_block.author(),
                    signed_block_digest,
                );
                if *requested_block_ref != received_block_ref {
                    return Err(ConsensusError::UnexpectedBlockForCommit {
                        peer: peer.clone(),
                        requested: *requested_block_ref,
                        received: received_block_ref,
                    });
                }
                blocks.push(VerifiedBlock::new_verified(signed_block, serialized));
            }
            inner
                .size_estimate
                .observe_blocks(total_bytes, blocks.len() as u64);
            inner
                .size_estimate
                .observe_stream(total_bytes, start.elapsed());
            Ok(blocks)
        }
        .await;
        (peer_index, block_refs, result)
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
    fn fetched_ranges(&self) -> BTreeMap<CommitRange, CertifiedCommits> {
        self.fetched_ranges
            .iter()
            .map(|(range, (commits, _))| (range.clone(), commits.clone()))
            .collect()
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

// Max number of peers tried per attempt to fetch a commit range, and max number of failed block
// chunk requests before a range fetch fails.
const MAX_NUM_TARGETS: usize = 24;

// Number of blocks per fetch request that keeps the expected bytes within the chunk budget.
fn blocks_per_chunk(chunk_bytes: usize, block_bytes: u64, max_blocks: usize) -> usize {
    let blocks = chunk_bytes as u64 / block_bytes.max(1);
    (blocks.min(usize::MAX as u64) as usize).clamp(1, max_blocks.max(1))
}

// Number of commits per range that keeps the expected block bytes within the range budget.
// Without an estimate of bytes per commit, the full batch size is used.
fn commits_per_range(batch_size: u32, range_bytes: usize, commit_bytes: Option<u64>) -> u32 {
    let batch_size = batch_size.max(1);
    match commit_bytes {
        Some(bytes) if bytes > 0 => {
            ((range_bytes as u64 / bytes).min(u32::MAX as u64) as u32).clamp(1, batch_size)
        }
        _ => batch_size,
    }
}

// Number of block fetch requests to keep in flight per range so that the active ranges together
// approach the target throughput at the measured throughput per request stream.
fn chunk_parallelism(
    target_bytes_per_second: u64,
    active_ranges: usize,
    stream_bytes_per_second: u64,
    min_parallelism: usize,
    max_parallelism: usize,
) -> usize {
    let min_parallelism = min_parallelism.max(1);
    let max_parallelism = max_parallelism.max(min_parallelism);
    let per_range = target_bytes_per_second / active_ranges.max(1) as u64;
    let streams = per_range.div_ceil(stream_bytes_per_second.max(1));
    (streams.min(usize::MAX as u64) as usize).clamp(min_parallelism, max_parallelism)
}

// Length of the longest prefix of commits whose estimated block bytes fit the range budget.
// The first commit is always included.
fn commits_within_budget(commits: &[TrustedCommit], range_bytes: usize, block_bytes: u64) -> usize {
    let mut total = 0u64;
    let mut kept = 0;
    for commit in commits {
        total = total.saturating_add(commit.blocks().len() as u64 * block_bytes);
        if kept > 0 && total > range_bytes as u64 {
            break;
        }
        kept += 1;
    }
    kept
}

const INITIAL_BLOCK_BYTES_ESTIMATE: u64 = 32 << 10;
const MIN_BLOCK_BYTES_ESTIMATE: u64 = 1 << 10;
const INITIAL_STREAM_BYTES_PER_SECOND: u64 = 16 << 20;
const MIN_STREAM_BYTES_PER_SECOND: u64 = 64 << 10;

// Running estimates of serialized bytes per block, blocks per commit and throughput per request
// stream, used to size fetch units and choose parallelism. All are exponentially weighted moving
// averages of observed fetches. The block size estimate starts high so the first chunks are small
// until observations arrive, and it grows when a chunk returns nothing so the next attempt is
// smaller.
struct SizeEstimate {
    block_bytes: AtomicU64,
    blocks_per_commit: AtomicU64,
    stream_bytes_per_second: AtomicU64,
    max_block_bytes: u64,
}

impl SizeEstimate {
    fn new(max_block_bytes: u64) -> Self {
        let max_block_bytes = max_block_bytes.max(MIN_BLOCK_BYTES_ESTIMATE);
        Self {
            block_bytes: AtomicU64::new(
                INITIAL_BLOCK_BYTES_ESTIMATE.clamp(MIN_BLOCK_BYTES_ESTIMATE, max_block_bytes),
            ),
            blocks_per_commit: AtomicU64::new(0),
            stream_bytes_per_second: AtomicU64::new(INITIAL_STREAM_BYTES_PER_SECOND),
            max_block_bytes,
        }
    }

    fn block_bytes(&self) -> u64 {
        self.block_bytes.load(Ordering::Relaxed)
    }

    fn stream_bytes_per_second(&self) -> u64 {
        self.stream_bytes_per_second.load(Ordering::Relaxed)
    }

    fn observe_stream(&self, bytes: u64, elapsed: Duration) {
        if bytes == 0 {
            return;
        }
        let seconds = elapsed.as_secs_f64().max(0.001);
        let observed = (bytes as f64 / seconds) as u64;
        let current = self.stream_bytes_per_second();
        let next = (current * 3 + observed) / 4;
        self.stream_bytes_per_second
            .store(next.max(MIN_STREAM_BYTES_PER_SECOND), Ordering::Relaxed);
    }

    fn observe_blocks(&self, total_bytes: u64, num_blocks: u64) {
        if num_blocks == 0 {
            return;
        }
        let observed = total_bytes / num_blocks;
        let current = self.block_bytes();
        let next = (current * 3 + observed) / 4;
        self.block_bytes.store(
            next.clamp(MIN_BLOCK_BYTES_ESTIMATE, self.max_block_bytes),
            Ordering::Relaxed,
        );
    }

    fn inflate_block_bytes(&self) {
        let current = self.block_bytes();
        let next = current.saturating_mul(3) / 2;
        self.block_bytes.store(
            next.clamp(MIN_BLOCK_BYTES_ESTIMATE, self.max_block_bytes),
            Ordering::Relaxed,
        );
    }

    fn observe_commits(&self, num_commits: u64, num_blocks: u64) {
        if num_commits == 0 {
            return;
        }
        let observed = num_blocks.div_ceil(num_commits).max(1);
        let current = self.blocks_per_commit.load(Ordering::Relaxed);
        let next = if current == 0 {
            observed
        } else {
            (current * 3 + observed) / 4
        };
        self.blocks_per_commit.store(next.max(1), Ordering::Relaxed);
    }

    // Estimated block bytes per commit, or None before any commit has been fetched.
    fn commit_bytes(&self) -> Option<u64> {
        let blocks_per_commit = self.blocks_per_commit.load(Ordering::Relaxed);
        if blocks_per_commit == 0 {
            None
        } else {
            Some(blocks_per_commit.saturating_mul(self.block_bytes()))
        }
    }
}

// Token bucket over the expected bytes of block fetch requests, so a lagging node cannot request
// more than a configured rate.
struct IngressPacer {
    bytes_per_second: u64,
    capacity: u64,
    state: Mutex<(f64, Instant)>,
}

impl IngressPacer {
    fn new(bytes_per_second: u64, min_capacity: u64) -> Self {
        let bytes_per_second = bytes_per_second.max(1);
        let capacity = bytes_per_second.max(min_capacity);
        Self {
            bytes_per_second,
            capacity,
            state: Mutex::new((capacity as f64, Instant::now())),
        }
    }

    async fn acquire(&self, bytes: u64) {
        let bytes = bytes.min(self.capacity) as f64;
        loop {
            let wait = {
                let mut state = self.state.lock();
                let now = Instant::now();
                let refill =
                    now.duration_since(state.1).as_secs_f64() * self.bytes_per_second as f64;
                state.0 = (state.0 + refill).min(self.capacity as f64);
                state.1 = now;
                if state.0 >= bytes {
                    state.0 -= bytes;
                    return;
                }
                Duration::from_secs_f64((bytes - state.0) / self.bytes_per_second as f64)
            };
            sleep(wait).await;
        }
    }
}

struct Inner<VC: ValidatorNetworkClient, OC: ObserverNetworkClient> {
    context: Arc<Context>,
    core_thread_dispatcher: Arc<dyn CoreThreadDispatcher>,
    commit_vote_monitor: Arc<CommitVoteMonitor>,
    commit_consumer_monitor: Arc<CommitConsumerMonitor>,
    block_verifier: Arc<dyn BlockVerifier>,
    transaction_vote_tracker: TransactionVoteTracker,
    round_tracker: Arc<RwLock<RoundTracker>>,
    network_client: Arc<CommitSyncerClient<VC, OC>>,
    dag_state: Arc<RwLock<DagState>>,
    peers_pool: Arc<PeersPool>,
    size_estimate: SizeEstimate,
    pacer: Option<IngressPacer>,
    // Estimated bytes still to be fetched across inflight ranges, maintained by InflightCharge.
    inflight_bytes: AtomicU64,
    // Number of ranges being fetched, maintained by InflightCharge.
    active_fetches: AtomicUsize,
}

// Share of the inflight byte budget held by one range fetch. The scheduler creates it with the
// range's estimated bytes, the fetch task lowers it as blocks arrive, and dropping it releases the
// remainder.
struct InflightCharge<VC: ValidatorNetworkClient, OC: ObserverNetworkClient> {
    inner: Arc<Inner<VC, OC>>,
    charged: u64,
}

impl<VC: ValidatorNetworkClient, OC: ObserverNetworkClient> InflightCharge<VC, OC> {
    fn new(inner: Arc<Inner<VC, OC>>, bytes: u64) -> Self {
        inner.inflight_bytes.fetch_add(bytes, Ordering::Relaxed);
        inner.active_fetches.fetch_add(1, Ordering::Relaxed);
        Self {
            inner,
            charged: bytes,
        }
    }

    fn set(&mut self, bytes: u64) {
        if bytes > self.charged {
            self.inner
                .inflight_bytes
                .fetch_add(bytes - self.charged, Ordering::Relaxed);
        } else {
            self.inner
                .inflight_bytes
                .fetch_sub(self.charged - bytes, Ordering::Relaxed);
        }
        self.charged = bytes;
    }
}

impl<VC: ValidatorNetworkClient, OC: ObserverNetworkClient> Drop for InflightCharge<VC, OC> {
    fn drop(&mut self) {
        self.inner
            .inflight_bytes
            .fetch_sub(self.charged, Ordering::Relaxed);
        self.inner.active_fetches.fetch_sub(1, Ordering::Relaxed);
    }
}

impl<VC: ValidatorNetworkClient, OC: ObserverNetworkClient> Inner<VC, OC> {
    /// Verifies the commits and certifies them using the provided vote blocks for the last commit.
    /// Returns, in order:
    /// - the verified commit chain as trusted commits;
    /// - the verified blocks that certify the last commit, paired with locally rejected
    ///   transaction indices for transaction vote tracking.
    fn verify_commits(
        context: &Context,
        block_verifier: &dyn BlockVerifier,
        peer: PeerId,
        commit_range: CommitRange,
        serialized_commits: Vec<Bytes>,
        serialized_vote_blocks: Vec<Bytes>,
    ) -> ConsensusResult<(
        Vec<TrustedCommit>,
        Vec<(VerifiedBlock, Vec<TransactionIndex>)>,
    )> {
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
        let mut commit_certifying_blocks = Vec::new();
        for serialized in serialized_vote_blocks {
            let block: SignedBlock =
                bcs::from_bytes(&serialized).map_err(ConsensusError::MalformedBlock)?;
            // Only block signatures need to be verified, to verify commit votes.
            // But the blocks will be sent to Core, so they need to be fully verified.
            let (block, reject_transaction_votes) =
                block_verifier.verify_and_vote(block, serialized)?;
            for vote in block.commit_votes() {
                if *vote == end_commit_ref {
                    stake_aggregator.add(block.author(), &context.committee);
                }
            }
            commit_certifying_blocks.push((block, reject_transaction_votes));
        }

        // Check if the end commit has enough votes.
        if !stake_aggregator.reached_threshold(&context.committee) {
            return Err(ConsensusError::NotEnoughCommitVotes {
                stake: stake_aggregator.stake(),
                peer,
                commit: Box::new(end_commit.clone()),
            });
        }

        let trusted_commits = commits
            .into_iter()
            .zip_debug_eq(serialized_commits)
            .map(|((_d, c), s)| TrustedCommit::new_trusted(c, s))
            .collect();
        Ok((trusted_commits, commit_certifying_blocks))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        sync::Arc,
        time::Duration,
    };

    use std::sync::atomic::Ordering;

    use bytes::Bytes;
    use consensus_config::{AuthorityIndex, NetworkKeyPair, Parameters};
    use consensus_types::block::{BlockRef, Round};
    use mysten_common::ZipDebugEqIteratorExt;
    use parking_lot::{Mutex, RwLock};

    use super::{
        InflightCharge, SizeEstimate, blocks_per_chunk, chunk_parallelism, commits_per_range,
        commits_within_budget,
    };
    use crate::{
        CommitConsumerMonitor, CommitDigest, CommitRef,
        block::{TestBlock, VerifiedBlock},
        block_verifier::NoopBlockVerifier,
        commit::{CommitRange, TrustedCommit},
        commit_syncer::CommitSyncer,
        commit_vote_monitor::CommitVoteMonitor,
        context::Context,
        core_thread::MockCoreThreadDispatcher,
        dag_state::DagState,
        error::{ConsensusError, ConsensusResult},
        network::{
            BlockStream, CommitSyncerClient, ObserverNetworkClient, PeerId, ValidatorNetworkClient,
        },
        peers_pool::{PeerService, PeersPool},
        round_tracker::RoundTracker,
        storage::mem_store::MemStore,
        transaction_vote_tracker::TransactionVoteTracker,
    };

    #[derive(Clone, Copy)]
    enum FakeBehavior {
        // Every block fetch request fails.
        Fail,
        // Block fetch requests return at most this many blocks.
        Prefix(usize),
    }

    #[derive(Default)]
    struct FakeNetworkClient {
        blocks: BTreeMap<BlockRef, Bytes>,
        behaviors: BTreeMap<AuthorityIndex, FakeBehavior>,
        calls: Mutex<BTreeMap<AuthorityIndex, usize>>,
    }

    #[async_trait::async_trait]
    impl ValidatorNetworkClient for FakeNetworkClient {
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
            peer: AuthorityIndex,
            block_refs: Vec<BlockRef>,
            _fetch_after_rounds: Vec<Round>,
            _fetch_missing_ancestors: bool,
            _timeout: Duration,
        ) -> ConsensusResult<Vec<Bytes>> {
            *self.calls.lock().entry(peer).or_default() += 1;
            let limit = match self.behaviors.get(&peer) {
                Some(FakeBehavior::Fail) => {
                    return Err(ConsensusError::NetworkRequestTimeout(
                        "injected failure".to_string(),
                    ));
                }
                Some(FakeBehavior::Prefix(limit)) => *limit,
                None => usize::MAX,
            };
            Ok(block_refs
                .iter()
                .take(limit)
                .map(|block_ref| self.blocks.get(block_ref).expect("unknown block").clone())
                .collect())
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

        #[cfg(all(test, not(msim)))]
        async fn send_block(
            &self,
            _peer: AuthorityIndex,
            _block: &VerifiedBlock,
            _timeout: Duration,
        ) -> ConsensusResult<()> {
            unimplemented!("Unimplemented")
        }
    }

    #[async_trait::async_trait]
    impl ObserverNetworkClient for FakeNetworkClient {
        async fn stream_blocks(
            &self,
            _peer: crate::network::PeerId,
            _highest_round_per_authority: Vec<Round>,
            _filter: crate::network::BlockStreamFilter,
            _timeout: Duration,
        ) -> ConsensusResult<crate::network::ObserverBlockStream> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_blocks(
            &self,
            _peer: crate::network::PeerId,
            _block_refs: Vec<BlockRef>,
            _highest_accepted_rounds: Vec<Round>,
            _breadth_first: bool,
            _timeout: Duration,
        ) -> ConsensusResult<Vec<Bytes>> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_commits(
            &self,
            _peer: crate::network::PeerId,
            _commit_range: CommitRange,
            _timeout: Duration,
        ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
            unimplemented!("Unimplemented")
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn commit_syncer_observer_node_basic() {
        // Test basic Observer node behavior for commit syncing
        // An Observer node should be able to sync from both validator and observer peers

        // Create an Observer node context (own_index = MAX)
        let (mut context, _) = Context::new_for_test(4);
        context.own_index = AuthorityIndex::MAX; // Mark this as an observer node
        context.parameters = Parameters {
            commit_sync_batch_size: 5,
            commit_sync_batches_ahead: 10,
            commit_sync_parallel_fetches: 5,
            max_blocks_per_fetch: 5,
            ..context.parameters
        };
        let context = Arc::new(context);

        // Setup observer node commit syncer
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let core_thread_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let mock_client = Arc::new(FakeNetworkClient::default());
        let network_client = Arc::new(CommitSyncerClient::new(
            context.clone(),
            Some(mock_client.clone()),
            Some(mock_client.clone()),
        ));
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let commit_consumer_monitor = Arc::new(CommitConsumerMonitor::new(0, Some(0)));
        let round_tracker = Arc::new(RwLock::new(RoundTracker::new(context.clone(), vec![])));

        // Create PeersPool - Observer typically connects to one validator
        let peers_pool = Arc::new(PeersPool::new(context.clone()));
        // Register the validator peer that the observer connects to
        peers_pool
            .register_validator(
                AuthorityIndex::new_for_test(0),
                vec![PeerService::Validator, PeerService::Observer],
            )
            .unwrap();

        let mut commit_syncer = CommitSyncer::new(
            context.clone(),
            core_thread_dispatcher,
            commit_vote_monitor.clone(),
            commit_consumer_monitor.clone(),
            block_verifier,
            transaction_vote_tracker,
            round_tracker,
            network_client,
            dag_state,
            peers_pool.clone(),
        );

        // Verify this is recognized as an observer
        assert!(!context.is_validator(), "Should be an observer node");

        // Simulate the observer seeing commits from its connected validator
        for i in 0..3 {
            let test_block = TestBlock::new(10, i)
                .set_commit_votes(vec![CommitRef::new(5, CommitDigest::MIN)])
                .build();
            let block = VerifiedBlock::new_for_test(test_block);
            commit_vote_monitor.observe_block(&block);
        }

        // Observer should be able to schedule commit fetches
        commit_syncer.try_schedule_once();
        assert_eq!(commit_syncer.pending_fetches().len(), 1);
        assert_eq!(commit_syncer.highest_scheduled_index(), Some(5));

        // Start fetches - observer should be able to fetch from its single peer
        commit_syncer.try_start_fetches();
        assert_eq!(
            commit_syncer.inflight_fetches.len(),
            1,
            "Should start fetch from single peer"
        );
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn commit_syncer_observer_with_multiple_peers() {
        // Test Observer node behavior when connected to multiple peers
        // This simulates an observer that can sync from multiple sources

        let (mut context, _) = Context::new_for_test(4);
        context.own_index = AuthorityIndex::MAX; // Observer node
        context.parameters = Parameters {
            commit_sync_batch_size: 5,
            commit_sync_batches_ahead: 10,
            commit_sync_parallel_fetches: 8, // Allow more parallelism
            max_blocks_per_fetch: 5,
            ..context.parameters
        };
        let context = Arc::new(context);

        // Setup
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let core_thread_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let network_client = Arc::new(CommitSyncerClient::new(
            context.clone(),
            Some(Arc::new(FakeNetworkClient::default())),
            Some(Arc::new(FakeNetworkClient::default())),
        ));
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let commit_consumer_monitor = Arc::new(CommitConsumerMonitor::new(0, Some(0)));
        let round_tracker = Arc::new(RwLock::new(RoundTracker::new(context.clone(), vec![])));

        // Create PeersPool with multiple peers (validators and another observer)
        let peers_pool = Arc::new(PeersPool::new(context.clone()));
        // Register multiple validator peers
        peers_pool
            .register_validator(
                AuthorityIndex::new_for_test(0),
                vec![PeerService::Validator, PeerService::Observer],
            )
            .unwrap();
        peers_pool
            .register_validator(
                AuthorityIndex::new_for_test(1),
                vec![PeerService::Validator, PeerService::Observer],
            )
            .unwrap();
        peers_pool
            .register_validator(
                AuthorityIndex::new_for_test(2),
                vec![PeerService::Validator, PeerService::Observer],
            )
            .unwrap();

        // Now register another observer peer (simulating observer-to-observer sync)
        let observer_peer = NetworkKeyPair::generate(&mut rand::thread_rng()).public();
        peers_pool.register_observer(observer_peer);

        let mut commit_syncer = CommitSyncer::new(
            context.clone(),
            core_thread_dispatcher,
            commit_vote_monitor.clone(),
            commit_consumer_monitor.clone(),
            block_verifier,
            transaction_vote_tracker,
            round_tracker,
            network_client,
            dag_state,
            peers_pool.clone(),
        );

        // Simulate heavy commit load that requires parallel fetching
        for i in 0..3 {
            let test_block = TestBlock::new(100, i)
                .set_commit_votes(vec![CommitRef::new(50, CommitDigest::MIN)])
                .build();
            let block = VerifiedBlock::new_for_test(test_block);
            commit_vote_monitor.observe_block(&block);
        }

        commit_syncer.try_schedule_once();

        // Should schedule multiple batches
        let pending_fetches = commit_syncer.pending_fetches().len();
        assert!(pending_fetches > 0, "Should schedule fetches");

        // Start fetches - observer should utilize multiple peers in parallel
        commit_syncer.try_start_fetches();

        // Observer with 4 peers (3 validators + 1 observer) should be able to
        // fetch from multiple peers in parallel, not limited by 2/3 rule
        let inflight = commit_syncer.inflight_fetches.len();
        let known_peers = peers_pool.get_known_peers().len();

        assert_eq!(known_peers, 4, "Should have 3 validators + 1 observer peer");

        // Observer should be able to use full parallelism up to configured limit
        // Not restricted by the validator's 2/3 limitation
        let max_parallel = context
            .parameters
            .commit_sync_parallel_fetches
            .min(pending_fetches)
            .min(context.parameters.commit_sync_batches_ahead);

        assert!(
            inflight <= max_parallel,
            "Observer should respect configured parallelism limit: {} <= {}",
            inflight,
            max_parallel
        );

        // Verify observer can potentially use more parallelism than a validator would
        // A validator with 4 peers would be limited to 4 * 2/3 = 2 parallel fetches
        // But an observer can use more
        if pending_fetches >= 3 {
            assert!(
                max_parallel > 2,
                "Observer should be able to use more parallelism than validator's 2/3 limit"
            );
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
        let mock_client = Arc::new(FakeNetworkClient::default());
        let network_client = Arc::new(CommitSyncerClient::new(
            context.clone(),
            Some(mock_client.clone()),
            Some(mock_client.clone()),
        ));
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let commit_consumer_monitor = Arc::new(CommitConsumerMonitor::new(0, Some(0)));
        let round_tracker = Arc::new(RwLock::new(RoundTracker::new(context.clone(), vec![])));
        let peers_pool = Arc::new(PeersPool::new(context.clone()));
        let mut commit_syncer = CommitSyncer::new(
            context,
            core_thread_dispatcher,
            commit_vote_monitor.clone(),
            commit_consumer_monitor.clone(),
            block_verifier,
            transaction_vote_tracker,
            round_tracker,
            network_client,
            dag_state,
            peers_pool,
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
        for (range, start) in pending_fetches.iter().zip_debug_eq((1..35).step_by(5)) {
            assert_eq!(range.start(), start);
            assert_eq!(range.end(), start + 4);
        }
    }

    #[test]
    fn fetch_unit_sizing() {
        // Default-load block sizes resolve to the count ceilings.
        assert_eq!(blocks_per_chunk(16 << 20, 6 << 10, 1000), 1000);
        assert_eq!(blocks_per_chunk(16 << 20, 225 << 10, 1000), 72);
        assert_eq!(blocks_per_chunk(16 << 20, 64 << 20, 1000), 1);

        assert_eq!(commits_per_range(100, 128 << 20, None), 100);
        assert_eq!(commits_per_range(100, 128 << 20, Some(0)), 100);
        assert_eq!(commits_per_range(100, 128 << 20, Some(730 << 10)), 100);
        assert_eq!(commits_per_range(100, 128 << 20, Some(15 << 20)), 8);
        assert_eq!(commits_per_range(100, 128 << 20, Some(1 << 40)), 1);

        let estimate = SizeEstimate::new(1 << 20);
        assert_eq!(estimate.block_bytes(), 32 << 10);
        assert_eq!(estimate.commit_bytes(), None);
        for _ in 0..40 {
            estimate.observe_blocks(100 << 10, 100);
        }
        assert_eq!(estimate.block_bytes(), 1 << 10);
        estimate.inflate_block_bytes();
        assert_eq!(estimate.block_bytes(), 1536);
        for _ in 0..40 {
            estimate.inflate_block_bytes();
        }
        assert_eq!(estimate.block_bytes(), 1 << 20);
        estimate.observe_commits(10, 500);
        assert_eq!(estimate.commit_bytes(), Some(50 << 20));
        estimate.observe_commits(0, 0);
        assert_eq!(estimate.commit_bytes(), Some(50 << 20));

        assert_eq!(estimate.stream_bytes_per_second(), 16 << 20);
        for _ in 0..40 {
            estimate.observe_stream(2 << 20, Duration::from_secs(1));
        }
        let converged = estimate.stream_bytes_per_second();
        assert!(converged.abs_diff(2 << 20) < 1 << 10, "{converged}");
        estimate.observe_stream(0, Duration::from_secs(1));
        assert_eq!(estimate.stream_bytes_per_second(), converged);

        // 256 MB/s over 8 ranges needs 32 MB/s per range: 16 streams at 2 MB/s, 4 at 8 MB/s,
        // and the minimum when a single stream is enough.
        assert_eq!(chunk_parallelism(256 << 20, 8, 2 << 20, 3, 16), 16);
        assert_eq!(chunk_parallelism(256 << 20, 8, 8 << 20, 3, 16), 4);
        assert_eq!(chunk_parallelism(256 << 20, 8, 32 << 20, 3, 16), 3);
        assert_eq!(chunk_parallelism(256 << 20, 1, 1 << 20, 3, 16), 16);
        assert_eq!(chunk_parallelism(256 << 20, 0, 1 << 20, 3, 2), 3);
    }

    #[test]
    fn commits_within_budget_keeps_prefix() {
        let block_refs = |n: u32| {
            (0..n)
                .map(|i| BlockRef::new(i + 1, AuthorityIndex::new_for_test(0), Default::default()))
                .collect::<Vec<_>>()
        };
        let leader = BlockRef::new(1, AuthorityIndex::new_for_test(0), Default::default());
        let commits: Vec<TrustedCommit> = (1..=4)
            .map(|i| TrustedCommit::new_for_test(i, CommitDigest::MIN, 0, leader, block_refs(4)))
            .collect();
        // Each commit is 4 blocks x 1 KiB.
        assert_eq!(commits_within_budget(&commits, 100 << 10, 1 << 10), 4);
        assert_eq!(commits_within_budget(&commits, 8 << 10, 1 << 10), 2);
        assert_eq!(commits_within_budget(&commits, 9 << 10, 1 << 10), 2);
        // The first commit is kept even when it exceeds the budget on its own.
        assert_eq!(commits_within_budget(&commits, 1 << 10, 1 << 10), 1);
        assert_eq!(commits_within_budget(&[], 1 << 10, 1 << 10), 0);
    }

    fn make_commit_syncer(
        context: Arc<Context>,
        client: Arc<FakeNetworkClient>,
        peers: &[u32],
    ) -> (
        CommitSyncer<FakeNetworkClient, FakeNetworkClient>,
        Arc<CommitVoteMonitor>,
    ) {
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let core_thread_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let network_client = Arc::new(CommitSyncerClient::new(
            context.clone(),
            Some(client.clone()),
            Some(client),
        ));
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let commit_consumer_monitor = Arc::new(CommitConsumerMonitor::new(0, Some(0)));
        let round_tracker = Arc::new(RwLock::new(RoundTracker::new(context.clone(), vec![])));
        let peers_pool = Arc::new(PeersPool::new(context.clone()));
        for peer in peers {
            peers_pool
                .register_validator(
                    AuthorityIndex::new_for_test(*peer),
                    vec![PeerService::Validator],
                )
                .unwrap();
        }
        let commit_syncer = CommitSyncer::new(
            context,
            core_thread_dispatcher,
            commit_vote_monitor.clone(),
            commit_consumer_monitor,
            block_verifier,
            transaction_vote_tracker,
            round_tracker,
            network_client,
            dag_state,
            peers_pool,
        );
        (commit_syncer, commit_vote_monitor)
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn commit_syncer_shortens_ranges_for_large_blocks() {
        let (mut context, _) = Context::new_for_test(4);
        context.own_index = AuthorityIndex::new_for_test(3);
        context.parameters = Parameters {
            commit_sync_batch_size: 5,
            commit_sync_batches_ahead: 10,
            commit_sync_parallel_fetches: 5,
            commit_sync_range_bytes: 10 << 10,
            ..context.parameters
        };
        let context = Arc::new(context);
        let (mut commit_syncer, commit_vote_monitor) =
            make_commit_syncer(context, Arc::new(FakeNetworkClient::default()), &[0, 1, 2]);

        // 4 blocks per commit at 1 KiB per block: 4 KiB per commit, so 2 commits fit in 10 KiB.
        commit_syncer.inner.size_estimate.observe_commits(1, 4);
        for _ in 0..40 {
            commit_syncer.inner.size_estimate.observe_blocks(4 << 10, 4);
        }
        assert_eq!(
            commit_syncer.inner.size_estimate.commit_bytes(),
            Some(4 << 10)
        );

        for i in 0..3 {
            let test_block = TestBlock::new(15, i)
                .set_commit_votes(vec![CommitRef::new(10, CommitDigest::MIN)])
                .build();
            commit_vote_monitor.observe_block(&VerifiedBlock::new_for_test(test_block));
        }
        commit_syncer.try_schedule_once();

        let pending_fetches = commit_syncer.pending_fetches();
        assert_eq!(pending_fetches.len(), 5);
        for (range, start) in pending_fetches.iter().zip_debug_eq((1..=10).step_by(2)) {
            assert_eq!(range.start(), start);
            assert_eq!(range.end(), start + 1);
        }
        assert_eq!(commit_syncer.highest_scheduled_index(), Some(10));
    }

    fn make_blocks(client: &mut FakeNetworkClient, count: u32) -> Vec<VerifiedBlock> {
        let blocks: Vec<VerifiedBlock> = (0..count)
            .map(|i| VerifiedBlock::new_for_test(TestBlock::new(i / 4 + 1, i % 4).build()))
            .collect();
        for block in &blocks {
            client
                .blocks
                .insert(block.reference(), block.serialized().clone());
        }
        blocks
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn fetch_blocks_in_chunks_moves_to_other_peers_on_failure() {
        let (mut context, _) = Context::new_for_test(4);
        context.own_index = AuthorityIndex::new_for_test(3);
        context.parameters = Parameters {
            max_blocks_per_fetch: 4,
            commit_sync_chunk_bytes: 1 << 20,
            commit_sync_chunk_parallelism: 2,
            commit_sync_chunk_base_timeout: Duration::from_secs(1),
            ..context.parameters
        };
        let context = Arc::new(context);
        let mut client = FakeNetworkClient::default();
        let blocks = make_blocks(&mut client, 10);
        client
            .behaviors
            .insert(AuthorityIndex::new_for_test(0), FakeBehavior::Fail);
        client
            .behaviors
            .insert(AuthorityIndex::new_for_test(1), FakeBehavior::Fail);
        let client = Arc::new(client);
        let (commit_syncer, _) = make_commit_syncer(context, client.clone(), &[0, 1, 2]);

        let block_refs: BTreeSet<BlockRef> = blocks.iter().map(|b| b.reference()).collect();
        let mut charge = InflightCharge::new(commit_syncer.inner.clone(), 0);
        let fetched = CommitSyncer::fetch_blocks_in_chunks(
            commit_syncer.inner.clone(),
            PeerId::Validator(AuthorityIndex::new_for_test(0)),
            block_refs.clone(),
            &mut charge,
        )
        .await
        .unwrap();
        drop(charge);

        assert_eq!(fetched.len(), 10);
        for (block_ref, block) in &fetched {
            assert_eq!(*block_ref, block.reference());
            assert!(block_refs.contains(block_ref));
        }
        // The target peer is tried first, the remaining peers in random order, so peer 1 may be
        // skipped when peer 2 comes first.
        let calls = client.calls.lock().clone();
        assert!(calls[&AuthorityIndex::new_for_test(0)] >= 1);
        assert!(calls[&AuthorityIndex::new_for_test(2)] >= 3);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn fetch_blocks_in_chunks_keeps_partial_responses() {
        let (mut context, _) = Context::new_for_test(4);
        context.own_index = AuthorityIndex::new_for_test(3);
        context.parameters = Parameters {
            max_blocks_per_fetch: 4,
            commit_sync_chunk_bytes: 1 << 20,
            commit_sync_chunk_parallelism: 1,
            commit_sync_chunk_base_timeout: Duration::from_secs(1),
            ..context.parameters
        };
        let context = Arc::new(context);
        let mut client = FakeNetworkClient::default();
        let blocks = make_blocks(&mut client, 10);
        client
            .behaviors
            .insert(AuthorityIndex::new_for_test(0), FakeBehavior::Prefix(3));
        let client = Arc::new(client);
        let (commit_syncer, _) = make_commit_syncer(context, client.clone(), &[0]);
        let initial_estimate = commit_syncer.inner.size_estimate.block_bytes();

        let block_refs: BTreeSet<BlockRef> = blocks.iter().map(|b| b.reference()).collect();
        let mut charge = InflightCharge::new(commit_syncer.inner.clone(), 0);
        let fetched = CommitSyncer::fetch_blocks_in_chunks(
            commit_syncer.inner.clone(),
            PeerId::Validator(AuthorityIndex::new_for_test(0)),
            block_refs.clone(),
            &mut charge,
        )
        .await
        .unwrap();
        drop(charge);

        assert_eq!(fetched.len(), 10);
        let calls = client.calls.lock().clone();
        assert!(calls[&AuthorityIndex::new_for_test(0)] >= 4);
        // Short responses update the block size estimate from the blocks they carry instead of
        // inflating it; test blocks are far smaller than the initial estimate.
        assert!(commit_syncer.inner.size_estimate.block_bytes() < initial_estimate);
        // The charge is released once the fetch is done.
        assert_eq!(
            commit_syncer.inner.inflight_bytes.load(Ordering::Relaxed),
            0
        );
        assert_eq!(
            commit_syncer.inner.active_fetches.load(Ordering::Relaxed),
            0
        );
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn inflight_charge_tracks_remaining_bytes() {
        let (mut context, _) = Context::new_for_test(4);
        context.own_index = AuthorityIndex::new_for_test(3);
        let context = Arc::new(context);
        let (commit_syncer, _) =
            make_commit_syncer(context, Arc::new(FakeNetworkClient::default()), &[0]);
        let inner = commit_syncer.inner.clone();

        let mut first = InflightCharge::new(inner.clone(), 100);
        let second = InflightCharge::new(inner.clone(), 50);
        assert_eq!(inner.inflight_bytes.load(Ordering::Relaxed), 150);
        assert_eq!(inner.active_fetches.load(Ordering::Relaxed), 2);
        first.set(40);
        assert_eq!(inner.inflight_bytes.load(Ordering::Relaxed), 90);
        first.set(60);
        assert_eq!(inner.inflight_bytes.load(Ordering::Relaxed), 110);
        drop(second);
        assert_eq!(inner.inflight_bytes.load(Ordering::Relaxed), 60);
        assert_eq!(inner.active_fetches.load(Ordering::Relaxed), 1);
        drop(first);
        assert_eq!(inner.inflight_bytes.load(Ordering::Relaxed), 0);
        assert_eq!(inner.active_fetches.load(Ordering::Relaxed), 0);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn fetch_blocks_in_chunks_fails_after_exhausting_peers() {
        let (mut context, _) = Context::new_for_test(4);
        context.own_index = AuthorityIndex::new_for_test(3);
        context.parameters = Parameters {
            max_blocks_per_fetch: 4,
            commit_sync_chunk_parallelism: 1,
            commit_sync_chunk_base_timeout: Duration::from_secs(1),
            ..context.parameters
        };
        let context = Arc::new(context);
        let mut client = FakeNetworkClient::default();
        let blocks = make_blocks(&mut client, 4);
        for peer in 0..3 {
            client
                .behaviors
                .insert(AuthorityIndex::new_for_test(peer), FakeBehavior::Fail);
        }
        let client = Arc::new(client);
        let (commit_syncer, _) = make_commit_syncer(context, client.clone(), &[0, 1, 2]);

        let block_refs: BTreeSet<BlockRef> = blocks.iter().map(|b| b.reference()).collect();
        let mut charge = InflightCharge::new(commit_syncer.inner.clone(), 0);
        let result = CommitSyncer::fetch_blocks_in_chunks(
            commit_syncer.inner.clone(),
            PeerId::Validator(AuthorityIndex::new_for_test(0)),
            block_refs,
            &mut charge,
        )
        .await;
        assert!(result.is_err());
        let calls = client.calls.lock().clone();
        assert_eq!(calls.values().sum::<usize>(), super::MAX_NUM_TARGETS);
    }
}
