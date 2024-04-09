// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! CommitSyncer implements efficient synchronization of past committed data.
//!
//! During the operation of a committee of authorities for consensus, one or more authorities
//! can fall behind the quorum in their received and accepted blocks. This can happen due to
//! network disruptions, host crash, or other reasons. Authories fell behind need to catch up to
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
//! Commit sychronization is an expensive operation, involving transfering large amount of data via
//! the network. And it is not on the critical path of block processing. So the heuristics for
//! synchronization, including triggers and retries, should be chosen to favor throughput and
//! efficient resource usage, over faster reactions.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use futures::{
    stream::{FuturesOrdered, FuturesUnordered},
    StreamExt,
};
use parking_lot::{Mutex, RwLock};
use rand::prelude::SliceRandom as _;
use tokio::{
    task::JoinSet,
    time::{sleep, Instant, MissedTickBehavior},
};
use tracing::{debug, info};

use crate::{
    block::{BlockAPI, BlockRef, SignedBlock, VerifiedBlock},
    block_verifier::BlockVerifier,
    commit::{
        Commit, CommitAPI as _, CommitDigest, CommitRef, TrustedCommit, GENESIS_COMMIT_INDEX,
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
    tasks: JoinSet<()>,
    _phantom: std::marker::PhantomData<C>,
}

impl<C: NetworkClient> CommitSyncer<C> {
    pub(crate) fn new(
        context: Arc<Context>,
        core_thread_dispatcher: Arc<dyn CoreThreadDispatcher>,
        highest_commit_monitor: Arc<HighestCommitMonitor>,
        network_client: Arc<C>,
        block_verifier: Arc<dyn BlockVerifier>,
        dag_state: Arc<RwLock<DagState>>,
    ) -> Self {
        let fetch_state = Arc::new(Mutex::new(FetchState::new(&context)));
        let inner = Arc::new(Inner {
            context,
            core_thread_dispatcher,
            highest_commit_monitor,
            network_client,
            block_verifier,
            dag_state,
        });
        let mut tasks = JoinSet::new();
        tasks.spawn(Self::schedule_loop(inner, fetch_state));
        CommitSyncer {
            tasks,
            _phantom: Default::default(),
        }
    }

    pub(crate) fn stop(&mut self) {
        self.tasks.abort_all();
    }

    async fn schedule_loop(inner: Arc<Inner<C>>, fetch_state: Arc<Mutex<FetchState>>) {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // Inflight requests to fetch commits from different authorities.
        let mut inflight_fetches = FuturesUnordered::new();
        // Additional ranges (inclusive start and end) of commits to fetch.
        let mut pending_fetches = VecDeque::<(CommitIndex, CommitIndex)>::new();
        // Highest end index among inflight and pending fetches.
        let mut highest_scheduled_index = Option::<CommitIndex>::None;
        // Fetched commits and blocks by commit indices.
        let mut fetched_blocks = BTreeMap::<(CommitIndex, CommitIndex), Vec<VerifiedBlock>>::new();
        // The commit index that is the max of local last commit index and highest commit index of blocks sent to Core.
        let mut sycned_commit_index = inner.dag_state.read().last_commit_index();

        'run: loop {
            tokio::select! {
                // Periodially, clean up inflight fetches that are no longer needed, and schedule new
                // fetches if the node is falling behind.
                _ = interval.tick() => {
                    let highest_valid_index = inner.highest_commit_monitor.highest_valid_index(&inner.context);
                    // Update sycned_commit_index periodically to make sure it is no smaller than
                    // local commit index.
                    sycned_commit_index = sycned_commit_index.max(inner.dag_state.read().last_commit_index());
                    // TODO: cleanup inflight fetches that are no longer needed.
                    let fetch_after_index = sycned_commit_index.max(highest_scheduled_index.unwrap_or(0));
                    info!(
                        "Checking to schedule fetches: sycned_commit_index={}, fetch_after_index={}, highest_valid_index={}",
                        sycned_commit_index, fetch_after_index, highest_valid_index
                    );
                    // When the node is falling behind, schedule pending fetches which will be executed on later.
                    'pending: for prev_end in (fetch_after_index..=highest_valid_index).step_by(inner.context.parameters.commit_sync_batch_size as usize) {
                        // Create range with inclusive start and end.
                        let range_start = prev_end + 1;
                        let range_end = prev_end + inner.context.parameters.commit_sync_batch_size;
                        // Do not fetch with small batches. Block subscription and synchronization will help the node catchup.
                        if range_end > highest_valid_index {
                            break 'pending;
                        }
                        pending_fetches.push_back((range_start, range_end));
                        // highest_valid_index should be non-decreasing, so this should not
                        // decrease as well.
                        highest_scheduled_index = Some(range_end);
                    }
                }

                // Processed fetched blocks.
                Some(result) = inflight_fetches.next(), if !inflight_fetches.is_empty() => {
                    if let Err(e) = result {
                        debug!("Failed to fetch: {}", e);
                        continue 'run;
                    }
                    let (target_end, commits, blocks): (CommitIndex, Vec<TrustedCommit>, Vec<VerifiedBlock>) = result.unwrap();
                    assert!(!commits.is_empty());
                    let metrics = &inner.context.metrics.node_metrics;
                    metrics.commit_sync_fetched_commits.inc_by(commits.len() as u64);
                    metrics.commit_sync_fetched_blocks.inc_by(blocks.len() as u64);

                    let (commit_start, commit_end) = (commits.first().unwrap().index(), commits.last().unwrap().index());
                    // Allow returning partial results, and try fetching the rest separately.
                    if commit_end < target_end {
                        pending_fetches.push_front((commit_end, target_end));
                    }
                    // Make sure sycned_commit_index is up to date.
                    sycned_commit_index = sycned_commit_index.max(inner.dag_state.read().last_commit_index());
                    // Only add new blocks if at least some of them are not already committed.
                    if sycned_commit_index < commit_end {
                        fetched_blocks.insert((commit_start, commit_end), blocks);
                    }
                    // Try to process as many blocks as possible, as long as there is no gap between
                    'fetched: while let Some(((fetched_start, _end), _blocks)) = fetched_blocks.first_key_value() {
                        // Only pop fetched_blocks if there is no gap with blocks already sent to Core.
                        // Note: start is exclusive and sycned_commit_index is inclusive.
                        let ((_start, fetched_end), blocks) = if *fetched_start <= sycned_commit_index {
                            fetched_blocks.pop_first().unwrap()
                        } else {
                            break 'fetched;
                        };
                        // Only send blocks that are not already sent to Core.
                        if fetched_end <= sycned_commit_index {
                            continue 'fetched;
                        }
                        // If core thread cannot handle the incoming blocks, it is ok to block here.
                        if let Err(e) = inner.core_thread_dispatcher.add_blocks(blocks).await {
                            debug!("Failed to add blocks, shutting down: {}", e);
                            return;
                        }
                        // Once commits and blocks are sent to Core, rachet up sycned_commit_index
                        sycned_commit_index = sycned_commit_index.max(fetched_end);
                    }
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
                let Some((start, end)) = pending_fetches.pop_front() else {
                    break;
                };
                inflight_fetches.push(tokio::spawn(Self::fetch_loop(
                    inner.clone(),
                    fetch_state.clone(),
                    start,
                    end,
                )));
            }

            let metrics = &inner.context.metrics.node_metrics;
            metrics
                .commit_sync_inflight_fetches
                .set(inflight_fetches.len() as i64);
            metrics
                .commit_sync_pending_fetches
                .set(pending_fetches.len() as i64);
            metrics
                .commit_sync_highest_index
                .set(sycned_commit_index as i64);
        }
    }

    async fn fetch_loop(
        inner: Arc<Inner<C>>,
        fetch_state: Arc<Mutex<FetchState>>,
        start: CommitIndex,
        end: CommitIndex,
    ) -> (CommitIndex, Vec<TrustedCommit>, Vec<VerifiedBlock>) {
        let _timer = inner
            .context
            .metrics
            .node_metrics
            .commit_sync_fetch_loop_latency
            .start_timer();
        info!(
            "Starting to fetch commits from index {} (exclusive) to {} ...",
            start, end
        );
        loop {
            match Self::fetch_once(inner.clone(), fetch_state.clone(), start, end).await {
                Ok((commits, blocks)) => {
                    info!(
                        "Finished fetching commits from index {} (exclusive) to {} ...",
                        start, end
                    );
                    return (end, commits, blocks);
                }
                Err(e) => {
                    info!("Failed to fetch: {}", e);
                }
            }
        }
    }

    async fn fetch_once(
        inner: Arc<Inner<C>>,
        fetch_state: Arc<Mutex<FetchState>>,
        start: CommitIndex,
        end: CommitIndex,
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

        // Find an available authority to fetch data.
        let Some((available_time, target_authority, retries)) =
            fetch_state.lock().available_authorities.pop_first()
        else {
            sleep(MAX_RETRY_INTERVAL).await;
            return Err(ConsensusError::NoAvailableAuthorityToFetchCommits);
        };
        let now = Instant::now();
        if now < available_time {
            sleep(available_time - now).await;
        }

        // Fetch and verify commits.
        let (serialized_commits, serialized_blocks) = match inner
            .network_client
            .fetch_commits(target_authority, start, end, FETCH_COMMITS_TIMEOUT)
            .await
        {
            Ok(result) => {
                let mut fetch_state = fetch_state.lock();
                let now = Instant::now();
                fetch_state
                    .available_authorities
                    .insert((now, target_authority, 0));
                result
            }
            Err(e) => {
                let mut fetch_state = fetch_state.lock();
                let now = Instant::now();
                fetch_state.available_authorities.insert((
                    now + FETCH_RETRY_BASE_INTERVAL * retries.min(FETCH_RETRY_INTERVAL_LIMIT),
                    target_authority,
                    retries.saturating_add(1),
                ));
                return Err(e);
            }
        };

        let commits = inner.verify_commits(
            target_authority,
            start,
            end,
            serialized_commits,
            serialized_blocks,
        )?;

        // Fetch blocks and verify with digests.
        let block_refs: Vec<_> = commits.iter().flat_map(|c| c.blocks()).cloned().collect();
        let mut requests: FuturesOrdered<_> = block_refs
            .chunks(inner.context.parameters.max_blocks_per_fetch)
            .enumerate()
            .map(|(i, reqeust_block_refs)| {
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
                            reqeust_block_refs.to_vec(),
                            FETCH_BLOCKS_TIMEOUT,
                        )
                        .await?;
                    let signed_blocks = serialized_blocks
                        .iter()
                        .map(|serialized| {
                            let block: SignedBlock = bcs::from_bytes(serialized)
                                .map_err(ConsensusError::MalformedBlock)?;
                            Ok(block)
                        })
                        .collect::<ConsensusResult<Vec<_>>>()?;
                    // Fetched blocks matching the requested block refs are verified, because the
                    // requested block refs come from verified commits.
                    let mut blocks = Vec::new();
                    for ((requested_block_ref, signed_block), serialized) in reqeust_block_refs
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

        Ok((commits, fetched_blocks))
    }
}

