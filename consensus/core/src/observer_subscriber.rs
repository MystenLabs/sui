// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use consensus_types::block::Round;
use futures::StreamExt;
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use parking_lot::Mutex;
use tokio::{
    sync::oneshot,
    task::{JoinError, JoinHandle, JoinSet},
    time::sleep,
};
use tracing::{debug, info, warn};

use crate::{
    block::BlockAPI,
    commit_vote_monitor::{CommitVoteMonitor, is_commit_lagging},
    context::Context,
    dag_state::DagState,
    error::ConsensusError,
    network::{ObserverNetworkClient, ObserverNetworkService, PeerId, RandomnessSignatureHandler},
    task::{join_and_propagate_panic, reap_finished_task, shutdown_join_set},
};

/// Number of commit-sync batches of lag below which a suspended subscription is resumed.
/// The subscription is suspended at `COMMIT_LAG_MULTIPLIER` (5) batches of lag, where the
/// observer service starts rejecting streamed blocks anyway; the band between the two
/// thresholds is hysteresis preventing rapid suspend/resume flapping.
///
/// The value must stay within `1..=COMMIT_LAG_MULTIPLIER`:
/// - `>= 1` is load-bearing for liveness: commit sync never fetches partial batches, so
///   it can leave up to `commit_sync_batch_size - 1` commits of residual lag that only
///   streamed blocks can close. With `0`, the resume condition (zero lag) would be
///   unreachable and the subscription would stay suspended forever.
/// - `<= COMMIT_LAG_MULTIPLIER`, or the subscription resumes while still commit-lagging
///   and the in-stream lag check immediately drops it again, churning reconnects until
///   commit sync shrinks the lag below the suspension threshold.
const RESUBSCRIBE_LAG_BATCHES: u32 = 1;

/// How often a suspended subscription re-evaluates commit lag.
const SUSPENSION_CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// ObserverSubscriber manages block stream subscriptions to peers (validators or other observers),
/// taking care of retrying when subscription streams break. Blocks returned from peers are sent
/// to the observer service for processing. The `ObserverSubscriber` can only subscribe to one peer at a time.
///
/// While the local commit index lags the quorum commit index too much, streamed blocks
/// would be rejected by the observer service and re-fetched later via commit sync, so the
/// subscription drops its stream and holds off reconnecting until commit sync has nearly
/// caught up, saving the bandwidth and verification on both ends. While suspended, the quorum
/// commit index stays effectively frozen: commit sync only fetches up to the already-known
/// quorum commit index, so its observed commit votes cannot push it further. Instead, each
/// resubscription refreshes the quorum commit index from the freshly streamed blocks, and
/// if the node is still too far behind, the subscription is suspended again — so a deep
/// catch-up proceeds as a bounded cycle of suspend, catch up, briefly resubscribe, re-suspend.
pub(crate) struct ObserverSubscriber<C: ObserverNetworkClient, S: ObserverNetworkService> {
    context: Arc<Context>,
    network_client: Arc<C>,
    observer_service: Arc<S>,
    commit_vote_monitor: Arc<CommitVoteMonitor>,
    dag_state: Arc<parking_lot::RwLock<DagState>>,
    subscriptions: Mutex<Subscriptions>,
    randomness_signature_handler: Option<Arc<dyn RandomnessSignatureHandler>>,
}

/// The current and retired subscriptions are tracked under a single lock, so replacing the
/// current subscription and retiring the previous one is atomic w.r.t. concurrent subscribe()
/// and stop() calls.
struct Subscriptions {
    current: Option<ObserverSubscription>,
    // Retain replaced subscription tasks so stop() can await them and propagate panics.
    retired: Vec<ObserverSubscription>,
}

struct ObserverSubscription {
    // Signals the subscription task to stop cooperatively, instead of aborting it. Aborting
    // would drop the block handler JoinSet owned by the task, which aborts the handlers without
    // awaiting them: they could briefly keep running (holding strong references to the observer
    // service) and any panic recorded in the set would be discarded. On this signal the task
    // stops streaming, then cancels the remaining block handlers and awaits their termination,
    // so join() returns only after no block handler is running and recorded handler panics
    // have propagated.
    shutdown_sender: Option<oneshot::Sender<()>>,
    subscription_task: JoinHandle<()>,
}

impl ObserverSubscription {
    fn request_shutdown(&mut self) {
        if let Some(sender) = self.shutdown_sender.take() {
            let _ = sender.send(());
        }
    }

    async fn join(self) {
        join_and_propagate_panic(self.subscription_task).await;
    }
}

impl<C: ObserverNetworkClient, S: ObserverNetworkService> ObserverSubscriber<C, S> {
    pub(crate) fn new(
        context: Arc<Context>,
        network_client: Arc<C>,
        observer_service: Arc<S>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        dag_state: Arc<parking_lot::RwLock<DagState>>,
        randomness_signature_handler: Option<Arc<dyn RandomnessSignatureHandler>>,
    ) -> Self {
        Self {
            context,
            network_client,
            observer_service,
            commit_vote_monitor,
            dag_state,
            subscriptions: Mutex::new(Subscriptions {
                current: None,
                retired: Vec::new(),
            }),
            randomness_signature_handler,
        }
    }

