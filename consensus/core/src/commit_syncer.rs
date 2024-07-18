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
use parking_lot::{Mutex, RwLock};
use rand::prelude::SliceRandom as _;
use tokio::{
    sync::oneshot,
    task::{JoinHandle, JoinSet},
    time::{sleep, Instant, MissedTickBehavior},
};
use tracing::{debug, info, warn};

use crate::{
    block::{BlockAPI, BlockRef, SignedBlock, VerifiedBlock},
    block_verifier::BlockVerifier,
    commit::{
        Commit, CommitAPI as _, CommitDigest, CommitRange, CommitRef, TrustedCommit,
        GENESIS_COMMIT_INDEX,
    },
    context::Context,
    core_thread::CoreThreadDispatcher,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    network::NetworkClient,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    CommitIndex,
};

pub(crate) struct CommitSyncer<C: NetworkClient> {
    schedule_task: JoinHandle<()>,
    tx_shutdown: oneshot::Sender<()>,
    _phantom: std::marker::PhantomData<C>,
}

impl<C: NetworkClient> CommitSyncer<C> {
    pub(crate) fn new(
        context: Arc<Context>,
        core_thread_dispatcher: Arc<dyn CoreThreadDispatcher>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        network_client: Arc<C>,
        block_verifier: Arc<dyn BlockVerifier>,
        dag_state: Arc<RwLock<DagState>>,
    ) -> Self {
        let fetch_state = Arc::new(Mutex::new(FetchState::new(&context)));
        let inner = Arc::new(Inner {
            context,
            core_thread_dispatcher,
            commit_vote_monitor,
            network_client,
            block_verifier,
            dag_state,
        });
        let (tx_shutdown, rx_shutdown) = oneshot::channel();
        let schedule_task =
            spawn_logged_monitored_task!(Self::schedule_loop(inner, fetch_state, rx_shutdown));
        CommitSyncer {
            schedule_task,
            tx_shutdown,
            _phantom: Default::default(),
        }
    }

    pub(crate) async fn stop(self) {
        let _ = self.tx_shutdown.send(());
        // Do not abort schedule task, which waits for fetches to shut down.
        let _ = self.schedule_task.await;
    }

