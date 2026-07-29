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
use tokio::{sync::Semaphore, task::JoinHandle, time::sleep};
use tracing::{debug, error, info};

use crate::{
    block::{BlockAPI as _, VerifiedBlock},
    block_inflater::BlockInflater,
    context::Context,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    minimal_block::InflateError,
    network::{ExtendedSerializedBlock, ValidatorNetworkClient, ValidatorNetworkService},
    task::{join_and_propagate_panic, reap_finished_task},
};

/// Malformed minimal encodings tolerated per subscription session before the stream is
/// reset (reconnect backoff is the peer's penalty). Honest senders produce none.
const MAX_MALFORMED_PER_SUBSCRIPTION: u32 = 3;

/// In-flight off-stream recovery fetches allowed per peer. Recovery beyond this bound is
/// skipped: later blocks from the same peer keep spawning recoveries, and blocks that do
/// inflate register the gap with block_manager, so a lost recovery is never load-bearing.
const MAX_INFLIGHT_RECOVERIES: usize = 8;

/// Timeout for one off-stream recovery fetch. Generous because nothing waits on it: the
/// subscription stream has already moved on.
const RECOVERY_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Delays between local inflation re-attempts for a dropped block, prior to any network
/// fetch (attempts land ~10/30/60/120 ms after the drop). A missing ancestor is almost
/// always simply in flight on its own stream: at production round cadence it lands within
/// tens of milliseconds, so cheap local retries resolve the vast majority of drops. Live
/// measurement behind this: ~47% of received minimal blocks hit an in-flight ancestor,
/// and fetching each one cost a full-block round trip that fed into commit tail latency.
/// The total local window stays ~2 rounds so a genuinely unavailable block is not fetched
/// too late; tune against the drop-to-submission latency histogram.
const RECOVERY_INFLATE_RETRY_DELAYS: [Duration; 4] = [
    Duration::from_millis(10),
    Duration::from_millis(20),
    Duration::from_millis(30),
    Duration::from_millis(60),
];

/// Outcome of processing one received block envelope before it is handed to the service.
enum InflateOutcome {
    /// A full-form block, or a minimal block successfully inflated to full form.
    Block(ExtendedSerializedBlock),
    /// A minimal block that local state cannot inflate right now. It is dropped from the
    /// stream; the retained encoding and claimed ref drive off-stream recovery.
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

    /// Replaces a minimal-form block with its inflated full serialization.
    ///
    /// Returns `Dropped` when the block cannot be inflated from local state; the caller
    /// continues the stream immediately and recovers the block off-stream (see
    /// `recover_dropped_block`). Dropping without recovery is not an option: a receiver
    /// that falls a round behind finds every subsequent block from every peer
    /// un-inflatable (each references blocks it lacks), nothing reaches `block_manager`,
    /// so no missing ancestor is ever registered and the node wedges permanently.
    ///
    /// This path must never wait on the network. Blocks from a peer are processed
    /// sequentially, so any await here delays every later block from that peer —
    /// head-of-line blocking. An earlier version fetched the full block inline (up to a
    /// 2 s timeout) and slept between inflate retries; on high-latency links that
    /// compounded into a throughput collapse, because a slower stream falls further
    /// behind on ancestors, which causes more stalls.
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

        // Cap the envelope before decoding: a legitimate minimal block is bounded by the
        // max transaction payload plus small per-ancestor structure, so anything larger
        // is a peer fault and must not cost a BCS parse of attacker-sized buffers.
        let max_minimal_size =
            (context.protocol_config.max_transactions_in_block_bytes() as usize).saturating_mul(2);
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

