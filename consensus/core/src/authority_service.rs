// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, pin::Pin, sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, Round};
use futures::{Stream, StreamExt, ready, stream, task};
use mysten_metrics::spawn_monitored_task;
use parking_lot::RwLock;
use sui_macros::fail_point_async;
use tap::TapFallible;
use tokio::{sync::broadcast, time::Instant};
use tokio_util::sync::ReusableBoxFuture;
use tracing::{debug, info, warn};

use crate::{
    block::{BlockAPI as _, ExtendedBlock, GENESIS_ROUND, SignedBlock, VerifiedBlock},
    block_sync_service::BlockSyncService,
    block_verifier::BlockVerifier,
    commit::{CommitRange, TrustedCommit},
    commit_vote_monitor::{CommitVoteMonitor, is_commit_lagging},
    context::Context,
    core_thread::CoreThreadDispatcher,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    network::{
        BlockStream, ExtendedSerializedBlock, PeerId, SerializedBlockForm, ValidatorNetworkService,
    },
    round_tracker::RoundTracker,
    synchronizer::SynchronizerHandle,
    task::spawn_blocking,
    transaction_vote_tracker::TransactionVoteTracker,
};

/// How often a live subscription re-checks its subscriber's lag. Bounds how often the
/// fan-out path takes the DagState read lock.
const GATE_REEVALUATION_INTERVAL: Duration = Duration::from_secs(1);

/// Minimum time between mode switches on one subscription. Applies only to returning to
/// slim blocks, and only once a switch has happened.
const GATE_MIN_DWELL: Duration = Duration::from_secs(10);

/// The lag at which a subscriber loses slim blocks, and the lower lag at which it
/// earns them back.
///
/// Both are fractions of `gc_depth`, the horizon within which a receiver can still
/// resolve ancestors. Slim stops short of it so a receiver keeps room before its own GC
/// collects the slots a parked reconstruction waits on. The gap between the two keeps
/// lag noise around a single boundary from driving the mode.
fn gate_thresholds(gc_depth: Round) -> (Round, Round) {
    (gc_depth - gc_depth.div_ceil(4), gc_depth / 2)
}

/// Whether a subscription should be in slim mode at `lag`, given the mode it is in.
///
/// Asymmetric: slim is lost at one lag and earned back only at a much lower one.
fn gate_next_mode(emit_slim: bool, lag: Round, lag_to_full: Round, lag_to_slim: Round) -> bool {
    if emit_slim {
        lag <= lag_to_full
    } else {
        lag <= lag_to_slim
    }
}

/// Whether a mode change should be applied now.
///
/// Losing slim blocks takes effect immediately; earning them back waits out the dwell.
fn gate_should_switch(emit_slim: bool, now_slim: bool, dwell_elapsed: bool) -> bool {
    now_slim != emit_slim && (!now_slim || dwell_elapsed)
}

/// How far `peer` trails this node's accepted frontier.
///
/// Measured only from the peer's own latest block in our DAG. A validator proposes at a
/// round only after accepting a quorum below it, so its block here is evidence of where
/// it was; the round it reports at subscribe is its own unverified claim, and taking the
/// later of the two would let a peer talk its way out of ever being demoted.
fn subscriber_lag(dag_state: &DagState, peer: AuthorityIndex) -> Round {
    let frontier = dag_state.highest_accepted_round();
    frontier.saturating_sub(dag_state.get_last_block_for_authority(peer).round())
}

/// Authority's network service implementation, agnostic to the actual networking stack used.
pub(crate) struct AuthorityService<C: CoreThreadDispatcher> {
    context: Arc<Context>,
    commit_vote_monitor: Arc<CommitVoteMonitor>,
    block_verifier: Arc<dyn BlockVerifier>,
    synchronizer: Arc<SynchronizerHandle>,
    core_dispatcher: Arc<C>,
    rx_block_broadcast: broadcast::Receiver<ExtendedBlock>,
    subscription_counter: Arc<SubscriptionCounter>,
    transaction_vote_tracker: TransactionVoteTracker,
    dag_state: Arc<RwLock<DagState>>,
    round_tracker: Arc<RwLock<RoundTracker>>,
    block_sync_service: Arc<BlockSyncService>,
}

impl<C: CoreThreadDispatcher> AuthorityService<C> {
    pub(crate) fn new(
        context: Arc<Context>,
        block_verifier: Arc<dyn BlockVerifier>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        round_tracker: Arc<RwLock<RoundTracker>>,
        synchronizer: Arc<SynchronizerHandle>,
        core_dispatcher: Arc<C>,
        rx_block_broadcast: broadcast::Receiver<ExtendedBlock>,
        transaction_vote_tracker: TransactionVoteTracker,
        dag_state: Arc<RwLock<DagState>>,
        block_sync_service: Arc<BlockSyncService>,
    ) -> Self {
        let subscription_counter = Arc::new(SubscriptionCounter::new(context.clone()));
        Self {
            context,
            block_verifier,
            commit_vote_monitor,
            synchronizer,
            core_dispatcher,
            rx_block_broadcast,
            subscription_counter,
            transaction_vote_tracker,
            dag_state,
            round_tracker,
            block_sync_service,
        }
    }

    // Parses and validates serialized excluded ancestors.
    fn parse_excluded_ancestors(
        &self,
        peer: AuthorityIndex,
        block: &VerifiedBlock,
        mut excluded_ancestors: Vec<Vec<u8>>,
    ) -> ConsensusResult<Vec<BlockRef>> {
        let peer_hostname = &self.context.committee.authority(peer).hostname;

        let excluded_ancestors_limit = self.context.committee.size() * 2;
        if excluded_ancestors.len() > excluded_ancestors_limit {
            debug!(
                "Dropping {} excluded ancestor(s) from {} {} due to size limit",
                excluded_ancestors.len() - excluded_ancestors_limit,
                peer,
                peer_hostname,
            );
            excluded_ancestors.truncate(excluded_ancestors_limit);
        }

        let excluded_ancestors = excluded_ancestors
            .into_iter()
            .map(|serialized| {
                let block_ref: BlockRef =
                    bcs::from_bytes(&serialized).map_err(ConsensusError::MalformedBlock)?;
                if !self.context.committee.is_valid_index(block_ref.author) {
                    return Err(ConsensusError::InvalidAuthorityIndex {
                        loc: format!("excluded ancestor {}", block_ref),
                        index: block_ref.author,
                        max: self.context.committee.size() - 1,
                    });
                }
                if block_ref.round >= block.round() {
                    return Err(ConsensusError::InvalidAncestorRound {
                        ancestor: block_ref.round,
                        block: block.round(),
                    });
                }
                Ok(block_ref)
            })
            .collect::<ConsensusResult<Vec<BlockRef>>>()?;

        for excluded_ancestor in &excluded_ancestors {
            let excluded_ancestor_hostname = &self
                .context
                .committee
                .authority(excluded_ancestor.author)
                .hostname;
            self.context
                .metrics
                .node_metrics
                .network_excluded_ancestors_count_by_authority
                .with_label_values(&[excluded_ancestor_hostname])
                .inc();
        }
        self.context
            .metrics
            .node_metrics
            .network_received_excluded_ancestors_from_authority
            .with_label_values(&[peer_hostname])
            .inc_by(excluded_ancestors.len() as u64);

        Ok(excluded_ancestors)
    }
}

