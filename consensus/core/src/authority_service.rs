// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{pin::Pin, sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::AuthorityIndex;
use futures::{ready, stream, task, Stream, StreamExt};
use parking_lot::RwLock;
use sui_macros::fail_point_async;
use tokio::{sync::broadcast, time::sleep};
use tokio_util::sync::ReusableBoxFuture;
use tracing::{debug, info, warn};

use crate::{
    block::{BlockAPI as _, BlockRef, ExtendedBlock, SignedBlock, VerifiedBlock, GENESIS_ROUND},
    block_verifier::BlockVerifier,
    commit::{CommitAPI as _, CommitRange, TrustedCommit},
    commit_vote_monitor::CommitVoteMonitor,
    context::Context,
    core_thread::CoreThreadDispatcher,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    network::{BlockStream, ExtendedSerializedBlock, NetworkService},
    stake_aggregator::{QuorumThreshold, StakeAggregator},
    storage::Store,
    synchronizer::SynchronizerHandle,
    CommitIndex, Round,
};

pub(crate) const COMMIT_LAG_MULTIPLIER: u32 = 5;

/// Authority's network service implementation, agnostic to the actual networking stack used.
pub(crate) struct AuthorityService<C: CoreThreadDispatcher> {
    context: Arc<Context>,
    commit_vote_monitor: Arc<CommitVoteMonitor>,
    block_verifier: Arc<dyn BlockVerifier>,
    synchronizer: Arc<SynchronizerHandle>,
    core_dispatcher: Arc<C>,
    rx_block_broadcaster: broadcast::Receiver<ExtendedBlock>,
    subscription_counter: Arc<SubscriptionCounter>,
    dag_state: Arc<RwLock<DagState>>,
    store: Arc<dyn Store>,
}

impl<C: CoreThreadDispatcher> AuthorityService<C> {
    pub(crate) fn new(
        context: Arc<Context>,
        block_verifier: Arc<dyn BlockVerifier>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        synchronizer: Arc<SynchronizerHandle>,
        core_dispatcher: Arc<C>,
        rx_block_broadcaster: broadcast::Receiver<ExtendedBlock>,
        dag_state: Arc<RwLock<DagState>>,
        store: Arc<dyn Store>,
    ) -> Self {
        let subscription_counter = Arc::new(SubscriptionCounter::new(
            context.clone(),
            core_dispatcher.clone(),
        ));
        Self {
            context,
            block_verifier,
            commit_vote_monitor,
            synchronizer,
            core_dispatcher,
            rx_block_broadcaster,
            subscription_counter,
            dag_state,
            store,
        }
    }
}

