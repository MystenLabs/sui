// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use consensus_config::AuthorityIndex;
use consensus_types::block::Round;
use futures::StreamExt;
use mysten_metrics::spawn_monitored_task;
use parking_lot::{Mutex, RwLock};
use tokio::{task::JoinHandle, time::sleep};
use tracing::{debug, error, info};

use crate::{
    block::{BlockAPI as _, VerifiedBlock},
    block_inflater::BlockInflater,
    context::Context,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    minimal_block::{FallbackReason, InflateError},
    network::{ExtendedSerializedBlock, ValidatorNetworkClient, ValidatorNetworkService},
    task::{join_and_propagate_panic, reap_finished_task},
};

/// Timeout for the strict full-block fetch after a failed minimal-block inflation.
const FALLBACK_FETCH_TIMEOUT: Duration = Duration::from_secs(2);
/// Fallback-fetch budget: at most `MAX_FETCHES` in any rolling window of `WINDOW`
/// received minimal blocks, so varied claimed digests cannot multiply fetch work.
const FALLBACK_BUDGET_WINDOW: u32 = 32;
const FALLBACK_BUDGET_MAX_FETCHES: u32 = 4;
/// Malformed minimal encodings tolerated per subscription session before the stream is
/// reset (reconnect backoff is the peer's penalty). Honest senders produce none.
const MAX_MALFORMED_PER_SUBSCRIPTION: u32 = 3;

/// Bounds fallback fetches per subscription. Stream processing is sequential per peer,
/// so this also bounds fetch concurrency to one.
struct FallbackBudget {
    window: u32,
    max_fetches: u32,
    blocks_seen: u32,
    fetches_used: u32,
}

impl FallbackBudget {
    fn new(window: u32, max_fetches: u32) -> Self {
        Self {
            window,
            max_fetches,
            blocks_seen: 0,
            fetches_used: 0,
        }
    }

    /// Records one received minimal block, rolling the window over when it fills.
    fn record_block(&mut self) {
        if self.blocks_seen >= self.window {
            self.blocks_seen = 0;
            self.fetches_used = 0;
        }
        self.blocks_seen += 1;
    }

    /// Consumes one fetch from the budget; false when the window is exhausted.
    fn try_fetch(&mut self) -> bool {
        if self.fetches_used < self.max_fetches {
            self.fetches_used += 1;
            true
        } else {
            false
        }
    }
}