#[async_trait]
impl<C: CoreThreadDispatcher> ValidatorNetworkService for AuthorityService<C> {
    async fn handle_send_block(
        &self,
        peer: AuthorityIndex,
        serialized_block: ExtendedSerializedBlock,
    ) -> ConsensusResult<()> {
        fail_point_async!("consensus-rpc-response");

        let peer_hostname = &self.context.committee.authority(peer).hostname;

        // TODO: dedup block verifications, here and with fetched blocks.
        // Only the full form reaches this service: a slim payload is decoded back to
        // full upstream (or dropped, before the decoder exists).
        let SerializedBlockForm::Full(serialized_bytes) = serialized_block.block else {
            return Err(ConsensusError::UnexpectedBlockForm);
        };
        let signed_block: SignedBlock =
            bcs::from_bytes(&serialized_bytes).map_err(ConsensusError::MalformedBlock)?;

        // Reject blocks not produced by the peer.
        if peer != signed_block.author() {
            self.context
                .metrics
                .node_metrics
                .invalid_blocks
                .with_label_values(&[
                    peer_hostname.as_str(),
                    "handle_send_block",
                    "UnexpectedAuthority",
                ])
                .inc();
            let e = ConsensusError::UnexpectedAuthority(signed_block.author(), peer);
            info!("Block with wrong authority from {}: {}", peer, e);
            return Err(e);
        }

        // Reject blocks failing parsing and validations.
        let block_verifier = self.block_verifier.clone();
        let serialized = serialized_bytes.clone();
        let (verified_block, reject_txn_votes) =
            spawn_blocking(move || block_verifier.verify_and_vote(signed_block, serialized))
                .await?
                .tap_err(|e| {
                    self.context
                        .metrics
                        .node_metrics
                        .invalid_blocks
                        .with_label_values(&[peer_hostname.as_str(), "handle_send_block", e.name()])
                        .inc();
                    info!("Invalid block from {}: {}", peer, e);
                })?;
        let excluded_ancestors = self
            .parse_excluded_ancestors(peer, &verified_block, serialized_block.excluded_ancestors)
            .tap_err(|e| {
                debug!("Failed to parse excluded ancestors from {peer} {peer_hostname}: {e}");
                self.context
                    .metrics
                    .node_metrics
                    .invalid_blocks
                    .with_label_values(&[peer_hostname.as_str(), "handle_send_block", e.name()])
                    .inc();
            })?;

        let block_ref = verified_block.reference();
        debug!("Received block {} via send block.", block_ref);

        self.context
            .metrics
            .node_metrics
            .verified_blocks
            .with_label_values(&[peer_hostname])
            .inc();

        let now = self.context.clock.timestamp_utc_ms();
        let forward_time_drift =
            Duration::from_millis(verified_block.timestamp_ms().saturating_sub(now));

        self.context
            .metrics
            .node_metrics
            .block_timestamp_drift_ms
            .with_label_values(&[peer_hostname.as_str(), "handle_send_block"])
            .inc_by(forward_time_drift.as_millis() as u64);

        // Observe the block for the commit votes. When local commit is lagging too much,
        // commit sync loop will trigger fetching.
        self.commit_vote_monitor.observe_block(&verified_block);

        // Update own received rounds and peer accepted rounds from this verified block.
        self.round_tracker
            .write()
            .update_from_verified_block(&ExtendedBlock {
                block: verified_block.clone(),
                excluded_ancestors: excluded_ancestors.clone(),
                slim: None,
            });

        // Reject blocks when local commit index is lagging too far from quorum commit index,
        // to avoid the memory overhead from suspended blocks.
        //
        // IMPORTANT: this must be done after observing votes from the block, otherwise
        // observed quorum commit will no longer progress.
        //
        // Since the main issue with too many suspended blocks is memory usage not CPU,
        // it is ok to reject after block verifications instead of before.
        let last_commit_index = self.dag_state.read().last_commit_index();
        let quorum_commit_index = self.commit_vote_monitor.quorum_commit_index();
        if is_commit_lagging(
            self.context.as_ref(),
            last_commit_index,
            quorum_commit_index,
        ) {
            self.context
                .metrics
                .node_metrics
                .rejected_blocks
                .with_label_values(&["commit_lagging"])
                .inc();
            debug!(
                "Block {:?} is rejected because last commit index is lagging quorum commit index too much ({} < {})",
                block_ref, last_commit_index, quorum_commit_index,
            );
            return Err(ConsensusError::BlockRejected {
                block_ref,
                reason: format!(
                    "Last commit index is lagging quorum commit index too much ({} < {})",
                    last_commit_index, quorum_commit_index,
                ),
            });
        }

        // The block is verified and current, so record own votes on the block
        // before sending the block to Core.
        if self.context.protocol_config.transaction_voting_enabled() {
            self.transaction_vote_tracker
                .add_voted_blocks(vec![(verified_block.clone(), reject_txn_votes)]);
        }

        // Send the block to Core to try accepting it into the DAG.
        let missing_ancestors = self
            .core_dispatcher
            .add_blocks(vec![verified_block.clone()])
            .await
            .map_err(|_| ConsensusError::Shutdown)?;

        // Schedule fetching missing ancestors from this peer in the background.
        if !missing_ancestors.is_empty() {
            self.context
                .metrics
                .node_metrics
                .handler_received_block_missing_ancestors
                .with_label_values(&[peer_hostname])
                .inc_by(missing_ancestors.len() as u64);
            let synchronizer = self.synchronizer.clone();
            spawn_monitored_task!(async move {
                // This does not wait for the fetch request to complete.
                // It only waits for synchronizer to queue the request to a peer.
                // When this fails, it usually means the queue is full.
                // The fetch will retry from other peers via live and periodic syncs.
                if let Err(err) = synchronizer
                    .fetch_blocks(missing_ancestors, PeerId::Validator(peer))
                    .await
                {
                    debug!("Failed to fetch missing ancestors via synchronizer: {err}");
                }
            });
        }

        // Schedule fetching missing soft links from this peer in the background.
        let missing_excluded_ancestors = self
            .core_dispatcher
            .check_block_refs(excluded_ancestors)
            .await
            .map_err(|_| ConsensusError::Shutdown)?;
        if !missing_excluded_ancestors.is_empty() {
            self.context
                .metrics
                .node_metrics
                .network_excluded_ancestors_sent_to_fetch
                .with_label_values(&[peer_hostname])
                .inc_by(missing_excluded_ancestors.len() as u64);

            let synchronizer = self.synchronizer.clone();
            spawn_monitored_task!(async move {
                if let Err(err) = synchronizer
                    .fetch_blocks(missing_excluded_ancestors, PeerId::Validator(peer))
                    .await
                {
                    debug!("Failed to fetch excluded ancestors via synchronizer: {err}");
                }
            });
        }

        Ok(())
    }

