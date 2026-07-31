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
use mysten_common::sync::notify_read::NotifyRead;
use mysten_metrics::spawn_monitored_task;
use parking_lot::{Mutex, RwLock};
use tokio::{
    sync::watch,
    task::{JoinHandle, JoinSet},
    time::sleep,
};
use tracing::{debug, error, info};

use crate::{
    block::{BlockAPI as _, Slot},
    block_inflater::BlockInflater,
    context::Context,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    minimal_block::InflateError,
    minimal_block_receive::{RecoveryQuotas, RepairLimits, recover_minimal_block},
    network::{ExtendedSerializedBlock, ValidatorNetworkClient, ValidatorNetworkService},
    task::{join_and_propagate_panic, reap_finished_task},
};

#[cfg(test)]
use crate::minimal_block_receive::RecoveryLimits;

/// Malformed minimal encodings tolerated per subscription session before the stream is
/// reset (reconnect backoff is the peer's penalty). Honest senders produce none.
const MAX_MALFORMED_PER_SUBSCRIPTION: u32 = 3;

/// Sender-side full-form grant bar: a subscriber resuming more than this many rounds
/// behind is served full form (see `authority_service`). Receive-side recovery no
/// longer keys off this margin — slot waits terminate through acceptance or GC at any
/// lag — but granting full form to deep laggards remains cheaper for both sides.
pub(crate) const PARKING_ROUND_MARGIN: Round = 32;