    /// Recovers a dropped minimal block off the subscription stream: first by re-trying
    /// local inflation on a short schedule (the missing ancestor is almost always in
    /// flight on its own stream and lands within tens of milliseconds), then — only if
    /// local state still cannot resolve it — by fetching the full form from the sending
    /// peer. Either way the block is submitted through the normal `handle_send_block`
    /// path and enters `block_manager` like any other, so residual gaps suspend and the
    /// regular missing-ancestor sync takes over recursively.
    ///
    /// Sleeping here is safe precisely because this task is off the stream; the same
    /// sleeps on the stream path caused the Run 1 regional collapse.
    ///
    /// The fetch targets the sender only: the stream carries only the peer's own
    /// proposals, so the sender is the author and must hold the full block, and an
    /// unverified claimed ref is never injected into the shared sync machinery. The
    /// response must hash to the claimed digest, so the peer cannot substitute a
    /// different block.
    async fn recover_dropped_block(
        context: Arc<Context>,
        block_inflater: Arc<BlockInflater>,
        network_client: Arc<C>,
        authority_service: Weak<S>,
        peer: AuthorityIndex,
        block_ref: BlockRef,
        minimal: Bytes,
    ) {
        let dropped_at = std::time::Instant::now();
        let peer_hostname = context.committee.authority(peer).hostname.as_str();
        let node_metrics = &context.metrics.node_metrics;
        let recovery_metric = &node_metrics.minimal_block_recovery;

        let mut recovered: Option<(Bytes, &'static str, &'static str)> = None;
        const ATTEMPTS: [&str; RECOVERY_INFLATE_RETRY_DELAYS.len()] = ["1", "2", "3", "4"];
        for (index, delay) in RECOVERY_INFLATE_RETRY_DELAYS.into_iter().enumerate() {
            let attempt = ATTEMPTS[index];
            sleep(delay).await;
            match block_inflater.inflate(&minimal, peer) {
                Ok((_signed_block, serialized)) => {
                    node_metrics
                        .minimal_blocks_received_bytes_saved
                        .with_label_values(&[peer_hostname])
                        .inc_by(serialized.len().saturating_sub(minimal.len()) as u64);
                    recovered = Some((serialized, "inflated_late", attempt));
                    break;
                }
                Err(InflateError::NeedFullBlock { .. }) => continue,
                // Structurally invalid bytes cannot become valid; the stream path has
                // already counted the peer fault.
                Err(InflateError::Malformed(_)) => return,
            }
        }
        if recovered.is_none() {
            let result = network_client
                .fetch_blocks(peer, vec![block_ref], vec![], false, RECOVERY_FETCH_TIMEOUT)
                .await;
            let serialized = match result {
                Ok(blocks) => blocks
                    .into_iter()
                    .find(|bytes| VerifiedBlock::compute_digest(bytes) == block_ref.digest),
                Err(e) => {
                    debug!(
                        "Recovery fetch of dropped block {} from peer {} {} failed: {}",
                        block_ref, peer, peer_hostname, e
                    );
                    recovery_metric
                        .with_label_values(&[peer_hostname, "fetch_failed", "fetch"])
                        .inc();
                    return;
                }
            };
            let Some(serialized) = serialized else {
                // The author could not produce its own claimed block: a peer fault.
                recovery_metric
                    .with_label_values(&[peer_hostname, "digest_mismatch", "fetch"])
                    .inc();
                return;
            };
            recovered = Some((serialized, "fetch_recovered", "fetch"));
        }

        let Some((serialized, result, attempt)) = recovered else {
            return;
        };
        let Some(authority_service) = authority_service.upgrade() else {
            return;
        };
        let block = ExtendedSerializedBlock {
            block: serialized,
            excluded_ancestors: vec![],
            minimal: None,
        };
        // A service rejection is counted as such, not as success. Ok covers accepted,
        // suspended, and duplicate blocks alike (block_manager dedups), so this measures
        // delivery, not acceptance.
        let outcome = match authority_service.handle_send_block(peer, block).await {
            Ok(()) => result,
            Err(_) => "rejected",
        };
        recovery_metric
            .with_label_values(&[peer_hostname, outcome, attempt])
            .inc();
        node_metrics
            .minimal_block_recovery_latency
            .observe(dropped_at.elapsed().as_secs_f64());
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
        // Bounds concurrent off-stream recovery fetches to this peer, across reconnects.
        let recovery_permits = Arc::new(Semaphore::new(MAX_INFLIGHT_RECOVERIES));
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
                            peer,
                            block,
                        ) {
                            // Dropped from the stream, recovered off it. The stream
                            // itself is healthy — it delivered a well-formed block — so
                            // the reconnect backoff resets as for an accepted block.
                            // (Malformed errors below deliberately do NOT reset:
                            // escalating backoff is the penalty for a misbehaving peer.)
                            Ok(InflateOutcome::Dropped { block_ref, minimal }) => {
                                retries = 0;
                                backoff.reset();
                                match recovery_permits.clone().try_acquire_owned() {
                                    Ok(permit) => {
                                        let context = context.clone();
                                        let block_inflater = block_inflater.clone();
                                        let network_client = network_client.clone();
                                        // Weak: a detached recovery task must not keep the
                                        // service (and through it DagState) alive across
                                        // an authority shutdown.
                                        let authority_service = Arc::downgrade(&authority_service);
                                        spawn_monitored_task!(async move {
                                            let _permit = permit;
                                            Self::recover_dropped_block(
                                                context,
                                                block_inflater,
                                                network_client,
                                                authority_service,
                                                peer,
                                                block_ref,
                                                minimal,
                                            )
                                            .await;
                                        });
                                    }
                                    Err(_) => {
                                        // Saturated: skip. Later drops keep spawning
                                        // recoveries, and any block that does inflate
                                        // registers the gap with block_manager, so no
                                        // single recovery is load-bearing.
                                        context
                                            .metrics
                                            .node_metrics
                                            .minimal_block_recovery
                                            .with_label_values(&[
                                                peer_hostname.as_str(),
                                                "saturated",
                                                "none",
                                            ])
                                            .inc();
                                    }
                                }
                                continue 'stream;
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

    /// Serves a fixed block sequence, then holds the stream open. `fetchable` seeds the
    /// blocks the peer can serve to recovery fetches, like the author's store would.
    struct FixedStreamClient {
        blocks: Vec<ExtendedSerializedBlock>,
        fetchable: Vec<Bytes>,
        subscribe_calls: Mutex<usize>,
        fetch_calls: Mutex<Vec<Vec<BlockRef>>>,
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
            *self.subscribe_calls.lock() += 1;
            Ok(Box::pin(
                stream::iter(self.blocks.clone()).chain(stream::pending()),
            ))
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

    /// The drop-and-recover path end-to-end at the stream level: an un-inflatable minimal
    /// block is dropped without stalling or resetting the stream (the peer's next block is
    /// still inflated and delivered), and the dropped block itself arrives through the
    /// off-stream recovery fetch. On-stream stalls were the Run 1 incident; dropping
    /// without recovery wedges any receiver that falls a round behind.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn un_inflatable_block_dropped_and_stream_continues() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let peer = context.committee.to_authority_index(2).unwrap();

        // Sender-side state: round-2 blocks from all authorities, which the receiver
        // will NOT have.
        let sender_dag = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let genesis_refs: Vec<BlockRef> = crate::block::genesis_blocks(&context)
            .iter()
            .map(|b| b.reference())
            .collect();
        let mut round2_refs = Vec::new();
        for authority in 0..4u32 {
            let block = VerifiedBlock::new_for_test(
                crate::block::TestBlock::new(2, authority)
                    .set_ancestors_raw(genesis_refs.clone())
                    .build(),
            );
            round2_refs.push(block.reference());
            sender_dag.write().accept_block(block);
        }
        round2_refs.sort_by_key(|r| (r.author != peer, r.author));
        // Keep `sender_dag` alive: the inflater holds DagState weakly, and a dead resolver
        // silently degrades the encoding to all-explicit digests, voiding the scenario.
        let sender_inflater = BlockInflater::new(context.clone(), sender_dag.clone());

        // Block A: references round-2 ancestors the receiver lacks => un-inflatable there.
        let block_a = VerifiedBlock::new_for_test(
            crate::block::TestBlock::new(3, peer.value() as u32)
                .set_ancestors_raw(round2_refs)
                .build(),
        );
        // Block B: genesis ancestors resolve deterministically on any receiver.
        let mut own_first_genesis = genesis_refs.clone();
        own_first_genesis.sort_by_key(|r| (r.author != peer, r.author));
        let block_b = VerifiedBlock::new_for_test(
            crate::block::TestBlock::new(1, peer.value() as u32)
                .set_ancestors_raw(own_first_genesis)
                .build(),
        );
        let to_wire = |block: &VerifiedBlock| ExtendedSerializedBlock {
            block: block.serialized().clone(),
            minimal: Some(sender_inflater.serialize(block, 0).unwrap()),
            excluded_ancestors: vec![],
        };

        let network_client = Arc::new(FixedStreamClient {
            blocks: vec![to_wire(&block_a), to_wire(&block_b)],
            fetchable: vec![block_a.serialized().clone()],
            subscribe_calls: Mutex::new(0),
            fetch_calls: Mutex::new(Vec::new()),
        });
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        // Receiver has nothing but genesis.
        let receiver_dag = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag,
        );
        subscriber.subscribe(peer);

        for _ in 0..10 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if authority_service.lock().handle_send_block.len() >= 2 {
                break;
            }
        }

        // A was dropped from the stream and recovered off it; B flowed through directly.
        // Both reach the service in full inflated form, and the stream never reconnected —
        // proof the drop neither stalled nor reset it.
        let received = authority_service.lock().handle_send_block.clone();
        assert_eq!(received.len(), 2);
        assert!(received.iter().all(|(from, _)| *from == peer));
        let received_bytes: Vec<_> = received.iter().map(|(_, b)| &b.block).collect();
        assert!(received_bytes.contains(&block_b.serialized()));
        assert!(received_bytes.contains(&block_a.serialized()));
        assert_eq!(*network_client.subscribe_calls.lock(), 1);
        assert_eq!(
            network_client.fetch_calls.lock().as_slice(),
            &[vec![block_a.reference()]]
        );
        let peer_hostname = &context.committee.authority(peer).hostname;
        let node_metrics = &context.metrics.node_metrics;
        assert_eq!(
            node_metrics
                .minimal_block_inflate_drop
                .with_label_values(&[peer_hostname.as_str(), "missing_ancestor"])
                .get(),
            1
        );
        // Ancestors never arrive locally, so recovery exhausted the local retries and
        // escalated to the digest-verified sender fetch.
        assert_eq!(
            node_metrics
                .minimal_block_recovery
                .with_label_values(&[peer_hostname.as_str(), "fetch_recovered", "fetch"])
                .get(),
            1
        );
    }

