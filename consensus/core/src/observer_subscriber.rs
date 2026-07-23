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

/// ObserverSubscriber manages block stream subscriptions to peers (validators or other observers),
/// taking care of retrying when subscription streams break. Blocks returned from peers are sent
/// to the observer service for processing. The `ObserverSubscriber` can only subscribe to one peer at a time.
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
                context,
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

                        // During catch-up (commit lagging behind quorum), drop silently --
                        // those rounds will arrive via checkpoint sync, same as lagging validators.
                        let is_commit_lagging = dag_state.upgrade().is_none_or(|dag_state| {
                            is_commit_lagging(
                                &context,
                                dag_state.read().last_commit_index(),
                                commit_vote_monitor.quorum_commit_index(),
                            )
                        });
                        if let Some(handler) = &randomness_signature_handler
                            && !is_commit_lagging
                        {
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

    struct ObserverSubscriberTestClient {}

    impl ObserverSubscriberTestClient {
        fn new() -> Self {
            Self {}
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
            })
            .take(10);
            Ok(Box::pin(block_stream))
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
}
