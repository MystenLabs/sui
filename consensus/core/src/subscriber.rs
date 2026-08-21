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
use tokio::{
    sync::broadcast,
    task::JoinHandle,
    time::{sleep, timeout},
};
use tracing::{debug, error, info};

use crate::{
    block::{BlockAPI as _, VerifiedBlock},
    context::Context,
    dag_state::DagState,
    error::ConsensusError,
    network::{SerializedBlockForm, ValidatorNetworkClient, ValidatorNetworkService},
    pending_reconstructions::{
        PendingReconstructions, ReadyEntry, SlimClaim, run_reconstruction_worker,
    },
    seen_digests::SeenDigests,
    slim_block::{DecodeError, FallbackReason, SlimBlockCodec},
    task::{join_and_propagate_panic, reap_finished_task},
};

/// Bounds both establishing a subscription and waiting for the next block on it, so the
/// subscription is abandoned and retried when either makes no progress for this long. A healthy
/// peer proposes blocks multiple times per second, and (re)subscribing to a peer that has proposed
/// before immediately yields at least its last proposed block, so timeouts and reconnections stay
/// rare unless the peer is not proposing. This primarily guards against subscriptions that stop
/// making progress without surfacing a transport error, e.g. a peer whose runtime stalls while its
/// connections stay open. Kept well above the expected gap between proposals, because a peer that
/// is reachable but not proposing gets resubscribed to on every timeout.
const SUBSCRIPTION_TIMEOUT: Duration = Duration::from_secs(30);

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
    slim_block_codec: Arc<SlimBlockCodec>,
    seen_digests: Arc<SeenDigests>,
    pending_reconstructions: Arc<Mutex<PendingReconstructions>>,
    reconciled_tx: tokio::sync::mpsc::UnboundedSender<Vec<ReadyEntry>>,
    reconstruction_worker: Mutex<Option<JoinHandle<()>>>,
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
        accepted_blocks: broadcast::Receiver<VerifiedBlock>,
    ) -> Self {
        let slim_block_codec = Arc::new(SlimBlockCodec::new(context.clone()));
        let seen_digests = Arc::new(SeenDigests::new(context.clone()));
        let pending_reconstructions =
            Arc::new(Mutex::new(PendingReconstructions::new(context.clone())));
        let (reconciled_tx, reconciled_rx) = tokio::sync::mpsc::unbounded_channel();
        // spawn_monitored_task! wraps its body in an async-move block, so hand it
        // owned values rather than expressions over the constructor's locals.
        let worker_context = context.clone();
        let worker_codec = slim_block_codec.clone();
        let worker_dag_state = Arc::downgrade(&dag_state);
        let worker_service = Arc::downgrade(&authority_service);
        let worker_pending = pending_reconstructions.clone();
        let worker_seen = seen_digests.clone();
        let reconstruction_worker = spawn_monitored_task!(run_reconstruction_worker(
            worker_context,
            worker_codec,
            worker_dag_state,
            worker_service,
            worker_pending,
            worker_seen,
            accepted_blocks,
            reconciled_rx,
        ));
        let subscriptions = (0..context.committee.size())
            .map(|_| None)
            .collect::<Vec<_>>();
        Self {
            context,
            network_client,
            authority_service,
            dag_state,
            slim_block_codec,
            seen_digests,
            pending_reconstructions,
            reconciled_tx,
            reconstruction_worker: Mutex::new(Some(reconstruction_worker)),
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
        let slim_block_codec = self.slim_block_codec.clone();
        let seen_digests = self.seen_digests.clone();
        let pending_reconstructions = self.pending_reconstructions.clone();
        let reconciled_tx = self.reconciled_tx.clone();

        let mut subscriptions = self.subscriptions.lock();
        self.unsubscribe_locked(peer, &mut subscriptions[peer.value()]);
        subscriptions[peer.value()] = Some(spawn_monitored_task!(Self::subscription_loop(
            context,
            network_client,
            authority_service,
            dag_state,
            slim_block_codec,
            seen_digests,
            pending_reconstructions,
            reconciled_tx,
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

        let worker = self.reconstruction_worker.lock().take();
        if let Some(worker) = worker {
            worker.abort();
            join_and_propagate_panic(worker).await;
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

    async fn subscription_loop(
        context: Arc<Context>,
        network_client: Arc<C>,
        authority_service: Weak<S>,
        dag_state: Weak<RwLock<DagState>>,
        slim_block_codec: Arc<SlimBlockCodec>,
        seen_digests: Arc<SeenDigests>,
        pending_reconstructions: Arc<Mutex<PendingReconstructions>>,
        reconciled_tx: tokio::sync::mpsc::UnboundedSender<Vec<ReadyEntry>>,
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

            let last_accepted: Round = {
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
            // `request_timeout` only bounds acquiring the channel, and the channel is usually
            // cached, so establishing the stream can otherwise block indefinitely waiting for the
            // peer's response headers, e.g. when the peer accepts connections but its runtime is
            // stalled. Bound it here rather than with a gRPC deadline on the request, which would
            // cap the lifetime of the whole subscription.
            let subscribe = timeout(
                SUBSCRIPTION_TIMEOUT,
                network_client.subscribe_blocks(peer, last_accepted, request_timeout),
            )
            .await;
            let mut blocks = match subscribe {
                Ok(Ok(blocks)) => {
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
                Ok(Err(e)) => {
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
                Err(_) => {
                    debug!(
                        "Timed out subscribing to blocks from peer {} {} after {:?}",
                        peer, peer_hostname, SUBSCRIPTION_TIMEOUT
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

            'stream: loop {
                match timeout(SUBSCRIPTION_TIMEOUT, blocks.next()).await {
                    Ok(Some(block)) => {
                        context
                            .metrics
                            .node_metrics
                            .subscribed_blocks
                            .with_label_values(&[peer_hostname])
                            .inc();
                        let Some(authority_service) = authority_service.upgrade() else {
                            return;
                        };
                        let mut block = block;
                        // A slim payload is rebuilt into the full form here, against
                        // local state. One that cannot be rebuilt is dropped rather
                        // than passed on as an empty payload downstream would reject
                        // as a peer fault -- the usual cause is an ancestor this node
                        // has not accepted yet -- and is attributed by kind.
                        let deliver = match &block.block {
                            SerializedBlockForm::Full(_) => true,
                            SerializedBlockForm::Slim(slim) => {
                                let slim = slim.clone();
                                let Some(dag_state) = dag_state.upgrade() else {
                                    return;
                                };
                                let node_metrics = &context.metrics.node_metrics;
                                node_metrics
                                    .slim_blocks_received
                                    .with_label_values(&[peer_hostname])
                                    .inc();
                                match slim_block_codec.decode(
                                    &slim,
                                    peer,
                                    &dag_state,
                                    &seen_digests,
                                ) {
                                    Ok((_signed, serialized)) => {
                                        block.block = SerializedBlockForm::Full(serialized);
                                        true
                                    }
                                    Err(error) => {
                                        node_metrics
                                            .slim_block_decode_failures
                                            .with_label_values(&[
                                                peer_hostname,
                                                error.metric_label(),
                                            ])
                                            .inc();
                                        // A block we could not rebuild still tells us
                                        // its own digest, and `parse_slim` has
                                        // already bound its author to this peer. Record
                                        // it so blocks referencing this slot rebuild
                                        // without waiting for us to accept it -- a
                                        // Malformed payload carries no ref worth
                                        // trusting, so it records nothing.
                                        if let DecodeError::NeedFullBlock { block_ref, .. } = &error
                                        {
                                            let gc_round = dag_state.read().gc_round();
                                            seen_digests.observe(*block_ref, gc_round);
                                        }
                                        match &error {
                                            DecodeError::Malformed(_) => info!(
                                                "Malformed slim block from peer {} {}: {}",
                                                peer, peer_hostname, error
                                            ),
                                            DecodeError::NeedFullBlock {
                                                block_ref,
                                                reason: FallbackReason::MissingAncestors(slots),
                                            } => {
                                                let (gc_round, local_round) = {
                                                    let guard = dag_state.read();
                                                    (
                                                        guard.gc_round(),
                                                        guard.highest_accepted_round(),
                                                    )
                                                };
                                                let claim = SlimClaim {
                                                    block_ref: *block_ref,
                                                    slim: slim.clone(),
                                                    excluded_ancestors: block
                                                        .excluded_ancestors
                                                        .clone(),
                                                    peer,
                                                    missing: slots.clone(),
                                                };
                                                let missing = claim.missing.clone();
                                                match pending_reconstructions.lock().try_admit(
                                                    claim,
                                                    gc_round,
                                                    local_round,
                                                ) {
                                                    Ok(()) => {
                                                        node_metrics
                                                            .slim_block_park_outcomes
                                                            .with_label_values(&["parked"])
                                                            .inc();
                                                        // An acceptance between decode and
                                                        // admission has already passed the
                                                        // worker's broadcast; re-check so the
                                                        // entry cannot sleep through it.
                                                        let filled: Vec<_> = {
                                                            let guard = dag_state.read();
                                                            missing
                                                                .iter()
                                                                .flat_map(|slot| {
                                                                    guard
                                                                        .get_uncommitted_blocks_at_slot(*slot)
                                                                        .into_iter()
                                                                        .map(|b| b.reference())
                                                                        .take(1)
                                                                })
                                                                .collect()
                                                        };
                                                        if !filled.is_empty() {
                                                            let ready = pending_reconstructions
                                                                .lock()
                                                                .on_blocks_accepted(&filled);
                                                            if !ready.is_empty() {
                                                                let _ = reconciled_tx.send(ready);
                                                            }
                                                        }
                                                    }
                                                    Err(refusal) => node_metrics
                                                        .slim_block_park_refusals
                                                        .with_label_values(
                                                            &[refusal.metric_label()],
                                                        )
                                                        .inc(),
                                                }
                                            }
                                            DecodeError::NeedFullBlock { .. } => debug!(
                                                "Cannot rebuild block from peer {} {}: {}",
                                                peer, peer_hostname, error
                                            ),
                                        }
                                        false
                                    }
                                }
                            }
                        };
                        if deliver {
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
                        }
                        // Reset the retry counter and backoff when a block is received, so a peer
                        // that recovers after flapping reconnects promptly instead of inheriting
                        // the previously escalated delay.
                        retries = 0;
                        backoff.reset();
                    }
                    Ok(None) => {
                        debug!(
                            "Subscription to blocks from peer {} {} ended",
                            peer, peer_hostname
                        );
                        retries += 1;
                        break 'stream;
                    }
                    Err(_) => {
                        info!(
                            "Subscription to blocks from peer {} {} made no progress for {:?}",
                            peer, peer_hostname, SUBSCRIPTION_TIMEOUT
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
        TestBlock, VerifiedBlock,
        commit::CommitRange,
        error::ConsensusResult,
        network::{BlockStream, ExtendedSerializedBlock, test_network::TestService},
        storage::mem_store::MemStore,
    };

    struct SubscriberTestClient {
        // Records the `last_received` round passed to each subscribe_blocks() call.
        subscribe_calls: Mutex<Vec<Round>>,
        // Interval between blocks on the returned stream. None keeps the stream open
        // forever without yielding any block.
        block_interval: Option<Duration>,
        // When true, subscribe_blocks() itself never returns.
        hang_on_subscribe: bool,
        emit_slim: bool,
    }

    impl SubscriberTestClient {
        fn new() -> Self {
            Self::new_with_block_interval(Duration::from_millis(1))
        }

        fn new_pending() -> Self {
            Self {
                subscribe_calls: Mutex::new(Vec::new()),
                block_interval: None,
                hang_on_subscribe: false,
                emit_slim: false,
            }
        }

        fn new_with_block_interval(interval: Duration) -> Self {
            Self {
                subscribe_calls: Mutex::new(Vec::new()),
                block_interval: Some(interval),
                hang_on_subscribe: false,
                emit_slim: false,
            }
        }

        fn new_hanging_subscribe() -> Self {
            Self {
                subscribe_calls: Mutex::new(Vec::new()),
                block_interval: None,
                hang_on_subscribe: true,
                emit_slim: false,
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
            if self.hang_on_subscribe {
                std::future::pending::<()>().await;
            }
            let Some(interval) = self.block_interval else {
                return Ok(Box::pin(stream::pending()));
            };
            let emit_slim = self.emit_slim;
            let block_stream = stream::unfold(0u8, move |i| async move {
                sleep(interval).await;
                // With emit_slim set, every other payload is the slim form, which the
                // subscriber must drop without delivering.
                let block = if emit_slim && i % 2 == 0 {
                    ExtendedSerializedBlock {
                        block: SerializedBlockForm::Slim(Bytes::from(vec![2u8; 8])),
                        excluded_ancestors: vec![],
                    }
                } else {
                    ExtendedSerializedBlock {
                        block: SerializedBlockForm::Full(Bytes::from(vec![1u8; 8])),
                        excluded_ancestors: vec![],
                    }
                };
                Some((block, i.wrapping_add(1)))
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
            dag_state.clone(),
            broadcast::channel(8).1,
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
                    block: SerializedBlockForm::Full(Bytes::from(vec![1u8; 8])),
                    excluded_ancestors: vec![],
                }
            );
        }
    }

    /// A slim payload that cannot be rebuilt (here: garbage bytes) is dropped at the
    /// subscriber and counted as malformed, while full payloads on the same stream
    /// keep flowing.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn subscriber_drops_slim_payloads_without_delivering() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let network_client = Arc::new(SubscriberTestClient {
            subscribe_calls: Mutex::new(Vec::new()),
            block_interval: Some(Duration::from_millis(1)),
            hang_on_subscribe: false,
            emit_slim: true,
        });
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let subscriber = Subscriber::new(
            context.clone(),
            network_client,
            authority_service.clone(),
            dag_state.clone(),
            broadcast::channel(8).1,
        );

        let peer = context.committee.to_authority_index(2).unwrap();
        subscriber.subscribe(peer);

        for _ in 0..10 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if authority_service.lock().handle_send_block.len() >= 20 {
                break;
            }
        }

        let service = authority_service.lock();
        assert!(!service.handle_send_block.is_empty());
        for (_, block) in service.handle_send_block.iter() {
            assert!(
                matches!(block.block, SerializedBlockForm::Full(_)),
                "a slim payload must never be delivered"
            );
        }
        assert!(
            context
                .metrics
                .node_metrics
                .slim_block_decode_failures
                .with_label_values(&[
                    context.committee.authority(peer).hostname.as_str(),
                    "malformed",
                ])
                .get()
                > 0,
            "undecodable slim payloads must be counted as malformed"
        );
    }

    /// End to end through parking: a slim block whose ancestor is not yet accepted
    /// parks instead of dropping, and the acceptance of that ancestor reconstructs
    /// and delivers it.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn parked_slim_block_is_delivered_once_its_ancestor_is_accepted() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let peer = context.committee.to_authority_index(1).unwrap();

        // The sender has accepted `missing_ancestor`, so its digest is omitted from
        // the slim form; the receiver has not, so decoding parks.
        let missing_ancestor = VerifiedBlock::new_for_test(TestBlock::new(1, 2).build());
        let own_parent = VerifiedBlock::new_for_test(TestBlock::new(1, 1).build());
        let block = VerifiedBlock::new_for_test(
            TestBlock::new(2, 1)
                .set_ancestors(vec![own_parent.reference(), missing_ancestor.reference()])
                .build(),
        );
        let slim = {
            let sender_dag_state = Arc::new(RwLock::new(DagState::new(
                context.clone(),
                Arc::new(MemStore::new()),
            )));
            sender_dag_state.write().accept_block(own_parent.clone());
            sender_dag_state
                .write()
                .accept_block(missing_ancestor.clone());
            SlimBlockCodec::new(context.clone())
                .encode(&block, &sender_dag_state)
                .unwrap()
        };

        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let network_client = Arc::new(OneShotSlimClient {
            slim: Mutex::new(Some(slim)),
        });
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let (accepted_tx, accepted_rx) = broadcast::channel(8);
        let subscriber = Subscriber::new(
            context.clone(),
            network_client,
            authority_service.clone(),
            dag_state.clone(),
            accepted_rx,
        );
        subscriber.subscribe(peer);

        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if context.metrics.node_metrics.slim_block_parked_entries.get() > 0 {
                break;
            }
        }
        assert_eq!(
            context.metrics.node_metrics.slim_block_parked_entries.get(),
            1,
            "the undecodable block must park, not drop"
        );
        assert!(authority_service.lock().handle_send_block.is_empty());

        dag_state.write().accept_block(missing_ancestor.clone());
        accepted_tx.send(missing_ancestor).unwrap();

        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if !authority_service.lock().handle_send_block.is_empty() {
                break;
            }
        }
        let service = authority_service.lock();
        assert_eq!(service.handle_send_block.len(), 1);
        let (from, delivered) = &service.handle_send_block[0];
        assert_eq!(*from, peer);
        match &delivered.block {
            SerializedBlockForm::Full(bytes) => {
                assert_eq!(bytes, block.serialized(), "byte-identical reconstruction")
            }
            SerializedBlockForm::Slim(_) => panic!("must deliver the rebuilt full form"),
        }
        assert_eq!(
            context.metrics.node_metrics.slim_block_parked_entries.get(),
            0
        );
        assert_eq!(
            context.metrics.node_metrics.slim_block_parked_bytes.get(),
            0
        );
    }

    /// Emits one slim payload, then stays open without yielding.
    struct OneShotSlimClient {
        slim: Mutex<Option<Bytes>>,
    }

    #[async_trait]
    impl ValidatorNetworkClient for OneShotSlimClient {
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
            let slim = self.slim.lock().take();
            let block_stream = stream::unfold(slim, move |slim| async move {
                let Some(slim) = slim else {
                    std::future::pending::<()>().await;
                    unreachable!();
                };
                let block = ExtendedSerializedBlock {
                    block: SerializedBlockForm::Slim(slim),
                    excluded_ancestors: vec![],
                };
                Some((block, None))
            });
            Ok(Box::pin(block_stream))
        }

        async fn fetch_blocks(
            &self,
            _peer: AuthorityIndex,
            _block_refs: Vec<BlockRef>,
            _highest_accepted_rounds: Vec<Round>,
            _breadth_first: bool,
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
    async fn subscriber_reconnects_when_stream_makes_no_progress() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let network_client = Arc::new(SubscriberTestClient::new_pending());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service,
            dag_state.clone(),
            broadcast::channel(8).1,
        );

        let peer = context.committee.to_authority_index(2).unwrap();
        subscriber.subscribe(peer);

        tokio::time::sleep(SUBSCRIPTION_TIMEOUT + Duration::from_millis(1)).await;

        assert!(
            network_client.subscribe_calls().len() >= 2,
            "an idle subscription should be re-established"
        );
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn subscriber_retries_when_subscribing_makes_no_progress() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        // The peer accepts the subscription but never responds, so the request to establish the
        // stream never completes.
        let network_client = Arc::new(SubscriberTestClient::new_hanging_subscribe());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service,
            dag_state.clone(),
            broadcast::channel(8).1,
        );

        let peer = context.committee.to_authority_index(2).unwrap();
        subscriber.subscribe(peer);

        tokio::time::sleep(SUBSCRIPTION_TIMEOUT + Duration::from_millis(1)).await;

        assert!(
            network_client.subscribe_calls().len() >= 2,
            "subscribing should be abandoned and retried when the peer never responds"
        );
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn subscriber_stays_subscribed_when_stream_progresses_within_idle_timeout() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        // Blocks arrive slower than from a healthy peer but within the idle timeout, so the
        // timeout must reset on every received block and never tear down the subscription.
        let network_client = Arc::new(SubscriberTestClient::new_with_block_interval(
            SUBSCRIPTION_TIMEOUT - Duration::from_secs(1),
        ));
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service.clone(),
            dag_state.clone(),
            broadcast::channel(8).1,
        );

        let peer = context.committee.to_authority_index(2).unwrap();
        subscriber.subscribe(peer);

        tokio::time::sleep(SUBSCRIPTION_TIMEOUT * 4).await;

        assert_eq!(
            network_client.subscribe_calls().len(),
            1,
            "a stream that keeps delivering blocks within the idle timeout should not be re-established"
        );
        assert!(
            !authority_service.lock().handle_send_block.is_empty(),
            "blocks from the slow stream should have been processed"
        );
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
            broadcast::channel(8).1,
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
