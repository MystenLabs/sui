// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use futures::StreamExt;
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use parking_lot::Mutex;
use tokio::{
    task::{JoinHandle, JoinSet},
    time::sleep,
};
use tracing::{debug, info, warn};

use crate::{
    block::BlockAPI,
    context::Context,
    dag_state::DagState,
    error::ConsensusError,
    network::{ObserverNetworkClient, ObserverNetworkService, PeerId},
};

/// ObserverSubscriber manages block stream subscriptions to peers (validators or other observers),
/// taking care of retrying when subscription streams break. Blocks returned from peers are sent
/// to the observer service for processing.
pub(crate) struct ObserverSubscriber<C: ObserverNetworkClient, S: ObserverNetworkService> {
    context: Arc<Context>,
    network_client: Arc<C>,
    observer_service: Arc<S>,
    dag_state: Arc<parking_lot::RwLock<DagState>>,
    subscriptions: Arc<Mutex<BTreeMap<PeerId, Option<JoinHandle<()>>>>>,
}

impl<C: ObserverNetworkClient, S: ObserverNetworkService> ObserverSubscriber<C, S> {
    pub(crate) fn new(
        context: Arc<Context>,
        network_client: Arc<C>,
        observer_service: Arc<S>,
        dag_state: Arc<parking_lot::RwLock<DagState>>,
    ) -> Self {
        Self {
            context,
            network_client,
            observer_service,
            dag_state,
            subscriptions: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Subscribe to a peer (validator or observer) to receive block streams.
    pub(crate) fn subscribe(&self, peer: PeerId) {
        let context = self.context.clone();
        let network_client = self.network_client.clone();
        let observer_service = self.observer_service.clone();

        // Get the highest rounds we've seen per authority from DagState
        let highest_round_per_authority = {
            let dag_state = self.dag_state.read();
            let mut rounds = vec![0u64; context.committee.size()];
            for (authority, _) in context.committee.authorities() {
                rounds[authority.value()] =
                    dag_state.get_last_block_for_authority(authority).round() as u64;
            }
            rounds
        };

        let mut subscriptions = self.subscriptions.lock();
        self.unsubscribe_locked(&peer, &mut subscriptions);
        subscriptions.insert(
            peer.clone(),
            Some(spawn_monitored_task!(Self::subscription_loop(
                context,
                network_client,
                observer_service,
                peer,
                highest_round_per_authority,
            ))),
        );
    }

    /// Stop all active subscriptions.
    pub(crate) fn stop(&self) {
        let mut subscriptions = self.subscriptions.lock();
        let peers: Vec<_> = subscriptions.keys().cloned().collect();
        for peer in peers {
            self.unsubscribe_locked(&peer, &mut subscriptions);
        }
    }

    fn unsubscribe_locked(
        &self,
        peer: &PeerId,
        subscriptions: &mut BTreeMap<PeerId, Option<JoinHandle<()>>>,
    ) {
        if let Some(Some(handle)) = subscriptions.remove(peer) {
            handle.abort();
        }
    }

    async fn subscription_loop(
        context: Arc<Context>,
        network_client: Arc<C>,
        observer_service: Arc<S>,
        peer: PeerId,
        highest_round_per_authority: Vec<u64>,
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

            // Subscribe to stream blocks from the peer.
            let request_timeout = MIN_TIMEOUT.max(delay);
            let mut blocks = match network_client
                .stream_blocks(
                    peer.clone(),
                    highest_round_per_authority.clone(),
                    request_timeout,
                )
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

            // On-demand task spawning with atomic counter for tracking active tasks
            let max_parallel_tasks = context.committee.size();
            let active_tasks = Arc::new(AtomicUsize::new(0));
            let mut tasks = JoinSet::new();

            'stream: loop {
                let _scope = monitored_scope("ObserverSubscriberStreamConsumer");

                match blocks.next().await {
                    Some(block) => {
                        context
                            .metrics
                            .node_metrics
                            .observer_subscribed_blocks
                            .inc();

                        // Backpressure: wait if we've hit max parallelism
                        while active_tasks.load(Ordering::Acquire) >= max_parallel_tasks {
                            // Block until a task completes instead of busy-waiting
                            match tasks.join_next().await {
                                Some(Ok(_)) => {
                                    // Task completed successfully, counter already decremented
                                }
                                Some(Err(e)) => {
                                    warn!("Task failed with error: {}", e);
                                }
                                None => {
                                    // This shouldn't happen if our counter is accurate
                                    warn!(
                                        "No tasks to join but counter shows {} active tasks",
                                        active_tasks.load(Ordering::Acquire)
                                    );
                                    break;
                                }
                            }
                        }

                        // Spawn a new task to handle this block
                        active_tasks.fetch_add(1, Ordering::AcqRel);
                        let counter = active_tasks.clone();
                        let observer_service = observer_service.clone();
                        let peer_cloned = peer.clone();

                        tasks.spawn(async move {
                            let _scope = monitored_scope("ObserverSubscriberTask::handle_block");

                            let result = observer_service
                                .handle_block(peer_cloned.clone(), block)
                                .await;
                            if let Err(e) = result {
                                match e {
                                    ConsensusError::BlockRejected { block_ref, reason } => {
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

                            // Decrement counter when done
                            counter.fetch_sub(1, Ordering::AcqRel);
                        });

                        // Reset retries when a block is received.
                        retries = 0;
                    }
                    None => {
                        debug!("Subscription to blocks from peer {:?} ended", peer);
                        retries += 1;
                        break 'stream;
                    }
                }
            }

            // Wait for all spawned tasks to complete
            while let Some(result) = tasks.join_next().await {
                match result {
                    Ok(_) => {} // Task completed successfully
                    Err(e) => warn!("Task failed during cleanup: {}", e),
                }
            }

            // Sanity check: ensure all tasks have completed
            let remaining = active_tasks.load(Ordering::Acquire);
            if remaining > 0 {
                warn!(
                    "Warning: {} tasks still marked as active after cleanup",
                    remaining
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use async_trait::async_trait;
    use bytes::Bytes;
    use consensus_types::block::{BlockRef, Round};
    use futures::stream;
    use parking_lot::{Mutex, RwLock};
    use tokio::time::sleep;

    use super::*;
    use crate::{
        VerifiedBlock,
        commit::{CommitRange, TrustedCommit},
        context::Context,
        error::ConsensusResult,
        network::{NodeId, ObserverBlockStream, ObserverBlockStreamItem},
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
            _peer: PeerId,
            _highest_round_per_authority: Vec<u64>,
            _timeout: Duration,
        ) -> ConsensusResult<ObserverBlockStream> {
            let block_stream = stream::unfold((), |_| async {
                sleep(Duration::from_millis(1)).await;
                let item = ObserverBlockStreamItem {
                    block: Bytes::from(vec![1u8; 8]),
                    highest_commit_index: 42,
                };
                Some((item, ()))
            })
            .take(10);
            Ok(Box::pin(block_stream))
        }

        async fn fetch_blocks(
            &self,
            _peer: PeerId,
            _block_refs: Vec<BlockRef>,
            _highest_accepted_rounds: Vec<Round>,
            _breadth_first: bool,
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
        handle_block_calls: Mutex<Vec<(PeerId, ObserverBlockStreamItem)>>,
    }

    impl ObserverSubscriberTestService {
        fn new() -> Self {
            Self {
                handle_block_calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl ObserverNetworkService for ObserverSubscriberTestService {
        async fn handle_block(
            &self,
            peer: PeerId,
            item: ObserverBlockStreamItem,
        ) -> ConsensusResult<()> {
            self.handle_block_calls.lock().push((peer, item));
            Ok(())
        }

        async fn handle_stream_blocks(
            &self,
            _peer: NodeId,
            _highest_round_per_authority: Vec<u64>,
        ) -> ConsensusResult<ObserverBlockStream> {
            unimplemented!("Unimplemented")
        }

        async fn handle_fetch_blocks(
            &self,
            _peer: NodeId,
            _block_refs: Vec<BlockRef>,
            _highest_accepted_rounds: Vec<Round>,
            _breadth_first: bool,
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
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        let subscriber = ObserverSubscriber::new(
            context.clone(),
            network_client,
            observer_service.clone(),
            dag_state,
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
        for (p, item) in calls.iter() {
            assert_eq!(*p, peer);
            assert_eq!(item.block, Bytes::from(vec![1u8; 8]));
            assert_eq!(item.highest_commit_index, 42);
        }
    }

    #[tokio::test]
    async fn test_observer_subscriber_stop() {
        telemetry_subscribers::init_for_testing();
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let observer_service = Arc::new(ObserverSubscriberTestService::new());
        let network_client = Arc::new(ObserverSubscriberTestClient::new());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        let subscriber = ObserverSubscriber::new(
            context.clone(),
            network_client,
            observer_service.clone(),
            dag_state,
        );

        // Subscribe to a validator peer
        let peer = PeerId::Validator(context.committee.to_authority_index(2).unwrap());
        subscriber.subscribe(peer);

        // Wait a bit for some blocks to be received
        sleep(Duration::from_millis(100)).await;
        let initial_count = observer_service.handle_block_calls.lock().len();
        assert!(initial_count > 0);

        // Stop all subscriptions
        subscriber.stop();

        // Wait a bit more and verify no new blocks are received
        sleep(Duration::from_millis(100)).await;
        let final_count = observer_service.handle_block_calls.lock().len();
        assert_eq!(initial_count, final_count);
    }
}
