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
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use parking_lot::{Mutex, RwLock};
use tokio::{
    task::JoinHandle,
    time::{sleep, timeout},
};
use tracing::{debug, error, info};

use crate::{
    block::BlockAPI as _,
    block_inflater::BlockInflater,
    commit_vote_monitor::{CommitVoteMonitor, is_commit_lagging},
    context::Context,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    minimal_block::{FallbackReason, InflateError},
    minimal_block::{max_excluded_ancestors_size, max_minimal_size},
    network::{ExtendedSerializedBlock, ValidatorNetworkClient, ValidatorNetworkService},
    pending_reconstructions::{
        MissingBlockRegistry, PendingReconstructions, ReconstructionHook, run_reconstruction_worker,
    },
    round_tracker::RoundTracker,
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

/// Malformed minimal encodings tolerated per subscription session before the stream is
/// reset (reconnect backoff is the peer's penalty). Honest senders produce none.

/// Floor delay before reconnecting after a reset this node initiated for cause
/// (quota overflow, recovery horizon, malformed threshold). The regular backoff can
/// be cleared by any admitted block, so a peer interleaving valid blocks with
/// reset-triggering ones could otherwise force immediate reconnects in an unbounded
/// loop. Jittered per (node, peer, attempt): a fixed delay
/// synchronizes resets across every subscription into committee-wide thrash.
const CAUSED_RESET_DELAY_FLOOR_MS: u64 = 700;
const CAUSED_RESET_DELAY_JITTER_MS: u64 = 600;

fn caused_reset_delay(own: AuthorityIndex, peer: AuthorityIndex, attempt: i64) -> Duration {
    let jitter = (own.value() as u64 * 31 + peer.value() as u64 * 7 + attempt as u64 * 13)
        % CAUSED_RESET_DELAY_JITTER_MS;
    Duration::from_millis(CAUSED_RESET_DELAY_FLOOR_MS + jitter)
}

/// Outcome of processing one received block envelope before it is handed to the service.
enum InflateOutcome {
    /// A full-form block, or a minimal block successfully inflated to full form.
    Block(ExtendedSerializedBlock),
    /// A minimal block that local state cannot inflate right now. It is dropped from
    /// the stream and recovered by a per-block task that waits on the missing slot
    /// (see `pending_reconstructions`).
    Dropped {
        block_ref: BlockRef,
        reason: FallbackReason,
        minimal: Bytes,
        excluded_ancestors: Vec<Vec<u8>>,
        commit_votes: Vec<crate::commit::CommitVote>,
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
    pending_reconstructions: Arc<Mutex<PendingReconstructions>>,
    missing_block_registry: Arc<dyn MissingBlockRegistry>,
    round_tracker: Arc<RwLock<RoundTracker>>,
    commit_vote_monitor: Arc<CommitVoteMonitor>,
    subscriptions: Arc<Mutex<Box<[Option<JoinHandle<()>>]>>>,
    // Retain replaced subscription tasks so stop() can await them and propagate panics.
    retired_subscriptions: Arc<Mutex<Vec<JoinHandle<()>>>>,
    reconstruction_worker: Mutex<Option<JoinHandle<()>>>,
    effects_tx:
        tokio::sync::mpsc::UnboundedSender<crate::pending_reconstructions::AcceptanceEffects>,
}

impl<C: ValidatorNetworkClient, S: ValidatorNetworkService> Subscriber<C, S> {
    pub(crate) fn new(
        context: Arc<Context>,
        network_client: Arc<C>,
        authority_service: Arc<S>,
        dag_state: Arc<RwLock<DagState>>,
        missing_block_registry: Arc<dyn MissingBlockRegistry>,
        round_tracker: Arc<RwLock<RoundTracker>>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
    ) -> Self {
        let pending_reconstructions =
            Arc::new(Mutex::new(PendingReconstructions::new(context.clone())));
        let subscriptions = (0..context.committee.size())
            .map(|_| None)
            .collect::<Vec<_>>();
        let block_inflater = Arc::new(BlockInflater::new(context.clone()));

        // Wire the acceptance/GC hooks and start the reconstruction worker. The
        // subscriber is the receive side's owner, constructed once per validator,
        // so the hook is registered here rather than threading a constructor
        // parameter through Core.
        missing_block_registry
            .install_pending_slot_floor(pending_reconstructions.lock().slot_floor());
        let (effects_tx, effects_rx) = tokio::sync::mpsc::unbounded_channel();
        dag_state
            .write()
            .set_reconstruction_hook(ReconstructionHook {
                pending_reconstructions: pending_reconstructions.clone(),
                effects: effects_tx.clone(),
            });
        // spawn_monitored_task! wraps its body in an async-move coroutine, so the
        // captures are prepared outside it.
        let worker_context = context.clone();
        let worker_inflater = block_inflater.clone();
        let worker_dag_state = Arc::downgrade(&dag_state);
        let worker_registry = missing_block_registry.clone();
        let worker_service = Arc::downgrade(&authority_service);
        let worker_pending = pending_reconstructions.clone();
        let reconstruction_worker = spawn_monitored_task!(run_reconstruction_worker(
            worker_context,
            worker_inflater,
            worker_dag_state,
            worker_registry,
            worker_service,
            worker_pending,
            effects_rx,
        ));

        Self {
            context,
            network_client,
            authority_service,
            dag_state,
            block_inflater,
            pending_reconstructions,
            effects_tx,
            missing_block_registry,
            round_tracker,
            commit_vote_monitor,
            subscriptions: Arc::new(Mutex::new(subscriptions.into_boxed_slice())),
            retired_subscriptions: Arc::new(Mutex::new(Vec::new())),
            reconstruction_worker: Mutex::new(Some(reconstruction_worker)),
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
        let pending_reconstructions = self.pending_reconstructions.clone();
        let effects_tx = self.effects_tx.clone();
        let missing_block_registry = self.missing_block_registry.clone();
        let round_tracker = self.round_tracker.clone();
        let commit_vote_monitor = self.commit_vote_monitor.clone();

        let mut subscriptions = self.subscriptions.lock();
        self.unsubscribe_locked(peer, &mut subscriptions[peer.value()]);
        subscriptions[peer.value()] = Some(spawn_monitored_task!(Self::subscription_loop(
            context,
            network_client,
            authority_service,
            dag_state,
            block_inflater,
            pending_reconstructions,
            effects_tx,
            missing_block_registry,
            round_tracker,
            commit_vote_monitor,
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

        if let Some(worker) = self.reconstruction_worker.lock().take() {
            worker.abort();
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
        pending_reconstructions: &Mutex<PendingReconstructions>,
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

        let sidecar_bytes: usize = block.excluded_ancestors.iter().map(Vec::len).sum();
        if sidecar_bytes > max_excluded_ancestors_size(context) {
            return Err(ConsensusError::MalformedMinimalBlock(format!(
                "excluded-ancestors sidecar of {sidecar_bytes} bytes exceeds the cap"
            )));
        }
        let max_minimal_size = max_minimal_size(context);
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

        match block_inflater.inflate(&minimal, peer, dag_state, Some(pending_reconstructions)) {
            Ok((_signed_block, serialized)) => {
                block.block = serialized;
                Ok(InflateOutcome::Block(block))
            }
            Err(InflateError::NeedFullBlock {
                block_ref,
                reason,
                commit_votes,
            }) => {
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
                    excluded_ancestors: std::mem::take(&mut block.excluded_ancestors),
                    commit_votes,
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
        pending_reconstructions: Arc<Mutex<PendingReconstructions>>,
        effects_tx: tokio::sync::mpsc::UnboundedSender<
            crate::pending_reconstructions::AcceptanceEffects,
        >,
        missing_block_registry: Arc<dyn MissingBlockRegistry>,
        round_tracker: Arc<RwLock<RoundTracker>>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
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
        // Set while commit-lag shedding; on exit the next uninflatable block routes
        // to the exact-fetch lane so recovery gets the same exact-reference anchor a
        // full-form block would provide through block_manager, instead of waiting on
        // periodic range repair.
        let mut exiting_lag = false;
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
            // Bound establishing the stream: a peer that accepts connections but whose
            // runtime is stalled can otherwise block indefinitely on response headers.
            let subscribe = timeout(
                SUBSCRIPTION_TIMEOUT,
                network_client.subscribe_blocks(peer, last_received, request_timeout),
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
                // Bound waiting for the next block: a subscription that stops
                // progressing without a transport error is abandoned and retried.
                let next = timeout(SUBSCRIPTION_TIMEOUT, blocks.next()).await;
                match next {
                    Ok(Some(block)) => {
                        context
                            .metrics
                            .node_metrics
                            .subscribed_blocks
                            .with_label_values(&[peer_hostname])
                            .inc();
                        let Some(service) = authority_service.upgrade() else {
                            return;
                        };
                        let (outcome, gc_round, tip) = {
                            let Some(dag_state) = dag_state.upgrade() else {
                                return;
                            };
                            // Scoped so the DagState read-guard hold time on the
                            // receive path is measurable against the writer-side
                            // BlockManager::try_accept_blocks scope: reconstruction
                            // and hashing run under this guard today, and whether
                            // that delays acceptance is a question only data settles.
                            let _scope = monitored_scope("MinimalBlock::inflate_at_receipt");
                            let guard = dag_state.read();
                            (
                                Self::inflate_received_block(
                                    &context,
                                    &block_inflater,
                                    peer,
                                    block,
                                    &guard,
                                    &pending_reconstructions,
                                ),
                                guard.gc_round(),
                                guard.highest_accepted_round(),
                            )
                        };
                        let block = match outcome {
                            // Dropped from the stream: admission decides whether it
                            // parks in the slot index. Ambiguity and digest mismatch
                            // have no slot frontier to wait on — only the exact block
                            // resolves them — so they go straight to the fetch lane.
                            Ok(InflateOutcome::Dropped {
                                block_ref,
                                reason,
                                minimal,
                                excluded_ancestors,
                                commit_votes,
                            }) => {
                                let node_metrics = &context.metrics.node_metrics;
                                // Full-form streams verify every block at receipt, so
                                // every block's commit votes reach the monitor before
                                // any accept/reject decision — and commit-sync
                                // targeting plus the periodic-sync gate read it. A
                                // minimal block that cannot be reconstructed must feed
                                // it the same way (deserialize_minimal has already
                                // enforced author == peer), or a lagging node starves
                                // the very signal its recovery runs on.
                                commit_vote_monitor.observe_stream_claim(peer, &commit_votes);
                                // Record the envelope's own claimed digest so blocks
                                // referencing this slot can rebuild their ancestor
                                // vectors without waiting for local acceptance — the
                                // full-form pipeline shape. Own-author only: the
                                // decoder has already bound envelope author to this
                                // stream's peer; sidecars never feed this.
                                let claim_ready = pending_reconstructions.lock().observe_claim(
                                    crate::block::Slot::from(block_ref),
                                    block_ref.digest,
                                );
                                if !claim_ready.is_empty() {
                                    let _ = effects_tx.send(
                                        crate::pending_reconstructions::AcceptanceEffects {
                                            ready: claim_ready,
                                            ..Default::default()
                                        },
                                    );
                                }
                                // While local commits lag the quorum, core rejects
                                // every block after reconstruction anyway; the
                                // full-form door consumes and discards cheaply, and
                                // this path must match it. Votes are harvested above;
                                // recovery is commit sync, as for full-form
                                // rejection. Nothing enters the parked pipeline, and
                                // evicting what is already parked releases byte caps
                                // that frozen GC would otherwise keep pinned.
                                let lagging = {
                                    let Some(dag_state) = dag_state.upgrade() else {
                                        return;
                                    };
                                    let local = dag_state.read().last_commit_index();
                                    is_commit_lagging(
                                        &context,
                                        local,
                                        commit_vote_monitor.quorum_commit_index(),
                                    )
                                };
                                if lagging {
                                    exiting_lag = true;
                                    node_metrics
                                        .minimal_block_recovery_outcomes
                                        .with_label_values(&["shed_commit_lag"])
                                        .inc();
                                    let evicted = pending_reconstructions.lock().quiesce();
                                    if evicted > 0 {
                                        node_metrics
                                            .minimal_block_recovery_outcomes
                                            .with_label_values(&["quiesced"])
                                            .inc_by(evicted as u64);
                                    }
                                    // The claim was genuinely received: credit it like
                                    // the equivalent full-form receipt would be.
                                    let credit_top = tip.saturating_add(
                                        (context.parameters.dag_state_cached_rounds as Round)
                                            .max(2 * context.protocol_config.gc_depth()),
                                    );
                                    if block_ref.round <= credit_top {
                                        round_tracker.write().update_received_claim(
                                            block_ref.author,
                                            block_ref.round,
                                        );
                                    }
                                    continue 'stream;
                                }
                                if exiting_lag {
                                    exiting_lag = false;
                                    if missing_block_registry
                                        .register_missing_block(block_ref)
                                        .await
                                        .is_err()
                                    {
                                        return;
                                    }
                                    pending_reconstructions.lock().hold_sidecar(
                                        block_ref,
                                        peer,
                                        excluded_ancestors,
                                    );
                                    continue 'stream;
                                }
                                let missing = match reason {
                                    FallbackReason::MissingAncestors(slots) => slots,
                                    FallbackReason::AmbiguousSlot(_)
                                    | FallbackReason::DigestMismatch => {
                                        if missing_block_registry
                                            .register_missing_block(block_ref)
                                            .await
                                            .is_err()
                                        {
                                            return;
                                        }
                                        pending_reconstructions.lock().hold_sidecar(
                                            block_ref,
                                            peer,
                                            excluded_ancestors,
                                        );
                                        continue 'stream;
                                    }
                                };
                                let missing_snapshot = missing.clone();
                                let admitted = pending_reconstructions.lock().try_admit(
                                    block_ref,
                                    minimal,
                                    excluded_ancestors,
                                    peer,
                                    missing,
                                    gc_round,
                                    tip,
                                );
                                // Received credit fires for every claim that is not
                                // FAR-FUTURE, refused or not. A recovering author's
                                // low-round blocks are refused here (below peers' GC)
                                // yet were genuinely received — upstream credits the
                                // equivalent full-block receipts, and withholding the
                                // credit starves the author's received-quorum until
                                // the propagation gate silences it — and a silenced
                                // node stops proposing, so its credit never recovers.
                                // Only rounds past the admission window top go
                                // uncredited: those are the unbackable claims the
                                // monotonic tracker must not advertise.
                                // Mirrors the admission window top exactly, so the
                                // credit and refusal boundaries cannot drift apart.
                                let credit_top = tip.saturating_add(
                                    (context.parameters.dag_state_cached_rounds as Round)
                                        .max(2 * context.protocol_config.gc_depth()),
                                );
                                if block_ref.round <= credit_top {
                                    round_tracker
                                        .write()
                                        .update_received_claim(block_ref.author, block_ref.round);
                                }
                                match admitted {
                                    Ok(()) => {
                                        retries = 0;
                                        backoff.reset();
                                        // Close the read-then-admit race: any slot
                                        // accepted between the frontier read and the
                                        // admit would never re-fire the hook.
                                        let (filled, gc_now): (Vec<_>, _) = {
                                            let Some(dag_state) = dag_state.upgrade() else {
                                                return;
                                            };
                                            let guard = dag_state.read();
                                            (
                                                missing_snapshot
                                                    .iter()
                                                    .flat_map(|slot| {
                                                        guard
                                                            .get_uncommitted_blocks_at_slot(*slot)
                                                            .into_iter()
                                                            .map(|b| b.reference())
                                                            .take(1)
                                                    })
                                                    .collect(),
                                                guard.gc_round(),
                                            )
                                        };
                                        if !filled.is_empty() {
                                            let effects = pending_reconstructions
                                                .lock()
                                                .on_blocks_accepted(&filled);
                                            if !effects.is_empty() {
                                                let _ = effects_tx.send(effects);
                                            }
                                        }
                                        // GC advancing between the snapshot and the
                                        // insert would miss this entry too: replay
                                        // the sweep so a dead frontier escalates
                                        // instead of waiting on a hook that already
                                        // fired.
                                        if gc_now > gc_round {
                                            let mut effects =
                                                crate::pending_reconstructions::AcceptanceEffects::default();
                                            effects.frontier_dead =
                                                pending_reconstructions.lock().on_gc(gc_now, tip);
                                            if !effects.is_empty() {
                                                let _ = effects_tx.send(effects);
                                            }
                                        }
                                    }
                                    Err(refusal) => {
                                        node_metrics
                                            .minimal_block_recovery_outcomes
                                            .with_label_values(&[refusal.metric_label()])
                                            .inc();
                                        // A refusal because the RECEIVER is behind the
                                        // claim (round past the window top) is the one
                                        // case with no other guaranteed recovery: the
                                        // live stream keeps the connection healthy, so
                                        // no timeout fires, and refused claims carry no
                                        // commit votes to trigger commit sync. Reset
                                        // for full-form replay — the jitter prevents a
                                        // committee-wide reconnect storm.
                                        let receiver_behind = matches!(
                                            refusal,
                                            crate::pending_reconstructions::AdmitRefusal::OutsideWindow
                                        ) && block_ref.round > credit_top;
                                        // Byte-cap refusals get the same escape: any
                                        // refusal that drops a live block needs the
                                        // replay path, and "the cap only binds under
                                        // attack" proved false at bootstrap.
                                        let cap_bound = matches!(
                                            refusal,
                                            crate::pending_reconstructions::AdmitRefusal::PeerBytes
                                                | crate::pending_reconstructions::AdmitRefusal::TotalBytes
                                        );
                                        if receiver_behind || cap_bound {
                                            info!(
                                                "Minimal block {} from {} {} is past the                                                  admission window; resetting subscription                                                  for full replay",
                                                block_ref, peer, peer_hostname
                                            );
                                            sleep(caused_reset_delay(
                                                context.own_index,
                                                peer,
                                                retries,
                                            ))
                                            .await;
                                            continue 'subscription;
                                        }
                                        if matches!(
                                            refusal,
                                            crate::pending_reconstructions::AdmitRefusal::DeadFrontierSlot
                                        ) && missing_block_registry
                                            .register_missing_block(block_ref)
                                            .await
                                            .is_err()
                                        {
                                            return;
                                        }
                                    }
                                }
                                continue 'stream;
                            }
                            Ok(InflateOutcome::Block(block)) => block,
                            Err(e) => {
                                // The stream is authenticated and reliable, so a
                                // malformed encoding is a peer fault, not noise:
                                // reset immediately, with escalating reconnect
                                // backoff as the penalty.
                                if matches!(e, ConsensusError::MalformedMinimalBlock(_)) {
                                    info!(
                                        "Malformed minimal block from peer {} {}; resetting subscription",
                                        peer, peer_hostname
                                    );
                                    sleep(caused_reset_delay(context.own_index, peer, retries))
                                        .await;
                                    continue 'subscription;
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
                    Ok(None) => {
                        debug!(
                            "Subscription to blocks from peer {} {} ended",
                            peer, peer_hostname
                        );
                        retries += 1;
                        break 'stream;
                    }
                    Err(_) => {
                        debug!(
                            "Timed out waiting for blocks from peer {} {} after {:?}",
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
        VerifiedBlock,
        block::{TestBlock, genesis_blocks},
        commit::{CommitDigest, CommitRange, CommitRef, TrustedCommit},
        commit_vote_monitor::COMMIT_LAG_MULTIPLIER,
        error::ConsensusResult,
        network::{BlockStream, ExtendedSerializedBlock, test_network::TestService},
        pending_reconstructions::MAX_PARKED_BYTES_PER_PEER,
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
            }
        }

        fn new_with_block_interval(interval: Duration) -> Self {
            Self {
                subscribe_calls: Mutex::new(Vec::new()),
                block_interval: Some(interval),
                hang_on_subscribe: false,
            }
        }

        fn new_hanging_subscribe() -> Self {
            Self {
                subscribe_calls: Mutex::new(Vec::new()),
                block_interval: None,
                hang_on_subscribe: true,
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
            let block_stream = stream::unfold((), move |_| async move {
                sleep(interval).await;
                let block = ExtendedSerializedBlock {
                    block: Bytes::from(vec![1u8; 8]),
                    minimal: None,
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

    #[derive(Default)]
    struct NoopRegistry {
        registered: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl crate::pending_reconstructions::MissingBlockRegistry for NoopRegistry {
        async fn register_missing_block(
            &self,
            _block_ref: BlockRef,
        ) -> crate::error::ConsensusResult<()> {
            self.registered
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }
    }

    fn test_subscriber_deps(
        context: &Arc<Context>,
    ) -> (
        Arc<dyn crate::pending_reconstructions::MissingBlockRegistry>,
        Arc<RwLock<RoundTracker>>,
        Arc<CommitVoteMonitor>,
    ) {
        (
            Arc::new(NoopRegistry::default()),
            Arc::new(RwLock::new(RoundTracker::new(context.clone(), vec![]))),
            Arc::new(CommitVoteMonitor::new(context.clone())),
        )
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
        let (registry, tracker, monitor) = test_subscriber_deps(&s.context);
        let subscriber = Subscriber::new(
            s.context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag.clone(),
            registry,
            tracker,
            monitor,
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

    /// A malformed minimal envelope — oversized payloads included — resets the
    /// stream immediately: the transport is authenticated and reliable, so a bad
    /// frame is a peer fault, not noise.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn malformed_minimal_resets_stream() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let peer = context.committee.to_authority_index(2).unwrap();
        let oversize = (context.protocol_config.max_transactions_in_block_bytes() as usize) * 2 + 1;
        let wire = vec![ExtendedSerializedBlock {
            block: Bytes::new(),
            minimal: Some(Bytes::from(vec![0u8; oversize])),
            excluded_ancestors: vec![],
        }];
        let mut client = FixedStreamClient::new(wire);
        // Quiet stream after the reset, so the malformed count stays at one pass.
        client.blocks_after_reset = Some(vec![]);
        let network_client = Arc::new(client);
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&context);
        let (registry, tracker, monitor) = test_subscriber_deps(&context);
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag,
            registry,
            tracker,
            monitor,
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
            1
        );
    }

    /// A claim past the admission window top means the RECEIVER is behind the
    /// stream. That refusal has no other guaranteed recovery — the live stream keeps
    /// the connection healthy so no timeout fires, and refused claims carry no
    /// commit votes — so it resets the subscription for full-form replay.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn far_behind_refusal_resets_for_full_replay() {
        // Claimed round 2001 vs receiver frontier 1499: far past frontier + gc_depth.
        let s = minimal_wire_scenario(2, 2000, 1);
        let network_client = Arc::new(FixedStreamClient::new(s.wire.clone()));
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&s.context);
        receiver_dag
            .write()
            .accept_block(VerifiedBlock::new_for_test(TestBlock::new(1499, 0).build()));
        let (registry, tracker, monitor) = test_subscriber_deps(&s.context);
        let subscriber = Subscriber::new(
            s.context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag,
            registry,
            tracker,
            monitor,
        );
        subscriber.subscribe(s.peer);

        let context = s.context.clone();
        wait_until(|| {
            context
                .metrics
                .node_metrics
                .minimal_block_recovery_outcomes
                .with_label_values(&["outside_window"])
                .get()
                >= 1
        })
        .await;
        // Nothing parked, and the stream reset for full replay.
        assert_eq!(
            context
                .metrics
                .node_metrics
                .minimal_block_recovery_parked
                .get(),
            0
        );
        wait_until(|| *network_client.subscribe_calls.lock() >= 2).await;
    }

    /// A parked minimal block must credit the round tracker's RECEIVED vector at
    /// receipt — before quota/horizon decisions — so pending_reconstructions cannot distort the
    /// propagation-delay signal for the park's duration. Accepted rows must not move.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn parked_block_credits_received_round_at_receipt() {
        let s = minimal_wire_scenario(2, 2, 1);
        let network_client = Arc::new(FixedStreamClient::new(s.wire.clone()));
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&s.context);
        let (registry, tracker, monitor) = test_subscriber_deps(&s.context);
        let subscriber = Subscriber::new(
            s.context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag.clone(),
            registry,
            tracker.clone(),
            monitor,
        );
        subscriber.subscribe(s.peer);

        let node_metrics = &s.context.metrics.node_metrics;
        wait_until(|| node_metrics.minimal_block_recovery_parked.get() == 1).await;
        // Parked, unsubmitted — yet already counted as received at the claimed round.
        assert!(authority_service.lock().handle_send_block.is_empty());
        assert_eq!(
            tracker.read().local_highest_received_rounds()[s.peer],
            s.blocks[0].round()
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
        let (registry, tracker, monitor) = test_subscriber_deps(&context);
        let subscriber = Subscriber::new(
            context.clone(),
            network_client,
            authority_service.clone(),
            dag_state,
            registry,
            tracker,
            monitor,
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
        let (registry, tracker, monitor) = test_subscriber_deps(&context);
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service,
            dag_state.clone(),
            registry,
            tracker,
            monitor,
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
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn subscriber_reconnects_when_stream_makes_no_progress() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let network_client = Arc::new(SubscriberTestClient::new_pending());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let (registry, tracker, monitor) = test_subscriber_deps(&context);
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service,
            dag_state,
            registry,
            tracker,
            monitor,
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
        let (registry, tracker, monitor) = test_subscriber_deps(&context);
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service,
            dag_state,
            registry,
            tracker,
            monitor,
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
        let (registry, tracker, monitor) = test_subscriber_deps(&context);
        let subscriber = Subscriber::new(
            context.clone(),
            network_client.clone(),
            authority_service.clone(),
            dag_state,
            registry,
            tracker,
            monitor,
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

    /// Observes commit votes at `index` from a quorum of authorities so
    /// `quorum_commit_index()` reaches it.
    fn observe_quorum_commit_votes(monitor: &CommitVoteMonitor, index: u32) {
        for authority in [0u32, 1, 3] {
            let block = VerifiedBlock::new_for_test(
                TestBlock::new(1, authority)
                    .set_commit_votes(vec![CommitRef::new(index, CommitDigest::MIN)])
                    .build(),
            );
            monitor.observe_block(&block);
        }
    }

    /// While local commits lag the quorum, an uninflatable minimal block is shed at
    /// receipt — before admission — with no stream reset, and anything already parked
    /// or held is quiesced so the byte caps cannot stay pinned. Once the lag clears,
    /// cap refusals resume the full-replay reset.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn sheds_at_receipt_while_commit_lagging_then_resets_after_catch_up() {
        let s = minimal_wire_scenario(2, 2, 3);
        let network_client = Arc::new(FixedStreamClient::new(vec![s.wire[0].clone()]));
        let (live_tx, live_rx) = tokio::sync::mpsc::unbounded_channel();
        *network_client.live.lock() = Some(live_rx);
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&s.context);
        let (registry, tracker, monitor) = test_subscriber_deps(&s.context);
        let lag_threshold = s.context.parameters.commit_sync_batch_size * COMMIT_LAG_MULTIPLIER;
        observe_quorum_commit_votes(&monitor, lag_threshold + 1);
        let subscriber = Subscriber::new(
            s.context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag.clone(),
            registry,
            tracker,
            monitor,
        );
        // Pre-held bytes stand in for parked state that quiesce must evict.
        subscriber.pending_reconstructions.lock().hold_sidecar(
            s.ancestors[1].reference(),
            s.peer,
            vec![vec![0u8; MAX_PARKED_BYTES_PER_PEER]],
        );
        subscriber.subscribe(s.peer);

        let node_metrics = &s.context.metrics.node_metrics;
        wait_until(|| {
            node_metrics
                .minimal_block_recovery_outcomes
                .with_label_values(&["shed_commit_lag"])
                .get()
                >= 1
        })
        .await;
        assert_eq!(*network_client.subscribe_calls.lock(), 1);
        assert!(
            node_metrics
                .minimal_block_recovery_outcomes
                .with_label_values(&["quiesced"])
                .get()
                >= 1,
            "held bytes must be evicted on entry into commit-lag shedding"
        );
        assert_eq!(node_metrics.minimal_block_recovery_parked.get(), 0);
        assert_eq!(node_metrics.minimal_block_recovery_parked_bytes.get(), 0);

        // Catch up: one commit clears the lag; a cap refusal then resets as always.
        receiver_dag.write().accept_block(s.ancestors[0].clone());
        receiver_dag.write().add_commit(TrustedCommit::new_for_test(
            1,
            CommitDigest::MIN,
            0,
            s.ancestors[0].reference(),
            vec![],
        ));
        subscriber.pending_reconstructions.lock().hold_sidecar(
            s.ancestors[2].reference(),
            s.peer,
            vec![vec![0u8; MAX_PARKED_BYTES_PER_PEER]],
        );
        // First post-lag block consumes the exact-lane anchor; the second exercises
        // the restored cap-refusal reset.
        live_tx.send(s.wire[1].clone()).unwrap();
        live_tx.send(s.wire[2].clone()).unwrap();
        wait_until(|| *network_client.subscribe_calls.lock() >= 2).await;
        assert_eq!(
            node_metrics
                .minimal_block_recovery_outcomes
                .with_label_values(&["shed_commit_lag"])
                .get(),
            1,
            "shedding must stop once commits are caught up"
        );
    }

    /// A minimal block that cannot be inflated still feeds its author's commit votes
    /// to the monitor at receipt — the invariant full-form verification provides.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn uninflatable_minimal_feeds_commit_vote_monitor() {
        let s = minimal_wire_scenario(2, 2, 1);
        // Rebuild the streamed block with commit votes attached, minimally encoded
        // against a sender DAG that holds its ancestors.
        let sender_dag = empty_receiver_dag(&s.context);
        for ancestor in &s.ancestors {
            sender_dag.write().accept_block(ancestor.clone());
        }
        let mut refs: Vec<_> = s.ancestors.iter().map(|b| b.reference()).collect();
        refs.sort_by_key(|r| (r.author != s.peer, r.author));
        let voted = VerifiedBlock::new_for_test(
            TestBlock::new(3, s.peer.value() as u32)
                .set_ancestors_raw(refs)
                .set_commit_votes(vec![CommitRef::new(5, CommitDigest::MIN)])
                .build(),
        );
        let minimal = BlockInflater::new(s.context.clone())
            .serialize(&voted, &sender_dag.read())
            .unwrap();
        let wire = vec![ExtendedSerializedBlock {
            block: voted.serialized().clone(),
            minimal: Some(minimal),
            excluded_ancestors: vec![],
        }];
        let network_client = Arc::new(FixedStreamClient::new(wire));
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&s.context);
        let (registry, tracker, monitor) = test_subscriber_deps(&s.context);
        // Two other columns already vote index 5; the streamed claim completes the quorum.
        for authority in [0u32, 1] {
            let block = VerifiedBlock::new_for_test(
                TestBlock::new(1, authority)
                    .set_commit_votes(vec![CommitRef::new(5, CommitDigest::MIN)])
                    .build(),
            );
            monitor.observe_block(&block);
        }
        assert_eq!(monitor.quorum_commit_index(), 0);
        let subscriber = Subscriber::new(
            s.context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag,
            registry,
            tracker,
            monitor.clone(),
        );
        subscriber.subscribe(s.peer);

        let node_metrics = &s.context.metrics.node_metrics;
        wait_until(|| node_metrics.minimal_block_recovery_parked.get() == 1).await;
        assert_eq!(
            monitor.quorum_commit_index(),
            5,
            "the parked block's votes must reach the monitor at receipt"
        );
        assert_eq!(*network_client.subscribe_calls.lock(), 1);
    }

    /// The first uninflatable minimal block after commit lag clears must anchor
    /// recovery through the exact-fetch lane (like full mode's exact ancestor
    /// scheduling), not wait on slot repair.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn first_uninflatable_block_after_lag_exit_routes_to_exact_lane() {
        let s = minimal_wire_scenario(2, 2, 2);
        let network_client = Arc::new(FixedStreamClient::new(vec![s.wire[0].clone()]));
        let (live_tx, live_rx) = tokio::sync::mpsc::unbounded_channel();
        *network_client.live.lock() = Some(live_rx);
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&s.context);
        let registry = Arc::new(NoopRegistry::default());
        let tracker = Arc::new(RwLock::new(RoundTracker::new(s.context.clone(), vec![])));
        let monitor = Arc::new(CommitVoteMonitor::new(s.context.clone()));
        let lag_threshold = s.context.parameters.commit_sync_batch_size * COMMIT_LAG_MULTIPLIER;
        observe_quorum_commit_votes(&monitor, lag_threshold + 1);
        let subscriber = Subscriber::new(
            s.context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag.clone(),
            registry.clone(),
            tracker,
            monitor,
        );
        subscriber.subscribe(s.peer);

        let node_metrics = &s.context.metrics.node_metrics;
        wait_until(|| {
            node_metrics
                .minimal_block_recovery_outcomes
                .with_label_values(&["shed_commit_lag"])
                .get()
                >= 1
        })
        .await;
        assert_eq!(
            registry
                .registered
                .load(std::sync::atomic::Ordering::Relaxed),
            0,
            "shed blocks must not feed the lane while lagging"
        );

        receiver_dag.write().accept_block(s.ancestors[0].clone());
        receiver_dag.write().add_commit(TrustedCommit::new_for_test(
            1,
            CommitDigest::MIN,
            0,
            s.ancestors[0].reference(),
            vec![],
        ));
        live_tx.send(s.wire[1].clone()).unwrap();
        wait_until(|| {
            registry
                .registered
                .load(std::sync::atomic::Ordering::Relaxed)
                >= 1
        })
        .await;
        assert_eq!(node_metrics.minimal_block_recovery_parked.get(), 0);
        assert_eq!(*network_client.subscribe_calls.lock(), 1);
        assert_eq!(
            node_metrics
                .minimal_block_recovery_outcomes
                .with_label_values(&["shed_commit_lag"])
                .get(),
            1
        );
    }

    /// Without commit lag a byte-cap refusal keeps its full-replay reset.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn cap_refusal_resets_when_commits_current() {
        let s = minimal_wire_scenario(2, 2, 1);
        let network_client = Arc::new(FixedStreamClient::new(s.wire.clone()));
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&s.context);
        let (registry, tracker, monitor) = test_subscriber_deps(&s.context);
        let subscriber = Subscriber::new(
            s.context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag,
            registry,
            tracker,
            monitor,
        );
        subscriber.pending_reconstructions.lock().hold_sidecar(
            s.ancestors[1].reference(),
            s.peer,
            vec![vec![0u8; MAX_PARKED_BYTES_PER_PEER]],
        );
        subscriber.subscribe(s.peer);

        wait_until(|| *network_client.subscribe_calls.lock() >= 2).await;
        assert_eq!(
            s.context
                .metrics
                .node_metrics
                .minimal_block_recovery_outcomes
                .with_label_values(&["shed_commit_lag"])
                .get(),
            0
        );
    }

    /// The shed exception must not fire at the exact lag threshold: equality is not
    /// lagging, so the reset path applies.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn cap_refusal_at_exact_lag_threshold_resets() {
        let s = minimal_wire_scenario(2, 2, 1);
        let network_client = Arc::new(FixedStreamClient::new(s.wire.clone()));
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&s.context);
        let (registry, tracker, monitor) = test_subscriber_deps(&s.context);
        let lag_threshold = s.context.parameters.commit_sync_batch_size * COMMIT_LAG_MULTIPLIER;
        observe_quorum_commit_votes(&monitor, lag_threshold);
        let subscriber = Subscriber::new(
            s.context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag,
            registry,
            tracker,
            monitor,
        );
        subscriber.pending_reconstructions.lock().hold_sidecar(
            s.ancestors[1].reference(),
            s.peer,
            vec![vec![0u8; MAX_PARKED_BYTES_PER_PEER]],
        );
        subscriber.subscribe(s.peer);

        wait_until(|| *network_client.subscribe_calls.lock() >= 2).await;
        assert_eq!(
            s.context
                .metrics
                .node_metrics
                .minimal_block_recovery_outcomes
                .with_label_values(&["shed_commit_lag"])
                .get(),
            0
        );
    }

    /// The wavefront breaker: a parked block whose missing ancestor is only CLAIMED
    /// (its envelope observed, its content not accepted) reconstructs through the
    /// claim resolver and reaches the service as a full block — where block_manager
    /// suspension takes over, exactly like full-form mode.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn parked_block_reconstructs_from_claim_without_acceptance() {
        let s = minimal_wire_scenario(2, 2, 1);
        // The streamed block's ancestors are the four round-2 blocks; accept all but
        // author 1's at the receiver, so the block parks missing exactly that slot.
        let network_client = Arc::new(FixedStreamClient::new(s.wire.clone()));
        let authority_service = Arc::new(Mutex::new(TestService::new()));
        let receiver_dag = empty_receiver_dag(&s.context);
        let missing_author = s.context.committee.to_authority_index(1).unwrap();
        let mut missing_digest = None;
        for ancestor in &s.ancestors {
            if ancestor.author() == missing_author {
                missing_digest = Some(ancestor.digest());
                continue;
            }
            receiver_dag.write().accept_block(ancestor.clone());
        }
        let (registry, tracker, monitor) = test_subscriber_deps(&s.context);
        let subscriber = Subscriber::new(
            s.context.clone(),
            network_client.clone(),
            authority_service.clone(),
            receiver_dag.clone(),
            registry,
            tracker,
            monitor,
        );
        subscriber.subscribe(s.peer);

        let node_metrics = &s.context.metrics.node_metrics;
        wait_until(|| node_metrics.minimal_block_recovery_parked.get() == 1).await;
        assert!(authority_service.lock().handle_send_block.is_empty());

        // The missing author's envelope claim arrives (content still not accepted).
        let missing_slot = crate::block::Slot::new(2, missing_author);
        let ready = subscriber
            .pending_reconstructions
            .lock()
            .observe_claim(missing_slot, missing_digest.unwrap());
        assert_eq!(ready.len(), 1);
        subscriber
            .effects_tx
            .send(crate::pending_reconstructions::AcceptanceEffects {
                ready,
                ..Default::default()
            })
            .unwrap();

        wait_until(|| !authority_service.lock().handle_send_block.is_empty()).await;
        let received = authority_service.lock().handle_send_block[0].1.clone();
        assert_eq!(&received.block, s.blocks[0].serialized());
    }
}