    /// Subscribe to a peer (validator or observer) to receive block streams. The `ObserverSubscriber` can only subscribe to one peer at a time.
    /// The method will stop the existing subscription (if any) and start a new one.
    pub(crate) fn subscribe(&self, peer: PeerId) {
        let context = self.context.clone();
        let network_client = self.network_client.clone();
        // ObserverSubscriber already holds these resources strongly. Give subscription tasks weak
        // references so they do not become additional owners during shutdown.
        let observer_service = Arc::downgrade(&self.observer_service);
        let commit_vote_monitor = self.commit_vote_monitor.clone();
        let dag_state = Arc::downgrade(&self.dag_state);
        let randomness_signature_handler = self.randomness_signature_handler.clone();

        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let subscription_task = spawn_monitored_task!(Self::subscription_loop(
            context,
            network_client,
            observer_service,
            commit_vote_monitor,
            dag_state,
            peer,
            randomness_signature_handler,
            shutdown_receiver,
        ));

        let mut subscriptions = self.subscriptions.lock();
        // Reap retired subscriptions that have finished, so the list stays bounded under
        // repeated resubscriptions.
        subscriptions
            .retired
            .retain_mut(|subscription| !reap_finished_task(&mut subscription.subscription_task));
        let previous_subscription = subscriptions.current.replace(ObserverSubscription {
            shutdown_sender: Some(shutdown_sender),
            subscription_task,
        });
        if let Some(mut previous_subscription) = previous_subscription {
            previous_subscription.request_shutdown();
            subscriptions.retired.push(previous_subscription);
        }
    }

    /// Stop the active subscription (if any).
    pub(crate) async fn stop(&self) {
        let mut subscriptions = {
            let mut subscriptions = self.subscriptions.lock();
            let mut all = std::mem::take(&mut subscriptions.retired);
            all.extend(subscriptions.current.take());
            all
        };
        for subscription in &mut subscriptions {
            subscription.request_shutdown();
        }
        for subscription in subscriptions {
            subscription.join().await;
        }
    }

    async fn subscription_loop(
        context: Arc<Context>,
        network_client: Arc<C>,
        observer_service: Weak<S>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        dag_state: Weak<parking_lot::RwLock<DagState>>,
        peer: PeerId,
        randomness_signature_handler: Option<Arc<dyn RandomnessSignatureHandler>>,
        mut shutdown_receiver: oneshot::Receiver<()>,
    ) {
        let mut tasks = JoinSet::new();
        {
            // The `subscription` future borrows `tasks` mutably to spawn block handlers, so it
            // is scoped to this block. When shutdown is signaled, the select below returns and
            // `subscription` is dropped at the end of the block. This stops block streaming, but
            // the current task (subscription_loop) and the block handlers already spawned into
            // `tasks` keep running. Dropping `subscription` also ends the `&mut tasks` borrow,
            // so the block handlers in `tasks` can be awaited below.
            let subscription = Self::run_subscription(
                context.clone(),
                network_client,
                observer_service,
                commit_vote_monitor,
                dag_state,
                peer,
                randomness_signature_handler,
                &mut tasks,
            );
            tokio::pin!(subscription);
            tokio::select! {
                _ = &mut shutdown_receiver => {}
                _ = &mut subscription => {}
            }
        }

        shutdown_join_set(&mut tasks).await;

        // The task may have been torn down mid-suspension (peer switch or stop), leaving
        // the gauge at 1 with no task left to clear it. Reset it here; if a replacement
        // task is genuinely suspended, it re-asserts 1 within SUSPENSION_CHECK_INTERVAL.
        context
            .metrics
            .node_metrics
            .observer_subscription_suspended
            .set(0);
    }