    async fn schedule_loop(
        inner: Arc<Inner<C>>,
        fetch_state: Arc<Mutex<FetchState>>,
        mut rx_shutdown: oneshot::Receiver<()>,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // Inflight requests to fetch commits from different authorities.
        let mut inflight_fetches = JoinSet::new();
        // Additional ranges (inclusive start and end) of commits to fetch.
        let mut pending_fetches = BTreeSet::<CommitRange>::new();
        // Fetched commits and blocks by commit indices.
        let mut fetched_blocks = BTreeMap::<CommitRange, Vec<VerifiedBlock>>::new();
        // Highest end index among inflight and pending fetches.
        // Used to determine if and which new ranges to fetch.
        let mut highest_scheduled_index = Option::<CommitIndex>::None;
        // The commit index that is the max of local last commit index and highest commit index of blocks sent to Core.
        // Used to determine if fetched blocks can be sent to Core without gaps.
        let mut synced_commit_index = inner.dag_state.read().last_commit_index();
        let mut highest_fetched_commit_index = 0;

        loop {
            tokio::select! {
                // Periodically, schedule new fetches if the node is falling behind.
                _ = interval.tick() => {
                    let quorum_commit_index = inner.commit_vote_monitor.quorum_commit_index();
                    let local_commit_index = inner.dag_state.read().last_commit_index();
                    let metrics = &inner.context.metrics.node_metrics;
                    metrics.commit_sync_quorum_index.set(quorum_commit_index as i64);
                    metrics.commit_sync_local_index.set(local_commit_index as i64);
                    // Update synced_commit_index periodically to make sure it is not smaller than
                    // local commit index.
                    synced_commit_index = synced_commit_index.max(local_commit_index);
                    info!(
                        "Checking to schedule fetches: synced_commit_index={}, highest_scheduled_index={}, quorum_commit_index={}",
                        synced_commit_index, highest_scheduled_index.unwrap_or(0), quorum_commit_index,
                    );
                    // TODO: pause commit sync when execution of commits is lagging behind, maybe through Core.
                    // TODO: cleanup inflight fetches that are no longer needed.
                    let fetch_after_index = synced_commit_index.max(highest_scheduled_index.unwrap_or(0));
                    // When the node is falling behind, schedule pending fetches which will be executed on later.
                    'pending: for prev_end in (fetch_after_index..=quorum_commit_index).step_by(inner.context.parameters.commit_sync_batch_size as usize) {
                        // Create range with inclusive start and end.
                        let range_start = prev_end + 1;
                        let range_end = prev_end + inner.context.parameters.commit_sync_batch_size;
                        // When the condition below is true, [range_start, range_end] contains less number of commits
                        // than the target batch size. Not creating the smaller batch is intentional, to avoid the
                        // cost of processing more and smaller batches.
                        // Block broadcast, subscription and synchronization will help the node catchup.
                        if range_end > quorum_commit_index {
                            break 'pending;
                        }
                        pending_fetches.insert((range_start..=range_end).into());
                        // quorum_commit_index should be non-decreasing, so highest_scheduled_index should not
                        // decrease either.
                        highest_scheduled_index = Some(range_end);
                    }
                }

                // Processed fetched blocks.
                Some(result) = inflight_fetches.join_next(), if !inflight_fetches.is_empty() => {
                    if let Err(e) = result {
                        warn!("Fetch cancelled or panicked, CommitSyncer shutting down: {}", e);
                        // If any fetch is cancelled or panicked, try to shutdown and exit the loop.
                        inflight_fetches.shutdown().await;
                        return;
                    }
                    let (target_end, commits, blocks): (CommitIndex, Vec<TrustedCommit>, Vec<VerifiedBlock>) = result.unwrap();
                    assert!(!commits.is_empty());
                    let metrics = &inner.context.metrics.node_metrics;
                    metrics.commit_sync_fetched_commits.inc_by(commits.len() as u64);
                    metrics.commit_sync_fetched_blocks.inc_by(blocks.len() as u64);
                    metrics.commit_sync_total_fetched_blocks_size.inc_by(
                        blocks.iter().map(|b| b.serialized().len() as u64).sum::<u64>()
                    );

                    let (commit_start, commit_end) = (commits.first().unwrap().index(), commits.last().unwrap().index());

                    highest_fetched_commit_index = highest_fetched_commit_index.max(commit_end);
                    metrics.commit_sync_highest_fetched_index.set(highest_fetched_commit_index.into());

                    // Allow returning partial results, and try fetching the rest separately.
                    if commit_end < target_end {
                        pending_fetches.insert((commit_end + 1..=target_end).into());
                    }
                    // Make sure synced_commit_index is up to date.
                    synced_commit_index = synced_commit_index.max(inner.dag_state.read().last_commit_index());
                    // Only add new blocks if at least some of them are not already synced.
                    if synced_commit_index < commit_end {
                        fetched_blocks.insert((commit_start..=commit_end).into(), blocks);
                    }
                    // Try to process as many fetched blocks as possible.
                    'fetched: while let Some((fetched_commit_range, _blocks)) = fetched_blocks.first_key_value() {
                        // Only pop fetched_blocks if there is no gap with blocks already synced.
                        // Note: start, end and synced_commit_index are all inclusive.
                        let (fetched_commit_range, blocks) = if fetched_commit_range.start() <= synced_commit_index + 1 {
                            fetched_blocks.pop_first().unwrap()
                        } else {
                            // Found gap between earliest fetched block and latest synced block,
                            // so not sending additional blocks to Core.
                            metrics.commit_sync_gap_on_processing.inc();
                            break 'fetched;
                        };
                        // Avoid sending to Core a whole batch of already synced blocks.
                        if fetched_commit_range.end() <= synced_commit_index {
                            continue 'fetched;
                        }
                        debug!(
                            "Fetched certified blocks: {}",
                            blocks
                                .iter()
                                .map(|b| b.reference().to_string())
                                .join(","),
                        );
                        // If core thread cannot handle the incoming blocks, it is ok to block here.
                        // Also it is possible to have missing ancestors because an equivocating validator
                        // may produce blocks that are not included in commits but are ancestors to other blocks.
                        // Synchronizer is needed to fill in the missing ancestors in this case.
                        match inner.core_thread_dispatcher.add_blocks(blocks).await {
                            Ok(missing) => {
                                if !missing.is_empty() {
                                    warn!("Fetched blocks have missing ancestors: {:?}", missing);
                                }
                            }
                            Err(e) => {
                                info!("Failed to add blocks, shutting down: {}", e);
                                return;
                            }
                        };
                        // Once commits and blocks are sent to Core, ratchet up synced_commit_index
                        synced_commit_index = synced_commit_index.max(fetched_commit_range.end());
                    }
                }

                _ = &mut rx_shutdown => {
                    // Shutdown requested.
                    info!("CommitSyncer shutting down ...");
                    inflight_fetches.shutdown().await;
                    return;
                }
            }

