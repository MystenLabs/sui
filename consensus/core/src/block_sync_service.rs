// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
    sync::Arc,
    time::{Duration, Instant},
};

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, Round};
use mysten_common::ZipDebugEqIteratorExt;
use mysten_metrics::InflightGuard;
use parking_lot::{Mutex, RwLock};
use prometheus::IntGauge;
use rand::seq::SliceRandom;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::{
    CommitIndex, VerifiedBlock,
    block::{BlockAPI, GENESIS_ROUND},
    commit::{CommitAPI, CommitRange, TrustedCommit},
    context::Context,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    network::PeerId,
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    storage::Store,
    task::spawn_blocking,
};

/// Time budget for the periodic/failover heap scan, which can walk the store up to
/// max_blocks_per_fetch times. Past this the server returns the blocks gathered so far, which
/// is a valid partial response for that shape. The budget is below the client's 2s
/// FETCH_REQUEST_TIMEOUT in synchronizer.rs, so the partial response can reach the client
/// before the client gives up. The commit-sync shape reads its refs in one batch that the
/// budget cannot interrupt; the admission caps bound how many such batches run at once.
const STORE_FETCH_TIME_BUDGET: Duration = Duration::from_millis(1500);

/// Per-peer cap for periodic sync requests. A node runs one periodic sync at a time and sends at
/// most one request of it to a given peer, so two slots leave room for one overlapping run.
const PERIODIC_FETCH_MAX_PER_PEER: usize = 2;

/// The admission budget that a store-backed FetchBlocks request uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StoreFetchKind {
    /// Commit sync (block refs only) and failover (fetch_after_rounds only). Each request reads
    /// up to max_blocks_per_fetch blocks from the store.
    Bulk,
    /// Periodic sync (block refs and fetch_after_rounds). Each request returns at most
    /// max_blocks_per_sync blocks. The requested blocks come from the DagState cache, and the
    /// additional blocks come from one store range scan per returned block, as in failover.
    Periodic,
}

impl StoreFetchKind {
    fn label(self) -> &'static str {
        match self {
            StoreFetchKind::Bulk => "bulk",
            StoreFetchKind::Periodic => "periodic",
        }
    }
}

/// Caps for one kind of store-backed request.
struct KindCaps {
    max_per_peer: usize,
    max_observer: usize,
    max_global: usize,
}

/// Counts of in-flight store-backed requests of one kind.
#[derive(Default)]
struct KindCounts {
    global: usize,
    observer: usize,
    per_peer: BTreeMap<PeerId, usize>,
}

/// Counts of in-flight store-backed requests. Admission reads and updates all counts under one
/// lock, so every cap is checked together.
#[derive(Default)]
struct FetchBlocksCounts {
    bulk: KindCounts,
    periodic: KindCounts,
}

impl FetchBlocksCounts {
    fn total(&self) -> usize {
        self.bulk.global + self.periodic.global
    }

    fn observer_total(&self) -> usize {
        self.bulk.observer + self.periodic.observer
    }

    fn kind_mut(&mut self, kind: StoreFetchKind) -> &mut KindCounts {
        match kind {
            StoreFetchKind::Bulk => &mut self.bulk,
            StoreFetchKind::Periodic => &mut self.periodic,
        }
    }
}

/// Bounds how many store-backed FetchBlocks requests run concurrently on the blocking pool. On
/// Tidehunter the store reads are expensive, so the caps bound the CPU that serving can use.
/// Admission does not wait: a request that does not fit is rejected with FetchBlocksThrottled,
/// and the client retries.
///
/// Bulk and periodic requests have separate caps, so bulk requests from lagging peers cannot
/// throttle the periodic sync that keeps the committee live. The caps derive from
/// commit_sync_parallel_fetches (P):
/// - Bulk: P per peer, 2P for all observers, 3P for the kind. A catching-up observer sends its
///   commit-sync fetches to its one configured peer, so the per-peer cap matches its
///   parallelism. The kind cap keeps one per-peer share for committee peers when observers
///   hold their full share.
/// - Periodic: PERIODIC_FETCH_MAX_PER_PEER per peer, P for all observers, 3P for the kind. Each
///   node sends periodic requests to at most MAX_PERIODIC_SYNC_PEERS peers, so committee peers
///   rarely need more than the 2P slots that observers leave free.
/// - Observers across both kinds: 2P.
/// - Total across both kinds: 4P. Only periodic requests from committee peers may use the last
///   P slots. Bulk requests and observer periodic requests share the first 3P.
///
/// When observers hold their full 2P, committee peers share the other 2P. Committee bulk
/// requests can use at most P of them, so P slots stay available to committee periodic sync.
struct FetchBlocksLimiter {
    counts: Arc<Mutex<FetchBlocksCounts>>,
    bulk: KindCaps,
    periodic: KindCaps,
    max_observer_total: usize,
    max_total: usize,
    /// Slots of `max_total` that only periodic requests from committee peers may use.
    committee_periodic_reserve: usize,
}

impl FetchBlocksLimiter {
    fn new(context: &Context) -> Self {
        let parallel_fetches = context.parameters.commit_sync_parallel_fetches.max(1);
        Self {
            counts: Arc::new(Mutex::new(FetchBlocksCounts::default())),
            bulk: KindCaps {
                max_per_peer: parallel_fetches,
                max_observer: 2 * parallel_fetches,
                max_global: 3 * parallel_fetches,
            },
            periodic: KindCaps {
                max_per_peer: PERIODIC_FETCH_MAX_PER_PEER,
                max_observer: parallel_fetches,
                max_global: 3 * parallel_fetches,
            },
            max_observer_total: 2 * parallel_fetches,
            max_total: 4 * parallel_fetches,
            committee_periodic_reserve: parallel_fetches,
        }
    }

    fn caps(&self, kind: StoreFetchKind) -> &KindCaps {
        match kind {
            StoreFetchKind::Bulk => &self.bulk,
            StoreFetchKind::Periodic => &self.periodic,
        }
    }