    async fn run_subscription(
        context: Arc<Context>,
        network_client: Arc<C>,
        observer_service: Weak<S>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        dag_state: Weak<parking_lot::RwLock<DagState>>,
        peer: PeerId,
        randomness_signature_handler: Option<Arc<dyn RandomnessSignatureHandler>>,
        tasks: &mut JoinSet<()>,
    ) {
        const IMMEDIATE_RETRIES: i64 = 3;
        const MIN_TIMEOUT: Duration = Duration::from_millis(500);
        let mut backoff = mysten_common::backoff::ExponentialBackoff::new(
            Duration::from_millis(100),
            Duration::from_secs(10),
        );
        let mut retries: i64 = 0;

        'subscription: loop {
            let mut delay = Duration::ZERO;
            if retries > IMMEDIATE_RETRIES {
                delay = backoff.next().unwrap();
                debug!(
                    "Delaying retry {} of peer {:?} subscription, in {} seconds",
                    retries,
                    peer.clone(),
                    delay.as_secs_f32(),
                );
                sleep(delay).await;
            } else if retries > 0 {
                tokio::task::yield_now().await;
            }
            retries += 1;

            // Hold off (re)connecting while commit-lagging: streamed blocks would be
            // rejected by the observer service and later re-fetched via commit sync.
            if !Self::wait_while_commit_lagging(&context, &commit_vote_monitor, &dag_state).await {
                return;
            }

            // Recompute highest rounds from DagState before each connection attempt
            // so reconnections resume from where we left off rather than re-fetching
            // already-seen blocks. Clamp to the GC round, since blocks below it would
            // be skipped anyway.
            let highest_round_per_authority = {
                let Some(dag_state) = dag_state.upgrade() else {
                    return;
                };
                let ds = dag_state.read();
                let gc_round = ds.gc_round();
                let mut rounds = vec![0 as Round; context.committee.size()];
                for (authority, _) in context.committee.authorities() {
                    rounds[authority.value()] = ds
                        .get_last_block_for_authority(authority)
                        .round()
                        .max(gc_round);
                }
                rounds
            };

            // Subscribe to stream blocks from the peer.
            let request_timeout = MIN_TIMEOUT.max(delay);
            let mut blocks = match network_client
                .stream_blocks(peer.clone(), highest_round_per_authority, request_timeout)
                .await
            {
                Ok(blocks) => {
                    debug!("Subscribed to peer {:?} after {} attempts", peer, retries);
                    blocks
                }
                Err(e) => {
                    debug!("Failed to subscribe to blocks from peer {:?}: {}", peer, e);
                    continue 'subscription;
                }
            };

            let max_parallel_tasks = context.committee.size();
            'stream: loop {
                let _scope = monitored_scope("ObserverSubscriberStreamConsumer");

                let next_item = tokio::select! {
                    result = tasks.join_next(), if !tasks.is_empty() => {
                        Self::handle_task_result(result);
                        continue 'stream;
                    }
                    item = blocks.next(), if tasks.len() < max_parallel_tasks => item,
                };

                match next_item {
                    Some(item) => {
                        context
                            .metrics
                            .node_metrics
                            .observer_subscribed_blocks_batch_size
                            .observe(item.blocks.len() as f64);

                        // During catch-up (commit lagging behind quorum), streamed blocks
                        // would be rejected by the observer service and re-fetched via
                        // commit sync, so drop the stream and hold off reconnecting until
                        // nearly caught up, saving the bandwidth on both ends. The
                        // disconnection is deliberate, so reconnect without backoff.
                        let is_commit_lagging = dag_state.upgrade().is_none_or(|dag_state| {
                            is_commit_lagging(
                                &context,
                                dag_state.read().last_commit_index(),
                                commit_vote_monitor.quorum_commit_index(),
                            )
                        });
                        if is_commit_lagging {
                            retries = 0;
                            backoff.reset();
                            break 'stream;
                        }

                        if let Some(handler) = &randomness_signature_handler {
                            for sig in item.auxiliary_data.randomness_signatures {
                                handler.handle_randomness_signature(sig);
                            }
                        }

                        for block in item.blocks {
                            // Backpressure: wait if we've hit max parallelism.
                            while tasks.len() >= max_parallel_tasks {
                                Self::handle_task_result(tasks.join_next().await);
                            }

                            let observer_service = observer_service.clone();
                            let peer_cloned = peer.clone();
                            tasks.spawn(async move {
                                let _scope =
                                    monitored_scope("ObserverSubscriberTask::handle_block");

                                let Some(observer_service) = observer_service.upgrade() else {
                                    return;
                                };
                                let result = observer_service
                                    .handle_block(peer_cloned.clone(), block)
                                    .await;
                                if let Err(e) = result {
                                    match e {
                                        ConsensusError::BlockRejected {
                                            block_ref,
                                            reason,
                                        } => {
                                            debug!(
                                                "Failed to process block from peer for block {:?}: {}",
                                                block_ref, reason
                                            );
                                        }
                                        _ => {
                                            info!("Received invalid block from peer: {}", e);
                                        }
                                    }
                                }
                            });
                        }

                        // Reset retries when a block is received and also reset the backoff.
                        retries = 0;
                        backoff.reset();
                    }
                    None => {
                        debug!("Subscription to blocks from peer {:?} ended", peer);
                        retries += 1;
                        break 'stream;
                    }
                }
            }

