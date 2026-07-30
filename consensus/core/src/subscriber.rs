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
use mysten_metrics::{monitored_mpsc, spawn_monitored_task};
use parking_lot::{Mutex, RwLock};
use tokio::{
    sync::{broadcast, mpsc::error::TrySendError},
    task::JoinHandle,
    time::sleep,
};
use tracing::{debug, error, info};

use crate::{
    block::{BlockAPI as _, SignedBlock, Slot, VerifiedBlock},
    block_inflater::BlockInflater,
    context::Context,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    minimal_block::{self, InflateError},
    minimal_block_recovery::{
        HINT_CHANNEL_CAPACITY, HintCache, PARK_CHANNEL_CAPACITY, ParkCommand, RecoveryManager,
    },
    network::{ExtendedSerializedBlock, ValidatorNetworkClient, ValidatorNetworkService},
    task::{join_and_propagate_panic, reap_finished_task},
};

/// Malformed minimal encodings tolerated per subscription session before the stream is
/// reset (reconnect backoff is the peer's penalty). Honest senders produce none.
const MAX_MALFORMED_PER_SUBSCRIPTION: u32 = 3;

/// Outcome of processing one received block envelope before it is handed to the service.
enum InflateOutcome {
    /// A full-form block, or a minimal block successfully inflated to full form.
    Block(ExtendedSerializedBlock),
    /// A minimal block that local state cannot inflate right now. It is dropped from
    /// the stream and handed to the recovery manager: parked on its missing slot, or
    /// fetched immediately when there is no slot whose arrival could repair it.
    Dropped {
        block_ref: BlockRef,
        minimal: Bytes,
        missing: Option<Slot>,
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
    hint_cache: Arc<HintCache>,
    park_commands: monitored_mpsc::Sender<ParkCommand>,
    hint_commands: monitored_mpsc::Sender<Slot>,
    recovery_manager: Mutex<Option<JoinHandle<()>>>,
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
        let subscriptions = (0..context.committee.size())
            .map(|_| None)
            .collect::<Vec<_>>();
        // Hint horizon matches DagState's in-memory cache: hints older than what the
        // DAG itself would remember cannot aid inflation anyway.
        let hint_cache = Arc::new(HintCache::new(
            context.parameters.dag_state_cached_rounds as Round,
            context.committee.size(),
        ));
        let block_inflater = Arc::new(BlockInflater::with_hints(
            context.clone(),
            dag_state.clone(),
            Some(hint_cache.clone()),
        ));
        // Parks are lossless (full channel => the sending stream awaits); hint wakes
        // are lossy (full channel => shed with a metric; the at-park recheck, the
        // accepted broadcast, and the deadline all cover a lost wake).
        let (park_commands, park_receiver) =
            monitored_mpsc::channel("consensus_minimal_block_parks", PARK_CHANNEL_CAPACITY);
        let (hint_commands, hint_receiver) =
            monitored_mpsc::channel("consensus_minimal_block_hint_wakes", HINT_CHANNEL_CAPACITY);
        let recovery_manager = RecoveryManager::new(
            context.clone(),
            block_inflater.clone(),
            hint_cache.clone(),
            network_client.clone(),
            Arc::downgrade(&authority_service),
        );
        let recovery_task = spawn_monitored_task!(recovery_manager.run(
            park_receiver,
            hint_receiver,
            accepted_blocks,
        ));
        Self {
            context,
            network_client,
            authority_service,
            dag_state,
            block_inflater,
            hint_cache,
            park_commands,
            hint_commands,
            recovery_manager: Mutex::new(Some(recovery_task)),
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
        let hint_cache = self.hint_cache.clone();
        let park_commands = self.park_commands.clone();
        let hint_commands = self.hint_commands.clone();

        let mut subscriptions = self.subscriptions.lock();
        self.unsubscribe_locked(peer, &mut subscriptions[peer.value()]);
        subscriptions[peer.value()] = Some(spawn_monitored_task!(Self::subscription_loop(
            context,
            network_client,
            authority_service,
            dag_state,
            block_inflater,
            hint_cache,
            park_commands,
            hint_commands,
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
        // With the producers stopped, stop the recovery manager and AWAIT it: dropping
        // the actor drops its JoinSet, which aborts every detached fetch/submission —
        // nothing recovery-related outlives this call.
        let manager = self.recovery_manager.lock().take();
        if let Some(manager) = manager {
            manager.abort();
            let _ = manager.await;
        }
        self.context
            .metrics
            .node_metrics
            .minimal_block_recovery_parked
            .set(0);
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
    /// continues the stream immediately and parks the block with the recovery manager
    /// (see `minimal_block_recovery`). Dropping without recovery is not an option: a receiver
    /// that falls a round behind finds every subsequent block from every peer
    /// un-inflatable (each references blocks it lacks), nothing reaches `block_manager`,
    /// so no missing ancestor is ever registered and the node wedges permanently.
    ///
    /// Cap on untrusted minimal bytes, enforced before ANY decoding: a legitimate
    /// minimal block is bounded by the max transaction payload plus small per-ancestor
    /// structure. The hint and inflation paths must agree on this bound.
    fn max_minimal_size(context: &Context) -> usize {
        (context.protocol_config.max_transactions_in_block_bytes() as usize).saturating_mul(2)
    }

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
                // Waiting cannot repair ambiguity or a digest mismatch; those go
                // straight to the digest-verified sender fetch.
                let missing = match reason {
                    crate::minimal_block::FallbackReason::MissingAncestor(slot) => Some(slot),
                    crate::minimal_block::FallbackReason::AmbiguousSlot(_)
                    | crate::minimal_block::FallbackReason::DigestMismatch => None,
                };
                Ok(InflateOutcome::Dropped {
                    block_ref,
                    minimal,
                    missing,
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

    /// Publishes a receipt-time digest hint for a just-received block, both forms:
    /// a full block's digest is a hash of its bytes; a minimal block carries its claimed
    /// digest. Identity is validated (author == authenticated peer, current epoch)
    /// before insertion so a peer can only hint its own slots. Newly inserted hints wake
    /// any blocks parked on that slot.
    fn publish_hint(
        context: &Context,
        hint_cache: &HintCache,
        hint_commands: &monitored_mpsc::Sender<Slot>,
        peer: AuthorityIndex,
        block: &ExtendedSerializedBlock,
    ) {
        let identity = match &block.minimal {
            Some(minimal) => {
                // Same pre-decode cap as inflation: hint extraction must not become a
                // parse-DoS bypass around it.
                if minimal.len() > Self::max_minimal_size(context) {
                    return;
                }
                minimal_block::peek_identity(minimal, &context.committee, peer)
            }
            None => {
                let Ok(signed) = bcs::from_bytes::<SignedBlock>(&block.block) else {
                    return;
                };
                if signed.author() != peer || signed.epoch() != context.committee.epoch() {
                    return;
                }
                Some(BlockRef::new(
                    signed.round(),
                    signed.author(),
                    VerifiedBlock::compute_digest(&block.block),
                ))
            }
        };
        let Some(identity) = identity else {
            return;
        };
        let slot = Slot::from(identity);
        let outcome = hint_cache.insert(slot, identity.digest);
        context
            .metrics
            .node_metrics
            .minimal_block_hint_inserts
            .with_label_values(&[outcome.label()])
            .inc();
        if outcome == crate::minimal_block_recovery::HintInsert::Inserted
            && hint_commands.try_send(slot).is_err()
        {
            context
                .metrics
                .node_metrics
                .minimal_block_recovery_skipped_hint_wakes
                .with_label_values(&[context.committee.authority(peer).hostname.as_str()])
                .inc();
        }
    }

    async fn subscription_loop(
        context: Arc<Context>,
        network_client: Arc<C>,
        authority_service: Weak<S>,
        dag_state: Weak<RwLock<DagState>>,
        block_inflater: Arc<BlockInflater>,
        hint_cache: Arc<HintCache>,
        park_commands: monitored_mpsc::Sender<ParkCommand>,
        hint_commands: monitored_mpsc::Sender<Slot>,
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
                        Self::publish_hint(&context, &hint_cache, &hint_commands, peer, &block);
                        let block = match Self::inflate_received_block(
                            &context,
                            &block_inflater,
                            peer,
                            block,
                        ) {
                            // Dropped from the stream, parked with the recovery
                            // manager. The stream itself is healthy — it delivered a
                            // well-formed block — so the reconnect backoff resets as
                            // for an accepted block. (Malformed errors below
                            // deliberately do NOT reset: escalating backoff is the
                            // penalty for a misbehaving peer.)
                            Ok(InflateOutcome::Dropped {
                                block_ref,
                                minimal,
                                missing,
                            }) => {
                                retries = 0;
                                backoff.reset();
                                // Catch-up: the block itself is more than a full
                                // DAG-cache window ahead of everything this node has
                                // accepted. Parking cannot help (no wake arrives
                                // before the deadline; the storms throttle catch-up
                                // itself), but the block must not vanish either: full
                                // blocks are how a lagging node learns the quorum
                                // commit has moved at all — skip them entirely and
                                // commit sync sees no lag and the node freezes. So
                                // divert to the bounded fetch: a sampled trickle of
                                // full tip blocks keeps commit votes and the
                                // missing-ancestor pool fresh, intent-queue overflow
                                // sheds the rest, and batched sync does the bulk.
                                // Keyed on the BLOCK's round (not the missing
                                // ancestor's) and on DagState's accepted round (not
                                // the async hint horizon), so a healthy node can
                                // never false-positive here.
                                if missing.is_some()
                                    && let Some(dag) = dag_state.upgrade()
                                    && block_ref.round
                                        > dag.read().highest_accepted_round().saturating_add(
                                            context.parameters.dag_state_cached_rounds as Round,
                                        )
                                {
                                    context
                                        .metrics
                                        .node_metrics
                                        .minimal_block_recovery_skipped_work
                                        .with_label_values(&["catchup"])
                                        .inc();
                                    let fetch = ParkCommand {
                                        peer,
                                        block_ref,
                                        // The bytes cannot inflate here and the fetch
                                        // path never uses them.
                                        minimal: Bytes::new(),
                                        missing: None,
                                    };
                                    // Lossy, unlike parks: the divert is a SAMPLE.
                                    // When the channel is busy — e.g. a full parking
                                    // table has gated the receive arm — dropping the
                                    // excess is the sampling; awaiting here would
                                    // choke the catch-up stream on its own trickle.
                                    let _ = park_commands.try_send(fetch);
                                    continue 'stream;
                                }
                                let park = ParkCommand {
                                    peer,
                                    block_ref,
                                    minimal,
                                    missing,
                                };
                                // Lossless: a full channel blocks this one peer's
                                // stream rather than dropping — a lost park is a block
                                // no recovery path owns, and its descendants wedge in
                                // block_manager until sync refetches it.
                                if let Err(TrySendError::Full(park)) = park_commands.try_send(park)
                                {
                                    context
                                        .metrics
                                        .node_metrics
                                        .minimal_block_recovery_park_backpressure
                                        .inc();
                                    let _ = park_commands.send(park).await;
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
    /// off-stream recovery fetch.
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
            minimal: Some(sender_inflater.serialize(block).unwrap()),
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
        let (_accepted_tx, accepted_rx) = broadcast::channel(64);
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag,
            accepted_rx,
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

    /// Shared parking scenario: one minimal block (round 3, from `peer`) streams in
    /// while the receiver lacks its round-2 ancestors, so it parks at receipt. Each
    /// test drives a different wake path to heal it.
    struct ParkScenario {
        context: Arc<Context>,
        peer: AuthorityIndex,
        round2_blocks: Vec<VerifiedBlock>,
        round2_refs: Vec<BlockRef>,
        block_a: VerifiedBlock,
        network_client: Arc<FixedStreamClient>,
        authority_service: Arc<Mutex<TestService>>,
        receiver_dag: Arc<RwLock<DagState>>,
        accepted_tx: broadcast::Sender<VerifiedBlock>,
        subscriber: Subscriber<FixedStreamClient, Mutex<TestService>>,
    }

    fn park_scenario(accepted_capacity: usize) -> ParkScenario {
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
        // Own ancestor first, as the verifier requires.
        round2_refs.sort_by_key(|r| (r.author != peer, r.author));
        let sender_inflater = BlockInflater::new(context.clone(), sender_dag.clone());
        let block_a = VerifiedBlock::new_for_test(
            crate::block::TestBlock::new(3, peer.value() as u32)
                .set_ancestors_raw(round2_refs.clone())
                .build(),
        );
        let network_client = Arc::new(FixedStreamClient {
            blocks: vec![ExtendedSerializedBlock {
                block: block_a.serialized().clone(),
                minimal: Some(sender_inflater.serialize(&block_a).unwrap()),
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
        let (accepted_tx, accepted_rx) = broadcast::channel(accepted_capacity);
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag.clone(),
            accepted_rx,
        );
        subscriber.subscribe(peer);
        ParkScenario {
            context,
            peer,
            round2_blocks,
            round2_refs,
            block_a,
            network_client,
            authority_service,
            receiver_dag,
            accepted_tx,
            subscriber,
        }
    }

    /// A parked block wakes when its missing ancestor is announced on Core's
    /// accepted-block broadcast (the path for ancestors that arrive via sync rather
    /// than our streams) and heals by re-inflation — no network fetch at all.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn parked_block_wakes_on_accepted_broadcast_without_fetch() {
        let s = park_scenario(64);

        // Let the stream deliver and park A, then land the ancestors and announce them
        // on the accepted-block broadcast.
        tokio::time::sleep(Duration::from_millis(2)).await;
        for block in s.round2_blocks {
            s.receiver_dag.write().accept_block(block.clone());
            s.accepted_tx.send(block).unwrap();
        }
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if !s.authority_service.lock().handle_send_block.is_empty() {
                break;
            }
        }

        let received = s.authority_service.lock().handle_send_block.clone();
        assert_eq!(received.len(), 1);
        assert_eq!(&received[0].1.block, s.block_a.serialized());
        // No fetch: the block healed by local re-inflation on the first retry.
        assert!(s.network_client.fetch_calls.lock().is_empty());
        let peer_hostname = &s.context.committee.authority(s.peer).hostname;
        assert_eq!(
            s.context
                .metrics
                .node_metrics
                .minimal_block_recovery
                .with_label_values(&[peer_hostname.as_str(), "inflated_late", "event"])
                .get(),
            1
        );
    }

    /// Falling behind the accepted-block broadcast must not wedge the actor: the lag
    /// is counted, the receiver jumps to the head, and wakes sent afterwards still
    /// heal parked blocks (wakes skipped by the jump are the deadline's job).
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn parked_block_recovers_after_accepted_broadcast_lag() {
        // Tiny broadcast so a burst of unrelated accepted blocks overflows it.
        let s = park_scenario(2);

        // Park A, then overflow the broadcast while the actor is idle.
        tokio::time::sleep(Duration::from_millis(2)).await;
        for round in 0..6u32 {
            let unrelated =
                VerifiedBlock::new_for_test(crate::block::TestBlock::new(1, round % 2).build());
            s.accepted_tx.send(unrelated).unwrap();
        }
        // Let the actor observe the lag and resubscribe at the head.
        tokio::time::sleep(Duration::from_millis(2)).await;
        assert!(
            s.context
                .metrics
                .node_metrics
                .minimal_block_recovery_accepted_lag
                .get()
                >= 1
        );

        // Wakes sent after the jump must still work: land the real ancestors, paced so
        // the tiny test channel cannot lag again between sends.
        for block in s.round2_blocks {
            s.receiver_dag.write().accept_block(block.clone());
            s.accepted_tx.send(block).unwrap();
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if !s.authority_service.lock().handle_send_block.is_empty() {
                break;
            }
        }

        let received = s.authority_service.lock().handle_send_block.clone();
        assert_eq!(received.len(), 1);
        assert_eq!(&received[0].1.block, s.block_a.serialized());
        assert!(s.network_client.fetch_calls.lock().is_empty());
    }

    /// In catch-up (the block is beyond the node's DAG-cache future window) the block
    /// is never parked — no wake arrives before the deadline — but it must not vanish
    /// either: it diverts to the bounded fetch so full blocks keep feeding commit
    /// votes and the missing-ancestor pool (skipping entirely blinds commit sync and
    /// freezes the node). The stream keeps flowing throughout.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn deep_catchup_blocks_skip_recovery() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let peer = context.committee.to_authority_index(2).unwrap();

        let sender_dag = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let mut ancestor_refs = Vec::new();
        for authority in 0..4u32 {
            let block =
                VerifiedBlock::new_for_test(crate::block::TestBlock::new(1999, authority).build());
            ancestor_refs.push(block.reference());
            sender_dag.write().accept_block(block);
        }
        ancestor_refs.sort_by_key(|r| (r.author != peer, r.author));
        let sender_inflater = BlockInflater::new(context.clone(), sender_dag.clone());
        let block_a = VerifiedBlock::new_for_test(
            crate::block::TestBlock::new(2000, peer.value() as u32)
                .set_ancestors_raw(ancestor_refs)
                .build(),
        );
        // A later full-form block from the same stream: the skip must not stall or
        // reset the subscription.
        let block_b = VerifiedBlock::new_for_test(
            crate::block::TestBlock::new(2001, peer.value() as u32).build(),
        );
        let network_client = Arc::new(FixedStreamClient {
            blocks: vec![
                ExtendedSerializedBlock {
                    block: block_a.serialized().clone(),
                    minimal: Some(sender_inflater.serialize(&block_a).unwrap()),
                    excluded_ancestors: vec![],
                },
                ExtendedSerializedBlock {
                    block: block_b.serialized().clone(),
                    minimal: None,
                    excluded_ancestors: vec![],
                },
            ],
            fetchable: vec![],
            subscribe_calls: Mutex::new(0),
            fetch_calls: Mutex::new(Vec::new()),
        });
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        // The receiver has accepted nothing past round 1499. The BLOCK (round 2000) is
        // beyond 1499 + dag_state_cached_rounds (500) — catch-up — even though its
        // missing ancestor (1999) is within the window: a tip block with older
        // ancestors must not slip into parking on a lagging node.
        receiver_dag
            .write()
            .accept_block(VerifiedBlock::new_for_test(
                crate::block::TestBlock::new(1499, 0).build(),
            ));
        let (_accepted_tx, accepted_rx) = broadcast::channel(64);
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag,
            accepted_rx,
        );
        subscriber.subscribe(peer);
        tokio::time::sleep(Duration::from_millis(100)).await;

        // The un-inflatable block was not parked; it went to the bounded fetch, and
        // the stream carried on to deliver the next block.
        let received = authority_service.lock().handle_send_block.clone();
        assert_eq!(received.len(), 1);
        assert_eq!(&received[0].1.block, block_b.serialized());
        assert_eq!(
            network_client.fetch_calls.lock().clone(),
            vec![vec![block_a.reference()]]
        );
        assert_eq!(
            context
                .metrics
                .node_metrics
                .minimal_block_recovery_parked
                .get(),
            0
        );
        assert_eq!(
            context
                .metrics
                .node_metrics
                .minimal_block_recovery_skipped_work
                .with_label_values(&["catchup"])
                .get(),
            1
        );
    }

    /// The receipt-time hint path end-to-end: a parked block heals the moment digests
    /// for its missing slots are HEARD (bytes-arrival), with an empty DAG and zero
    /// fetches.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn parked_block_wakes_on_receipt_hint_without_dag_or_fetch() {
        let s = park_scenario(64);

        // A parks (round-2 ancestors unknown). Publish receipt-time hints carrying the
        // TRUE digests — as if the ancestors' bytes just landed on their own streams —
        // and announce the slots. The receiver DAG stays empty throughout.
        tokio::time::sleep(Duration::from_millis(2)).await;
        // The at-park re-attempt is gated on candidate presence: with nothing local to
        // resolve A's missing slot, parking must not have spent a decode.
        assert_eq!(
            s.context
                .metrics
                .node_metrics
                .minimal_block_recovery_attempts
                .get(),
            0
        );
        for ancestor in &s.round2_refs {
            let slot = Slot::from(*ancestor);
            assert_eq!(
                s.subscriber.hint_cache.insert(slot, ancestor.digest),
                crate::minimal_block_recovery::HintInsert::Inserted
            );
            s.subscriber.hint_commands.try_send(slot).unwrap();
        }
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            if !s.authority_service.lock().handle_send_block.is_empty() {
                break;
            }
        }

        let received = s.authority_service.lock().handle_send_block.clone();
        assert_eq!(received.len(), 1);
        assert_eq!(&received[0].1.block, s.block_a.serialized());
        assert!(s.network_client.fetch_calls.lock().is_empty());
        let peer_hostname = &s.context.committee.authority(s.peer).hostname;
        assert_eq!(
            s.context
                .metrics
                .node_metrics
                .minimal_block_recovery
                .with_label_values(&[peer_hostname.as_str(), "hint_inflated", "event"])
                .get(),
            1
        );
    }

    /// A block missing several ancestors re-parks from slot to slot as each hint lands,
    /// healing with zero fetches once the last gap closes.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn parked_block_rekeys_across_missing_slots() {
        let s = park_scenario(64);
        tokio::time::sleep(Duration::from_millis(2)).await;

        // Feed the missing digests ONE slot at a time; each hint wakes the entry,
        // which re-parks on the next gap until the final hint completes it.
        for ancestor in s.round2_refs.iter().filter(|r| r.author != s.peer) {
            assert!(s.authority_service.lock().handle_send_block.is_empty());
            let slot = Slot::from(*ancestor);
            assert_eq!(
                s.subscriber.hint_cache.insert(slot, ancestor.digest),
                crate::minimal_block_recovery::HintInsert::Inserted
            );
            s.subscriber.hint_commands.try_send(slot).unwrap();
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            if !s.authority_service.lock().handle_send_block.is_empty() {
                break;
            }
        }
        let received = s.authority_service.lock().handle_send_block.clone();
        assert_eq!(received.len(), 1);
        assert_eq!(&received[0].1.block, s.block_a.serialized());
        assert!(s.network_client.fetch_calls.lock().is_empty());
    }

    /// Beyond the per-peer parking cap, new drops divert to the digest-verified fetch
    /// path instead of being silently discarded.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn parking_cap_overflow_diverts_to_fetch() {
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
        let sender_inflater = BlockInflater::new(context.clone(), sender_dag.clone());
        // 257 distinct un-inflatable blocks from one peer: one more than the per-peer
        // parking cap.
        let mut wire = Vec::new();
        let mut fetchable = Vec::new();
        for i in 0..257u64 {
            let block = VerifiedBlock::new_for_test(
                crate::block::TestBlock::new(3, peer.value() as u32)
                    .set_ancestors_raw(round2_refs.clone())
                    .set_timestamp_ms(1000 + i)
                    .build(),
            );
            wire.push(ExtendedSerializedBlock {
                block: block.serialized().clone(),
                minimal: Some(sender_inflater.serialize(&block).unwrap()),
                excluded_ancestors: vec![],
            });
            fetchable.push(block.serialized().clone());
        }
        let network_client = Arc::new(FixedStreamClient {
            blocks: wire,
            fetchable,
            subscribe_calls: Mutex::new(0),
            fetch_calls: Mutex::new(Vec::new()),
        });
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let (_accepted_tx, accepted_rx) = broadcast::channel(64);
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag,
            accepted_rx,
        );
        subscriber.subscribe(peer);

        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            if context
                .metrics
                .node_metrics
                .minimal_block_recovery_overflow
                .get()
                > 0
            {
                break;
            }
        }
        // The 257th drop overflowed the per-peer cap and went straight to fetch.
        assert!(
            context
                .metrics
                .node_metrics
                .minimal_block_recovery_overflow
                .get()
                >= 1
        );
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            if !network_client.fetch_calls.lock().is_empty() {
                break;
            }
        }
        assert!(!network_client.fetch_calls.lock().is_empty());
        assert!(
            context
                .metrics
                .node_metrics
                .minimal_block_recovery_parked
                .get()
                <= 256
        );
    }

    /// Hint identity is validated before insertion: a block authored by someone other
    /// than the authenticated stream peer, or from a foreign epoch, publishes nothing.
    #[tokio::test]
    async fn foreign_author_and_epoch_hints_are_rejected() {
        let (context, _keys) = Context::new_for_test(4);
        let peer = context.committee.to_authority_index(2).unwrap();
        let hint_cache = HintCache::new(512, 4);
        let (commands, mut command_rx) = monitored_mpsc::channel("test_hint_wakes", 16);

        // Full-form block authored by authority 1, delivered over peer 2's stream.
        let foreign = VerifiedBlock::new_for_test(crate::block::TestBlock::new(5, 1).build());
        let envelope = ExtendedSerializedBlock {
            block: foreign.serialized().clone(),
            minimal: None,
            excluded_ancestors: vec![],
        };
        Subscriber::<FixedStreamClient, Mutex<TestService>>::publish_hint(
            &context,
            &hint_cache,
            &commands,
            peer,
            &envelope,
        );
        assert!(
            hint_cache
                .candidates(Slot::from(foreign.reference()))
                .is_empty()
        );
        assert!(command_rx.try_recv().is_err());

        // Correct author publishes and wakes.
        let own = VerifiedBlock::new_for_test(
            crate::block::TestBlock::new(5, peer.value() as u32).build(),
        );
        let envelope = ExtendedSerializedBlock {
            block: own.serialized().clone(),
            minimal: None,
            excluded_ancestors: vec![],
        };
        Subscriber::<FixedStreamClient, Mutex<TestService>>::publish_hint(
            &context,
            &hint_cache,
            &commands,
            peer,
            &envelope,
        );
        assert_eq!(
            hint_cache.candidates(Slot::from(own.reference())),
            vec![own.reference().digest]
        );
        assert!(command_rx.try_recv().is_ok());

        // Right author, wrong epoch: publishes nothing.
        let foreign_epoch = VerifiedBlock::new_for_test(
            crate::block::TestBlock::new(6, peer.value() as u32)
                .set_epoch(context.committee.epoch() + 1)
                .build(),
        );
        let envelope = ExtendedSerializedBlock {
            block: foreign_epoch.serialized().clone(),
            minimal: None,
            excluded_ancestors: vec![],
        };
        Subscriber::<FixedStreamClient, Mutex<TestService>>::publish_hint(
            &context,
            &hint_cache,
            &commands,
            peer,
            &envelope,
        );
        assert!(
            hint_cache
                .candidates(Slot::from(foreign_epoch.reference()))
                .is_empty()
        );
        assert!(command_rx.try_recv().is_err());
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
            broadcast::channel(64).1,
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
            broadcast::channel(64).1,
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