    /// Tries to admit one store-backed request of `kind` from `peer`. Returns a permit that
    /// releases the capacity when dropped, or None when the request is rejected. The permit
    /// increments `inflight` while it is held.
    fn try_acquire(
        &self,
        kind: StoreFetchKind,
        peer: &PeerId,
        inflight: IntGauge,
    ) -> Option<FetchPermit> {
        let is_observer = matches!(peer, PeerId::Observer(_));
        let caps = self.caps(kind);
        let max_total = if kind == StoreFetchKind::Periodic && !is_observer {
            self.max_total
        } else {
            self.max_total - self.committee_periodic_reserve
        };
        {
            let mut counts = self.counts.lock();
            if counts.total() >= max_total
                || (is_observer && counts.observer_total() >= self.max_observer_total)
            {
                return None;
            }
            let kind_counts = counts.kind_mut(kind);
            if kind_counts.global >= caps.max_global
                || (is_observer && kind_counts.observer >= caps.max_observer)
                || kind_counts.per_peer.get(peer).copied().unwrap_or(0) >= caps.max_per_peer
            {
                return None;
            }
            kind_counts.global += 1;
            if is_observer {
                kind_counts.observer += 1;
            }
            *kind_counts.per_peer.entry(peer.clone()).or_insert(0) += 1;
        }
        Some(FetchPermit {
            counts: self.counts.clone(),
            kind,
            peer: peer.clone(),
            _inflight: InflightGuard::acquire(inflight),
        })
    }
}

/// Releases the capacity held for one admitted store-backed request when dropped.
struct FetchPermit {
    counts: Arc<Mutex<FetchBlocksCounts>>,
    kind: StoreFetchKind,
    peer: PeerId,
    _inflight: InflightGuard,
}

impl Drop for FetchPermit {
    fn drop(&mut self) {
        let mut counts = self.counts.lock();
        let kind_counts = counts.kind_mut(self.kind);
        kind_counts.global -= 1;
        if matches!(self.peer, PeerId::Observer(_)) {
            kind_counts.observer -= 1;
        }
        if let Some(count) = kind_counts.per_peer.get_mut(&self.peer) {
            *count -= 1;
            if *count == 0 {
                kind_counts.per_peer.remove(&self.peer);
            }
        }
    }
}

/// Limits for one store-backed request on the blocking pool. `cancel` is cancelled when the
/// serving future is dropped, for example after the client resets the request on its own
/// timeout, so the detached task can stop early.
struct StoreFetchLimits {
    cancel: CancellationToken,
    deadline: Instant,
}

/// Holds the state needed to serve block and commit reads. Kept behind an Arc so the
/// store-backed fetch path can move a cheap handle onto the blocking pool.
struct BlockSyncCore {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
    store: Arc<dyn Store>,
}

/// Provides shared block synchronization functionality for both AuthorityService and
/// ObserverService. This service handles fetch requests from synchronizer and commit_syncer
/// components, eliminating code duplication between the two network services.
pub(crate) struct BlockSyncService {
    core: Arc<BlockSyncCore>,
    // Set when Parameters::fetch_blocks_admission_control is enabled.
    limiter: Option<FetchBlocksLimiter>,
}

impl BlockSyncService {
    pub fn new(
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        store: Arc<dyn Store>,
    ) -> Self {
        let limiter = context
            .parameters
            .fetch_blocks_admission_control
            .then(|| FetchBlocksLimiter::new(&context));
        Self {
            core: Arc::new(BlockSyncCore {
                context,
                dag_state,
                store,
            }),
            limiter,
        }
    }

    /// Serves a FetchBlocks request. The request is validated first. Live sync
    /// (fetch_missing_ancestors) runs inline. With admission control enabled, every other shape
    /// passes admission control for its kind and runs on the blocking pool. This keeps the async
    /// runtime free for consensus when store reads are slow, and bounds the serving work that
    /// many lagging peers can cause. See issue #28611. With admission control disabled, every
    /// shape runs inline.
    pub async fn fetch_blocks(
        &self,
        peer: PeerId,
        mut block_refs: Vec<BlockRef>,
        fetch_after_rounds: Vec<Round>,
        fetch_missing_ancestors: bool,
    ) -> ConsensusResult<Vec<Bytes>> {
        let max_response_num_blocks = self.core.validate_fetch_blocks_request(
            &mut block_refs,
            &fetch_after_rounds,
            fetch_missing_ancestors,
        )?;

        // Live sync requests recently accepted blocks, which are normally in the DagState cache.
        // It is not gated, so the missing-ancestor path stays fast. A cache miss still reads the
        // store inside DagState::get_blocks.
        let limiter = match &self.limiter {
            Some(limiter) if !fetch_missing_ancestors => limiter,
            _ => {
                return self.core.serve_fetch_blocks(
                    block_refs,
                    fetch_after_rounds,
                    fetch_missing_ancestors,
                    max_response_num_blocks,
                    None,
                );
            }
        };

        let kind = if block_refs.is_empty() || fetch_after_rounds.is_empty() {
            StoreFetchKind::Bulk
        } else {
            StoreFetchKind::Periodic
        };
        let metrics = &self.core.context.metrics.node_metrics;
        let label = peer.labelname(&self.core.context);
        let labels = [label.as_str(), kind.label()];
        let inflight = metrics
            .block_sync_service_store_fetch_inflight
            .with_label_values(&labels);
        let Some(permit) = limiter.try_acquire(kind, &peer, inflight) else {
            metrics
                .block_sync_service_store_fetch_rejected
                .with_label_values(&labels)
                .inc();
            return Err(ConsensusError::FetchBlocksThrottled(peer.to_string()));
        };
        let duration = metrics
            .block_sync_service_store_fetch_duration
            .with_label_values(&[kind.label()]);

        let cancel = CancellationToken::new();
        // Cancels the detached task if the serving future is dropped before the task finishes.
        let _cancel_guard = cancel.clone().drop_guard();

        let core = self.core.clone();
        spawn_blocking(move || {
            // The permit and the timer live in the blocking task, so the slot is released and
            // the duration is recorded when the read ends. If the permit were released with the
            // serving future, a detached task could keep reading after its slot was reused, and
            // concurrent reads could exceed the caps when many clients reset their requests.
            let _permit = permit;
            // A request whose client left while it waited for a worker returns without reading,
            // so it does not record a duration.
            let _timer = (!cancel.is_cancelled()).then(|| duration.start_timer());
            // The budget starts when a blocking worker picks up the task, so time spent waiting
            // for a worker does not shrink the scan to an empty response.
            let limits = StoreFetchLimits {
                cancel,
                deadline: Instant::now() + STORE_FETCH_TIME_BUDGET,
            };
            core.serve_fetch_blocks(
                block_refs,
                fetch_after_rounds,
                fetch_missing_ancestors,
                max_response_num_blocks,
                Some(&limits),
            )
        })
        .await?
    }

    /// Fetches commits and their certifying blocks from the store.
    pub async fn fetch_commits(
        &self,
        commit_range: CommitRange,
    ) -> ConsensusResult<(Vec<TrustedCommit>, Vec<VerifiedBlock>)> {
        self.core.fetch_commits(commit_range).await
    }