    async fn handle_subscribe_blocks(
        &self,
        peer: AuthorityIndex,
        last_received: Round,
    ) -> ConsensusResult<BlockStream> {
        fail_point_async!("consensus-rpc-response");

        // Subscribe before snapshotting past blocks below. This can duplicate
        // a block in both the subscription stream and snapshot, which is fine.
        // Otherwise, it is possible to miss a block if it is broadcasted after snapshotting
        // but before subscribing.
        let broadcast_rx = self.rx_block_broadcast.resubscribe();

        // Find past proposed blocks as the initial blocks to send to the peer.
        //
        // If there are cached blocks in the range which the peer requested, send all of them.
        // The size is bounded by the local GC round and DagState cache size.
        //
        // Otherwise if there is no cached block in the range which the peer requested,
        // and this node has proposed blocks before, at least one block should be sent to the peer
        // to help with liveness.
        let past_proposed_blocks = {
            let dag_state = self.dag_state.read();

            // Saturate so an out-of-range round from the peer cannot wrap to 0 and
            // replay the entire block cache.
            let mut proposed_blocks = dag_state
                .get_cached_blocks(self.context.own_index, last_received.saturating_add(1));
            if proposed_blocks.is_empty() {
                let last_proposed_block = dag_state
                    .get_last_proposed_block()
                    .expect("Last proposed block should be returned on validators");
                proposed_blocks = if last_proposed_block.round() > GENESIS_ROUND {
                    vec![last_proposed_block]
                } else {
                    vec![]
                };
            }
            stream::iter(
                proposed_blocks
                    .into_iter()
                    .map(|block| ExtendedSerializedBlock {
                        block: SerializedBlockForm::Full(block.serialized().clone()),
                        excluded_ancestors: vec![],
                    }),
            )
        };

        // Ok to not batch own proposed blocks, which is < 20/s.
        const MAX_BLOCKS_PER_POLL: usize = 1;
        let broadcasted_blocks = BroadcastedBlockStream::new(
            PeerId::Validator(peer),
            broadcast_rx,
            MAX_BLOCKS_PER_POLL,
            self.subscription_counter.clone(),
        );

        if !self
            .context
            .protocol_config
            .slim_block_propagation_enabled()
        {
            return Ok(Box::pin(past_proposed_blocks.chain(
                broadcasted_blocks.flat_map(move |items| {
                    debug_assert!(
                        items.len() <= MAX_BLOCKS_PER_POLL,
                        "Too many blocks received from broadcast"
                    );
                    stream::iter(items.into_iter().map(ExtendedSerializedBlock::from))
                }),
            )));
        }

        // Slim blocks go only to subscribers close enough to rebuild them. Reconstruction
        // is all-or-nothing across a block's ancestors -- one unresolvable slot fails the
        // whole block -- so a receiver trailing the horizon fails most of them. Full blocks
        // put it back on the ordinary path: verify, suspend, fetch missing ancestors.
        //
        // The catch-up replay above always sends full blocks: it serves rounds where the
        // subscriber's state is unpredictable by construction.
        let (lag_to_full, lag_to_slim) = gate_thresholds(self.context.protocol_config.gc_depth());
        // Enter on the strict threshold: a subscriber must be demonstrably close before it
        // is served slim blocks.
        let mut emit_slim = subscriber_lag(&self.dag_state.read(), peer) <= lag_to_slim;
        // The gate keeps tracking the subscriber, so one that falls behind mid-stream stops
        // being served slim blocks rather than keeping them on the decision made at
        // subscribe time.
        let gate_dag_state = Arc::downgrade(&self.dag_state);
        let gate_context = self.context.clone();
        let peer_hostname = self.context.committee.authority(peer).hostname.clone();
        let slim_blocks_sent = self
            .context
            .metrics
            .node_metrics
            .slim_blocks_sent
            .with_label_values(&[peer_hostname.as_str()]);
        let mut gate_checked_at = Instant::now();
        let mut gate_switched_at: Option<Instant> = None;

        // Return a stream of blocks that first yields missed blocks as requested, then new blocks.
        Ok(Box::pin(past_proposed_blocks.chain(
            broadcasted_blocks.flat_map(move |items| {
                debug_assert!(
                    items.len() <= MAX_BLOCKS_PER_POLL,
                    "Too many blocks received from broadcast"
                );
                if gate_checked_at.elapsed() >= GATE_REEVALUATION_INTERVAL {
                    gate_checked_at = Instant::now();
                    match gate_dag_state.upgrade() {
                        Some(dag_state) => {
                            let lag = subscriber_lag(&dag_state.read(), peer);
                            let now_slim =
                                gate_next_mode(emit_slim, lag, lag_to_full, lag_to_slim);
                            if gate_should_switch(
                                emit_slim,
                                now_slim,
                                gate_switched_at
                                    .is_none_or(|at| at.elapsed() >= GATE_MIN_DWELL),
                            ) {
                                gate_switched_at = Some(Instant::now());
                                gate_context
                                    .metrics
                                    .node_metrics
                                    .slim_block_gate_transitions
                                    .with_label_values(&[if now_slim { "slim" } else { "full" }])
                                    .inc();
                                debug!(
                                    "Subscriber {peer} {peer_hostname} is {lag} rounds behind; {} slim emission",
                                    if now_slim { "resuming" } else { "suspending" }
                                );
                                emit_slim = now_slim;
                            }
                        }
                        // Without DagState the subscriber's lag cannot be established, so
                        // stop omitting.
                        None => emit_slim = false,
                    }
                }
                // Collected eagerly so the mapping borrows the captures instead of the
                // stream taking ownership of a clone of each per block.
                let blocks: Vec<ExtendedSerializedBlock> = items
                    .into_iter()
                    .map(|mut extended_block| {
                        // Proposal time already produced the encoding; dropping it here
                        // makes the conversion below pick the full form.
                        if !emit_slim {
                            extended_block.slim = None;
                        }
                        if extended_block.slim.is_some() {
                            slim_blocks_sent.inc();
                        }
                        ExtendedSerializedBlock::from(extended_block)
                    })
                    .collect();
                stream::iter(blocks)
            }),
        )))
    }

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
    async fn handle_fetch_blocks(
        &self,
        _peer: AuthorityIndex,
        block_refs: Vec<BlockRef>,
        fetch_after_rounds: Vec<Round>,
        fetch_missing_ancestors: bool,
    ) -> ConsensusResult<Vec<Bytes>> {
        fail_point_async!("consensus-rpc-response");

        // Delegate to BlockSyncService
        self.block_sync_service
            .fetch_blocks(block_refs, fetch_after_rounds, fetch_missing_ancestors)
            .await
    }

    async fn handle_fetch_commits(
        &self,
        _peer: AuthorityIndex,
        commit_range: CommitRange,
    ) -> ConsensusResult<(Vec<TrustedCommit>, Vec<VerifiedBlock>)> {
        fail_point_async!("consensus-rpc-response");

        // Delegate to BlockSyncService
        self.block_sync_service.fetch_commits(commit_range).await
    }

    async fn handle_fetch_latest_blocks(
        &self,
        peer: AuthorityIndex,
        authorities: Vec<AuthorityIndex>,
    ) -> ConsensusResult<Vec<Bytes>> {
        fail_point_async!("consensus-rpc-response");

        // Delegate to BlockSyncService
        self.block_sync_service
            .fetch_latest_blocks(peer, authorities)
            .await
    }

    async fn handle_get_latest_rounds(
        &self,
        _peer: AuthorityIndex,
    ) -> ConsensusResult<(Vec<Round>, Vec<Round>)> {
        fail_point_async!("consensus-rpc-response");

        let highest_received_rounds = self.round_tracker.read().local_highest_received_rounds();

        let blocks = self
            .dag_state
            .read()
            .get_last_cached_block_per_authority(Round::MAX);
        let highest_accepted_rounds = blocks
            .into_iter()
            .map(|(block, _)| block.round())
            .collect::<Vec<_>>();

        Ok((highest_received_rounds, highest_accepted_rounds))
    }
}

struct Counter {
    count: usize,
    subscriptions_by_peer: BTreeMap<PeerId, usize>,
}

/// Atomically counts the number of active subscriptions to the block broadcast stream.
pub(crate) struct SubscriptionCounter {
    context: Arc<Context>,
    counter: parking_lot::Mutex<Counter>,
}

impl SubscriptionCounter {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        // Set the subscribed peers by default to 0
        for (_, authority) in context.committee.authorities() {
            context
                .metrics
                .node_metrics
                .subscribed_by
                .with_label_values(&[authority.hostname.as_str()])
                .set(0);
        }