#[async_trait]
impl<C: CoreThreadDispatcher> NetworkService for AuthorityService<C> {
    async fn handle_send_block(
        &self,
        peer: AuthorityIndex,
        serialized_block: ExtendedSerializedBlock,
    ) -> ConsensusResult<()> {
        fail_point_async!("consensus-rpc-response");

        let peer_hostname = &self.context.committee.authority(peer).hostname;

        // TODO: dedup block verifications, here and with fetched blocks.
        let signed_block: SignedBlock =
            bcs::from_bytes(&serialized_block.block).map_err(ConsensusError::MalformedBlock)?;

        // Reject blocks not produced by the peer.
        if peer != signed_block.author() {
            self.context
                .metrics
                .node_metrics
                .invalid_blocks
                .with_label_values(&[peer_hostname, "handle_send_block", "UnexpectedAuthority"])
                .inc();
            let e = ConsensusError::UnexpectedAuthority(signed_block.author(), peer);
            info!("Block with wrong authority from {}: {}", peer, e);
            return Err(e);
        }
        let peer_hostname = &self.context.committee.authority(peer).hostname;

        // Reject blocks failing validations.
        let verification_result = self.block_verifier.verify_and_vote(&signed_block);
        if let Err(e) = verification_result {
            self.context
                .metrics
                .node_metrics
                .invalid_blocks
                .with_label_values(&[peer_hostname, "handle_send_block", e.name()])
                .inc();
            info!("Invalid block from {}: {}", peer, e);
            return Err(e);
        }
        let verified_block = VerifiedBlock::new_verified(signed_block, serialized_block.block);
        let block_ref = verified_block.reference();
        debug!("Received block {} via send block.", block_ref);

        // Reject block with timestamp too far in the future.
        let now = self.context.clock.timestamp_utc_ms();
        let forward_time_drift =
            Duration::from_millis(verified_block.timestamp_ms().saturating_sub(now));
        if forward_time_drift > self.context.parameters.max_forward_time_drift {
            self.context
                .metrics
                .node_metrics
                .rejected_future_blocks
                .with_label_values(&[peer_hostname])
                .inc();
            debug!(
                "Block {:?} timestamp ({} > {}) is too far in the future, rejected.",
                block_ref,
                verified_block.timestamp_ms(),
                now,
            );
            return Err(ConsensusError::BlockRejected {
                block_ref,
                reason: format!(
                    "Block timestamp is too far in the future: {} > {}",
                    verified_block.timestamp_ms(),
                    now
                ),
            });
        }

        // Wait until the block's timestamp is current.
        if forward_time_drift > Duration::ZERO {
            self.context
                .metrics
                .node_metrics
                .block_timestamp_drift_wait_ms
                .with_label_values(&[peer_hostname, "handle_send_block"])
                .inc_by(forward_time_drift.as_millis() as u64);
            debug!(
                "Block {:?} timestamp ({} > {}) is in the future, waiting for {}ms",
                block_ref,
                verified_block.timestamp_ms(),
                now,
                forward_time_drift.as_millis(),
            );
            sleep(forward_time_drift).await;
        }

        // Observe the block for the commit votes. When local commit is lagging too much,
        // commit sync loop will trigger fetching.
        self.commit_vote_monitor.observe_block(&verified_block);

        // Reject blocks when local commit index is lagging too far from quorum commit index.
        //
        // IMPORTANT: this must be done after observing votes from the block, otherwise
        // observed quorum commit will no longer progress.
        //
        // Since the main issue with too many suspended blocks is memory usage not CPU,
        // it is ok to reject after block verifications instead of before.
        let last_commit_index = self.dag_state.read().last_commit_index();
        let quorum_commit_index = self.commit_vote_monitor.quorum_commit_index();
        // The threshold to ignore block should be larger than commit_sync_batch_size,
        // to avoid excessive block rejections and synchronizations.
        if last_commit_index
            + self.context.parameters.commit_sync_batch_size * COMMIT_LAG_MULTIPLIER
            < quorum_commit_index
        {
            self.context
                .metrics
                .node_metrics
                .rejected_blocks
                .with_label_values(&["commit_lagging"])
                .inc();
            debug!(
                "Block {:?} is rejected because last commit index is lagging quorum commit index too much ({} < {})",
                block_ref,
                last_commit_index,
                quorum_commit_index,
            );
            return Err(ConsensusError::BlockRejected {
                block_ref,
                reason: format!(
                    "Last commit index is lagging quorum commit index too much ({} < {})",
                    last_commit_index, quorum_commit_index,
                ),
            });
        }

        self.context
            .metrics
            .node_metrics
            .verified_blocks
            .with_label_values(&[peer_hostname])
            .inc();

        let missing_ancestors = self
            .core_dispatcher
            .add_blocks(vec![verified_block])
            .await
            .map_err(|_| ConsensusError::Shutdown)?;
        if !missing_ancestors.is_empty() {
            // schedule the fetching of them from this peer
            if let Err(err) = self
                .synchronizer
                .fetch_blocks(missing_ancestors, peer)
                .await
            {
                warn!("Errored while trying to fetch missing ancestors via synchronizer: {err}");
            }
        }

        // ------------ After processing the block, process the excluded ancestors ------------

        let mut excluded_ancestors = serialized_block
            .excluded_ancestors
            .into_iter()
            .map(|serialized| bcs::from_bytes::<BlockRef>(&serialized))
            .collect::<Result<Vec<BlockRef>, bcs::Error>>()
            .map_err(ConsensusError::MalformedBlock)?;

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

        self.context
            .metrics
            .node_metrics
            .network_received_excluded_ancestors_from_authority
            .with_label_values(&[peer_hostname])
            .inc_by(excluded_ancestors.len() as u64);

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
            tokio::spawn(async move {
                // schedule the fetching of them from this peer in the background
                if let Err(err) = synchronizer
                    .fetch_blocks(missing_excluded_ancestors, peer)
                    .await
                {
                    warn!(
                            "Errored while trying to fetch missing excluded ancestors via synchronizer: {err}"
                        );
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

        let dag_state = self.dag_state.read();
        // Find recent own blocks that have not been received by the peer.
        // If last_received is a valid and more blocks have been proposed since then, this call is
        // guaranteed to return at least some recent blocks, which will help with liveness.
        let missed_blocks = stream::iter(
            dag_state
                .get_cached_blocks(self.context.own_index, last_received + 1)
                .into_iter()
                .map(|block| ExtendedSerializedBlock {
                    block: block.serialized().clone(),
                    excluded_ancestors: vec![],
                }),
        );

        let broadcasted_blocks = BroadcastedBlockStream::new(
            peer,
            self.rx_block_broadcaster.resubscribe(),
            self.subscription_counter.clone(),
        );

        // Return a stream of blocks that first yields missed blocks as requested, then new blocks.
        Ok(Box::pin(missed_blocks.chain(
            broadcasted_blocks.map(ExtendedSerializedBlock::from),
        )))
    }

    async fn handle_fetch_blocks(
        &self,
        peer: AuthorityIndex,
        block_refs: Vec<BlockRef>,
        highest_accepted_rounds: Vec<Round>,
    ) -> ConsensusResult<Vec<Bytes>> {
        fail_point_async!("consensus-rpc-response");

        const MAX_ADDITIONAL_BLOCKS: usize = 10;
        if block_refs.len() > self.context.parameters.max_blocks_per_fetch {
            return Err(ConsensusError::TooManyFetchBlocksRequested(peer));
        }

        if !highest_accepted_rounds.is_empty()
            && highest_accepted_rounds.len() != self.context.committee.size()
        {
            return Err(ConsensusError::InvalidSizeOfHighestAcceptedRounds(
                highest_accepted_rounds.len(),
                self.context.committee.size(),
            ));
        }

        // Some quick validation of the requested block refs
        for block in &block_refs {
            if !self.context.committee.is_valid_index(block.author) {
                return Err(ConsensusError::InvalidAuthorityIndex {
                    index: block.author,
                    max: self.context.committee.size(),
                });
            }
            if block.round == GENESIS_ROUND {
                return Err(ConsensusError::UnexpectedGenesisBlockRequested);
            }
        }

        // For now ask dag state directly
        let blocks = self.dag_state.read().get_blocks(&block_refs);

        // Now check if an ancestor's round is higher than the one that the peer has. If yes, then serve
        // that ancestor blocks up to `MAX_ADDITIONAL_BLOCKS`.
        let mut ancestor_blocks = vec![];
        if !highest_accepted_rounds.is_empty() {
            let all_ancestors = blocks
                .iter()
                .flatten()
                .flat_map(|block| block.ancestors().to_vec())
                .filter(|block_ref| highest_accepted_rounds[block_ref.author] < block_ref.round)
                .take(MAX_ADDITIONAL_BLOCKS)
                .collect::<Vec<_>>();

            if !all_ancestors.is_empty() {
                ancestor_blocks = self.dag_state.read().get_blocks(&all_ancestors);
            }
        }

        // Return the serialised blocks & the ancestor blocks
        let result = blocks
            .into_iter()
            .chain(ancestor_blocks)
            .flatten()
            .map(|block| block.serialized().clone())
            .collect::<Vec<_>>();

        Ok(result)
    }

    async fn handle_fetch_commits(
        &self,
        _peer: AuthorityIndex,
        commit_range: CommitRange,
    ) -> ConsensusResult<(Vec<TrustedCommit>, Vec<VerifiedBlock>)> {
        fail_point_async!("consensus-rpc-response");

        // Compute an inclusive end index and bound the maximum number of commits scanned.
        let inclusive_end = commit_range.end().min(
            commit_range.start() + self.context.parameters.commit_sync_batch_size as CommitIndex
                - 1,
        );
        let mut commits = self
            .store
            .scan_commits((commit_range.start()..=inclusive_end).into())?;
        let mut certifier_block_refs = vec![];
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
                commits.pop();
            }
        }
        let certifier_blocks = self
            .store
            .read_blocks(&certifier_block_refs)?
            .into_iter()
            .flatten()
            .collect();
        Ok((commits, certifier_blocks))
    }

    async fn handle_fetch_latest_blocks(
        &self,
        peer: AuthorityIndex,
        authorities: Vec<AuthorityIndex>,
    ) -> ConsensusResult<Vec<Bytes>> {
        fail_point_async!("consensus-rpc-response");

        if authorities.len() > self.context.committee.size() {
            return Err(ConsensusError::TooManyAuthoritiesProvided(peer));
        }

        // Ensure that those are valid authorities
        for authority in &authorities {
            if !self.context.committee.is_valid_index(*authority) {
                return Err(ConsensusError::InvalidAuthorityIndex {
                    index: *authority,
                    max: self.context.committee.size(),
                });
            }
        }

        // Read from the dag state to find the latest blocks.
        // TODO: at the moment we don't look into the block manager for suspended blocks. Ideally we
        // want in the future if we think we would like to tackle the majority of cases.
        let mut blocks = vec![];
        let dag_state = self.dag_state.read();
        for authority in authorities {
            let block = dag_state.get_last_block_for_authority(authority);

            debug!("Latest block for {authority}: {block:?} as requested from {peer}");

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

    async fn handle_get_latest_rounds(
        &self,
        _peer: AuthorityIndex,
    ) -> ConsensusResult<(Vec<Round>, Vec<Round>)> {
        fail_point_async!("consensus-rpc-response");

        let mut highest_received_rounds = self.core_dispatcher.highest_received_rounds();

        let blocks = self
            .dag_state
            .read()
            .get_last_cached_block_per_authority(Round::MAX);
        let highest_accepted_rounds = blocks
            .into_iter()
            .map(|(block, _)| block.round())
            .collect::<Vec<_>>();

        // Own blocks do not go through the core dispatcher, so they need to be set separately.
        highest_received_rounds[self.context.own_index] =
            highest_accepted_rounds[self.context.own_index];

        Ok((highest_received_rounds, highest_accepted_rounds))
    }
}

struct Counter {
    count: usize,
    subscriptions_by_authority: Vec<usize>,
}

/// Atomically counts the number of active subscriptions to the block broadcast stream,
/// and dispatch commands to core based on the changes.
struct SubscriptionCounter {
    context: Arc<Context>,
    counter: parking_lot::Mutex<Counter>,
    dispatcher: Arc<dyn CoreThreadDispatcher>,
}

impl SubscriptionCounter {
    fn new(context: Arc<Context>, dispatcher: Arc<dyn CoreThreadDispatcher>) -> Self {
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
                subscriptions_by_authority: vec![0; context.committee.size()],
            }),
            dispatcher,
            context,
        }
    }