    /// Fetches the latest blocks for the specified authorities.
    pub async fn fetch_latest_blocks(
        &self,
        peer: AuthorityIndex,
        authorities: Vec<AuthorityIndex>,
    ) -> ConsensusResult<Vec<Bytes>> {
        self.core.fetch_latest_blocks(peer, authorities).await
    }
}

impl BlockSyncCore {
    // Handles 3 types of requests:
    // 1. Live sync:
    //    - Both missing block refs and highest accepted rounds are specified.
    //    - fetch_missing_ancestors is true.
    //    - response returns max_blocks_per_sync blocks.
    // 2. Periodic sync:
    //    - Highest accepted rounds must be specified.
    //    - Missing block refs are optional.
    //    - fetch_missing_ancestors is false (default).
    //    - response returns max_blocks_per_fetch blocks.
    // 3. Commit sync:
    //    - Missing block refs are specified.
    //    - Highest accepted rounds are empty.
    //    - fetch_missing_ancestors is false (default).
    //    - response returns max_blocks_per_fetch blocks.
    //
    // Validates the request, truncates `block_refs` to the response limit, and returns the limit.
    fn validate_fetch_blocks_request(
        &self,
        block_refs: &mut Vec<BlockRef>,
        fetch_after_rounds: &[Round],
        fetch_missing_ancestors: bool,
    ) -> ConsensusResult<usize> {
        if block_refs.is_empty() && (fetch_missing_ancestors || fetch_after_rounds.is_empty()) {
            return Err(ConsensusError::InvalidFetchBlocksRequest("When no block refs are provided, fetch_after_rounds must be provided and fetch_missing_ancestors must be false".to_string()));
        }
        if fetch_missing_ancestors && fetch_after_rounds.is_empty() {
            return Err(ConsensusError::InvalidFetchBlocksRequest(
                "When fetch_missing_ancestors is true, fetch_after_rounds must be provided"
                    .to_string(),
            ));
        }
        if !fetch_after_rounds.is_empty()
            && fetch_after_rounds.len() != self.context.committee.size()
        {
            return Err(ConsensusError::InvalidSizeOfHighestAcceptedRounds(
                fetch_after_rounds.len(),
                self.context.committee.size(),
            ));
        }

        // Finds the suitable limit of # of blocks to return.
        let max_response_num_blocks = if !fetch_after_rounds.is_empty() && !block_refs.is_empty() {
            self.context.parameters.max_blocks_per_sync
        } else {
            self.context.parameters.max_blocks_per_fetch
        };
        if block_refs.len() > max_response_num_blocks {
            block_refs.truncate(max_response_num_blocks);
        }

        // Validate the requested block refs.
        for block in block_refs.iter() {
            if !self.context.committee.is_valid_index(block.author) {
                return Err(ConsensusError::InvalidAuthorityIndex {
                    loc: format!("requested block {}", block),
                    index: block.author,
                    max: self.context.committee.size() - 1,
                });
            }
            if block.round == GENESIS_ROUND {
                return Err(ConsensusError::UnexpectedGenesisBlockRequested);
            }
        }
        Ok(max_response_num_blocks)
    }

    // Serves a request that validate_fetch_blocks_request has accepted. The caller decides where
    // it runs: requests that skip admission control run inline on the async runtime with `limits`
    // unset, and admitted requests run on the blocking pool with `limits` set.
    fn serve_fetch_blocks(
        &self,
        block_refs: Vec<BlockRef>,
        fetch_after_rounds: Vec<Round>,
        fetch_missing_ancestors: bool,
        max_response_num_blocks: usize,
        limits: Option<&StoreFetchLimits>,
    ) -> ConsensusResult<Vec<Bytes>> {
        // Skip the store reads below if the client gave up while this request waited for a
        // blocking worker.
        if limits.is_some_and(|limits| limits.cancel.is_cancelled()) {
            return Ok(Vec::new());
        }

        // Get the requested blocks first. Commit sync requests (block refs only, no
        // fetch_after_rounds, no fetch_missing_ancestors) read old committed blocks, up
        // to max_blocks_per_fetch of them, so they read the store first to avoid holding
        // the DagState lock across large storage reads. Every other request shape
        // targets recently accepted blocks, capped at max_blocks_per_sync, which are
        // almost always in the DagState cache; reading the store first would only add a
        // wasted store read per request.
        let mut blocks = if fetch_after_rounds.is_empty() {
            self.read_blocks(&block_refs)?
        } else {
            self.dag_state
                .read()
                .get_blocks(&block_refs)
                .into_iter()
                .flatten()
                .collect()
        };

        // When fetch_missing_ancestors is true, fetch missing ancestors of the requested blocks.
        // Otherwise, fetch additional blocks depth-first from the requested block authorities.
        if blocks.len() < max_response_num_blocks && !fetch_after_rounds.is_empty() {
            if fetch_missing_ancestors {
                // Get unique missing ancestor blocks of the requested blocks (validated to be non-empty).
                // fetch_after_rounds will only be used to filter out already accepted blocks.
                let missing_ancestors = blocks
                    .iter()
                    .flat_map(|block| block.ancestors().to_vec())
                    .filter(|block_ref| fetch_after_rounds[block_ref.author] < block_ref.round)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();

                // If there are too many missing ancestors, randomly select a subset to avoid
                // fetching duplicated blocks across peers.
                let selected_num_blocks = max_response_num_blocks
                    .saturating_sub(blocks.len())
                    .min(missing_ancestors.len());
                if selected_num_blocks > 0 {
                    let selected_ancestor_refs = missing_ancestors
                        .choose_multiple(&mut mysten_common::random::get_rng(), selected_num_blocks)
                        .copied()
                        .collect::<Vec<_>>();
                    // Ancestors of recently accepted blocks are in the cache, like the
                    // requested blocks above.
                    let ancestor_blocks = self
                        .dag_state
                        .read()
                        .get_blocks(&selected_ancestor_refs)
                        .into_iter()
                        .flatten();
                    blocks.extend(ancestor_blocks);
                }
            } else {
                // Get additional blocks from authorities with missing block.
                // Compute the fetch round per requested authority, or all authorities.
                let mut limit_rounds = BTreeMap::<AuthorityIndex, Round>::new();
                if block_refs.is_empty() {
                    let dag_state = self.dag_state.read();
                    for (index, _authority) in self.context.committee.authorities() {
                        let last_block = dag_state.get_last_block_for_authority(index);
                        limit_rounds.insert(index, last_block.round());
                    }
                } else {
                    for block_ref in &block_refs {
                        let entry = limit_rounds
                            .entry(block_ref.author)
                            .or_insert(block_ref.round);
                        *entry = (*entry).min(block_ref.round);
                    }
                }

                // Use a min-heap to fetch blocks across authorities in ascending round order.
                // Each entry is (fetch_start_round, authority, limit_round).
                let mut heap = BinaryHeap::new();
                for (authority, limit_round) in &limit_rounds {
                    // fetch_after_rounds comes from the peer, so saturate instead of overflowing.
                    let fetch_start = fetch_after_rounds[*authority].saturating_add(1);
                    if fetch_start < *limit_round {
                        heap.push(Reverse((fetch_start, *authority, *limit_round)));
                    }
                }

                while let Some(Reverse((fetch_start, authority, limit_round))) = heap.pop() {
                    // Stop early if the client gave up (future dropped) or the time budget is
                    // spent; the blocks gathered so far are a valid partial response.
                    if let Some(limits) = limits
                        && (limits.cancel.is_cancelled() || Instant::now() >= limits.deadline)
                    {
                        break;
                    }
                    let fetched = self.store.scan_blocks_by_author_in_range(
                        authority,
                        fetch_start,
                        limit_round,
                        1,
                    )?;
                    if let Some(block) = fetched.into_iter().next() {
                        let next_start = block.round() + 1;
                        blocks.push(block);
                        if blocks.len() >= max_response_num_blocks {
                            blocks.truncate(max_response_num_blocks);
                            break;
                        }
                        if next_start < limit_round {
                            heap.push(Reverse((next_start, authority, limit_round)));
                        }
                    }
                }
            }
        }

        // Return the serialized blocks
        let bytes = blocks
            .into_iter()
            .map(|block| block.serialized().clone())
            .collect::<Vec<_>>();
        Ok(bytes)
    }