        Self {
            counter: parking_lot::Mutex::new(Counter {
                count: 0,
                subscriptions_by_peer: BTreeMap::new(),
            }),
            context,
        }
    }

    fn increment(&self, peer: &PeerId) {
        let mut counter = self.counter.lock();
        counter.count += 1;
        let peer_count = {
            let count = counter
                .subscriptions_by_peer
                .entry(peer.clone())
                .or_default();
            *count += 1;
            *count
        };

        match peer {
            PeerId::Validator(authority) => {
                let peer_hostname = &self.context.committee.authority(*authority).hostname;
                self.context
                    .metrics
                    .node_metrics
                    .subscribed_by
                    .with_label_values(&[peer_hostname])
                    .set(1);
            }
            PeerId::Observer(_) => {
                // Only count the first subscription from each peer.
                if peer_count == 1 {
                    self.context
                        .metrics
                        .node_metrics
                        .subscribed_by
                        .with_label_values(&["observer"])
                        .inc();
                }
            }
        }
    }

    fn decrement(&self, peer: &PeerId) {
        let mut counter = self.counter.lock();
        counter.count = counter.count.saturating_sub(1);
        let peer_count = counter
            .subscriptions_by_peer
            .entry(peer.clone())
            .or_default();
        *peer_count = peer_count.saturating_sub(1);

        if *peer_count == 0 {
            match peer {
                PeerId::Validator(authority) => {
                    let peer_hostname = &self.context.committee.authority(*authority).hostname;
                    self.context
                        .metrics
                        .node_metrics
                        .subscribed_by
                        .with_label_values(&[peer_hostname])
                        .set(0);
                }
                PeerId::Observer(_) => {
                    self.context
                        .metrics
                        .node_metrics
                        .subscribed_by
                        .with_label_values(&["observer"])
                        .dec();
                }
            }
        }
    }
}

/// Each broadcasted block stream wraps a broadcast receiver for blocks.
/// It yields blocks that are broadcasted after the stream is created.
type BroadcastedBlockStream = BroadcastStream<ExtendedBlock>;

/// Adapted from `tokio_stream::wrappers::BroadcastStream`. The main difference is that
/// this tolerates lags with only logging, without yielding errors.
pub(crate) struct BroadcastStream<T> {
    peer: Option<PeerId>,
    // Stores the receiver across poll_next() calls.
    inner: ReusableBoxFuture<
        'static,
        (
            Result<T, broadcast::error::RecvError>,
            broadcast::Receiver<T>,
        ),
    >,
    // Maximum number of items to return per poll.
    max_items_per_poll: usize,
    // Counts total subscriptions / active BroadcastStreams.
    subscription_counter: Option<Arc<SubscriptionCounter>>,
}

impl<T: 'static + Clone + Send> BroadcastStream<T> {
    pub fn new(
        peer: PeerId,
        rx: broadcast::Receiver<T>,
        max_items_per_poll: usize,
        subscription_counter: Arc<SubscriptionCounter>,
    ) -> Self {
        assert!(max_items_per_poll > 0, "max_items_per_poll must be > 0");
        subscription_counter.increment(&peer);
        Self {
            peer: Some(peer),
            inner: ReusableBoxFuture::new(make_recv_future(rx)),
            max_items_per_poll,
            subscription_counter: Some(subscription_counter),
        }
    }

    /// Creates a stream without subscription tracking.
    pub fn new_untracked(rx: broadcast::Receiver<T>, max_items_per_poll: usize) -> Self {
        assert!(max_items_per_poll > 0, "max_items_per_poll must be > 0");
        Self {
            peer: None,
            inner: ReusableBoxFuture::new(make_recv_future(rx)),
            max_items_per_poll,
            subscription_counter: None,
        }
    }
}

impl<T: 'static + Clone + Send> Stream for BroadcastStream<T> {
    type Item = Vec<T>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<Option<Self::Item>> {
        loop {
            let (result, mut rx) = ready!(self.inner.poll(cx));

            match result {
                Ok(item) => {
                    let mut items = Vec::new();
                    items.push(item);

                    // Drain any additional items that are already available, up to the cap.
                    while items.len() < self.max_items_per_poll {
                        match rx.try_recv() {
                            Ok(item) => items.push(item),
                            Err(broadcast::error::TryRecvError::Empty) => break,
                            Err(broadcast::error::TryRecvError::Closed) => break,
                            Err(broadcast::error::TryRecvError::Lagged(n)) => {
                                warn!("BroadcastStream {:?} lagged by {} messages", self.peer, n);
                                break;
                            }
                        }
                    }

                    self.inner.set(make_recv_future(rx));
                    return task::Poll::Ready(Some(items));
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("BroadcastStream {:?} closed", self.peer);
                    return task::Poll::Ready(None);
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("BroadcastStream {:?} lagged by {} messages", self.peer, n);
                    // Re-arm the future and loop to await the next item.
                    self.inner.set(make_recv_future(rx));
                    continue;
                }
            }
        }
    }
}

impl<T> Drop for BroadcastStream<T> {
    fn drop(&mut self) {
        if let (Some(counter), Some(peer)) = (&self.subscription_counter, &self.peer) {
            counter.decrement(peer);
        }
    }
}