            // Cap parallel fetches based on configured limit and committee size, to avoid overloading the network.
            // Also when there are too many fetched blocks that cannot be sent to Core before an earlier fetch
            // has not finished, reduce parallelism so the earlier fetch can retry on a better host and succeed.
            let target_parallel_fetches = inner
                .context
                .parameters
                .commit_sync_parallel_fetches
                .min(inner.context.committee.size() * 2 / 3)
                .min(
                    inner
                        .context
                        .parameters
                        .commit_sync_batches_ahead
                        .saturating_sub(fetched_blocks.len()),
                )
                .max(1);
            // Start new fetches if there are pending batches and available slots.
            loop {
                if inflight_fetches.len() >= target_parallel_fetches {
                    break;
                }
                let Some(commit_range) = pending_fetches.pop_first() else {
                    break;
                };
                inflight_fetches.spawn(Self::fetch_loop(
                    inner.clone(),
                    fetch_state.clone(),
                    commit_range,
                ));
            }

            let metrics = &inner.context.metrics.node_metrics;
            metrics
                .commit_sync_inflight_fetches
                .set(inflight_fetches.len() as i64);
            metrics
                .commit_sync_pending_fetches
                .set(pending_fetches.len() as i64);
            metrics
                .commit_sync_highest_synced_index
                .set(synced_commit_index as i64);
        }
    }

    // Retries fetching commits and blocks from available authorities, until a request succeeds
    // where at least a prefix of the commit range is fetched.
    // Returns the fetched commits and blocks referenced by the commits.
    async fn fetch_loop(
        inner: Arc<Inner<C>>,
        fetch_state: Arc<Mutex<FetchState>>,
        commit_range: CommitRange,
    ) -> (CommitIndex, Vec<TrustedCommit>, Vec<VerifiedBlock>) {
        let _timer = inner
            .context
            .metrics
            .node_metrics
            .commit_sync_fetch_loop_latency
            .start_timer();
        info!("Starting to fetch commits in {commit_range:?} ...",);
        loop {
            match Self::fetch_once(inner.clone(), fetch_state.clone(), commit_range.clone()).await {
                Ok((commits, blocks)) => {
                    info!("Finished fetching commits in {commit_range:?}",);
                    return (commit_range.end(), commits, blocks);
                }
                Err(e) => {
                    warn!("Failed to fetch: {}", e);
                    let error: &'static str = e.into();
                    inner
                        .context
                        .metrics
                        .node_metrics
                        .commit_sync_fetch_once_errors
                        .with_label_values(&[error])
                        .inc();
                }
            }
        }
    }

    // Fetches commits and blocks from a single authority. At a high level, first the commits are
    // fetched and verified. After that, blocks referenced in the certified commits are fetched
    // and sent to Core for processing.
    async fn fetch_once(
        inner: Arc<Inner<C>>,
        fetch_state: Arc<Mutex<FetchState>>,
        commit_range: CommitRange,
    ) -> ConsensusResult<(Vec<TrustedCommit>, Vec<VerifiedBlock>)> {
        const FETCH_COMMITS_TIMEOUT: Duration = Duration::from_secs(10);
        const FETCH_BLOCKS_TIMEOUT: Duration = Duration::from_secs(60);
        const FETCH_RETRY_BASE_INTERVAL: Duration = Duration::from_secs(1);
        const FETCH_RETRY_INTERVAL_LIMIT: u32 = 30;
        const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(1);

        let _timer = inner
            .context
            .metrics
            .node_metrics
            .commit_sync_fetch_once_latency
            .start_timer();

        // 1. Find an available authority to fetch commits and blocks from, and wait
        // if it is not yet ready.
        let Some((available_time, retries, target_authority)) =
            fetch_state.lock().available_authorities.pop_first()
        else {
            sleep(MAX_RETRY_INTERVAL).await;
            return Err(ConsensusError::NoAvailableAuthorityToFetchCommits);
        };
        let now = Instant::now();
        if now < available_time {
            sleep(available_time - now).await;
        }

        // 2. Fetch commits in the commit range from the selected authority.
        let (serialized_commits, serialized_blocks) = match inner
            .network_client
            .fetch_commits(
                target_authority,
                commit_range.clone(),
                FETCH_COMMITS_TIMEOUT,
            )
            .await
        {
            Ok(result) => {
                let mut fetch_state = fetch_state.lock();
                let now = Instant::now();
                fetch_state
                    .available_authorities
                    .insert((now, 0, target_authority));
                result
            }
            Err(e) => {
                let mut fetch_state = fetch_state.lock();
                let now = Instant::now();
                fetch_state.available_authorities.insert((
                    now + FETCH_RETRY_BASE_INTERVAL * retries.min(FETCH_RETRY_INTERVAL_LIMIT),
                    retries.saturating_add(1),
                    target_authority,
                ));
                return Err(e);
            }
        };

        // 3. Verify the response contains blocks that can certify the last returned commit,
        // and the returned commits are chained by digest, so earlier commits are certified
        // as well.
        let commits = inner.verify_commits(
            target_authority,
            commit_range,
            serialized_commits,
            serialized_blocks,
        )?;

        // 4. Fetch blocks referenced by the commits, from the same authority.
        let block_refs: Vec<_> = commits.iter().flat_map(|c| c.blocks()).cloned().collect();
        let mut requests: FuturesOrdered<_> = block_refs
            .chunks(inner.context.parameters.max_blocks_per_fetch)
            .enumerate()
            .map(|(i, request_block_refs)| {
                let i = i as u32;
                let inner = inner.clone();
                async move {
                    // Pipeline the requests to avoid overloading the target.
                    sleep(Duration::from_millis(200) * i).await;
                    // TODO: add some retries.
                    let serialized_blocks = inner
                        .network_client
                        .fetch_blocks(
                            target_authority,
                            request_block_refs.to_vec(),
                            vec![],
                            FETCH_BLOCKS_TIMEOUT,
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
                .with_label_values(&[peer_hostname, &"commit_syncer"])
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
}

/// Monitors commit votes from received and verified blocks,
/// and keeps track of the highest commit voted by each authority and certified by a quorum.
pub(crate) struct CommitVoteMonitor {
    context: Arc<Context>,
    // Highest commit index voted by each authority.
    highest_voted_commits: Mutex<Vec<CommitIndex>>,
}

impl CommitVoteMonitor {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        let highest_voted_commits = Mutex::new(vec![0; context.committee.size()]);
        Self {
            context,
            highest_voted_commits,
        }
    }

    // Records the highest commit index voted in each block.
    pub(crate) fn observe(&self, block: &VerifiedBlock) {
        let mut highest_voted_commits = self.highest_voted_commits.lock();
        for vote in block.commit_votes() {
            if vote.index > highest_voted_commits[block.author()] {
                highest_voted_commits[block.author()] = vote.index;
            }
        }
    }

    // Finds the highest commit index certified by a quorum.
    // When an authority votes for commit index S, it is also voting for all commit indices 1 <= i < S.
    // So the quorum commit index is the smallest index S such that the sum of stakes of authorities
    // voting for commit indices >= S passes the quorum threshold.
    pub(crate) fn quorum_commit_index(&self) -> CommitIndex {
        let highest_voted_commits = self.highest_voted_commits.lock();
        let mut highest_voted_commits = highest_voted_commits
            .iter()
            .zip(self.context.committee.authorities())
            .map(|(commit_index, (_, a))| (*commit_index, a.stake))
            .collect::<Vec<_>>();
        // Sort by commit index then stake, in descending order.
        highest_voted_commits.sort_by(|a, b| a.cmp(b).reverse());
        let mut total_stake = 0;
        for (commit_index, stake) in highest_voted_commits {
            total_stake += stake;
            if total_stake >= self.context.committee.quorum_threshold() {
                return commit_index;
            }
        }
        GENESIS_COMMIT_INDEX
    }
}

struct Inner<C: NetworkClient> {
    context: Arc<Context>,
    core_thread_dispatcher: Arc<dyn CoreThreadDispatcher>,
    commit_vote_monitor: Arc<CommitVoteMonitor>,
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
            // The block signature needs to be verified.
            self.block_verifier.verify(&block)?;
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

struct FetchState {
    // The value is a tuple of
    // - the next available time for the authority to fetch from,
    // - count of current consecutive failures fetching from the authority, reset on success,
    // - authority index.
    // TODO: move this to a separate module, add load balancing, add throttling, and consider
    // health of peer via previous request failures and leader scores.
    available_authorities: BTreeSet<(Instant, u32, AuthorityIndex)>,
}

impl FetchState {
    fn new(context: &Context) -> Self {
        // Randomize the initial order of authorities.
        let mut shuffled_authority_indices: Vec<_> = context
            .committee
            .authorities()
            .filter_map(|(index, _)| {
                if index != context.own_index {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        shuffled_authority_indices.shuffle(&mut rand::thread_rng());
        Self {
            available_authorities: shuffled_authority_indices
                .into_iter()
                .map(|i| (Instant::now(), 0, i))
                .collect(),
        }
    }
}

// TODO: add more unit and integration tests.
#[cfg(test)]
mod test {
    use std::sync::Arc;

    use super::CommitVoteMonitor;
    use crate::{
        block::{TestBlock, VerifiedBlock},
        commit::{CommitDigest, CommitRef},
        context::Context,
    };

    #[tokio::test]
    async fn test_commit_vote_monitor() {
        let context = Arc::new(Context::new_for_test(4).0);
        let monitor = CommitVoteMonitor::new(context.clone());

        // Observe commit votes for indices 5, 6, 7, 8 from blocks.
        let blocks = (0..4)
            .map(|i| {
                VerifiedBlock::new_for_test(
                    TestBlock::new(10, i)
                        .set_commit_votes(vec![CommitRef::new(5 + i, CommitDigest::MIN)])
                        .build(),
                )
            })
            .collect::<Vec<_>>();
        for b in blocks {
            monitor.observe(&b);
        }

        // CommitIndex 6 is the highest index supported by a quorum.
        assert_eq!(monitor.quorum_commit_index(), 6);

        // Observe new blocks with new votes from authority 0 and 1.
        let blocks = (0..2)
            .map(|i| {
                VerifiedBlock::new_for_test(
                    TestBlock::new(11, i)
                        .set_commit_votes(vec![
                            CommitRef::new(6 + i, CommitDigest::MIN),
                            CommitRef::new(7 + i, CommitDigest::MIN),
                        ])
                        .build(),
                )
            })
            .collect::<Vec<_>>();
        for b in blocks {
            monitor.observe(&b);
        }

        // Highest commit index per authority should be 7, 8, 7, 8 now.
        assert_eq!(monitor.quorum_commit_index(), 7);
    }
}