    fn increment(&self, peer: AuthorityIndex) -> Result<(), ConsensusError> {
        let mut counter = self.counter.lock();
        counter.count += 1;
        counter.subscriptions_by_authority[peer] += 1;

        let peer_hostname = &self.context.committee.authority(peer).hostname;
        self.context
            .metrics
            .node_metrics
            .subscribed_by
            .with_label_values(&[peer_hostname])
            .set(1);

        if counter.count == 1 {
            self.dispatcher
                .set_subscriber_exists(true)
                .map_err(|_| ConsensusError::Shutdown)?;
        }
        Ok(())
    }

    fn decrement(&self, peer: AuthorityIndex) -> Result<(), ConsensusError> {
        let mut counter = self.counter.lock();
        counter.count -= 1;
        counter.subscriptions_by_authority[peer] -= 1;

        if counter.subscriptions_by_authority[peer] == 0 {
            let peer_hostname = &self.context.committee.authority(peer).hostname;
            self.context
                .metrics
                .node_metrics
                .subscribed_by
                .with_label_values(&[peer_hostname])
                .set(0);
        }

        if counter.count == 0 {
            self.dispatcher
                .set_subscriber_exists(false)
                .map_err(|_| ConsensusError::Shutdown)?;
        }
        Ok(())
    }
}