    /// Reads blocks by refs like `DagState::get_blocks`, but reads the store first so no
    /// storage I/O happens while holding the DagState lock: commit sync requests read up
    /// to `max_blocks_per_fetch` blocks, and a read lock held across large store reads
    /// delays Core's writes on the consensus hot path. Blocks missing from the store are
    /// the most recently accepted ones that are not yet flushed, which are found in
    /// DagState's in-memory cache; only blocks that do not exist at all fall through to
    /// `get_blocks`' internal store read, and honest requesters never ask for those.
    /// Blocks found nowhere are dropped from the result.
    fn read_blocks(&self, block_refs: &[BlockRef]) -> ConsensusResult<Vec<VerifiedBlock>> {
        self.context
            .metrics
            .node_metrics
            .block_sync_service_read_blocks_count
            .inc();
        let mut blocks = self.store.read_blocks(block_refs)?;
        let missing: Vec<(usize, BlockRef)> = blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| block.is_none())
            .map(|(index, _)| (index, block_refs[index]))
            .collect();
        if !missing.is_empty() {
            let missing_refs: Vec<BlockRef> =
                missing.iter().map(|(_, block_ref)| *block_ref).collect();
            let cached_blocks = self.dag_state.read().get_blocks(&missing_refs);
            for ((index, _), block) in missing.into_iter().zip_debug_eq(cached_blocks) {
                blocks[index] = block;
            }
        }
        Ok(blocks.into_iter().flatten().collect())
    }

    /// Fetches commits and their certifying blocks from the store.
    ///
    /// Returns commits within the specified range along with the blocks that certify
    /// the last commit (if it has reached quorum).
    pub async fn fetch_commits(
        &self,
        commit_range: CommitRange,
    ) -> ConsensusResult<(Vec<TrustedCommit>, Vec<VerifiedBlock>)> {
        // Compute an inclusive end index and bound the maximum number of commits scanned
        let inclusive_end = commit_range.end().min(
            commit_range.start() + self.context.parameters.commit_sync_batch_size as CommitIndex
                - 1,
        );
        let mut commits = self
            .store
            .scan_commits((commit_range.start()..=inclusive_end).into())?;

        let mut certifier_block_refs = vec![];

        // Find the last commit that has reached quorum
        'commit: while let Some(c) = commits.last() {
            let index = c.index();
            let votes = self.store.read_commit_votes(index)?;
            let mut stake_aggregator = StakeAggregator::<QuorumThreshold>::new();
            for v in &votes {
                stake_aggregator.add(v.author, &self.context.committee);
            }
            if stake_aggregator.reached_threshold(&self.context.committee) {
                certifier_block_refs = votes;
                break 'commit;
            } else {
                debug!(
                    "Commit {} votes did not reach quorum to certify, {} < {}, skipping",
                    index,
                    stake_aggregator.stake(),
                    stake_aggregator.threshold(&self.context.committee)
                );
                self.context
                    .metrics
                    .node_metrics
                    .commit_sync_fetch_commits_handler_uncertified_skipped
                    .inc();
                commits.pop();
            }
        }

        // Fetch the certifier blocks
        let certifier_blocks = self
            .store
            .read_blocks(&certifier_block_refs)?
            .into_iter()
            .flatten()
            .collect();

        Ok((commits, certifier_blocks))
    }

    /// Fetches the latest blocks for the specified authorities.
    pub async fn fetch_latest_blocks(
        &self,
        peer: AuthorityIndex,
        authorities: Vec<AuthorityIndex>,
    ) -> ConsensusResult<Vec<Bytes>> {
        if authorities.len() > self.context.committee.size() {
            return Err(ConsensusError::TooManyAuthoritiesProvided(peer));
        }

        // Ensure that those are valid authorities
        for authority in &authorities {
            if !self.context.committee.is_valid_index(*authority) {
                return Err(ConsensusError::InvalidAuthorityIndex {
                    loc: format!("latest block request from peer {}", peer),
                    index: *authority,
                    max: self.context.committee.size() - 1,
                });
            }
        }

        // Read from the dag state to find the latest blocks
        // TODO: at the moment we don't look into the block manager for suspended blocks. Ideally we
        // want in the future if we think we would like to tackle the majority of cases.
        let mut blocks = vec![];
        let dag_state = self.dag_state.read();
        for authority in authorities {
            let block = dag_state.get_last_block_for_authority(authority);

            debug!("Latest block for {authority}: {block:?}");

            // no reason to serve back the genesis block - it's equal as if it has not received any block
            if block.round() != GENESIS_ROUND {
                blocks.push(block);
            }
        }

        // Return the serialised blocks
        let result = blocks
            .into_iter()
            .map(|block| block.serialized().clone())
            .collect::<Vec<_>>();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use consensus_config::{NetworkKeyPair, ProtocolKeyPair};

    #[cfg(not(msim))]
    use consensus_types::block::TransactionIndex;

    use super::*;
    #[cfg(not(msim))]
    use crate::commit::{CommitInfo, CommitRef};
    use crate::{
        block::{TestBlock, VerifiedBlock},
        storage::{WriteBatch, mem_store::MemStore},
    };

    fn setup_service() -> (BlockSyncService, Arc<RwLock<DagState>>, Arc<MemStore>) {
        setup_service_with_admission_control(true)
    }

    fn setup_service_with_admission_control(
        enabled: bool,
    ) -> (BlockSyncService, Arc<RwLock<DagState>>, Arc<MemStore>) {
        let context = Arc::new(test_context(enabled));
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let service = BlockSyncService::new(context, dag_state.clone(), store.clone());
        (service, dag_state, store)
    }

    fn test_context(admission_control: bool) -> Context {
        let (mut context, _keys) = Context::new_for_test(4);
        context.parameters.fetch_blocks_admission_control = admission_control;
        context
    }

    /// Returns the service's limiter, which tests enable through test_context.
    fn limiter(service: &BlockSyncService) -> &FetchBlocksLimiter {
        service.limiter.as_ref().expect("admission control enabled")
    }

    fn test_peer() -> PeerId {
        PeerId::Validator(AuthorityIndex::new_for_test(0))
    }

    fn test_blocks(round: Round) -> Vec<VerifiedBlock> {
        (0..4)
            .map(|author| VerifiedBlock::new_for_test(TestBlock::new(round, author).build()))
            .collect()
    }

    #[tokio::test]
    async fn test_fetch_blocks_from_store() {
        let (service, _dag_state, store) = setup_service();

        // Blocks in the store but not in the DagState cache, as after cache eviction.
        let blocks = test_blocks(1);
        store
            .write(WriteBatch::default().blocks(blocks.clone()))
            .unwrap();

        let block_refs: Vec<_> = blocks.iter().map(|b| b.reference()).collect();
        let fetched = service
            .fetch_blocks(test_peer(), block_refs, vec![], false)
            .await
            .unwrap();
        assert_eq!(
            fetched,
            blocks
                .iter()
                .map(|b| b.serialized().clone())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_fetch_blocks_unflushed_from_cache() {
        let (service, dag_state, _store) = setup_service();

        // Blocks accepted into the DagState but not yet flushed to the store.
        let blocks = test_blocks(1);
        dag_state.write().accept_blocks(blocks.clone());

        let block_refs: Vec<_> = blocks.iter().map(|b| b.reference()).collect();
        let fetched = service
            .fetch_blocks(test_peer(), block_refs, vec![], false)
            .await
            .unwrap();
        assert_eq!(
            fetched,
            blocks
                .iter()
                .map(|b| b.serialized().clone())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_fetch_blocks_mixed_store_and_cache() {
        let (service, dag_state, store) = setup_service();

        // Round 1 blocks are only in the store, round 2 blocks only in the cache,
        // as when older blocks have been flushed and evicted while recent ones are
        // not yet flushed.
        let flushed_blocks = test_blocks(1);
        store
            .write(WriteBatch::default().blocks(flushed_blocks.clone()))
            .unwrap();
        let unflushed_blocks = test_blocks(2);
        dag_state.write().accept_blocks(unflushed_blocks.clone());

        // Interleave the refs to check the response preserves request order.
        let all_blocks: Vec<_> = flushed_blocks
            .into_iter()
            .zip_debug_eq(unflushed_blocks)
            .flat_map(|(flushed, unflushed)| [flushed, unflushed])
            .collect();
        let block_refs: Vec<_> = all_blocks.iter().map(|b| b.reference()).collect();
        let fetched = service
            .fetch_blocks(test_peer(), block_refs, vec![], false)
            .await
            .unwrap();
        assert_eq!(
            fetched,
            all_blocks
                .iter()
                .map(|b| b.serialized().clone())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_fetch_blocks_live_sync_from_cache() {
        let (service, dag_state, _store) = setup_service();

        // Live sync shape: refs + fetch_after_rounds, served from the DagState cache
        // even when nothing has been flushed to the store.
        let blocks = test_blocks(1);
        dag_state.write().accept_blocks(blocks.clone());

        let block_refs: Vec<_> = blocks.iter().map(|b| b.reference()).collect();
        let fetched = service
            .fetch_blocks(test_peer(), block_refs, vec![0; 4], true)
            .await
            .unwrap();
        assert_eq!(
            fetched,
            blocks
                .iter()
                .map(|b| b.serialized().clone())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_fetch_blocks_missing_blocks_dropped() {
        let (service, _dag_state, store) = setup_service();

        let blocks = test_blocks(1);
        store
            .write(WriteBatch::default().blocks(blocks.clone()))
            .unwrap();

        // Request one existing block and one unknown block.
        let unknown_ref = VerifiedBlock::new_for_test(TestBlock::new(5, 0).build()).reference();
        let fetched = service
            .fetch_blocks(
                test_peer(),
                vec![blocks[0].reference(), unknown_ref],
                vec![],
                false,
            )
            .await
            .unwrap();
        assert_eq!(fetched, vec![blocks[0].serialized().clone()]);
    }

    #[tokio::test]
    async fn fetch_blocks_throttled_when_peer_saturated() {
        let (service, _dag_state, store) = setup_service();
        let blocks = test_blocks(1);
        store
            .write(WriteBatch::default().blocks(blocks.clone()))
            .unwrap();
        let block_refs: Vec<_> = blocks.iter().map(|b| b.reference()).collect();
        let peer = test_peer();

        // Hold the peer's whole bulk budget by acquiring its permits directly, then a commit-sync
        // request from the same peer is rejected instead of reading the store.
        let limiter = limiter(&service);
        let mut permits = acquire(
            limiter,
            StoreFetchKind::Bulk,
            &peer,
            limiter.bulk.max_per_peer,
        );
        let result = service
            .fetch_blocks(peer.clone(), block_refs.clone(), vec![], false)
            .await;
        assert!(matches!(
            result,
            Err(ConsensusError::FetchBlocksThrottled(_))
        ));

        // Releasing the held permits lets the same request through.
        permits.clear();
        let fetched = service
            .fetch_blocks(peer, block_refs, vec![], false)
            .await
            .unwrap();
        assert_eq!(fetched.len(), blocks.len());
    }

    #[tokio::test]
    async fn fetch_blocks_periodic_sync_uses_separate_budget() {
        let (service, dag_state, store) = setup_service();
        let blocks = test_blocks(1);
        store
            .write(WriteBatch::default().blocks(blocks.clone()))
            .unwrap();
        dag_state.write().accept_blocks(blocks.clone());
        let block_refs: Vec<_> = blocks.iter().map(|b| b.reference()).collect();
        let committee_size = service.core.context.committee.size();

        // Fill the global bulk budget with validator peers, each up to the per-peer cap.
        let limiter = limiter(&service);
        let peers_to_fill = limiter.bulk.max_global / limiter.bulk.max_per_peer;
        assert!(peers_to_fill < committee_size);
        let _permits: Vec<_> = (0..peers_to_fill)
            .flat_map(|i| {
                let peer = PeerId::Validator(AuthorityIndex::new_for_test(i as u32));
                acquire(
                    limiter,
                    StoreFetchKind::Bulk,
                    &peer,
                    limiter.bulk.max_per_peer,
                )
            })
            .collect();
        let peer = PeerId::Validator(AuthorityIndex::new_for_test(peers_to_fill as u32));

        // A bulk request from another validator is rejected.
        let result = service
            .fetch_blocks(peer.clone(), block_refs.clone(), vec![], false)
            .await;
        assert!(matches!(
            result,
            Err(ConsensusError::FetchBlocksThrottled(_))
        ));

        // A periodic sync request from the same validator is still served.
        let fetched = service
            .fetch_blocks(peer, block_refs, vec![0; committee_size], false)
            .await
            .unwrap();
        assert_eq!(fetched.len(), blocks.len());
    }

    #[tokio::test]
    async fn fetch_blocks_live_sync_bypasses_admission() {
        let (service, dag_state, _store) = setup_service();
        let blocks = test_blocks(1);
        dag_state.write().accept_blocks(blocks.clone());
        let block_refs: Vec<_> = blocks.iter().map(|b| b.reference()).collect();
        let peer = test_peer();

        // Hold the peer's whole bulk and periodic budgets. Live sync is still served.
        let limiter = limiter(&service);
        let _bulk = acquire(
            limiter,
            StoreFetchKind::Bulk,
            &peer,
            limiter.bulk.max_per_peer,
        );
        let _periodic = acquire(
            limiter,
            StoreFetchKind::Periodic,
            &peer,
            limiter.periodic.max_per_peer,
        );
        let fetched = service
            .fetch_blocks(peer, block_refs, vec![0; 4], true)
            .await
            .unwrap();
        assert_eq!(fetched.len(), blocks.len());
    }

    // The gated store blocks a spawn_blocking thread on an OS condvar. Under msim that never ends
    // the thread's turn and hangs the simulator.
    #[cfg(not(msim))]
    #[tokio::test]
    async fn fetch_blocks_permit_held_until_store_read_ends() {
        let (service, _dag_state, store) = setup_gated_service();
        let blocks = test_blocks(1);
        store
            .write(WriteBatch::default().blocks(blocks.clone()))
            .unwrap();
        let block_refs: Vec<_> = blocks.iter().map(|b| b.reference()).collect();

        // Start a commit-sync request and hold its store read open.
        let gate = store.close();
        let handle = tokio::spawn({
            let service = service.clone();
            async move {
                service
                    .fetch_blocks(test_peer(), block_refs, vec![], false)
                    .await
            }
        });
        store.wait_entered().await;

        // Dropping the serving future does not release the slot while the read is running.
        handle.abort();
        let _ = handle.await;
        assert_eq!(limiter(&service).counts.lock().total(), 1);

        // The slot is released once the read ends.
        drop(gate);
        wait_for_release(limiter(&service)).await;
    }

    #[cfg(not(msim))]
    #[tokio::test]
    async fn fetch_blocks_failover_scan_stops_when_cancelled() {
        let (service, dag_state, store) = setup_gated_service();
        let blocks: Vec<_> = (1..=5).flat_map(test_blocks).collect();
        store
            .write(WriteBatch::default().blocks(blocks.clone()))
            .unwrap();
        dag_state.write().accept_blocks(blocks);

        // Start a failover request and hold its first store scan open.
        let gate = store.close();
        let handle = tokio::spawn({
            let service = service.clone();
            async move {
                service
                    .fetch_blocks(test_peer(), vec![], vec![0; 4], false)
                    .await
            }
        });
        store.wait_entered().await;

        // After the client gives up, the scan stops after the read in progress. Without the
        // cancel check it would scan the remaining rounds of every authority.
        handle.abort();
        let _ = handle.await;
        drop(gate);
        wait_for_release(limiter(&service)).await;
        assert_eq!(store.scans(), 1);
    }

    #[cfg(not(msim))]
    #[tokio::test]
    async fn fetch_blocks_failover_scan_returns_partial_response_at_deadline() {
        let (service, dag_state, store) = setup_gated_service();
        let blocks: Vec<_> = (1..=5).flat_map(test_blocks).collect();
        store
            .write(WriteBatch::default().blocks(blocks.clone()))
            .unwrap();
        dag_state.write().accept_blocks(blocks);

        // Hold the first store scan open past the time budget.
        let gate = store.close();
        let handle = tokio::spawn({
            let service = service.clone();
            async move {
                service
                    .fetch_blocks(test_peer(), vec![], vec![0; 4], false)
                    .await
            }
        });
        store.wait_entered().await;
        tokio::time::sleep(STORE_FETCH_TIME_BUDGET + Duration::from_millis(100)).await;
        drop(gate);

        // The scan returns the one block it read before the deadline.
        let fetched = handle.await.unwrap().unwrap();
        assert_eq!(fetched.len(), 1);
        assert_eq!(store.scans(), 1);
    }

    fn test_gauge() -> IntGauge {
        IntGauge::new("test_store_fetch_inflight", "test").unwrap()
    }

    /// Acquires `count` permits of `kind` for `peer`, panicking if any is rejected.
    fn acquire(
        limiter: &FetchBlocksLimiter,
        kind: StoreFetchKind,
        peer: &PeerId,
        count: usize,
    ) -> Vec<FetchPermit> {
        (0..count)
            .map(|_| {
                limiter
                    .try_acquire(kind, peer, test_gauge())
                    .expect("request should be admitted")
            })
            .collect()
    }

    fn observer_peer(key: &(NetworkKeyPair, ProtocolKeyPair)) -> PeerId {
        PeerId::Observer(Box::new(key.0.public()))
    }

    // These tests build a Context, whose clock reads tokio time, so they run as tokio tests
    // to avoid panicking outside a runtime under msim.

    #[tokio::test]
    async fn fetch_blocks_limiter_per_peer_cap() {
        let (context, _keys) = Context::new_for_test(10);
        let limiter = FetchBlocksLimiter::new(&context);
        let peer = PeerId::Validator(AuthorityIndex::new_for_test(0));
        for kind in [StoreFetchKind::Bulk, StoreFetchKind::Periodic] {
            let max_per_peer = limiter.caps(kind).max_per_peer;
            let mut permits = acquire(&limiter, kind, &peer, max_per_peer);
            assert!(limiter.try_acquire(kind, &peer, test_gauge()).is_none());

            // Releasing one permit frees one slot for that peer.
            permits.pop();
            assert!(limiter.try_acquire(kind, &peer, test_gauge()).is_some());
        }
    }

    #[tokio::test]
    async fn fetch_blocks_limiter_kind_cap() {
        let (context, _keys) = Context::new_for_test(30);
        for kind in [StoreFetchKind::Bulk, StoreFetchKind::Periodic] {
            let limiter = FetchBlocksLimiter::new(&context);
            let caps = limiter.caps(kind);

            // Fill the kind's budget with validator peers, each up to the per-peer cap.
            let peers_to_fill = caps.max_global / caps.max_per_peer;
            let _permits: Vec<_> = (0..peers_to_fill)
                .flat_map(|i| {
                    let peer = PeerId::Validator(AuthorityIndex::new_for_test(i as u32));
                    acquire(&limiter, kind, &peer, caps.max_per_peer)
                })
                .collect();
            let extra = PeerId::Validator(AuthorityIndex::new_for_test(peers_to_fill as u32));
            assert!(limiter.try_acquire(kind, &extra, test_gauge()).is_none());
        }
    }

    #[tokio::test]
    async fn fetch_blocks_limiter_total_cap() {
        let (context, _keys) = Context::new_for_test(30);
        let limiter = FetchBlocksLimiter::new(&context);

        // Fill the bulk budget.
        let bulk = &limiter.bulk;
        let bulk_peers = bulk.max_global / bulk.max_per_peer;
        let _bulk_permits: Vec<_> = (0..bulk_peers)
            .flat_map(|i| {
                let peer = PeerId::Validator(AuthorityIndex::new_for_test(i as u32));
                acquire(&limiter, StoreFetchKind::Bulk, &peer, bulk.max_per_peer)
            })
            .collect();

        // Periodic requests are admitted only up to the total cap, below the periodic cap.
        let remaining = limiter.max_total - bulk.max_global;
        assert!(remaining < limiter.periodic.max_global);
        let mut admitted = 0;
        let mut periodic_permits = Vec::new();
        for i in 0..30 {
            let peer = PeerId::Validator(AuthorityIndex::new_for_test(i));
            while let Some(permit) =
                limiter.try_acquire(StoreFetchKind::Periodic, &peer, test_gauge())
            {
                periodic_permits.push(permit);
                admitted += 1;
            }
        }
        assert_eq!(admitted, remaining);
    }

    #[tokio::test]
    async fn fetch_blocks_limiter_reserves_budget_for_committee() {
        let (context, keys) = Context::new_for_test(10);
        let limiter = FetchBlocksLimiter::new(&context);
        let bulk = &limiter.bulk;

        // Observers may hold at most the observer share, even though the kind budget is larger.
        let observers_to_fill = bulk.max_observer / bulk.max_per_peer;
        let _observer_permits: Vec<_> = keys[..observers_to_fill]
            .iter()
            .flat_map(|key| {
                acquire(
                    &limiter,
                    StoreFetchKind::Bulk,
                    &observer_peer(key),
                    bulk.max_per_peer,
                )
            })
            .collect();
        let next_observer = observer_peer(&keys[observers_to_fill]);
        assert!(
            limiter
                .try_acquire(StoreFetchKind::Bulk, &next_observer, test_gauge())
                .is_none()
        );

        // A committee peer is still admitted, up to its per-peer cap, from the budget that the
        // observer share leaves free.
        let validator = PeerId::Validator(AuthorityIndex::new_for_test(0));
        let _validator_permits = acquire(
            &limiter,
            StoreFetchKind::Bulk,
            &validator,
            bulk.max_per_peer,
        );
    }

    #[tokio::test]
    async fn fetch_blocks_limiter_keeps_committee_capacity_when_saturated() {
        let (context, keys) = Context::new_for_test(30);
        let limiter = FetchBlocksLimiter::new(&context);
        let mut permits = Vec::new();
        let mut admit_all = |kind: StoreFetchKind, peers: &[PeerId]| {
            let mut admitted = 0;
            for peer in peers {
                while let Some(permit) = limiter.try_acquire(kind, peer, test_gauge()) {
                    permits.push(permit);
                    admitted += 1;
                }
            }
            admitted
        };
        let observers: Vec<_> = keys.iter().map(observer_peer).collect();
        let validators: Vec<_> = (0..30)
            .map(|i| PeerId::Validator(AuthorityIndex::new_for_test(i)))
            .collect();

        // Observers take every slot they can get, of both kinds.
        let observer_bulk = admit_all(StoreFetchKind::Bulk, &observers);
        let observer_periodic = admit_all(StoreFetchKind::Periodic, &observers);
        assert_eq!(
            observer_bulk + observer_periodic,
            limiter.max_observer_total
        );

        // Lagging committee peers still get bulk slots, but not the periodic reserve.
        let committee_bulk = admit_all(StoreFetchKind::Bulk, &validators);
        assert_eq!(
            committee_bulk,
            limiter.max_total - limiter.committee_periodic_reserve - limiter.max_observer_total
        );
        assert!(committee_bulk >= limiter.bulk.max_per_peer);

        // Committee periodic requests are still admitted, up to the reserve.
        let committee_periodic = admit_all(StoreFetchKind::Periodic, &validators);
        assert_eq!(committee_periodic, limiter.committee_periodic_reserve);
        assert_eq!(limiter.counts.lock().total(), limiter.max_total);
    }

    #[tokio::test]
    async fn fetch_blocks_limiter_releases_all_counts_on_drop() {
        let (context, keys) = Context::new_for_test(10);
        let limiter = FetchBlocksLimiter::new(&context);
        let observers_to_fill = limiter.bulk.max_observer / limiter.bulk.max_per_peer;
        let validator = PeerId::Validator(AuthorityIndex::new_for_test(0));

        let mut permits: Vec<_> = keys[..observers_to_fill]
            .iter()
            .flat_map(|key| {
                acquire(
                    &limiter,
                    StoreFetchKind::Bulk,
                    &observer_peer(key),
                    limiter.bulk.max_per_peer,
                )
            })
            .collect();
        permits.extend(acquire(&limiter, StoreFetchKind::Bulk, &validator, 1));
        permits.extend(acquire(&limiter, StoreFetchKind::Periodic, &validator, 1));
        drop(permits);

        let mut counts = limiter.counts.lock();
        assert_eq!(counts.total(), 0);
        for kind in [StoreFetchKind::Bulk, StoreFetchKind::Periodic] {
            let kind_counts = counts.kind_mut(kind);
            assert_eq!(kind_counts.global, 0);
            assert_eq!(kind_counts.observer, 0);
            assert!(kind_counts.per_peer.is_empty());
        }
    }

    #[tokio::test]
    async fn fetch_blocks_without_admission_control_serves_inline() {
        let (service, _dag_state, store) = setup_service_with_admission_control(false);
        assert!(service.limiter.is_none());
        let blocks = test_blocks(1);
        store
            .write(WriteBatch::default().blocks(blocks.clone()))
            .unwrap();
        let block_refs: Vec<_> = blocks.iter().map(|b| b.reference()).collect();

        let fetched = service
            .fetch_blocks(test_peer(), block_refs, vec![], false)
            .await
            .unwrap();
        assert_eq!(fetched.len(), blocks.len());
    }

    #[tokio::test]
    async fn fetch_blocks_rejects_invalid_request_before_admission() {
        let (service, _dag_state, _store) = setup_service();
        let peer = test_peer();

        // Hold the peer's whole budget. A malformed request still gets its validation error
        // rather than FetchBlocksThrottled, because validation runs before admission.
        let limiter = limiter(&service);
        let _permits = acquire(
            limiter,
            StoreFetchKind::Bulk,
            &peer,
            limiter.bulk.max_per_peer,
        );
        let result = service.fetch_blocks(peer, vec![], vec![], false).await;
        assert!(matches!(
            result,
            Err(ConsensusError::InvalidFetchBlocksRequest(_))
        ));
    }

    #[tokio::test]
    async fn fetch_blocks_failover_shape_tolerates_max_round() {
        let (service, _dag_state, _store) = setup_service();

        // fetch_after_rounds comes from the peer. u32::MAX must not overflow the scan start.
        let committee_size = service.core.context.committee.size();
        let fetched = service
            .fetch_blocks(test_peer(), vec![], vec![Round::MAX; committee_size], false)
            .await
            .unwrap();
        assert!(fetched.is_empty());
    }

    #[cfg(not(msim))]
    fn setup_gated_service() -> (
        Arc<BlockSyncService>,
        Arc<RwLock<DagState>>,
        Arc<GatedStore>,
    ) {
        let context = Arc::new(test_context(true));
        let store = Arc::new(GatedStore::default());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let service = BlockSyncService::new(context, dag_state.clone(), store.clone());
        (Arc::new(service), dag_state, store)
    }

    #[cfg(not(msim))]
    /// Waits until every permit of `limiter` is released.
    async fn wait_for_release(limiter: &FetchBlocksLimiter) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while limiter.counts.lock().total() != 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("permit was not released");
    }

    #[cfg(not(msim))]
    /// A MemStore whose block reads (`read_blocks` and `scan_blocks_by_author_in_range`) block
    /// while the gate is closed, so tests can hold a store-backed request in progress.
    #[derive(Default)]
    struct GatedStore {
        inner: MemStore,
        closed: Mutex<bool>,
        opened: parking_lot::Condvar,
        entered: tokio::sync::Notify,
        scans: std::sync::atomic::AtomicUsize,
    }

    /// Opens the gate when dropped. A failed assertion then unwinds through the guard and frees
    /// the blocking thread parked on the gate, so runtime shutdown does not hang the test.
    #[cfg(not(msim))]
    #[must_use]
    struct GateGuard<'a>(&'a GatedStore);

    #[cfg(not(msim))]
    impl Drop for GateGuard<'_> {
        fn drop(&mut self) {
            self.0.open();
        }
    }

    #[cfg(not(msim))]
    impl GatedStore {
        fn close(&self) -> GateGuard<'_> {
            *self.closed.lock() = true;
            GateGuard(self)
        }

        fn open(&self) {
            *self.closed.lock() = false;
            self.opened.notify_all();
        }

        async fn wait_entered(&self) {
            tokio::time::timeout(Duration::from_secs(10), self.entered.notified())
                .await
                .expect("store read did not start");
        }

        /// Number of scan_blocks_by_author_in_range calls, gated or not.
        fn scans(&self) -> usize {
            self.scans.load(std::sync::atomic::Ordering::SeqCst)
        }

        fn pass_gate(&self) {
            let mut closed = self.closed.lock();
            if !*closed {
                return;
            }
            self.entered.notify_one();
            while *closed {
                self.opened.wait(&mut closed);
            }
        }
    }

    #[cfg(not(msim))]
    impl Store for GatedStore {
        fn write(&self, write_batch: WriteBatch) -> ConsensusResult<()> {
            self.inner.write(write_batch)
        }

        fn read_blocks(&self, refs: &[BlockRef]) -> ConsensusResult<Vec<Option<VerifiedBlock>>> {
            self.pass_gate();
            self.inner.read_blocks(refs)
        }

        fn contains_blocks(&self, refs: &[BlockRef]) -> ConsensusResult<Vec<bool>> {
            self.inner.contains_blocks(refs)
        }

        fn scan_blocks_by_author(
            &self,
            authority: AuthorityIndex,
            start_round: Round,
        ) -> ConsensusResult<Vec<VerifiedBlock>> {
            self.inner.scan_blocks_by_author(authority, start_round)
        }

        fn scan_blocks_by_author_in_range(
            &self,
            author: AuthorityIndex,
            start_round: Round,
            end_round: Round,
            limit: usize,
        ) -> ConsensusResult<Vec<VerifiedBlock>> {
            self.scans.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.pass_gate();
            self.inner
                .scan_blocks_by_author_in_range(author, start_round, end_round, limit)
        }

        fn scan_last_blocks_by_author(
            &self,
            author: AuthorityIndex,
            num_of_rounds: u64,
            before_round: Option<Round>,
        ) -> ConsensusResult<Vec<VerifiedBlock>> {
            self.inner
                .scan_last_blocks_by_author(author, num_of_rounds, before_round)
        }

        fn read_last_commit(&self) -> ConsensusResult<Option<TrustedCommit>> {
            self.inner.read_last_commit()
        }

        fn scan_commits(&self, range: CommitRange) -> ConsensusResult<Vec<TrustedCommit>> {
            self.inner.scan_commits(range)
        }

        fn read_commit_votes(&self, commit_index: CommitIndex) -> ConsensusResult<Vec<BlockRef>> {
            self.inner.read_commit_votes(commit_index)
        }

        fn read_last_commit_info(&self) -> ConsensusResult<Option<(CommitRef, CommitInfo)>> {
            self.inner.read_last_commit_info()
        }

        fn read_last_finalized_commit(&self) -> ConsensusResult<Option<CommitRef>> {
            self.inner.read_last_finalized_commit()
        }

        fn read_rejected_transactions(
            &self,
            commit_ref: CommitRef,
        ) -> ConsensusResult<Option<BTreeMap<BlockRef, Vec<TransactionIndex>>>> {
            self.inner.read_rejected_transactions(commit_ref)
        }
    }
}