    /// The common production case: the ancestor a dropped block was missing arrives on
    /// its own stream milliseconds later, so recovery resolves by local re-inflation —
    /// no network fetch at all. This is what keeps recovery traffic near zero (live
    /// measurement: ~47% of received blocks drop on an in-flight ancestor).
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn dropped_block_inflates_late_without_fetch() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let peer = context.committee.to_authority_index(2).unwrap();

        let sender_dag = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let genesis_refs: Vec<BlockRef> = crate::block::genesis_blocks(&context)
            .iter()
            .map(|b| b.reference())
            .collect();
        let mut round2_blocks = Vec::new();
        let mut round2_refs = Vec::new();
        for authority in 0..4u32 {
            let block = VerifiedBlock::new_for_test(
                crate::block::TestBlock::new(2, authority)
                    .set_ancestors_raw(genesis_refs.clone())
                    .build(),
            );
            round2_refs.push(block.reference());
            sender_dag.write().accept_block(block.clone());
            round2_blocks.push(block);
        }
        round2_refs.sort_by_key(|r| (r.author != peer, r.author));
        let sender_inflater = BlockInflater::new(context.clone(), sender_dag.clone());

        let block_a = VerifiedBlock::new_for_test(
            crate::block::TestBlock::new(3, peer.value() as u32)
                .set_ancestors_raw(round2_refs)
                .build(),
        );
        let network_client = Arc::new(FixedStreamClient {
            blocks: vec![ExtendedSerializedBlock {
                block: block_a.serialized().clone(),
                minimal: Some(sender_inflater.serialize(&block_a, 0).unwrap()),
                excluded_ancestors: vec![],
            }],
            fetchable: vec![],
            subscribe_calls: Mutex::new(0),
            fetch_calls: Mutex::new(Vec::new()),
        });
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag.clone(),
        );
        subscriber.subscribe(peer);

        // Let the stream deliver and drop A (its ancestors are unknown), then land the
        // ancestors locally before the first 10 ms recovery retry fires.
        tokio::time::sleep(Duration::from_millis(2)).await;
        for block in round2_blocks {
            receiver_dag.write().accept_block(block);
        }
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if !authority_service.lock().handle_send_block.is_empty() {
                break;
            }
        }

        let received = authority_service.lock().handle_send_block.clone();
        assert_eq!(received.len(), 1);
        assert_eq!(&received[0].1.block, block_a.serialized());
        // No fetch: the block healed by local re-inflation on the first retry.
        assert!(network_client.fetch_calls.lock().is_empty());
        let peer_hostname = &context.committee.authority(peer).hostname;
        assert_eq!(
            context
                .metrics
                .node_metrics
                .minimal_block_recovery
                .with_label_values(&[peer_hostname.as_str(), "inflated_late", "1"])
                .get(),
            1
        );
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
}