/// Each broadcasted block stream wraps a broadcast receiver for blocks.
/// It yields blocks that are broadcasted after the stream is created.
type BroadcastedBlockStream = BroadcastStream<ExtendedBlock>;

/// Adapted from `tokio_stream::wrappers::BroadcastStream`. The main difference is that
/// this tolerates lags with only logging, without yielding errors.
struct BroadcastStream<T> {
    peer: AuthorityIndex,
    // Stores the receiver across poll_next() calls.
    inner: ReusableBoxFuture<
        'static,
        (
            Result<T, broadcast::error::RecvError>,
            broadcast::Receiver<T>,
        ),
    >,
    // Counts total subscriptions / active BroadcastStreams.
    subscription_counter: Arc<SubscriptionCounter>,
}

impl<T: 'static + Clone + Send> BroadcastStream<T> {
    pub fn new(
        peer: AuthorityIndex,
        rx: broadcast::Receiver<T>,
        subscription_counter: Arc<SubscriptionCounter>,
    ) -> Self {
        if let Err(err) = subscription_counter.increment(peer) {
            match err {
                ConsensusError::Shutdown => {}
                _ => panic!("Unexpected error: {err}"),
            }
        }
        Self {
            peer,
            inner: ReusableBoxFuture::new(make_recv_future(rx)),
            subscription_counter,
        }
    }
}