/// Outcome of processing one received block envelope before it is handed to the service.
enum InflateOutcome {
    /// A full-form block, or a minimal block successfully inflated to full form.
    Block(ExtendedSerializedBlock),
    /// A minimal block that local state cannot inflate right now. It is dropped from
    /// the stream and recovered by a per-block task that waits on the missing slot
    /// (see `minimal_block_receive`).
    Dropped { block_ref: BlockRef, minimal: Bytes },
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
    accepted_slots: Arc<NotifyRead<Slot, ()>>,
    gc_round: watch::Receiver<Round>,
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
        let recovery_quotas = RecoveryQuotas::new(context.clone());
        let repair_limits = RepairLimits::new(context.committee.size());
        // Neither the slot notifier nor the GC watch keep DagState alive: subscription
        // and recovery tasks may hold them across shutdown (authority_node fatally
        // asserts zero remaining DagState owners).
        let (accepted_slots, gc_round) = {
            let dag_state = dag_state.read();
            (
                dag_state.accepted_slot_notifier(),
                dag_state.gc_round_receiver(),
            )
        };
        Self {
            context,
            network_client,
            authority_service,
            dag_state,
            block_inflater,
            recovery_quotas,
            repair_limits,
            accepted_slots,
            gc_round,
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
        let accepted_slots = self.accepted_slots.clone();
        let gc_round = self.gc_round.clone();

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
            accepted_slots,
            gc_round,
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

        match block_inflater.inflate(&minimal, peer) {
            Ok((_signed_block, serialized)) => {
                node_metrics
                    .minimal_blocks_received_bytes_saved
                    .with_label_values(&[peer_hostname])
                    .inc_by(serialized.len().saturating_sub(minimal.len()) as u64);
                block.block = serialized;
                Ok(InflateOutcome::Block(block))
            }
            Err(InflateError::NeedFullBlock { block_ref, reason }) => {
                node_metrics
                    .minimal_block_inflate_drop
                    .with_label_values(&[peer_hostname, reason.label()])
                    .inc();
                // The recovery task re-derives the precise failure (missing slot,
                // ambiguity, digest mismatch) from its own inflation attempt and
                // routes accordingly: waits are for missing slots, everything else
                // escalates to the exact-reference repair.
                Ok(InflateOutcome::Dropped { block_ref, minimal })
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
        accepted_slots: Arc<NotifyRead<Slot, ()>>,
        gc_round: watch::Receiver<Round>,
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
                // Reap completed recovery tasks so the JoinSet stays bounded, and
                // propagate their panics like any other consensus task's.
                while let Some(result) = recoveries.try_join_next() {
                    if let Err(error) = result
                        && error.is_panic()
                    {
                        std::panic::resume_unwind(error.into_panic());
                    }
                }
                match blocks.next().await {
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
                        let block = match Self::inflate_received_block(
                            &context,
                            &block_inflater,
                            peer,
                            block,
                        ) {
                            // Dropped from the stream and owned by a recovery task.
                            Ok(InflateOutcome::Dropped { block_ref, minimal }) => {
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
                                            accepted_slots.clone(),
                                            gc_round.clone(),
                                            network_client.clone(),
                                            authority_service.clone(),
                                            repair_limits.clone(),
                                            peer,
                                            block_ref,
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
                                        continue 'subscription;
                                    }
                                }
                            }
                            Ok(InflateOutcome::Block(block)) => block,
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

    fn minimal_wire_scenario(peer_index: usize, ancestor_round: Round, count: usize) -> MinimalWireScenario {
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
            let block = VerifiedBlock::new_for_test(
                TestBlock::new(ancestor_round, authority).build(),
            );
            ancestor_refs.push(block.reference());
            sender_dag.write().accept_block(block.clone());
            ancestors.push(block);
        }
        // Own ancestor first, as the verifier requires.
        ancestor_refs.sort_by_key(|r| (r.author != peer, r.author));
        let sender_inflater = BlockInflater::new(context.clone(), sender_dag.clone());
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
                minimal: Some(sender_inflater.serialize(&block).unwrap()),
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
        assert_eq!(
            node_metrics
                .minimal_block_recoveries
                .with_label_values(&["inflated"])
                .get(),
            1
        );
    }

    /// Forced capacity 1-4: overflow at each cap resets the stream, the full-form
    /// replay redelivers everything, and once the parked tasks heal, permits and
    /// gauges return to zero.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn forced_recovery_capacity_one_to_four_makes_progress() {
        for cap in 1..=4usize {
            let s = minimal_wire_scenario(2, 2, cap + 1);
            let mut client = FixedStreamClient::new(s.wire.clone());
            // After the overflow reset, the sender replays the full forms.
            client.blocks_after_reset = Some(
                s.blocks
                    .iter()
                    .map(|block| ExtendedSerializedBlock {
                        block: block.serialized().clone(),
                        minimal: None,
                        excluded_ancestors: vec![],
                    })
                    .collect(),
            );
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
                peer_count: cap,
                ..RecoveryLimits::default()
            });
            subscriber.subscribe(s.peer);

            // The cap+1-th un-inflatable block overflows, resets the stream, and the
            // replay delivers every block in full form.
            wait_until(|| authority_service.lock().handle_send_block.len() >= cap + 1).await;
            let node_metrics = &s.context.metrics.node_metrics;
            assert_eq!(
                node_metrics
                    .minimal_block_quota_drops
                    .with_label_values(&["peer_count"])
                    .get(),
                1,
                "cap {cap}"
            );
            assert!(*network_client.subscribe_calls.lock() >= 2, "cap {cap}");
            let received = authority_service.lock().handle_send_block.clone();
            for block in &s.blocks {
                assert!(
                    received.iter().any(|(_, b)| &b.block == block.serialized()),
                    "cap {cap}: replayed block missing"
                );
            }
            // Heal the parked tasks; all permits must come back.
            receiver_dag.write().accept_blocks(s.ancestors.clone());
            wait_until(|| node_metrics.minimal_block_recovery_parked.get() == 0).await;
            assert_eq!(node_metrics.minimal_block_recovery_parked_bytes.get(), 0);
            assert!(network_client.fetch_calls.lock().is_empty(), "cap {cap}");
        }
    }

    /// Each quota bound (global bytes, per-peer bytes, per-peer count) drops with its
    /// own metric label and resets the stream immediately.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn quota_overflow_resets_stream_for_full_replay() {
        let cases: [(&str, RecoveryLimits); 3] = [
            (
                "global_bytes",
                RecoveryLimits {
                    global_bytes: 8,
                    ..RecoveryLimits::default()
                },
            ),
            (
                "peer_bytes",
                RecoveryLimits {
                    peer_bytes: 8,
                    ..RecoveryLimits::default()
                },
            ),
            (
                "peer_count",
                RecoveryLimits {
                    peer_count: 0,
                    ..RecoveryLimits::default()
                },
            ),
        ];
        for (label, limits) in cases {
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
            .with_recovery_limits(limits);
            subscriber.subscribe(s.peer);

            let context = s.context.clone();
            wait_until(|| {
                context
                    .metrics
                    .node_metrics
                    .minimal_block_quota_drops
                    .with_label_values(&[label])
                    .get()
                    >= 1
            })
            .await;
            wait_until(|| *network_client.subscribe_calls.lock() >= 2).await;
            // Nothing was admitted, so nothing is parked.
            assert_eq!(
                context.metrics.node_metrics.minimal_block_recovery_parked.get(),
                0
            );
        }
    }

    /// Sequential park-heal cycles through one permit: completed tasks release their
    /// quota for reuse and the reaped JoinSet does not accumulate. No drop is ever
    /// recorded despite capacity one.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn completed_recovery_tasks_are_reaped() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let peer = context.committee.to_authority_index(2).unwrap();
        let sender_dag = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let sender_inflater = BlockInflater::new(context.clone(), sender_dag.clone());

        let (live_tx, live_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut client = FixedStreamClient::new(vec![]);
        client.live = Mutex::new(Some(live_rx));
        let network_client = Arc::new(client);
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&context);
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag.clone(),
        )
        .with_recovery_limits(RecoveryLimits {
            peer_count: 1,
            ..RecoveryLimits::default()
        });
        subscriber.subscribe(peer);

        let node_metrics = &context.metrics.node_metrics;
        for cycle in 0..5u32 {
            // Fresh ancestor slots per cycle so waits never alias across cycles.
            let ancestor_round = 2 + 2 * cycle;
            let mut ancestors = Vec::new();
            let mut ancestor_refs = Vec::new();
            for authority in 0..4u32 {
                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(ancestor_round, authority).build(),
                );
                ancestor_refs.push(block.reference());
                sender_dag.write().accept_block(block.clone());
                ancestors.push(block);
            }
            ancestor_refs.sort_by_key(|r| (r.author != peer, r.author));
            let block = VerifiedBlock::new_for_test(
                TestBlock::new(ancestor_round + 1, peer.value() as u32)
                    .set_ancestors_raw(ancestor_refs)
                    .build(),
            );
            live_tx
                .send(ExtendedSerializedBlock {
                    block: block.serialized().clone(),
                    minimal: Some(sender_inflater.serialize(&block).unwrap()),
                    excluded_ancestors: vec![],
                })
                .unwrap();
            wait_until(|| node_metrics.minimal_block_recovery_parked.get() == 1).await;
            receiver_dag.write().accept_blocks(ancestors);
            wait_until(|| node_metrics.minimal_block_recovery_parked.get() == 0).await;
            wait_until(|| {
                authority_service.lock().handle_send_block.len() >= (cycle + 1) as usize
            })
            .await;
        }
        // One permit served all five cycles without a single quota drop or reset.
        assert_eq!(
            node_metrics
                .minimal_block_quota_drops
                .with_label_values(&["peer_count"])
                .get(),
            0
        );
        assert_eq!(*network_client.subscribe_calls.lock(), 1);
        assert_eq!(node_metrics.minimal_block_recovery_parked_bytes.get(), 0);
    }

    /// A deep laggard needs no mode machinery: live minimal blocks it cannot use fill
    /// the quota, the overflow resets the stream, and the sender's full-form replay
    /// carries it forward.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn laggard_recovers_via_full_replay_without_catchup_mode() {
        let s = minimal_wire_scenario(2, 1999, 3);
        let mut client = FixedStreamClient::new(s.wire.clone());
        client.blocks_after_reset = Some(
            s.blocks
                .iter()
                .map(|block| ExtendedSerializedBlock {
                    block: block.serialized().clone(),
                    minimal: None,
                    excluded_ancestors: vec![],
                })
                .collect(),
        );
        let network_client = Arc::new(client);
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&s.context);
        // The receiver is ~500 rounds behind the tip.
        receiver_dag
            .write()
            .accept_block(VerifiedBlock::new_for_test(TestBlock::new(1499, 0).build()));
        let subscriber = Subscriber::new(
            s.context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag,
        )
        .with_recovery_limits(RecoveryLimits {
            peer_count: 2,
            ..RecoveryLimits::default()
        });
        subscriber.subscribe(s.peer);

        // Overflow at the third tip minimal; the replay then delivers all three full.
        wait_until(|| authority_service.lock().handle_send_block.len() >= 3).await;
        let received = authority_service.lock().handle_send_block.clone();
        for block in &s.blocks {
            assert!(
                received.iter().any(|(_, b)| &b.block == block.serialized()),
                "full replay should deliver the tip range"
            );
        }
        assert!(*network_client.subscribe_calls.lock() >= 2);
        assert!(network_client.fetch_calls.lock().is_empty());
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
                received.iter().any(|(_, b)| &b.block == block.serialized()),
                "parked task should survive the reconnect"
            );
        }
        wait_until(|| node_metrics.minimal_block_recovery_parked.get() == 0).await;
        assert!(network_client.fetch_calls.lock().is_empty());
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