            while !tasks.is_empty() {
                Self::handle_task_result(tasks.join_next().await);
            }
        }
    }

    // While the local commit index lags the quorum commit index too much, waits for
    // commit sync to bring it within `RESUBSCRIBE_LAG_BATCHES` batches before returning;
    // the band between the suspension and resume thresholds is hysteresis preventing
    // rapid flapping. While waiting, the quorum commit index stays effectively frozen:
    // with the stream suspended no new blocks — and hence no new commit votes — arrive
    // from the peer, and the blocks fetched by commit sync only carry votes for commits
    // at or below the already-known quorum commit index, so they cannot advance it.
    // Only once the subscription resumes do the freshly streamed higher-round blocks
    // carry votes that push the quorum commit index forward; if that reveals the node
    // is still too far behind, the stream is dropped and suspended again.
    // Returns false when the node is shutting down.
    async fn wait_while_commit_lagging(
        context: &Context,
        commit_vote_monitor: &CommitVoteMonitor,
        dag_state: &Weak<parking_lot::RwLock<DagState>>,
    ) -> bool {
        let mut suspended = false;
        loop {
            let Some(dag_state) = dag_state.upgrade() else {
                return false;
            };
            let local_commit_index = dag_state.read().last_commit_index();
            drop(dag_state);
            let quorum_commit_index = commit_vote_monitor.quorum_commit_index();

            let caught_up = if suspended {
                quorum_commit_index.saturating_sub(local_commit_index)
                    <= context.parameters.commit_sync_batch_size * RESUBSCRIBE_LAG_BATCHES
            } else {
                !is_commit_lagging(context, local_commit_index, quorum_commit_index)
            };
            // The gauge writes are level-triggered (re-asserted every iteration) rather
            // than transition-triggered: a task torn down mid-suspension (peer switch or
            // stop) never performs the closing transition, so a stale value must be
            // overwritten by whichever task evaluates the suspension next. Logs stay on
            // transitions.
            if caught_up {
                if suspended {
                    info!(
                        "Resuming block stream subscription: local commit index {} caught up with quorum commit index {}",
                        local_commit_index, quorum_commit_index,
                    );
                }
                context
                    .metrics
                    .node_metrics
                    .observer_subscription_suspended
                    .set(0);
                return true;
            }
            if !suspended {
                suspended = true;
                info!(
                    "Suspending block stream subscription: local commit index {} lags quorum commit index {}, blocks will arrive via commit sync",
                    local_commit_index, quorum_commit_index,
                );
            }
            context
                .metrics
                .node_metrics
                .observer_subscription_suspended
                .set(1);
            sleep(SUSPENSION_CHECK_INTERVAL).await;
        }
    }

    fn handle_task_result(result: Option<Result<(), JoinError>>) {
        if let Some(Err(error)) = result {
            if error.is_panic() {
                std::panic::resume_unwind(error.into_panic());
            }
            warn!("Observer block handler task was cancelled: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{future::pending, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use bytes::Bytes;
    use consensus_types::block::{BlockRef, Round};
    use futures::stream;
    use parking_lot::{Mutex, RwLock};
    use tokio::{
        sync::Notify,
        time::{sleep, timeout},
    };

    use super::*;
    use crate::{
        VerifiedBlock,
        commit::{CommitRange, TrustedCommit},
        context::Context,
        error::ConsensusResult,
        network::{NodeId, ObserverBlockStream, ObserverStreamItem},
        storage::mem_store::MemStore,
    };

    struct ObserverSubscriberTestClient {
        stream_blocks_calls: std::sync::atomic::AtomicU32,
        // `None` streams blocks forever, so a subscription can only end through the
        // subscriber's own logic (e.g. the in-stream commit lag check), never because
        // the fake stream hung up on its own.
        stream_limit: Option<usize>,
    }

    impl ObserverSubscriberTestClient {
        fn new() -> Self {
            Self {
                stream_blocks_calls: std::sync::atomic::AtomicU32::new(0),
                stream_limit: Some(10),
            }
        }

        fn new_never_ending() -> Self {
            Self {
                stream_blocks_calls: std::sync::atomic::AtomicU32::new(0),
                stream_limit: None,
            }
        }

        fn stream_blocks_calls(&self) -> u32 {
            self.stream_blocks_calls
                .load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    #[async_trait]
    impl ObserverNetworkClient for ObserverSubscriberTestClient {
        async fn stream_blocks(
            &self,
            peer: PeerId,
            _highest_round_per_authority: Vec<Round>,
            _timeout: Duration,
        ) -> ConsensusResult<ObserverBlockStream> {
            self.stream_blocks_calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // Return different block content based on peer to distinguish them in tests
            let block_value = match peer {
                PeerId::Validator(idx) => idx.value() as u8 + 1,
                PeerId::Observer(_) => 99u8,
            };

            let block_stream = stream::unfold(block_value, move |val| async move {
                sleep(Duration::from_millis(1)).await;
                Some((
                    ObserverStreamItem {
                        blocks: vec![Bytes::from(vec![val; 8])],
                        auxiliary_data: Default::default(),
                    },
                    val,
                ))
            });
            Ok(match self.stream_limit {
                Some(limit) => Box::pin(block_stream.take(limit)),
                None => Box::pin(block_stream),
            })
        }

        async fn fetch_blocks(
            &self,
            _peer: PeerId,
            _block_refs: Vec<BlockRef>,
            _fetch_after_rounds: Vec<Round>,
            _fetch_missing_ancestors: bool,
            _timeout: Duration,
        ) -> ConsensusResult<Vec<Bytes>> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_commits(
            &self,
            _peer: PeerId,
            _commit_range: CommitRange,
            _timeout: Duration,
        ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
            unimplemented!("Unimplemented")
        }
    }

    struct ObserverSubscriberTestService {
        handle_block_calls: Mutex<Vec<(PeerId, Bytes)>>,
        block_handlers: bool,
        handler_started: Notify,
    }

    impl ObserverSubscriberTestService {
        fn new() -> Self {
            Self {
                handle_block_calls: Mutex::new(Vec::new()),
                block_handlers: false,
                handler_started: Notify::new(),
            }
        }

        fn new_blocking() -> Self {
            Self {
                handle_block_calls: Mutex::new(Vec::new()),
                block_handlers: true,
                handler_started: Notify::new(),
            }
        }
    }

    #[async_trait]
    impl ObserverNetworkService for ObserverSubscriberTestService {
        async fn handle_block(&self, peer: PeerId, block: Bytes) -> ConsensusResult<()> {
            self.handle_block_calls.lock().push((peer, block));
            self.handler_started.notify_one();
            if self.block_handlers {
                pending::<()>().await;
            }
            Ok(())
        }

        async fn handle_stream_blocks(
            &self,
            _peer: NodeId,
            _highest_round_per_authority: Vec<Round>,
        ) -> ConsensusResult<ObserverBlockStream> {
            unimplemented!("Unimplemented")
        }

        async fn handle_fetch_blocks(
            &self,
            _peer: NodeId,
            _block_refs: Vec<BlockRef>,
            _fetch_after_rounds: Vec<Round>,
            _fetch_missing_ancestors: bool,
        ) -> ConsensusResult<Vec<Bytes>> {
            unimplemented!("Unimplemented")
        }

        async fn handle_fetch_commits(
            &self,
            _peer: NodeId,
            _commit_range: CommitRange,
        ) -> ConsensusResult<(Vec<TrustedCommit>, Vec<VerifiedBlock>)> {
            unimplemented!("Unimplemented")
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_observer_subscriber_retries() {
        telemetry_subscribers::init_for_testing();
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let observer_service = Arc::new(ObserverSubscriberTestService::new());
        let network_client = Arc::new(ObserverSubscriberTestClient::new());
        let store = Arc::new(MemStore::new());
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        let subscriber = ObserverSubscriber::new(
            context.clone(),
            network_client,
            observer_service.clone(),
            commit_vote_monitor,
            dag_state,
            None,
        );

        // Subscribe to a validator peer
        let peer = PeerId::Validator(context.committee.to_authority_index(2).unwrap());
        subscriber.subscribe(peer.clone());

        // Wait for enough blocks to be received
        for _ in 0..10 {
            sleep(Duration::from_secs(1)).await;
            let calls = observer_service.handle_block_calls.lock();
            if calls.len() >= 100 {
                break;
            }
        }

        // Even if the stream ends after 10 blocks, the subscriber should retry and get enough
        // blocks eventually.
        let calls = observer_service.handle_block_calls.lock();
        assert!(calls.len() >= 100);
        for (p, block) in calls.iter() {
            assert_eq!(*p, peer);
            assert_eq!(*block, Bytes::from(vec![3u8; 8])); // Peer index 2 + 1 = 3
        }
    }

    #[tokio::test]
    async fn test_observer_subscriber_override() {
        telemetry_subscribers::init_for_testing();
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let observer_service = Arc::new(ObserverSubscriberTestService::new());
        let network_client = Arc::new(ObserverSubscriberTestClient::new());
        let store = Arc::new(MemStore::new());
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        let subscriber = ObserverSubscriber::new(
            context.clone(),
            network_client,
            observer_service.clone(),
            commit_vote_monitor,
            dag_state,
            None,
        );

        // Subscribe to first peer (validator 0)
        let peer1 = PeerId::Validator(context.committee.to_authority_index(0).unwrap());
        subscriber.subscribe(peer1.clone());

        // Wait for some blocks to be received from peer1
        sleep(Duration::from_millis(50)).await;
        {
            let calls = observer_service.handle_block_calls.lock();
            assert!(!calls.is_empty(), "Should have received blocks from peer1");
            // Verify blocks are from peer1 (value = 0 + 1 = 1)
            for (p, block) in calls.iter() {
                assert_eq!(*p, peer1);
                assert_eq!(*block, Bytes::from(vec![1u8; 8]));
            }
        }

        // Clear the received blocks for clarity
        observer_service.handle_block_calls.lock().clear();

        // Subscribe to second peer (validator 2) - this should override the first subscription
        let peer2 = PeerId::Validator(context.committee.to_authority_index(2).unwrap());
        subscriber.subscribe(peer2.clone());

        // Wait for blocks from the new peer
        sleep(Duration::from_millis(100)).await;
        {
            let calls = observer_service.handle_block_calls.lock();
            assert!(!calls.is_empty(), "Should have received blocks from peer2");
            // Verify ALL blocks are from peer2 (value = 2 + 1 = 3), none from peer1
            for (p, block) in calls.iter() {
                assert_eq!(*p, peer2, "All blocks should be from peer2 after override");
                assert_eq!(
                    *block,
                    Bytes::from(vec![3u8; 8]),
                    "Block content should match peer2"
                );
            }
        }

        // Clear blocks again
        let count_before_stop = observer_service.handle_block_calls.lock().len();

        // Test that stop() still works
        subscriber.stop().await;

        // Wait and verify no new blocks are received after stop
        sleep(Duration::from_millis(50)).await;
        let count_after_stop = observer_service.handle_block_calls.lock().len();
        assert_eq!(
            count_before_stop, count_after_stop,
            "No new blocks should be received after stop()"
        );
    }

    #[tokio::test]
    async fn test_stop_waits_for_block_handlers() {
        telemetry_subscribers::init_for_testing();
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let observer_service = Arc::new(ObserverSubscriberTestService::new_blocking());
        let network_client = Arc::new(ObserverSubscriberTestClient::new());
        let store = Arc::new(MemStore::new());
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        let subscriber = ObserverSubscriber::new(
            context.clone(),
            network_client,
            observer_service.clone(),
            commit_vote_monitor,
            dag_state,
            None,
        );

        let peer = PeerId::Validator(context.committee.to_authority_index(0).unwrap());
        subscriber.subscribe(peer);
        timeout(
            Duration::from_secs(1),
            observer_service.handler_started.notified(),
        )
        .await
        .expect("Block handler should start");
        assert!(Arc::strong_count(&observer_service) > 2);

        timeout(Duration::from_secs(1), subscriber.stop())
            .await
            .expect("Subscriber should stop after cancelling block handlers");
        assert_eq!(Arc::strong_count(&observer_service), 2);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_suspend_subscription_on_commit_lag_and_resume() {
        use consensus_config::Parameters;
        use consensus_types::block::BlockDigest;

        use crate::{
            block::TestBlock,
            commit::{CommitDigest, CommitRef},
        };

        telemetry_subscribers::init_for_testing();
        let (mut context, _keys) = Context::new_for_test(4);
        // Suspension threshold is batch_size * 5 = 25 commits of lag, resume at <= 5.
        context.parameters = Parameters {
            commit_sync_batch_size: 5,
            ..context.parameters
        };
        let context = Arc::new(context);
        let observer_service = Arc::new(ObserverSubscriberTestService::new());
        let network_client = Arc::new(ObserverSubscriberTestClient::new());
        let store = Arc::new(MemStore::new());
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        let subscriber = ObserverSubscriber::new(
            context.clone(),
            network_client.clone(),
            observer_service.clone(),
            commit_vote_monitor.clone(),
            dag_state.clone(),
            None,
        );
        let peer = PeerId::Validator(context.committee.to_authority_index(0).unwrap());
        subscriber.subscribe(peer.clone());

        // Blocks flow while not lagging.
        for _ in 0..100 {
            sleep(Duration::from_millis(100)).await;
            if network_client.stream_blocks_calls() > 0
                && !observer_service.handle_block_calls.lock().is_empty()
            {
                break;
            }
        }
        assert!(network_client.stream_blocks_calls() > 0);

        // A quorum votes for commit 100 while the local commit index is 0: lag exceeds
        // the suspension threshold, so the subscription is torn down.
        for author in 0..3 {
            let block = VerifiedBlock::new_for_test(
                TestBlock::new(10, author)
                    .set_commit_votes(vec![CommitRef::new(100, CommitDigest::MIN)])
                    .build(),
            );
            commit_vote_monitor.observe_block(&block);
        }
        // Wait for the suspension to engage, then verify no new subscription attempts happen.
        let suspended_metric = &context.metrics.node_metrics.observer_subscription_suspended;
        for _ in 0..100 {
            sleep(Duration::from_secs(1)).await;
            if suspended_metric.get() == 1 {
                break;
            }
        }
        assert_eq!(suspended_metric.get(), 1);
        sleep(Duration::from_secs(2)).await;
        let calls_while_suspended = network_client.stream_blocks_calls();
        sleep(Duration::from_secs(30)).await;
        assert_eq!(
            network_client.stream_blocks_calls(),
            calls_while_suspended,
            "No subscription attempts should happen while suspended"
        );

        // Commit sync catches up to within the resume threshold: lag 100 - 96 = 4 <= 5,
        // so the subscription resumes to the recorded peer.
        let leader_ref = BlockRef::new(
            96,
            context.committee.to_authority_index(0).unwrap(),
            BlockDigest::MIN,
        );
        dag_state
            .write()
            .set_last_commit(TrustedCommit::new_for_test(
                96,
                CommitDigest::MIN,
                0,
                leader_ref,
                vec![],
            ));
        for _ in 0..100 {
            sleep(Duration::from_secs(1)).await;
            if network_client.stream_blocks_calls() > calls_while_suspended {
                break;
            }
        }
        assert!(
            network_client.stream_blocks_calls() > calls_while_suspended,
            "Subscription should resume after catching up"
        );
        assert_eq!(suspended_metric.get(), 0);

        // In production the quorum commit index stays effectively frozen while suspended
        // (commit sync only fetches up to it) and is refreshed by the streamed blocks
        // once the subscription resumes. Simulate that refresh: a quorum now votes for
        // commit 200 while the local commit index is 96, so the subscription must be suspended again.
        for author in 0..3 {
            let block = VerifiedBlock::new_for_test(
                TestBlock::new(20, author)
                    .set_commit_votes(vec![CommitRef::new(200, CommitDigest::MIN)])
                    .build(),
            );
            commit_vote_monitor.observe_block(&block);
        }
        for _ in 0..100 {
            sleep(Duration::from_secs(1)).await;
            if suspended_metric.get() == 1 {
                break;
            }
        }
        assert_eq!(
            suspended_metric.get(),
            1,
            "Subscription should be suspended again when the refreshed quorum commit index reveals more lag"
        );
        let calls_while_resuspended = network_client.stream_blocks_calls();
        sleep(Duration::from_secs(30)).await;
        assert_eq!(
            network_client.stream_blocks_calls(),
            calls_while_resuspended,
            "No subscription attempts should happen while re-suspended"
        );

        // Commit sync catches up again: lag 200 - 196 = 4 <= 5, so the subscription
        // resumes a second time.
        let leader_ref = BlockRef::new(
            196,
            context.committee.to_authority_index(0).unwrap(),
            BlockDigest::MIN,
        );
        dag_state
            .write()
            .set_last_commit(TrustedCommit::new_for_test(
                196,
                CommitDigest::MIN,
                0,
                leader_ref,
                vec![],
            ));
        for _ in 0..100 {
            sleep(Duration::from_secs(1)).await;
            if network_client.stream_blocks_calls() > calls_while_resuspended {
                break;
            }
        }
        assert!(
            network_client.stream_blocks_calls() > calls_while_resuspended,
            "Subscription should resume again after catching up"
        );
        assert_eq!(suspended_metric.get(), 0);

        subscriber.stop().await;
    }

    // Unlike the other suspension tests, whose fake streams end on their own after 10
    // items (letting the suspension engage between reconnection attempts), this stream
    // never ends: the subscription can only be suspended through the in-stream lag check
    // dropping the stream.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_in_stream_lag_check_drops_never_ending_stream() {
        use consensus_config::Parameters;
        use consensus_types::block::BlockDigest;

        use crate::{
            block::TestBlock,
            commit::{CommitDigest, CommitRef},
        };

        telemetry_subscribers::init_for_testing();
        let (mut context, _keys) = Context::new_for_test(4);
        // Suspension threshold is batch_size * 5 = 25 commits of lag, resume at <= 5.
        context.parameters = Parameters {
            commit_sync_batch_size: 5,
            ..context.parameters
        };
        let context = Arc::new(context);
        let observer_service = Arc::new(ObserverSubscriberTestService::new());
        let network_client = Arc::new(ObserverSubscriberTestClient::new_never_ending());
        let store = Arc::new(MemStore::new());
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        let subscriber = ObserverSubscriber::new(
            context.clone(),
            network_client.clone(),
            observer_service.clone(),
            commit_vote_monitor.clone(),
            dag_state.clone(),
            None,
        );
        let peer = PeerId::Validator(context.committee.to_authority_index(0).unwrap());
        subscriber.subscribe(peer.clone());

        // Blocks flow while not lagging.
        for _ in 0..100 {
            sleep(Duration::from_millis(100)).await;
            if !observer_service.handle_block_calls.lock().is_empty() {
                break;
            }
        }
        assert!(!observer_service.handle_block_calls.lock().is_empty());
        assert_eq!(network_client.stream_blocks_calls(), 1);

        // A quorum votes for commit 100 while the local commit index is 0: lag exceeds
        // the suspension threshold.
        for author in 0..3 {
            let block = VerifiedBlock::new_for_test(
                TestBlock::new(10, author)
                    .set_commit_votes(vec![CommitRef::new(100, CommitDigest::MIN)])
                    .build(),
            );
            commit_vote_monitor.observe_block(&block);
        }
        let suspended_metric = &context.metrics.node_metrics.observer_subscription_suspended;
        for _ in 0..100 {
            sleep(Duration::from_secs(1)).await;
            if suspended_metric.get() == 1 {
                break;
            }
        }
        assert_eq!(
            suspended_metric.get(),
            1,
            "The in-stream lag check should drop the never-ending stream and suspend the subscription"
        );

        // The stream was dropped: no blocks arrive and no reconnections happen while suspended.
        let calls_while_suspended = observer_service.handle_block_calls.lock().len();
        let stream_calls_while_suspended = network_client.stream_blocks_calls();
        sleep(Duration::from_secs(30)).await;
        assert_eq!(
            observer_service.handle_block_calls.lock().len(),
            calls_while_suspended,
            "No blocks should be handled while suspended"
        );
        assert_eq!(
            network_client.stream_blocks_calls(),
            stream_calls_while_suspended,
            "No subscription attempts should happen while suspended"
        );

        // Commit sync catches up to within the resume threshold: lag 100 - 96 = 4 <= 5,
        // so the subscription resumes.
        let leader_ref = BlockRef::new(
            96,
            context.committee.to_authority_index(0).unwrap(),
            BlockDigest::MIN,
        );
        dag_state
            .write()
            .set_last_commit(TrustedCommit::new_for_test(
                96,
                CommitDigest::MIN,
                0,
                leader_ref,
                vec![],
            ));
        for _ in 0..100 {
            sleep(Duration::from_secs(1)).await;
            if network_client.stream_blocks_calls() > stream_calls_while_suspended {
                break;
            }
        }
        assert!(
            network_client.stream_blocks_calls() > stream_calls_while_suspended,
            "Subscription should resume after catching up"
        );
        assert_eq!(suspended_metric.get(), 0);

        subscriber.stop().await;
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_suspended_metric_reset_on_resubscription_and_stop() {
        use consensus_config::Parameters;
        use consensus_types::block::BlockDigest;

        use crate::{
            block::TestBlock,
            commit::{CommitDigest, CommitRef},
        };

        telemetry_subscribers::init_for_testing();
        let (mut context, _keys) = Context::new_for_test(4);
        // Suspension threshold is batch_size * 5 = 25 commits of lag, resume at <= 5.
        context.parameters = Parameters {
            commit_sync_batch_size: 5,
            ..context.parameters
        };
        let context = Arc::new(context);
        let observer_service = Arc::new(ObserverSubscriberTestService::new());
        let network_client = Arc::new(ObserverSubscriberTestClient::new());
        let store = Arc::new(MemStore::new());
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        let subscriber = ObserverSubscriber::new(
            context.clone(),
            network_client.clone(),
            observer_service.clone(),
            commit_vote_monitor.clone(),
            dag_state.clone(),
            None,
        );
        let authority_0 = context.committee.to_authority_index(0).unwrap();
        let authority_1 = context.committee.to_authority_index(1).unwrap();
        subscriber.subscribe(PeerId::Validator(authority_0));
        for _ in 0..100 {
            sleep(Duration::from_millis(100)).await;
            if network_client.stream_blocks_calls() > 0 {
                break;
            }
        }
        assert!(network_client.stream_blocks_calls() > 0);

        // A quorum votes for commit 100 while the local commit index is 0: the subscription
        // engages.
        for author in 0..3 {
            let block = VerifiedBlock::new_for_test(
                TestBlock::new(10, author)
                    .set_commit_votes(vec![CommitRef::new(100, CommitDigest::MIN)])
                    .build(),
            );
            commit_vote_monitor.observe_block(&block);
        }
        let suspended_metric = &context.metrics.node_metrics.observer_subscription_suspended;
        for _ in 0..100 {
            sleep(Duration::from_secs(1)).await;
            if suspended_metric.get() == 1 {
                break;
            }
        }
        assert_eq!(suspended_metric.get(), 1);

        // Replace the suspended subscription with another peer, and catch up before the
        // replacement task runs its first lag check. The torn-down task never performs
        // the suspended -> resumed transition, so the gauge must be cleared by the
        // replacement task's level-triggered write (and the exit reset), not stay
        // stuck at 1 on a healthy streaming node.
        let calls_before_switch = network_client.stream_blocks_calls();
        subscriber.subscribe(PeerId::Validator(authority_1));
        let leader_ref = BlockRef::new(100, authority_0, BlockDigest::MIN);
        dag_state
            .write()
            .set_last_commit(TrustedCommit::new_for_test(
                100,
                CommitDigest::MIN,
                0,
                leader_ref,
                vec![],
            ));
        for _ in 0..100 {
            sleep(Duration::from_secs(1)).await;
            if network_client.stream_blocks_calls() > calls_before_switch
                && suspended_metric.get() == 0
            {
                break;
            }
        }
        assert!(
            network_client.stream_blocks_calls() > calls_before_switch,
            "Replacement subscription should stream while caught up"
        );
        assert_eq!(
            suspended_metric.get(),
            0,
            "Gauge must not report suspended after the suspended task was replaced and the node caught up"
        );

        // Suspend again, then stop the subscriber: the exiting task must reset the gauge.
        for author in 0..3 {
            let block = VerifiedBlock::new_for_test(
                TestBlock::new(20, author)
                    .set_commit_votes(vec![CommitRef::new(300, CommitDigest::MIN)])
                    .build(),
            );
            commit_vote_monitor.observe_block(&block);
        }
        for _ in 0..100 {
            sleep(Duration::from_secs(1)).await;
            if suspended_metric.get() == 1 {
                break;
            }
        }
        assert_eq!(suspended_metric.get(), 1);
        subscriber.stop().await;
        assert_eq!(
            suspended_metric.get(),
            0,
            "Gauge must be reset when the subscription stops while suspended"
        );
    }
}