async fn make_recv_future<T: Clone>(
    mut rx: broadcast::Receiver<T>,
) -> (
    Result<T, broadcast::error::RecvError>,
    broadcast::Receiver<T>,
) {
    let result = rx.recv().await;
    (result, rx)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        sync::Arc,
        time::Duration,
    };

    use async_trait::async_trait;
    use bytes::Bytes;
    use consensus_config::AuthorityIndex;
    use consensus_types::block::{BlockDigest, BlockRef, Round};
    use parking_lot::{Mutex, RwLock};
    use tokio::{sync::broadcast, time::sleep};

    use futures::StreamExt as _;

    use super::{
        GATE_REEVALUATION_INTERVAL, gate_next_mode, gate_should_switch, gate_thresholds,
        subscriber_lag,
    };
    use crate::{
        authority_service::AuthorityService,
        block::{BlockAPI, ExtendedBlock, SignedBlock, TestBlock, VerifiedBlock},
        block_sync_service::BlockSyncService,
        commit::{CertifiedCommits, CommitRange},
        commit_vote_monitor::CommitVoteMonitor,
        context::Context,
        core_thread::{CoreError, CoreThreadDispatcher},
        dag_state::DagState,
        error::ConsensusResult,
        network::{
            BlockStream, ExtendedSerializedBlock, ObserverNetworkClient, SerializedBlockForm,
            SynchronizerClient, ValidatorNetworkClient, ValidatorNetworkService,
        },
        peers_pool::PeersPool,
        round_tracker::RoundTracker,
        storage::mem_store::MemStore,
        synchronizer::Synchronizer,
        test_dag_builder::DagBuilder,
        transaction_vote_tracker::TransactionVoteTracker,
    };

    /// Every wire payload in these tests is the full form.
    fn expect_full(form: &SerializedBlockForm) -> &[u8] {
        match form {
            SerializedBlockForm::Full(bytes) => bytes,
            SerializedBlockForm::Slim(_) => panic!("expected a full block"),
        }
    }

    /// Three quarters of the horizon to lose slim blocks, half to earn them back.
    #[test]
    fn gate_thresholds_are_fractions_of_the_horizon() {
        assert_eq!(gate_thresholds(60), (45, 30));
        assert_eq!(gate_thresholds(10), (7, 5));
        assert_eq!(gate_thresholds(6), (4, 3));
    }

    /// The two thresholds must leave a gap, so a subscriber sitting on one boundary does
    /// not flip on every re-evaluation, and slim must stop short of the horizon. Depths
    /// below 3 leave no room for either and are not a configured value.
    #[test]
    fn gate_thresholds_leave_a_hysteresis_gap() {
        for gc_depth in [3u32, 4, 6, 10, 60, 100, Round::MAX] {
            let (to_full, to_slim) = gate_thresholds(gc_depth);
            assert!(
                to_slim < to_full,
                "gc_depth {gc_depth}: {to_slim} must be strictly below {to_full}"
            );
            assert!(
                to_full < gc_depth,
                "gc_depth {gc_depth}: slim must stop short of the horizon, got {to_full}"
            );
        }
    }

    /// Inside the gap the mode is whatever it already was.
    #[test]
    fn gate_holds_its_mode_inside_the_gap() {
        let (to_full, to_slim) = gate_thresholds(60);
        for lag in (to_slim + 1)..=to_full {
            assert!(
                gate_next_mode(true, lag, to_full, to_slim),
                "lag {lag} must not lose slim once it has it"
            );
            assert!(
                !gate_next_mode(false, lag, to_full, to_slim),
                "lag {lag} must not earn slim back"
            );
        }
    }

    /// Outside the gap the mode is forced, whichever side it is entered from.
    #[test]
    fn gate_switches_outside_the_gap() {
        let (to_full, to_slim) = gate_thresholds(60);
        for emit_slim in [true, false] {
            assert!(
                gate_next_mode(emit_slim, to_slim, to_full, to_slim),
                "at or inside the strict threshold the subscriber gets slim"
            );
            assert!(
                !gate_next_mode(emit_slim, to_full + 1, to_full, to_slim),
                "past the horizon the subscriber gets full blocks"
            );
        }
    }

    /// Losing slim blocks is never delayed; only earning them back waits out the dwell.
    #[test]
    fn gate_switch_delays_only_the_return_to_slim() {
        assert!(gate_should_switch(true, false, false));
        assert!(gate_should_switch(true, false, true));

        assert!(!gate_should_switch(false, true, false));
        assert!(gate_should_switch(false, true, true));

        assert!(!gate_should_switch(true, true, false));
        assert!(!gate_should_switch(false, false, true));
    }

    /// Lag is the gap between the frontier and the peer's own latest block.
    #[tokio::test]
    async fn subscriber_lag_measures_the_peers_own_blocks() {
        const PEER_ROUND: Round = 5;
        const FRONTIER: Round = 12;

        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let peer = context.committee.to_authority_index(1).unwrap();

        let mut builder = DagBuilder::new(context.clone());
        builder.layers(1..=PEER_ROUND).build();
        for block in builder.all_blocks() {
            dag_state.write().accept_block(block);
        }
        // Advance everyone but `peer`, so the frontier runs ahead of what it proposed.
        for (authority, _) in context.committee.authorities() {
            if authority != peer {
                dag_state.write().accept_block(VerifiedBlock::new_for_test(
                    TestBlock::new(FRONTIER, authority.value() as u32).build(),
                ));
            }
        }

        assert_eq!(
            dag_state.read().get_last_block_for_authority(peer).round(),
            PEER_ROUND
        );
        assert_eq!(dag_state.read().highest_accepted_round(), FRONTIER);
        assert_eq!(
            subscriber_lag(&dag_state.read(), peer),
            FRONTIER - PEER_ROUND
        );

        // A peer that reaches the frontier has no lag.
        dag_state.write().accept_block(VerifiedBlock::new_for_test(
            TestBlock::new(FRONTIER, peer.value() as u32).build(),
        ));
        assert_eq!(subscriber_lag(&dag_state.read(), peer), 0);
    }

    /// A slim-enabled service, the DagState behind it, and the proposal broadcast.
    fn slim_gate_fixture() -> (
        Arc<Context>,
        Arc<AuthorityService<FakeCoreThreadDispatcher>>,
        Arc<RwLock<DagState>>,
        broadcast::Sender<ExtendedBlock>,
    ) {
        let (context, _keys) = Context::new_for_test(4);
        let mut protocol_config = context.protocol_config.clone();
        protocol_config.set_slim_block_propagation_enabled_for_testing(true);
        let context = Arc::new(context.with_protocol_config(protocol_config));

        let block_verifier = Arc::new(crate::block_verifier::NoopBlockVerifier {});
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let core_dispatcher = Arc::new(FakeCoreThreadDispatcher::new());
        let (tx_block_broadcast, rx_block_broadcast) = broadcast::channel(100);
        let fake_client = Arc::new(FakeNetworkClient::default());
        let network_client = Arc::new(SynchronizerClient::new(
            context.clone(),
            Some(fake_client.clone()),
            Some(fake_client.clone()),
        ));
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());
        let round_tracker = Arc::new(RwLock::new(RoundTracker::new(context.clone(), vec![])));
        let peers_pool = Arc::new(PeersPool::new(context.clone()));
        let synchronizer = Synchronizer::start(
            network_client,
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            block_verifier.clone(),
            transaction_vote_tracker.clone(),
            round_tracker.clone(),
            dag_state.clone(),
            peers_pool.clone(),
            false,
        );
        let block_sync_service = Arc::new(BlockSyncService::new(
            context.clone(),
            dag_state.clone(),
            store.clone(),
        ));
        let authority_service = Arc::new(AuthorityService::new(
            context.clone(),
            block_verifier,
            commit_vote_monitor,
            round_tracker,
            synchronizer,
            core_dispatcher,
            rx_block_broadcast,
            transaction_vote_tracker,
            dag_state.clone(),
            block_sync_service,
        ));
        (context, authority_service, dag_state, tx_block_broadcast)
    }

    fn accept_block_at(dag_state: &Arc<RwLock<DagState>>, round: Round, author: u32) {
        dag_state.write().accept_block(VerifiedBlock::new_for_test(
            TestBlock::new(round, author).build(),
        ));
    }

    fn broadcast_proposal(tx: &broadcast::Sender<ExtendedBlock>, round: Round) {
        tx.send(ExtendedBlock {
            block: VerifiedBlock::new_for_test(TestBlock::new(round, 0).build()),
            excluded_ancestors: vec![],
            slim: Some(Bytes::from_static(b"slim")),
        })
        .unwrap();
    }

    fn is_slim(block: &SerializedBlockForm) -> bool {
        matches!(block, SerializedBlockForm::Slim(_))
    }

    /// A subscriber that falls past the demote threshold mid-stream stops receiving slim
    /// blocks. Drives the gate closure itself, which the threshold tests do not reach.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn gate_stops_slim_emission_when_the_subscriber_falls_behind() {
        let (context, authority_service, dag_state, tx) = slim_gate_fixture();
        let (lag_to_full, _) = gate_thresholds(context.protocol_config.gc_depth());
        let peer = context.committee.to_authority_index(1).unwrap();

        // The subscriber is at the frontier, so the gate opens on subscribe.
        accept_block_at(&dag_state, 1, peer.value() as u32);
        let mut stream = authority_service
            .handle_subscribe_blocks(peer, 0)
            .await
            .unwrap();

        broadcast_proposal(&tx, 2);
        assert!(is_slim(&stream.next().await.unwrap().block));

        // Advance the frontier past the demote threshold without the subscriber following.
        accept_block_at(&dag_state, lag_to_full + 2, 2);
        tokio::time::advance(GATE_REEVALUATION_INTERVAL).await;

        broadcast_proposal(&tx, 3);
        assert!(
            !is_slim(&stream.next().await.unwrap().block),
            "a subscriber past the demote threshold receives the full form"
        );
    }

    /// A subscriber admitted as full is promoted on the first re-evaluation once it is
    /// close enough; the dwell applies only after a switch has happened.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn gate_promotes_a_new_subscriber_without_waiting_out_the_dwell() {
        let (context, authority_service, dag_state, tx) = slim_gate_fixture();
        let (lag_to_full, lag_to_slim) = gate_thresholds(context.protocol_config.gc_depth());
        let peer = context.committee.to_authority_index(1).unwrap();

        // The subscriber starts past the demote threshold, so the gate opens closed.
        accept_block_at(&dag_state, 1, peer.value() as u32);
        accept_block_at(&dag_state, lag_to_full + 2, 2);
        let mut stream = authority_service
            .handle_subscribe_blocks(peer, 0)
            .await
            .unwrap();

        broadcast_proposal(&tx, 2);
        assert!(!is_slim(&stream.next().await.unwrap().block));

        // Bring it inside the promote threshold and re-evaluate once.
        accept_block_at(
            &dag_state,
            lag_to_full + 2 - lag_to_slim,
            peer.value() as u32,
        );
        tokio::time::advance(GATE_REEVALUATION_INTERVAL).await;

        broadcast_proposal(&tx, 3);
        assert!(
            is_slim(&stream.next().await.unwrap().block),
            "the first promotion must not wait for the dwell"
        );
    }

    struct FakeCoreThreadDispatcher {
        blocks: Mutex<Vec<VerifiedBlock>>,
    }

    impl FakeCoreThreadDispatcher {
        fn new() -> Self {
            Self {
                blocks: Mutex::new(vec![]),
            }
        }

        fn get_blocks(&self) -> Vec<VerifiedBlock> {
            self.blocks.lock().clone()
        }
    }

    #[async_trait]
    impl CoreThreadDispatcher for FakeCoreThreadDispatcher {
        async fn add_blocks(
            &self,
            blocks: Vec<VerifiedBlock>,
        ) -> Result<BTreeSet<BlockRef>, CoreError> {
            let block_refs = blocks.iter().map(|b| b.reference()).collect();
            self.blocks.lock().extend(blocks);
            Ok(block_refs)
        }

        async fn check_block_refs(
            &self,
            _block_refs: Vec<BlockRef>,
        ) -> Result<BTreeSet<BlockRef>, CoreError> {
            Ok(BTreeSet::new())
        }

        async fn add_certified_commits(
            &self,
            _commits: CertifiedCommits,
        ) -> Result<BTreeSet<BlockRef>, CoreError> {
            todo!()
        }

        async fn new_block(&self, _round: Round, _force: bool) -> Result<(), CoreError> {
            Ok(())
        }

        async fn get_missing_blocks(&self) -> Result<BTreeSet<BlockRef>, CoreError> {
            Ok(Default::default())
        }

        fn set_propagation_delay(&self, _propagation_delay: Round) -> Result<(), CoreError> {
            todo!()
        }

        fn set_last_known_proposed_round(&self, _round: Round) -> Result<(), CoreError> {
            todo!()
        }
    }

    #[derive(Default)]
    struct FakeNetworkClient {}

    #[async_trait]
    impl ValidatorNetworkClient for FakeNetworkClient {
        async fn send_block(
            &self,
            _peer: AuthorityIndex,
            _block: &VerifiedBlock,
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
            _fetch_after_rounds: Vec<Round>,
            _fetch_missing_ancestors: bool,
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

    #[async_trait]
    impl ObserverNetworkClient for FakeNetworkClient {
        async fn stream_blocks(
            &self,
            _peer: crate::network::PeerId,
            _highest_round_per_authority: Vec<Round>,
            _timeout: Duration,
        ) -> ConsensusResult<crate::network::ObserverBlockStream> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_blocks(
            &self,
            _peer: crate::network::PeerId,
            _block_refs: Vec<BlockRef>,
            _fetch_after_rounds: Vec<Round>,
            _fetch_missing_ancestors: bool,
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
    async fn test_handle_send_block() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let block_verifier = Arc::new(crate::block_verifier::NoopBlockVerifier {});
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let core_dispatcher = Arc::new(FakeCoreThreadDispatcher::new());
        let (_tx_block_broadcast, rx_block_broadcast) = broadcast::channel(100);
        let fake_client = Arc::new(FakeNetworkClient::default());
        let network_client = Arc::new(SynchronizerClient::new(
            context.clone(),
            Some(fake_client.clone()),
            Some(fake_client.clone()),
        ));
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());
        let round_tracker = Arc::new(RwLock::new(RoundTracker::new(context.clone(), vec![])));
        let peers_pool = Arc::new(PeersPool::new(context.clone()));
        let synchronizer = Synchronizer::start(
            network_client,
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            block_verifier.clone(),
            transaction_vote_tracker.clone(),
            round_tracker.clone(),
            dag_state.clone(),
            peers_pool.clone(),
            false,
        );
        let block_sync_service = Arc::new(BlockSyncService::new(
            context.clone(),
            dag_state.clone(),
            store.clone(),
        ));
        let authority_service = Arc::new(AuthorityService::new(
            context.clone(),
            block_verifier,
            commit_vote_monitor,
            round_tracker,
            synchronizer,
            core_dispatcher.clone(),
            rx_block_broadcast,
            transaction_vote_tracker,
            dag_state,
            block_sync_service,
        ));

        // Test delaying blocks with time drift.
        let now = context.clock.timestamp_utc_ms();
        let max_drift = context.parameters.max_forward_time_drift;
        let input_block = VerifiedBlock::new_for_test(
            TestBlock::new(9, 0)
                .set_timestamp_ms(now + max_drift.as_millis() as u64)
                .build(),
        );

        let service = authority_service.clone();
        let serialized = ExtendedSerializedBlock {
            block: SerializedBlockForm::Full(input_block.serialized().clone()),
            excluded_ancestors: vec![],
        };

        tokio::spawn({
            let service = service.clone();
            let context = context.clone();
            async move {
                service
                    .handle_send_block(context.committee.to_authority_index(0).unwrap(), serialized)
                    .await
                    .unwrap();
            }
        });

        sleep(max_drift / 2).await;

        let blocks = core_dispatcher.get_blocks();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0], input_block);

        // Test invalid block.
        let invalid_block =
            VerifiedBlock::new_for_test(TestBlock::new(10, 1000).set_timestamp_ms(10).build());
        let extended_block = ExtendedSerializedBlock {
            block: SerializedBlockForm::Full(invalid_block.serialized().clone()),
            excluded_ancestors: vec![],
        };
        service
            .handle_send_block(
                context.committee.to_authority_index(0).unwrap(),
                extended_block,
            )
            .await
            .unwrap_err();

        // A slim payload that reaches the service is a bug upstream (the subscriber
        // drops them until the codec lands); it must be rejected, not parsed.
        let slim_block = ExtendedSerializedBlock {
            block: SerializedBlockForm::Slim(Bytes::from_static(b"slim")),
            excluded_ancestors: vec![],
        };
        let result = service
            .handle_send_block(context.committee.to_authority_index(0).unwrap(), slim_block)
            .await;
        assert!(matches!(
            result,
            Err(crate::error::ConsensusError::UnexpectedBlockForm)
        ));

        // Test invalid excluded ancestors.
        let invalid_excluded_ancestors = vec![
            bcs::to_bytes(&BlockRef::new(
                10,
                AuthorityIndex::new_for_test(1000),
                BlockDigest::MIN,
            ))
            .unwrap(),
            vec![3u8; 40],
            bcs::to_bytes(&invalid_block.reference()).unwrap(),
        ];
        let extended_block = ExtendedSerializedBlock {
            block: SerializedBlockForm::Full(input_block.serialized().clone()),
            excluded_ancestors: invalid_excluded_ancestors,
        };
        service
            .handle_send_block(
                context.committee.to_authority_index(0).unwrap(),
                extended_block,
            )
            .await
            .unwrap_err();
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_handle_fetch_blocks() {
        // GIVEN
        // Use NUM_AUTHORITIES and NUM_ROUNDS higher than max_blocks_per_sync to test limits.
        const NUM_AUTHORITIES: usize = 40;
        const NUM_ROUNDS: usize = 40;
        let (mut context, _keys) = Context::new_for_test(NUM_AUTHORITIES);
        context.parameters.max_blocks_per_fetch = 50;
        let context = Arc::new(context);
        let block_verifier = Arc::new(crate::block_verifier::NoopBlockVerifier {});
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let core_dispatcher = Arc::new(FakeCoreThreadDispatcher::new());
        let (_tx_block_broadcast, rx_block_broadcast) = broadcast::channel(100);
        let fake_client = Arc::new(FakeNetworkClient::default());
        let network_client = Arc::new(SynchronizerClient::new(
            context.clone(),
            Some(fake_client.clone()),
            Some(fake_client.clone()),
        ));
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());
        let round_tracker = Arc::new(RwLock::new(RoundTracker::new(context.clone(), vec![])));
        let peers_pool = Arc::new(PeersPool::new(context.clone()));
        let synchronizer = Synchronizer::start(
            network_client,
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            block_verifier.clone(),
            transaction_vote_tracker.clone(),
            round_tracker.clone(),
            dag_state.clone(),
            peers_pool.clone(),
            false,
        );
        let block_sync_service = Arc::new(BlockSyncService::new(
            context.clone(),
            dag_state.clone(),
            store.clone(),
        ));
        let authority_service = Arc::new(AuthorityService::new(
            context.clone(),
            block_verifier,
            commit_vote_monitor,
            round_tracker,
            synchronizer,
            core_dispatcher.clone(),
            rx_block_broadcast,
            transaction_vote_tracker,
            dag_state.clone(),
            block_sync_service,
        ));

        // GIVEN: 40 rounds of blocks in the dag state.
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder
            .layers(1..=(NUM_ROUNDS as u32))
            .build()
            .persist_layers(dag_state.clone());
        dag_state.write().flush();
        let all_blocks = dag_builder.all_blocks();

        // WHEN: Request 2 blocks from round 40, fetch missing ancestors enabled.
        let missing_block_refs: Vec<BlockRef> = all_blocks
            .iter()
            .rev()
            .take(2)
            .map(|b| b.reference())
            .collect();
        let highest_accepted_rounds: Vec<Round> = vec![1; NUM_AUTHORITIES];
        let results = authority_service
            .handle_fetch_blocks(
                AuthorityIndex::new_for_test(0),
                missing_block_refs.clone(),
                highest_accepted_rounds,
                true,
            )
            .await
            .unwrap();

        // THEN: the expected number of unique blocks are returned.
        let blocks: BTreeMap<BlockRef, VerifiedBlock> = results
            .iter()
            .map(|b| {
                let signed = bcs::from_bytes(b).unwrap();
                let block = VerifiedBlock::new_verified(signed, b.clone());
                (block.reference(), block)
            })
            .collect();
        assert_eq!(blocks.len(), context.parameters.max_blocks_per_sync);
        // All missing blocks are returned.
        for b in &missing_block_refs {
            assert!(blocks.contains_key(b));
        }
        let num_missing_ancestors = blocks
            .keys()
            .filter(|b| b.round == NUM_ROUNDS as Round - 1)
            .count();
        assert_eq!(
            num_missing_ancestors,
            context.parameters.max_blocks_per_sync - missing_block_refs.len()
        );

        // WHEN: Request 2 blocks from round 37, fetch missing ancestors disabled.
        let missing_round = NUM_ROUNDS as Round - 3;
        let missing_block_refs: Vec<BlockRef> = all_blocks
            .iter()
            .filter(|b| b.reference().round == missing_round)
            .map(|b| b.reference())
            .take(2)
            .collect();
        let mut highest_accepted_rounds: Vec<Round> = vec![1; NUM_AUTHORITIES];
        // Try to fill up the blocks from the 1st authority in missing_block_refs.
        highest_accepted_rounds[missing_block_refs[0].author] = missing_round - 5;
        let results = authority_service
            .handle_fetch_blocks(
                AuthorityIndex::new_for_test(0),
                missing_block_refs.clone(),
                highest_accepted_rounds,
                false,
            )
            .await
            .unwrap();

        // THEN: the expected number of unique blocks are returned.
        let blocks: BTreeMap<BlockRef, VerifiedBlock> = results
            .iter()
            .map(|b| {
                let signed = bcs::from_bytes(b).unwrap();
                let block = VerifiedBlock::new_verified(signed, b.clone());
                (block.reference(), block)
            })
            .collect();
        assert_eq!(blocks.len(), context.parameters.max_blocks_per_sync);
        // All missing blocks are returned.
        for b in &missing_block_refs {
            assert!(blocks.contains_key(b));
        }
        // Ancestor blocks are from the expected rounds and authorities.
        let expected_authors = [missing_block_refs[0].author, missing_block_refs[1].author];
        for b in blocks.keys() {
            assert!(b.round <= missing_round);
            assert!(expected_authors.contains(&b.author));
        }

        // WHEN: Request with empty block_refs, fetch missing ancestors disabled.
        let mut highest_accepted_rounds: Vec<Round> = vec![1; NUM_AUTHORITIES];
        // Set a few authorities to higher accepted rounds.
        highest_accepted_rounds[0] = (NUM_ROUNDS as Round) - 5;
        highest_accepted_rounds[1] = (NUM_ROUNDS as Round) - 3;
        let results = authority_service
            .handle_fetch_blocks(
                AuthorityIndex::new_for_test(0),
                vec![],
                highest_accepted_rounds.clone(),
                false,
            )
            .await
            .unwrap();

        // THEN: the expected number of unique blocks are returned.
        let blocks: BTreeMap<BlockRef, VerifiedBlock> = results
            .iter()
            .map(|b| {
                let signed = bcs::from_bytes(b).unwrap();
                let block = VerifiedBlock::new_verified(signed, b.clone());
                (block.reference(), block)
            })
            .collect();
        assert_eq!(blocks.len(), context.parameters.max_blocks_per_fetch);
        // Blocks should be from all authorities, within the expected round range.
        for block_ref in blocks.keys() {
            let accepted = highest_accepted_rounds[block_ref.author];
            assert!(block_ref.round > accepted);
        }
        // Blocks should be fetched in ascending round order across authorities,
        // so blocks should have low rounds near the accepted rounds.
        let max_round_in_result = blocks.keys().map(|b| b.round).max().unwrap();
        // With 40 authorities mostly at accepted round 1 and max_blocks_per_fetch=50,
        // the min-heap fills ~1-2 rounds per authority.
        assert!(
            max_round_in_result <= 4,
            "Expected low rounds from fair round-order fetching, got max round {}",
            max_round_in_result
        );

        // WHEN: Request 5 block from round 40, not getting ancestors.
        let missing_block_refs: Vec<BlockRef> = all_blocks
            .iter()
            .filter(|b| b.reference().round == NUM_ROUNDS as Round - 10)
            .map(|b| b.reference())
            .take(5)
            .collect();
        let results = authority_service
            .handle_fetch_blocks(
                AuthorityIndex::new_for_test(0),
                missing_block_refs.clone(),
                vec![],
                false,
            )
            .await
            .unwrap();

        // THEN: the expected number of unique blocks are returned.
        let blocks: BTreeMap<BlockRef, VerifiedBlock> = results
            .iter()
            .map(|b| {
                let signed = bcs::from_bytes(b).unwrap();
                let block = VerifiedBlock::new_verified(signed, b.clone());
                (block.reference(), block)
            })
            .collect();
        assert_eq!(blocks.len(), 5);
        for b in &missing_block_refs {
            assert!(blocks.contains_key(b));
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_handle_fetch_latest_blocks() {
        // GIVEN
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let block_verifier = Arc::new(crate::block_verifier::NoopBlockVerifier {});
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let core_dispatcher = Arc::new(FakeCoreThreadDispatcher::new());
        let (_tx_block_broadcast, rx_block_broadcast) = broadcast::channel(100);
        let fake_client = Arc::new(FakeNetworkClient::default());
        let network_client = Arc::new(SynchronizerClient::new(
            context.clone(),
            Some(fake_client.clone()),
            Some(fake_client.clone()),
        ));
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());
        let round_tracker = Arc::new(RwLock::new(RoundTracker::new(context.clone(), vec![])));
        let peers_pool = Arc::new(PeersPool::new(context.clone()));
        let synchronizer = Synchronizer::start(
            network_client,
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            block_verifier.clone(),
            transaction_vote_tracker.clone(),
            round_tracker.clone(),
            dag_state.clone(),
            peers_pool.clone(),
            true,
        );
        let block_sync_service = Arc::new(BlockSyncService::new(
            context.clone(),
            dag_state.clone(),
            store.clone(),
        ));
        let authority_service = Arc::new(AuthorityService::new(
            context.clone(),
            block_verifier,
            commit_vote_monitor,
            round_tracker,
            synchronizer,
            core_dispatcher.clone(),
            rx_block_broadcast,
            transaction_vote_tracker,
            dag_state.clone(),
            block_sync_service,
        ));

        // Create some blocks for a few authorities. Create some equivocations as well and store in dag state.
        let mut dag_builder = DagBuilder::new(context.clone());
        dag_builder
            .layers(1..=10)
            .authorities(vec![AuthorityIndex::new_for_test(2)])
            .equivocate(1)
            .build()
            .persist_layers(dag_state);

        // WHEN
        let authorities_to_request = vec![
            AuthorityIndex::new_for_test(1),
            AuthorityIndex::new_for_test(2),
        ];
        let results = authority_service
            .handle_fetch_latest_blocks(AuthorityIndex::new_for_test(1), authorities_to_request)
            .await;

        // THEN
        let serialised_blocks = results.unwrap();
        for serialised_block in serialised_blocks {
            let signed_block: SignedBlock =
                bcs::from_bytes(&serialised_block).expect("Error while deserialising block");
            let verified_block = VerifiedBlock::new_verified(signed_block, serialised_block);

            assert_eq!(verified_block.round(), 10);
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_handle_subscribe_blocks() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let block_verifier = Arc::new(crate::block_verifier::NoopBlockVerifier {});
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let core_dispatcher = Arc::new(FakeCoreThreadDispatcher::new());
        let (_tx_block_broadcast, rx_block_broadcast) = broadcast::channel(100);
        let fake_client = Arc::new(FakeNetworkClient::default());
        let network_client = Arc::new(SynchronizerClient::new(
            context.clone(),
            Some(fake_client.clone()),
            Some(fake_client.clone()),
        ));
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());
        let round_tracker = Arc::new(RwLock::new(RoundTracker::new(context.clone(), vec![])));
        let peers_pool = Arc::new(PeersPool::new(context.clone()));
        let synchronizer = Synchronizer::start(
            network_client,
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            block_verifier.clone(),
            transaction_vote_tracker.clone(),
            round_tracker.clone(),
            dag_state.clone(),
            peers_pool.clone(),
            false,
        );
        let block_sync_service = Arc::new(BlockSyncService::new(
            context.clone(),
            dag_state.clone(),
            store.clone(),
        ));

        // Create 3 proposed blocks at rounds 5, 10, 15 for own authority (index 0)
        dag_state
            .write()
            .accept_block(VerifiedBlock::new_for_test(TestBlock::new(5, 0).build()));
        dag_state
            .write()
            .accept_block(VerifiedBlock::new_for_test(TestBlock::new(10, 0).build()));
        dag_state
            .write()
            .accept_block(VerifiedBlock::new_for_test(TestBlock::new(15, 0).build()));

        let authority_service = Arc::new(AuthorityService::new(
            context.clone(),
            block_verifier,
            commit_vote_monitor,
            round_tracker,
            synchronizer,
            core_dispatcher.clone(),
            rx_block_broadcast,
            transaction_vote_tracker,
            dag_state.clone(),
            block_sync_service,
        ));

        let peer = context.committee.to_authority_index(1).unwrap();

        // Case A: Subscribe with last_received = 100 (after all proposed blocks)
        // Should return last proposed block (round 15) as fallback
        {
            let mut stream = authority_service
                .handle_subscribe_blocks(peer, 100)
                .await
                .unwrap();
            let block: SignedBlock =
                bcs::from_bytes(expect_full(&stream.next().await.unwrap().block)).unwrap();
            assert_eq!(
                block.round(),
                15,
                "Should return last proposed block as fallback"
            );
            assert_eq!(block.author().value(), 0);
        }

        // Case B: Subscribe with last_received = 7 (includes rounds 10, 15)
        // Should return cached blocks from round 8+
        {
            let mut stream = authority_service
                .handle_subscribe_blocks(peer, 7)
                .await
                .unwrap();

            let block1: SignedBlock =
                bcs::from_bytes(expect_full(&stream.next().await.unwrap().block)).unwrap();
            assert_eq!(block1.round(), 10, "Should return block at round 10");

            let block2: SignedBlock =
                bcs::from_bytes(expect_full(&stream.next().await.unwrap().block)).unwrap();
            assert_eq!(block2.round(), 15, "Should return block at round 15");
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_handle_subscribe_blocks_not_proposed() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let block_verifier = Arc::new(crate::block_verifier::NoopBlockVerifier {});
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let core_dispatcher = Arc::new(FakeCoreThreadDispatcher::new());
        let (_tx_block_broadcast, rx_block_broadcast) = broadcast::channel(100);
        let fake_client = Arc::new(FakeNetworkClient::default());
        let network_client = Arc::new(SynchronizerClient::new(
            context.clone(),
            Some(fake_client.clone()),
            Some(fake_client.clone()),
        ));
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());
        let round_tracker = Arc::new(RwLock::new(RoundTracker::new(context.clone(), vec![])));
        let peers_pool = Arc::new(PeersPool::new(context.clone()));
        let synchronizer = Synchronizer::start(
            network_client,
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            block_verifier.clone(),
            transaction_vote_tracker.clone(),
            round_tracker.clone(),
            dag_state.clone(),
            peers_pool.clone(),
            false,
        );
        let block_sync_service = Arc::new(BlockSyncService::new(
            context.clone(),
            dag_state.clone(),
            store.clone(),
        ));

        // No blocks added to DagState - only genesis exists

        let authority_service = Arc::new(AuthorityService::new(
            context.clone(),
            block_verifier,
            commit_vote_monitor,
            round_tracker,
            synchronizer,
            core_dispatcher.clone(),
            rx_block_broadcast,
            transaction_vote_tracker,
            dag_state.clone(),
            block_sync_service,
        ));

        let peer = context.committee.to_authority_index(1).unwrap();

        // Subscribe - no blocks have been proposed yet (only genesis exists)
        let mut stream = authority_service
            .handle_subscribe_blocks(peer, 0)
            .await
            .unwrap();

        // Should NOT receive any block (genesis must not be returned)
        use futures::poll;
        use std::task::Poll;
        let poll_result = poll!(stream.next());
        assert!(
            matches!(poll_result, Poll::Pending),
            "Should not receive genesis block on subscription stream"
        );
    }
}
