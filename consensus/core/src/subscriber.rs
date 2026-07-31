// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockRef, Round};
use futures::StreamExt;
use mysten_metrics::spawn_monitored_task;
use parking_lot::{Mutex, RwLock};
use tokio::{
    task::{JoinHandle, JoinSet},
    time::sleep,
};
use tracing::{debug, error, info};

use crate::{
    block::BlockAPI as _,
    block_inflater::BlockInflater,
    context::Context,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    minimal_block::{FallbackReason, InflateError},
    minimal_block_receive::{RecoveryQuotas, RepairLimits, recover_minimal_block},
    network::{ExtendedSerializedBlock, ValidatorNetworkClient, ValidatorNetworkService},
    task::{join_and_propagate_panic, reap_finished_task},
};

#[cfg(test)]
use crate::minimal_block_receive::RecoveryLimits;

/// Malformed minimal encodings tolerated per subscription session before the stream is
/// reset (reconnect backoff is the peer's penalty). Honest senders produce none.
const MAX_MALFORMED_PER_SUBSCRIPTION: u32 = 3;

/// Floor delay before reconnecting after a reset this node initiated for cause
/// (quota overflow, recovery horizon, malformed threshold). The regular backoff can
/// be cleared by any admitted block, so a peer interleaving valid blocks with
/// reset-triggering ones could otherwise force immediate reconnects in an unbounded
/// loop; this floor caps that at one reset per second regardless of backoff state.
/// An honest deep laggard pays it once per overflow, ahead of a replay that covers
/// hundreds of rounds.
const CAUSED_RESET_DELAY: Duration = Duration::from_secs(1);