pub(crate) struct HighestCommitMonitor {
    // Highest commit voted by each authority.
    highest_voted_commits: Mutex<Vec<CommitIndex>>,
}

impl HighestCommitMonitor {
    pub(crate) fn new(context: &Context) -> Self {
        Self {
            highest_voted_commits: Mutex::new(vec![0; context.committee.size()]),
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

    // Finds the highest index that must be valid.
    fn highest_valid_index(&self, context: &Context) -> CommitIndex {
        let highest_voted_commits = self.highest_voted_commits.lock();
        let mut highest_voted_commits = context
            .committee
            .authorities()
            .zip(highest_voted_commits.iter())
            .map(|((_i, a), r)| (*r, a.stake))
            .collect::<Vec<_>>();
        // Sort by commit ref (which is index) then stake, in descending order.
        highest_voted_commits.sort_by(|a, b| a.cmp(b).reverse());
        let mut total_stake = 0;
        for (index, stake) in highest_voted_commits {
            total_stake += stake;
            if total_stake >= context.committee.validity_threshold() {
                return index;
            }
        }
        GENESIS_COMMIT_INDEX
    }
}

struct Inner<C: NetworkClient> {
    context: Arc<Context>,
    core_thread_dispatcher: Arc<dyn CoreThreadDispatcher>,
    highest_commit_monitor: Arc<HighestCommitMonitor>,
    network_client: Arc<C>,
    block_verifier: Arc<dyn BlockVerifier>,
    dag_state: Arc<RwLock<DagState>>,
}

impl<C: NetworkClient> Inner<C> {
    fn verify_commits(
        &self,
        peer: AuthorityIndex,
        start: CommitIndex,
        end: CommitIndex,
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
                if commit.index() != start {
                    return Err(ConsensusError::UnexpectedStartCommit {
                        peer,
                        start,
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
            if commit.index() > end {
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
    // Tuple of the first time an authority is available to be fetched, authority index and
    // previous consequtive failures count.
    available_authorities: BTreeSet<(Instant, AuthorityIndex, u32)>,
}

impl FetchState {
    fn new(context: &Context) -> Self {
        // Randomize the initial order of authorities.
        let mut available_authorities: Vec<_> = context
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
        available_authorities.shuffle(&mut rand::thread_rng());
        Self {
            available_authorities: available_authorities
                .into_iter()
                .map(|i| (Instant::now(), i, 0))
                .collect(),
        }
    }
}