impl<T: 'static + Clone + Send> Stream for BroadcastStream<T> {
    type Item = T;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<Option<Self::Item>> {
        let peer = self.peer;
        let maybe_item = loop {
            let (result, rx) = ready!(self.inner.poll(cx));
            self.inner.set(make_recv_future(rx));

            match result {
                Ok(item) => break Some(item),
                Err(broadcast::error::RecvError::Closed) => {
                    info!("Block BroadcastedBlockStream {} closed", peer);
                    break None;
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(
                        "Block BroadcastedBlockStream {} lagged by {} messages",
                        peer, n
                    );
                    continue;
                }
            }
        };
        task::Poll::Ready(maybe_item)
    }
}

impl<T> Drop for BroadcastStream<T> {
    fn drop(&mut self) {
        if let Err(err) = self.subscription_counter.decrement(self.peer) {
            match err {
                ConsensusError::Shutdown => {}
                _ => panic!("Unexpected error: {err}"),
            }
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

// TODO: add a unit test for BroadcastStream.

#[cfg(test)]
mod tests {
    use crate::{
        authority_service::AuthorityService,
        block::{BlockAPI, BlockRef, SignedBlock, TestBlock, VerifiedBlock},
        commit::CommitRange,
        commit_vote_monitor::CommitVoteMonitor,
        context::Context,
        core_thread::{CoreError, CoreThreadDispatcher},
        dag_state::DagState,
        error::ConsensusResult,
        network::{BlockStream, ExtendedSerializedBlock, NetworkClient, NetworkService},
        round_prober::QuorumRound,
        storage::mem_store::MemStore,
        synchronizer::Synchronizer,
        test_dag_builder::DagBuilder,
        Round,
    };
    use async_trait::async_trait;
    use bytes::Bytes;
    use consensus_config::AuthorityIndex;
    use parking_lot::{Mutex, RwLock};
    use std::collections::BTreeSet;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::broadcast;
    use tokio::time::sleep;

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

        async fn new_block(&self, _round: Round, _force: bool) -> Result<(), CoreError> {
            Ok(())
        }

        async fn get_missing_blocks(&self) -> Result<BTreeSet<BlockRef>, CoreError> {
            Ok(Default::default())
        }

        fn set_subscriber_exists(&self, _exists: bool) -> Result<(), CoreError> {
            todo!()
        }

        fn set_propagation_delay_and_quorum_rounds(
            &self,
            _delay: Round,
            _received_quorum_rounds: Vec<QuorumRound>,
            _accepted_quorum_rounds: Vec<QuorumRound>,
        ) -> Result<(), CoreError> {
            todo!()
        }

        fn set_last_known_proposed_round(&self, _round: Round) -> Result<(), CoreError> {
            todo!()
        }

        fn highest_received_rounds(&self) -> Vec<Round> {
            todo!()
        }
    }

    #[derive(Default)]
    struct FakeNetworkClient {}

    #[async_trait]
    impl NetworkClient for FakeNetworkClient {
        const SUPPORT_STREAMING: bool = false;

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
    async fn test_handle_send_block() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let block_verifier = Arc::new(crate::block_verifier::NoopBlockVerifier {});
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let core_dispatcher = Arc::new(FakeCoreThreadDispatcher::new());
        let (_tx_block_broadcast, rx_block_broadcast) = broadcast::channel(100);
        let network_client = Arc::new(FakeNetworkClient::default());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let synchronizer = Synchronizer::start(
            network_client,
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            block_verifier.clone(),
            dag_state.clone(),
            false,
        );
        let authority_service = Arc::new(AuthorityService::new(
            context.clone(),
            block_verifier,
            commit_vote_monitor,
            synchronizer,
            core_dispatcher.clone(),
            rx_block_broadcast,
            dag_state,
            store,
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
            block: input_block.serialized().clone(),
            excluded_ancestors: vec![],
        };

        tokio::spawn(async move {
            service
                .handle_send_block(context.committee.to_authority_index(0).unwrap(), serialized)
                .await
                .unwrap();
        });

        sleep(max_drift / 2).await;
        assert!(core_dispatcher.get_blocks().is_empty());

        sleep(max_drift).await;
        let blocks = core_dispatcher.get_blocks();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0], input_block);
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
        let network_client = Arc::new(FakeNetworkClient::default());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let synchronizer = Synchronizer::start(
            network_client,
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            block_verifier.clone(),
            dag_state.clone(),
            true,
        );
        let authority_service = Arc::new(AuthorityService::new(
            context.clone(),
            block_verifier,
            commit_vote_monitor,
            synchronizer,
            core_dispatcher.clone(),
            rx_block_broadcast,
            dag_state.clone(),
            store,
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
}