/// Outcome of processing one received block envelope before it is handed to the service.
enum InflateOutcome {
    /// A full-form block, or a minimal block successfully inflated to full form.
    Block(ExtendedSerializedBlock),
    /// A minimal block that local state cannot inflate right now. It is dropped from
    /// the stream and recovered by a per-block task that waits on the missing slot
    /// (see `minimal_block_receive`).
    Dropped {
        block_ref: BlockRef,
        reason: FallbackReason,
        minimal: Bytes,
    },
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
    recovery_quotas: Arc<RecoveryQuotas>,
    repair_limits: Arc<RepairLimits>,
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
        let block_inflater = Arc::new(BlockInflater::new(context.clone()));
        let recovery_quotas = RecoveryQuotas::new(context.clone());
        let repair_limits = RepairLimits::new(context.committee.size());
        Self {
            context,
            network_client,
            authority_service,
            dag_state,
            block_inflater,
            recovery_quotas,
            repair_limits,
            subscriptions: Arc::new(Mutex::new(subscriptions.into_boxed_slice())),
            retired_subscriptions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Replaces the recovery quotas with test-controlled limits. Must be called before
    /// any subscribe(): running tasks keep permits from the quotas they started with.
    #[cfg(test)]
    pub(crate) fn with_recovery_limits(mut self, limits: RecoveryLimits) -> Self {
        self.recovery_quotas = RecoveryQuotas::with_limits(self.context.clone(), limits);
        self
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
        let recovery_quotas = self.recovery_quotas.clone();
        let repair_limits = self.repair_limits.clone();

        let mut subscriptions = self.subscriptions.lock();
        self.unsubscribe_locked(peer, &mut subscriptions[peer.value()]);
        subscriptions[peer.value()] = Some(spawn_monitored_task!(Self::subscription_loop(
            context,
            network_client,
            authority_service,
            dag_state,
            block_inflater,
            recovery_quotas,
            repair_limits,
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
        // Awaiting them drops each subscription loop's JoinSet, which aborts every
        // recovery task; the aborted tasks release their permits (and correct the
        // parked gauges) as the runtime drops them.
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

    /// Cap on untrusted minimal bytes, enforced before ANY decoding: a legitimate
    /// minimal block is bounded by the max transaction payload plus small per-ancestor
    /// structure.
    fn max_minimal_size(context: &Context) -> usize {
        (context.protocol_config.max_transactions_in_block_bytes() as usize).saturating_mul(2)
    }

    /// Replaces a minimal-form block with its inflated full serialization.
    ///
    /// Returns `Dropped` when the block cannot be inflated from local state; the caller
    /// continues the stream immediately and hands the block to a recovery task.
    /// Dropping without recovery is not an option: a receiver that falls a round behind
    /// finds every subsequent block from every peer un-inflatable (each references
    /// blocks it lacks), nothing reaches `block_manager`, so no missing ancestor is
    /// ever registered and the node wedges permanently.
    ///
    /// This path must never wait on the network: blocks from a peer are processed
    /// sequentially, so any await here delays every later block from that peer, and on
    /// high-latency links that compounds — a stream that falls behind on ancestors
    /// stalls more, not less.
    fn inflate_received_block(
        context: &Context,
        block_inflater: &BlockInflater,
        peer: AuthorityIndex,
        mut block: ExtendedSerializedBlock,
        dag_state: &DagState,
    ) -> ConsensusResult<InflateOutcome> {
        let Some(minimal) = block.minimal.take() else {
            return Ok(InflateOutcome::Block(block));
        };
        let peer_hostname = context.committee.authority(peer).hostname.as_str();
        let node_metrics = &context.metrics.node_metrics;
        node_metrics
            .minimal_blocks_received
            .with_label_values(&[peer_hostname])
            .inc();

        let max_minimal_size = Self::max_minimal_size(context);
        if minimal.len() > max_minimal_size {
            node_metrics
                .minimal_block_inflate_drop
                .with_label_values(&[peer_hostname, "malformed"])
                .inc();
            return Err(ConsensusError::MalformedMinimalBlock(format!(
                "minimal block of {} bytes exceeds the {max_minimal_size}-byte cap",
                minimal.len()
            )));
        }

        match block_inflater.inflate(&minimal, peer, dag_state) {
            Ok((_signed_block, serialized)) => {
                block.block = serialized;
                Ok(InflateOutcome::Block(block))
            }
            Err(InflateError::NeedFullBlock { block_ref, reason }) => {
                node_metrics
                    .minimal_block_inflate_drop
                    .with_label_values(&[peer_hostname, reason.label()])
                    .inc();
                // The recovery task starts from this classification: waits for a
                // missing slot, everything else escalates to exact-reference repair.
                Ok(InflateOutcome::Dropped {
                    block_ref,
                    reason,
                    minimal,
                })
            }
            Err(error @ InflateError::Malformed(_)) => {
                node_metrics
                    .minimal_block_inflate_drop
                    .with_label_values(&[peer_hostname, "malformed"])
                    .inc();
                Err(ConsensusError::MalformedMinimalBlock(error.to_string()))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn subscription_loop(
        context: Arc<Context>,
        network_client: Arc<C>,
        authority_service: Weak<S>,
        dag_state: Weak<RwLock<DagState>>,
        block_inflater: Arc<BlockInflater>,
        recovery_quotas: Arc<RecoveryQuotas>,
        repair_limits: Arc<RepairLimits>,
        peer: AuthorityIndex,
    ) {
        const IMMEDIATE_RETRIES: i64 = 3;
        const MIN_TIMEOUT: Duration = Duration::from_millis(500);
        // When not immediately retrying, limit retry delay between 100ms and 10s.
        let mut backoff = mysten_common::backoff::ExponentialBackoff::new(
            Duration::from_millis(100),
            Duration::from_secs(10),
        );

        // Owned by the subscription LOOP, not one stream instance: recovery tasks
        // survive stream resets and reconnects, and are aborted (releasing quota
        // permits and slot registrations) only when the whole loop is dropped.
        let mut recoveries: JoinSet<()> = JoinSet::new();

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

            let mut malformed_blocks: u32 = 0;
            'stream: loop {
                // Reap completed recovery tasks even while the stream is idle — a
                // parked task's panic must surface like any other consensus task's,
                // not sit unobserved until the next block arrives. (The simulator's
                // patched tokio has no try_join_next; select over join_next covers
                // both needs.)
                let next = tokio::select! {
                    result = recoveries.join_next(), if !recoveries.is_empty() => {
                        if let Some(Err(error)) = result
                            && error.is_panic()
                        {
                            std::panic::resume_unwind(error.into_panic());
                        }
                        continue 'stream;
                    }
                    next = blocks.next() => next,
                };
                match next {
                    Some(block) => {
                        context
                            .metrics
                            .node_metrics
                            .subscribed_blocks
                            .with_label_values(&[peer_hostname])
                            .inc();
                        let Some(service) = authority_service.upgrade() else {
                            return;
                        };
                        let (outcome, tip) = {
                            let Some(dag_state) = dag_state.upgrade() else {
                                return;
                            };
                            let guard = dag_state.read();
                            (
                                Self::inflate_received_block(
                                    &context,
                                    &block_inflater,
                                    peer,
                                    block,
                                    &guard,
                                ),
                                guard.highest_accepted_round(),
                            )
                        };
                        let block = match outcome {
                            // Dropped from the stream and owned by a recovery task.
                            Ok(InflateOutcome::Dropped {
                                block_ref,
                                reason,
                                minimal,
                            }) => {
                                // Spawn-eligibility horizon: an admitted wait is
                                // GC-bounded only if the claimed round is one the
                                // local DAG can plausibly reach — a fabricated
                                // far-future round would pin its quota until GC
                                // crosses it, indefinitely. Beyond the horizon the
                                // stream resets instead: for an honest sender this
                                // means the receiver is deeply behind and full-form
                                // replay is strictly the better path; for a Byzantine
                                // sender it wastes only its own stream.
                                let horizon = tip.saturating_add(
                                    context.parameters.dag_state_cached_rounds as Round,
                                );
                                if block_ref.round > horizon {
                                    context
                                        .metrics
                                        .node_metrics
                                        .minimal_block_quota_drops
                                        .with_label_values(&["round_horizon"])
                                        .inc();
                                    info!(
                                        "Minimal block {} from peer {} {} claims a round \
                                         beyond the recovery horizon ({}); resetting \
                                         subscription for full replay",
                                        block_ref, peer, peer_hostname, horizon
                                    );
                                    sleep(CAUSED_RESET_DELAY).await;
                                    continue 'subscription;
                                }
                                match recovery_quotas.try_acquire(peer, minimal.len()) {
                                    // The stream itself is healthy — it delivered a
                                    // well-formed block — so the reconnect backoff
                                    // resets as for an accepted block. The reset must
                                    // NOT happen on the quota-overflow arm below: a
                                    // recurring overflow reconnects in a loop, and
                                    // only the escalating backoff paces it.
                                    Ok(permit) => {
                                        retries = 0;
                                        backoff.reset();
                                        recoveries.spawn(recover_minimal_block(
                                            context.clone(),
                                            block_inflater.clone(),
                                            dag_state.clone(),
                                            network_client.clone(),
                                            authority_service.clone(),
                                            repair_limits.clone(),
                                            peer,
                                            block_ref,
                                            reason,
                                            minimal,
                                            permit,
                                        ));
                                        continue 'stream;
                                    }
                                    Err(limit) => {
                                        context
                                            .metrics
                                            .node_metrics
                                            .minimal_block_quota_drops
                                            .with_label_values(&[limit.label()])
                                            .inc();
                                        // The dropped bytes must not be stranded: only
                                        // a reconnect re-delivers them (as full-form
                                        // replay, which bypasses these quotas), and a
                                        // natural reconnect may never come. Existing
                                        // recovery tasks survive the reset — the
                                        // JoinSet belongs to the loop, not the stream.
                                        info!(
                                            "Recovery quota ({}) exhausted for peer {} {}; \
                                             resetting subscription for full replay",
                                            limit.label(),
                                            peer,
                                            peer_hostname
                                        );
                                        sleep(CAUSED_RESET_DELAY).await;
                                        continue 'subscription;
                                    }
                                }
                            }
                            Ok(InflateOutcome::Block(block)) => block,
                            Err(e) => {
                                // A peer repeatedly sending malformed encodings gets its
                                // stream reset; escalating reconnect backoff is the
                                // penalty. Individual failures are visible through
                                // minimal_block_inflate_drop{authority,reason}.
                                if matches!(e, ConsensusError::MalformedMinimalBlock(_)) {
                                    malformed_blocks += 1;
                                    if malformed_blocks > MAX_MALFORMED_PER_SUBSCRIPTION {
                                        info!(
                                            "Too many malformed minimal blocks from peer {} {}; resetting subscription",
                                            peer, peer_hostname
                                        );
                                        sleep(CAUSED_RESET_DELAY).await;
                                        continue 'subscription;
                                    }
                                }
                                continue 'stream;
                            }
                        };
                        let result = service.handle_send_block(peer, block).await;
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
        block::{TestBlock, genesis_blocks},
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

    /// Serves a fixed block sequence, then holds the stream open (or hands over to a
    /// live push channel on the first subscription). `blocks_after_reset` replaces the
    /// sequence on re-subscriptions, modeling a sender that serves full-form replay
    /// after a renegotiating handshake. `fetchable` seeds the blocks the peer can
    /// serve to repair fetches, like the author's store would.
    struct FixedStreamClient {
        blocks: Vec<ExtendedSerializedBlock>,
        blocks_after_reset: Option<Vec<ExtendedSerializedBlock>>,
        live: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<ExtendedSerializedBlock>>>,
        fetchable: Vec<Bytes>,
        subscribe_calls: Mutex<usize>,
        fetch_calls: Mutex<Vec<Vec<BlockRef>>>,
    }

    impl FixedStreamClient {
        fn new(blocks: Vec<ExtendedSerializedBlock>) -> Self {
            Self {
                blocks,
                blocks_after_reset: None,
                live: Mutex::new(None),
                fetchable: vec![],
                subscribe_calls: Mutex::new(0),
                fetch_calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl ValidatorNetworkClient for FixedStreamClient {
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
            let mut calls = self.subscribe_calls.lock();
            *calls += 1;
            let blocks = match (&self.blocks_after_reset, *calls > 1) {
                (Some(after), true) => after.clone(),
                _ => self.blocks.clone(),
            };
            let tail: BlockStream = match self.live.lock().take() {
                Some(receiver) => Box::pin(stream::unfold(receiver, |mut receiver| async {
                    receiver.recv().await.map(|block| (block, receiver))
                })),
                None => Box::pin(stream::pending()),
            };
            Ok(Box::pin(stream::iter(blocks).chain(tail)))
        }

        async fn fetch_blocks(
            &self,
            _peer: AuthorityIndex,
            block_refs: Vec<BlockRef>,
            _fetch_after_rounds: Vec<Round>,
            _fetch_missing_ancestors: bool,
            _timeout: Duration,
        ) -> ConsensusResult<Vec<Bytes>> {
            self.fetch_calls.lock().push(block_refs);
            Ok(self.fetchable.clone())
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

    /// Test fixture: a sender DAG seeded with one block per authority at
    /// `ancestor_round`, and `count` un-inflatable minimal blocks from `peer` at
    /// `ancestor_round + 1` referencing them.
    struct MinimalWireScenario {
        context: Arc<Context>,
        peer: AuthorityIndex,
        ancestors: Vec<VerifiedBlock>,
        blocks: Vec<VerifiedBlock>,
        wire: Vec<ExtendedSerializedBlock>,
    }

    fn minimal_wire_scenario(
        peer_index: usize,
        ancestor_round: Round,
        count: usize,
    ) -> MinimalWireScenario {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let peer = context.committee.to_authority_index(peer_index).unwrap();

        let sender_dag = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let mut ancestors = Vec::new();
        let mut ancestor_refs = Vec::new();
        for authority in 0..4u32 {
            let block =
                VerifiedBlock::new_for_test(TestBlock::new(ancestor_round, authority).build());
            ancestor_refs.push(block.reference());
            sender_dag.write().accept_block(block.clone());
            ancestors.push(block);
        }
        // Own ancestor first, as the verifier requires.
        ancestor_refs.sort_by_key(|r| (r.author != peer, r.author));
        let sender_inflater = BlockInflater::new(context.clone());
        let mut blocks = Vec::new();
        let mut wire = Vec::new();
        for i in 0..count {
            let block = VerifiedBlock::new_for_test(
                TestBlock::new(ancestor_round + 1, peer.value() as u32)
                    .set_ancestors_raw(ancestor_refs.clone())
                    .set_timestamp_ms(1000 + i as u64)
                    .build(),
            );
            wire.push(ExtendedSerializedBlock {
                block: block.serialized().clone(),
                minimal: Some(
                    sender_inflater
                        .serialize(&block, &sender_dag.read())
                        .unwrap(),
                ),
                excluded_ancestors: vec![],
            });
            blocks.push(block);
        }
        MinimalWireScenario {
            context,
            peer,
            ancestors,
            blocks,
            wire,
        }
    }

    fn empty_receiver_dag(context: &Arc<Context>) -> Arc<RwLock<DagState>> {
        Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )))
    }

    async fn wait_until(mut condition: impl FnMut() -> bool) {
        for _ in 0..200 {
            if condition() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("condition not reached in time");
    }

    /// The park-and-heal path end-to-end at the stream level: an un-inflatable minimal
    /// block is dropped without stalling or resetting the stream (the peer's next block
    /// is still inflated and delivered), and once its missing ancestors are accepted
    /// locally the recovery task inflates and submits it — no fetch at all.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn un_inflatable_block_parks_and_heals_on_acceptance() {
        let s = minimal_wire_scenario(2, 2, 1);
        // Block B: genesis ancestors resolve deterministically on any receiver.
        let genesis_refs: Vec<BlockRef> = genesis_blocks(&s.context)
            .iter()
            .map(|b| b.reference())
            .collect();
        let mut own_first_genesis = genesis_refs.clone();
        own_first_genesis.sort_by_key(|r| (r.author != s.peer, r.author));
        let block_b = VerifiedBlock::new_for_test(
            TestBlock::new(1, s.peer.value() as u32)
                .set_ancestors_raw(own_first_genesis)
                .build(),
        );
        let mut wire = s.wire.clone();
        wire.push(ExtendedSerializedBlock {
            block: block_b.serialized().clone(),
            minimal: None,
            excluded_ancestors: vec![],
        });
        let network_client = Arc::new(FixedStreamClient::new(wire));
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&s.context);
        let subscriber = Subscriber::new(
            s.context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag.clone(),
        );
        subscriber.subscribe(s.peer);

        // B flows through while A is parked; the stream neither stalls nor resets.
        wait_until(|| !authority_service.lock().handle_send_block.is_empty()).await;
        assert_eq!(
            s.context
                .metrics
                .node_metrics
                .minimal_block_recovery_parked
                .get(),
            1
        );
        // Land the missing ancestors in one atomic batch: the acceptance itself is the
        // wake, and the task heals by local re-inflation.
        receiver_dag.write().accept_blocks(s.ancestors.clone());
        wait_until(|| authority_service.lock().handle_send_block.len() >= 2).await;

        let received = authority_service.lock().handle_send_block.clone();
        assert!(received.iter().all(|(from, _)| *from == s.peer));
        let received_bytes: Vec<_> = received.iter().map(|(_, b)| &b.block).collect();
        assert!(received_bytes.contains(&block_b.serialized()));
        assert!(received_bytes.contains(&s.blocks[0].serialized()));
        assert!(network_client.fetch_calls.lock().is_empty());
        assert_eq!(*network_client.subscribe_calls.lock(), 1);
        let node_metrics = &s.context.metrics.node_metrics;
        wait_until(|| node_metrics.minimal_block_recovery_parked.get() == 0).await;
        assert_eq!(node_metrics.minimal_block_recovery_parked_bytes.get(), 0);
    }

    /// A quota overflow drops the block with its limit label and resets the stream so
    /// full replay redelivers it. Per-limit attribution across the three bounds is
    /// pinned at the unit level by quota_isolation_across_peers_and_rollback.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn quota_overflow_resets_stream_for_full_replay() {
        let s = minimal_wire_scenario(2, 2, 1);
        let mut client = FixedStreamClient::new(s.wire.clone());
        // Post-reset the sender replays full form (production behavior); without
        // this the same minimal re-overflows on every reconnect, and only the
        // escalating backoff — not a quiet stream — would pace the loop.
        client.blocks_after_reset = Some(vec![ExtendedSerializedBlock {
            block: s.blocks[0].serialized().clone(),
            minimal: None,
            excluded_ancestors: vec![],
        }]);
        let network_client = Arc::new(client);
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&s.context);
        let subscriber = Subscriber::new(
            s.context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag,
        )
        .with_recovery_limits(RecoveryLimits {
            global_bytes: 8,
            ..RecoveryLimits::default()
        });
        subscriber.subscribe(s.peer);

        let context = s.context.clone();
        wait_until(|| {
            context
                .metrics
                .node_metrics
                .minimal_block_quota_drops
                .with_label_values(&["global_bytes"])
                .get()
                >= 1
        })
        .await;
        // The caused-reset floor must actually pace the reconnect: paused time makes
        // the elapsed virtual duration deterministic.
        let dropped_at = tokio::time::Instant::now();
        wait_until(|| *network_client.subscribe_calls.lock() >= 2).await;
        assert!(
            dropped_at.elapsed() >= CAUSED_RESET_DELAY,
            "reconnect after a caused reset must wait out the floor delay"
        );
        // Nothing was admitted, so nothing is parked; the replayed full form arrives.
        assert_eq!(
            context
                .metrics
                .node_metrics
                .minimal_block_recovery_parked
                .get(),
            0
        );
        wait_until(|| !authority_service.lock().handle_send_block.is_empty()).await;
    }

    /// A quota-overflow stream reset must not cancel recovery tasks already parked:
    /// the JoinSet belongs to the subscription loop, not one stream instance.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn reconnect_does_not_cancel_existing_recovery_tasks() {
        let s = minimal_wire_scenario(2, 2, 3);
        let mut client = FixedStreamClient::new(s.wire.clone());
        // The reset replays only the overflowed third block in full form.
        client.blocks_after_reset = Some(vec![ExtendedSerializedBlock {
            block: s.blocks[2].serialized().clone(),
            minimal: None,
            excluded_ancestors: vec![],
        }]);
        let network_client = Arc::new(client);
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&s.context);
        let subscriber = Subscriber::new(
            s.context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag.clone(),
        )
        .with_recovery_limits(RecoveryLimits {
            peer_count: 2,
            ..RecoveryLimits::default()
        });
        subscriber.subscribe(s.peer);

        // Two blocks park, the third overflows and resets; its full form arrives.
        wait_until(|| !authority_service.lock().handle_send_block.is_empty()).await;
        let node_metrics = &s.context.metrics.node_metrics;
        assert_eq!(node_metrics.minimal_block_recovery_parked.get(), 2);
        assert!(*network_client.subscribe_calls.lock() >= 2);

        // The tasks parked before the reset still complete after it.
        receiver_dag.write().accept_blocks(s.ancestors.clone());
        wait_until(|| authority_service.lock().handle_send_block.len() >= 3).await;
        let received = authority_service.lock().handle_send_block.clone();
        for block in &s.blocks {
            assert!(
                received.iter().any(|(_, b)| b.block == *block.serialized()),
                "parked task should survive the reconnect"
            );
        }
        wait_until(|| node_metrics.minimal_block_recovery_parked.get() == 0).await;
        assert!(network_client.fetch_calls.lock().is_empty());
    }

    /// Malformed-minimal boundaries: an oversized payload counts as malformed, and
    /// the fourth malformed envelope in a session resets the stream.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn malformed_minimal_threshold_resets_stream() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let peer = context.committee.to_authority_index(2).unwrap();
        let oversize = (context.protocol_config.max_transactions_in_block_bytes() as usize) * 2 + 1;
        let mut wire = Vec::new();
        // Three garbage payloads + one oversized: all four count as malformed, and
        // the fourth crosses the threshold.
        for _ in 0..3 {
            wire.push(ExtendedSerializedBlock {
                block: Bytes::new(),
                minimal: Some(Bytes::from_static(b"\xff\xfe\xfd garbage")),
                excluded_ancestors: vec![],
            });
        }
        wire.push(ExtendedSerializedBlock {
            block: Bytes::new(),
            minimal: Some(Bytes::from(vec![0u8; oversize])),
            excluded_ancestors: vec![],
        });
        let mut client = FixedStreamClient::new(wire);
        // Quiet stream after the reset, so the malformed count stays at one pass.
        client.blocks_after_reset = Some(vec![]);
        let network_client = Arc::new(client);
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&context);
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag,
        );
        subscriber.subscribe(peer);

        wait_until(|| *network_client.subscribe_calls.lock() >= 2).await;
        let node_metrics = &context.metrics.node_metrics;
        let peer_hostname = context.committee.authority(peer).hostname.as_str();
        assert_eq!(
            node_metrics
                .minimal_block_inflate_drop
                .with_label_values(&[peer_hostname, "malformed"])
                .get(),
            4
        );
    }

    /// A minimal block claiming a round beyond the recovery horizon must not park —
    /// its wait would not be GC-bounded — and resets the stream for full replay.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn far_future_claim_resets_instead_of_parking() {
        // Claimed round 2001 vs receiver tip 1499: beyond 1499 + 500 cached rounds.
        let s = minimal_wire_scenario(2, 2000, 1);
        let mut client = FixedStreamClient::new(s.wire.clone());
        client.blocks_after_reset = Some(vec![ExtendedSerializedBlock {
            block: s.blocks[0].serialized().clone(),
            minimal: None,
            excluded_ancestors: vec![],
        }]);
        let network_client = Arc::new(client);
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&s.context);
        receiver_dag
            .write()
            .accept_block(VerifiedBlock::new_for_test(TestBlock::new(1499, 0).build()));
        let subscriber = Subscriber::new(
            s.context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag,
        );
        subscriber.subscribe(s.peer);

        let context = s.context.clone();
        wait_until(|| {
            context
                .metrics
                .node_metrics
                .minimal_block_quota_drops
                .with_label_values(&["round_horizon"])
                .get()
                >= 1
        })
        .await;
        wait_until(|| *network_client.subscribe_calls.lock() >= 2).await;
        // Nothing parked; the block arrived full via the replayed stream instead.
        assert_eq!(
            context
                .metrics
                .node_metrics
                .minimal_block_recovery_parked
                .get(),
            0
        );
        wait_until(|| !authority_service.lock().handle_send_block.is_empty()).await;
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

    // `last_received` must be recomputed from DagState before each connection attempt;
    // a value captured once at subscribe() time re-streams already-accepted blocks on
    // every reconnect.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn subscriber_recomputes_resume_round_on_reconnect() {
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
}