/// Subscriber manages the block stream subscriptions to other peers, taking care of retrying
/// when subscription streams break. Blocks returned from the peer are sent to the authority
/// service for processing.
/// Currently subscription management for individual peer is not exposed, but it could become
/// useful in future.
pub(crate) struct Subscriber<C: ValidatorNetworkClient, S: ValidatorNetworkService> {
    context: Arc<Context>,
    network_client: Arc<C>,
    authority_service: Arc<S>,
    dag_state: Arc<RwLock<DagState>>,
    block_inflater: Arc<BlockInflater>,
    subscriptions: Arc<Mutex<Box<[Option<JoinHandle<()>>]>>>,
    // Retain replaced subscription tasks so stop() can await them and propagate panics.
    retired_subscriptions: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl<C: ValidatorNetworkClient, S: ValidatorNetworkService> Subscriber<C, S> {
    pub(crate) fn new(
        context: Arc<Context>,
        network_client: Arc<C>,
        authority_service: Arc<S>,
        dag_state: Arc<RwLock<DagState>>,
    ) -> Self {
        let subscriptions = (0..context.committee.size())
            .map(|_| None)
            .collect::<Vec<_>>();
        let block_inflater = Arc::new(BlockInflater::new(context.clone(), dag_state.clone()));
        Self {
            context,
            network_client,
            authority_service,
            dag_state,
            block_inflater,
            subscriptions: Arc::new(Mutex::new(subscriptions.into_boxed_slice())),
            retired_subscriptions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(crate) fn subscribe(&self, peer: AuthorityIndex) {
        if peer == self.context.own_index {
            error!("Attempt to subscribe to own validator {peer} is ignored!");
            return;
        }
        let context = self.context.clone();
        let network_client = self.network_client.clone();
        // Subscriber already holds these resources strongly. Give subscription tasks weak
        // references so they do not become additional owners during shutdown.
        let authority_service = Arc::downgrade(&self.authority_service);
        let dag_state = Arc::downgrade(&self.dag_state);
        let block_inflater = self.block_inflater.clone();

        let mut subscriptions = self.subscriptions.lock();
        self.unsubscribe_locked(peer, &mut subscriptions[peer.value()]);
        subscriptions[peer.value()] = Some(spawn_monitored_task!(Self::subscription_loop(
            context,
            network_client,
            authority_service,
            dag_state,
            block_inflater,
            peer,
        )));
    }

    pub(crate) async fn stop(&self) {
        {
            let mut subscriptions = self.subscriptions.lock();
            for (peer, _) in self.context.committee.authorities() {
                self.unsubscribe_locked(peer, &mut subscriptions[peer.value()]);
            }
        }

        // All retired subscriptions have already been aborted by unsubscribe_locked().
        let subscriptions = std::mem::take(&mut *self.retired_subscriptions.lock());
        for subscription in subscriptions {
            join_and_propagate_panic(subscription).await;
        }
    }

    fn unsubscribe_locked(&self, peer: AuthorityIndex, subscription: &mut Option<JoinHandle<()>>) {
        let peer_hostname = &self.context.committee.authority(peer).hostname;
        if let Some(subscription) = subscription.take() {
            subscription.abort();
            let mut retired_subscriptions = self.retired_subscriptions.lock();
            // Reap retired subscriptions that have finished, so the list stays bounded under
            // repeated resubscriptions.
            retired_subscriptions.retain_mut(|task| !reap_finished_task(task));
            retired_subscriptions.push(subscription);
        }
        // There is a race between shutting down the subscription task and clearing the metric here.
        // TODO: fix the race when unsubscribe_locked() gets called outside of stop().
        self.context
            .metrics
            .node_metrics
            .subscribed_to
            .with_label_values(&[peer_hostname])
            .set(0);
    }

    /// Replaces a minimal-form block with its inflated full serialization, falling back to
    /// a strict fetch from the sending peer, which authored the block and must hold it.
    /// Fallback fetches are bounded by `budget`; a block dropped over budget is recovered
    /// via the descendant-driven missing-ancestor path once other validators reference it.
    async fn inflate_received_block(
        context: &Context,
        block_inflater: &BlockInflater,
        network_client: &C,
        peer: AuthorityIndex,
        mut block: ExtendedSerializedBlock,
        budget: &mut FallbackBudget,
    ) -> ConsensusResult<ExtendedSerializedBlock> {
        let Some(minimal) = block.minimal.take() else {
            return Ok(block);
        };
        let peer_hostname = context.committee.authority(peer).hostname.as_str();
        let node_metrics = &context.metrics.node_metrics;
        node_metrics
            .minimal_blocks_received
            .with_label_values(&[peer_hostname])
            .inc();
        budget.record_block();

        // Cap the envelope before decoding: a legitimate minimal block is bounded by the
        // max transaction payload plus small per-ancestor structure, so anything larger
        // is a peer fault and must not cost a BCS parse of attacker-sized buffers.
        let max_minimal_size =
            (context.protocol_config.max_transactions_in_block_bytes() as usize).saturating_mul(2);
        if minimal.len() > max_minimal_size {
            node_metrics
                .minimal_block_inflate_fallback
                .with_label_values(&[peer_hostname, "malformed"])
                .inc();
            return Err(ConsensusError::MalformedMinimalBlock(format!(
                "minimal block of {} bytes exceeds the {max_minimal_size}-byte cap",
                minimal.len()
            )));
        }

        match block_inflater.inflate_with_retry(&minimal, peer).await {
            Ok((_signed_block, serialized)) => {
                node_metrics
                    .minimal_blocks_received_bytes_saved
                    .with_label_values(&[peer_hostname])
                    .inc_by(serialized.len().saturating_sub(minimal.len()) as u64);
                block.block = serialized;
                Ok(block)
            }
            Err(InflateError::NeedFullBlock { block_ref, reason }) => {
                node_metrics
                    .minimal_block_inflate_fallback
                    .with_label_values(&[peer_hostname, reason.label()])
                    .inc();
                // Missing-ancestor fetches bypass the budget: they are legitimate
                // catch-up work (a receiver that is behind fails on most live blocks),
                // and dropping them can deadlock the committee — the budget only refills
                // as blocks arrive, but blocks only arrive if recovery proceeds. They
                // are naturally rate-limited anyway: fetches are sequential per peer,
                // sender-only, and the wait slows only that peer's own stream. The
                // budget bounds the state-divergence reasons, where repeated fetches
                // suggest equivocation games or claimed-digest abuse.
                let budgeted = !matches!(reason, FallbackReason::MissingAncestor(_));
                if budgeted && !budget.try_fetch() {
                    node_metrics
                        .minimal_block_inflate_fallback
                        .with_label_values(&[peer_hostname, "budget_exceeded"])
                        .inc();
                    return Err(ConsensusError::FallbackBudgetExceeded(peer));
                }
                // Strict fallback: fetch only from the sending peer and require the exact
                // claimed block in the response — never retry or fan out to other peers
                // for an unverified claimed digest.
                let serialized_blocks = network_client
                    .fetch_blocks(peer, vec![block_ref], vec![], false, FALLBACK_FETCH_TIMEOUT)
                    .await?;
                let fetched_bytes: usize = serialized_blocks.iter().map(|b| b.len()).sum();
                node_metrics
                    .minimal_block_fallback_fetch_bytes
                    .with_label_values(&[peer_hostname])
                    .inc_by(fetched_bytes as u64);
                let fetched = serialized_blocks
                    .into_iter()
                    .find(|b| VerifiedBlock::compute_digest(b) == block_ref.digest)
                    .ok_or(ConsensusError::UnexpectedFetchedBlock {
                        index: peer,
                        block_ref,
                    })?;
                block.block = fetched;
                Ok(block)
            }
            Err(error @ InflateError::Malformed(_)) => {
                node_metrics
                    .minimal_block_inflate_fallback
                    .with_label_values(&[peer_hostname, "malformed"])
                    .inc();
                Err(ConsensusError::MalformedMinimalBlock(error.to_string()))
            }
        }
    }

    async fn subscription_loop(
        context: Arc<Context>,
        network_client: Arc<C>,
        authority_service: Weak<S>,
        dag_state: Weak<RwLock<DagState>>,
        block_inflater: Arc<BlockInflater>,
        peer: AuthorityIndex,
    ) {
        const IMMEDIATE_RETRIES: i64 = 3;
        const MIN_TIMEOUT: Duration = Duration::from_millis(500);
        // When not immediately retrying, limit retry delay between 100ms and 10s.
        let mut backoff = mysten_common::backoff::ExponentialBackoff::new(
            Duration::from_millis(100),
            Duration::from_secs(10),
        );

        let peer_hostname = &context.committee.authority(peer).hostname;
        let mut retries: i64 = 0;
        'subscription: loop {
            context
                .metrics
                .node_metrics
                .subscribed_to
                .with_label_values(&[peer_hostname])
                .set(0);

            let mut delay = Duration::ZERO;
            if retries > IMMEDIATE_RETRIES {
                delay = backoff.next().unwrap();
                debug!(
                    "Delaying retry {} of peer {} subscription, in {} seconds",
                    retries,
                    peer_hostname,
                    delay.as_secs_f32(),
                );
                sleep(delay).await;
            } else if retries > 0 {
                // Retry immediately, but still yield to avoid monopolizing the thread.
                tokio::task::yield_now().await;
            }
            retries += 1;

            // Recompute the resume round from DagState before each connection attempt, so a
            // reconnection resumes from the latest accepted round rather than re-streaming and
            // re-verifying blocks that have been accepted since this subscription started.
            let last_received: Round = {
                let Some(dag_state) = dag_state.upgrade() else {
                    return;
                };
                let dag_state = dag_state.read();
                let gc_round = dag_state.gc_round();
                dag_state
                    .get_last_block_for_authority(peer)
                    .round()
                    .max(gc_round)
            };

            // Use longer timeout when retry delay is long, to adapt to slow network.
            let request_timeout = MIN_TIMEOUT.max(delay);
            let mut blocks = match network_client
                .subscribe_blocks(peer, last_received, request_timeout)
                .await
            {
                Ok(blocks) => {
                    debug!(
                        "Subscribed to peer {} {} after {} attempts",
                        peer, peer_hostname, retries
                    );
                    context
                        .metrics
                        .node_metrics
                        .subscriber_connection_attempts
                        .with_label_values(&[peer_hostname.as_str(), "success"])
                        .inc();
                    blocks
                }
                Err(e) => {
                    debug!(
                        "Failed to subscribe to blocks from peer {} {}: {}",
                        peer, peer_hostname, e
                    );
                    context
                        .metrics
                        .node_metrics
                        .subscriber_connection_attempts
                        .with_label_values(&[peer_hostname.as_str(), "failure"])
                        .inc();
                    continue 'subscription;
                }
            };

            // Now can consider the subscription successful
            context
                .metrics
                .node_metrics
                .subscribed_to
                .with_label_values(&[peer_hostname])
                .set(1);

            let mut fallback_budget =
                FallbackBudget::new(FALLBACK_BUDGET_WINDOW, FALLBACK_BUDGET_MAX_FETCHES);
            let mut malformed_blocks: u32 = 0;
            'stream: loop {
                match blocks.next().await {
                    Some(block) => {
                        context
                            .metrics
                            .node_metrics
                            .subscribed_blocks
                            .with_label_values(&[peer_hostname])
                            .inc();
                        let Some(authority_service) = authority_service.upgrade() else {
                            return;
                        };
                        let block = match Self::inflate_received_block(
                            &context,
                            &block_inflater,
                            network_client.as_ref(),
                            peer,
                            block,
                            &mut fallback_budget,
                        )
                        .await
                        {
                            Ok(block) => block,
                            Err(e) => {
                                info!(
                                    "Failed to inflate minimal block from peer {} {}: {}",
                                    peer, peer_hostname, e
                                );
                                // A peer repeatedly sending malformed encodings gets its
                                // stream reset; escalating reconnect backoff is the penalty.
                                if matches!(e, ConsensusError::MalformedMinimalBlock(_)) {
                                    malformed_blocks += 1;
                                    if malformed_blocks > MAX_MALFORMED_PER_SUBSCRIPTION {
                                        info!(
                                            "Too many malformed minimal blocks from peer {} {}; resetting subscription",
                                            peer, peer_hostname
                                        );
                                        continue 'subscription;
                                    }
                                }
                                continue 'stream;
                            }
                        };
                        let result = authority_service.handle_send_block(peer, block).await;
                        if let Err(e) = result {
                            match e {
                                ConsensusError::BlockRejected { block_ref, reason } => {
                                    debug!(
                                        "Failed to process block from peer {} {} for block {:?}: {}",
                                        peer, peer_hostname, block_ref, reason
                                    );
                                }
                                _ => {
                                    info!(
                                        "Invalid block received from peer {} {}: {}",
                                        peer, peer_hostname, e
                                    );
                                }
                            }
                        }
                        // Reset the retry counter and backoff when a block is received, so a peer
                        // that recovers after flapping reconnects promptly instead of inheriting
                        // the previously escalated delay.
                        retries = 0;
                        backoff.reset();
                    }
                    None => {
                        debug!(
                            "Subscription to blocks from peer {} {} ended",
                            peer, peer_hostname
                        );
                        retries += 1;
                        break 'stream;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use async_trait::async_trait;
    use bytes::Bytes;
    use consensus_types::block::BlockRef;
    use futures::stream;

    use super::*;
    use crate::{
        VerifiedBlock,
        commit::CommitRange,
        error::ConsensusResult,
        network::{BlockStream, ExtendedSerializedBlock, test_network::TestService},
        storage::mem_store::MemStore,
    };

    struct SubscriberTestClient {
        // Records the `last_received` round passed to each subscribe_blocks() call.
        subscribe_calls: Mutex<Vec<Round>>,
    }

    impl SubscriberTestClient {
        fn new() -> Self {
            Self {
                subscribe_calls: Mutex::new(Vec::new()),
            }
        }

        fn subscribe_calls(&self) -> Vec<Round> {
            self.subscribe_calls.lock().clone()
        }
    }

    #[async_trait]
    impl ValidatorNetworkClient for SubscriberTestClient {
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
            last_received: Round,
            _timeout: Duration,
        ) -> ConsensusResult<BlockStream> {
            self.subscribe_calls.lock().push(last_received);
            let block_stream = stream::unfold((), |_| async {
                sleep(Duration::from_millis(1)).await;
                let block = ExtendedSerializedBlock {
                    minimal: None,
                    block: Bytes::from(vec![1u8; 8]),
                    excluded_ancestors: vec![],
                };
                Some((block, ()))
            })
            .take(10);
            Ok(Box::pin(block_stream))
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

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn subscriber_retries() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let network_client = Arc::new(SubscriberTestClient::new());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let subscriber = Subscriber::new(
            context.clone(),
            network_client,
            authority_service.clone(),
            dag_state,
        );

        let peer = context.committee.to_authority_index(2).unwrap();
        subscriber.subscribe(peer);

        // Wait for enough blocks received.
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let service = authority_service.lock();
            if service.handle_send_block.len() >= 100 {
                break;
            }
        }

        // Even if the stream ends after 10 blocks, the subscriber should retry and get enough
        // blocks eventually.
        let service = authority_service.lock();
        assert!(service.handle_send_block.len() >= 100);
        for (p, block) in service.handle_send_block.iter() {
            assert_eq!(*p, peer);
            assert_eq!(
                *block,
                ExtendedSerializedBlock {
                    minimal: None,
                    block: Bytes::from(vec![1u8; 8]),
                    excluded_ancestors: vec![]
                }
            );
        }
    }

    // Regression test: `last_received` must be recomputed from DagState before each connection
    // attempt. Previously it was captured once at subscribe() time and reused for every reconnect,
    // causing already-accepted blocks to be re-streamed and re-verified.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn subscriber_recomputes_resume_round_on_reconnect() {
        use crate::block::TestBlock;

        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let network_client = Arc::new(SubscriberTestClient::new());
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service,
            dag_state.clone(),
        );

        let peer = context.committee.to_authority_index(2).unwrap();
        subscriber.subscribe(peer);

        // Before any block from the peer is accepted, every reconnect resumes from genesis (0).
        tokio::time::sleep(Duration::from_secs(3)).await;
        {
            let recorded = network_client.subscribe_calls();
            assert!(
                !recorded.is_empty() && recorded.iter().all(|&r| r == 0),
                "before a block is accepted, every reconnect should resume from round 0: {recorded:?}"
            );
        }

        // Advance the locally accepted round for the peer.
        const RESUME_ROUND: Round = 10;
        dag_state.write().accept_block(VerifiedBlock::new_for_test(
            TestBlock::new(RESUME_ROUND, peer.value() as u32).build(),
        ));

        // After the block is accepted, reconnects must resume from the advanced round. With the
        // bug, `last_received` would stay at 0 forever.
        let mut observed_resume = false;
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if network_client.subscribe_calls().last() == Some(&RESUME_ROUND) {
                observed_resume = true;
                break;
            }
        }
        assert!(
            observed_resume,
            "after accepting a block at round {RESUME_ROUND}, the subscriber should resume from it; \
             recorded resume rounds: {:?}",
            network_client.subscribe_calls()
        );
    }

    #[tokio::test]
    async fn fallback_budget_bounds_fetches_per_window() {
        let mut budget = FallbackBudget::new(4, 2);
        budget.record_block();
        assert!(budget.try_fetch());
        assert!(budget.try_fetch());
        // Budget exhausted for the rest of this window.
        assert!(!budget.try_fetch());
        budget.record_block();
        budget.record_block();
        budget.record_block();
        assert!(!budget.try_fetch());
        // The next block rolls the window over and restores the budget.
        budget.record_block();
        assert!(budget.try_fetch());
    }
}
