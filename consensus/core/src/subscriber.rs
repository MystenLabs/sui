// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use consensus_config::AuthorityIndex;
use futures::StreamExt;
use mysten_metrics::spawn_monitored_task;
use parking_lot::{Mutex, RwLock};
use tokio::{task::JoinHandle, time::sleep};
use tracing::{debug, error, info};

use crate::{
    block::BlockAPI as _,
    context::Context,
    dag_state::DagState,
    error::ConsensusError,
    network::{NetworkClient, NetworkService},
    Round,
};

/// Subscriber manages the block stream subscriptions to other peers, taking care of retrying
/// when subscription streams break. Blocks returned from the peer are sent to the authority
/// service for processing.
/// Currently subscription management for individual peer is not exposed, but it could become
/// useful in future.
pub(crate) struct Subscriber<C: NetworkClient, S: NetworkService> {
    context: Arc<Context>,
    network_client: Arc<C>,
    authority_service: Arc<S>,
    dag_state: Arc<RwLock<DagState>>,
    subscriptions: Arc<Mutex<Box<[Option<JoinHandle<()>>]>>>,
}

impl<C: NetworkClient, S: NetworkService> Subscriber<C, S> {
    pub(crate) fn new(
        context: Arc<Context>,
        network_client: Arc<C>,
        authority_service: Arc<S>,
        dag_state: Arc<RwLock<DagState>>,
    ) -> Self {
        let subscriptions = (0..context.committee.size())
            .map(|_| None)
            .collect::<Vec<_>>();
        Self {
            context,
            network_client,
            authority_service,
            dag_state,
            subscriptions: Arc::new(Mutex::new(subscriptions.into_boxed_slice())),
        }
    }

    pub(crate) fn subscribe(&self, peer: AuthorityIndex) {
        if peer == self.context.own_index {
            error!("Attempt to subscribe to own validator {peer} is ignored!");
            return;
        }
        let context = self.context.clone();
        let network_client = self.network_client.clone();
        let authority_service = self.authority_service.clone();
        let (mut last_received, gc_round, gc_enabled) = {
            let dag_state = self.dag_state.read();
            (
                dag_state.get_last_block_for_authority(peer).round(),
                dag_state.gc_round(),
                dag_state.gc_enabled(),
            )
        };

        // If the latest block we have accepted by an authority is older than the current gc round,
        // then do not attempt to fetch any blocks from that point as they will simply be skipped. Instead
        // do attempt to fetch from the gc round.
        if gc_enabled && last_received < gc_round {
            info!(
                "Last received block for peer {peer} is older than GC round, {last_received} < {gc_round}, fetching from GC round"
            );
            last_received = gc_round;
        }

        let mut subscriptions = self.subscriptions.lock();
        self.unsubscribe_locked(peer, &mut subscriptions[peer.value()]);
        subscriptions[peer.value()] = Some(spawn_monitored_task!(Self::subscription_loop(
            context,
            network_client,
            authority_service,
            peer,
            last_received,
        )));
    }

    pub(crate) fn stop(&self) {
        let mut subscriptions = self.subscriptions.lock();
        for (peer, _) in self.context.committee.authorities() {
            self.unsubscribe_locked(peer, &mut subscriptions[peer.value()]);
        }
    }

    fn unsubscribe_locked(&self, peer: AuthorityIndex, subscription: &mut Option<JoinHandle<()>>) {
        let peer_hostname = &self.context.committee.authority(peer).hostname;
        if let Some(subscription) = subscription.take() {
            subscription.abort();
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
        authority_service: Arc<S>,
        peer: AuthorityIndex,
        last_received: Round,
    ) {
        const IMMEDIATE_RETRIES: i64 = 3;
        // When not immediately retrying, limit retry delay between 100ms and 10s.
        const INITIAL_RETRY_INTERVAL: Duration = Duration::from_millis(100);
        const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(10);
        const RETRY_INTERVAL_MULTIPLIER: f32 = 1.2;
        let peer_hostname = &context.committee.authority(peer).hostname;
        let mut retries: i64 = 0;
        let mut delay = INITIAL_RETRY_INTERVAL;
        'subscription: loop {
            context
                .metrics
                .node_metrics
                .subscribed_to
                .with_label_values(&[peer_hostname])
                .set(0);

            if retries > IMMEDIATE_RETRIES {
                debug!(
                    "Delaying retry {} of peer {} subscription, in {} seconds",
                    retries,
                    peer_hostname,
                    delay.as_secs_f32(),
                );
                sleep(delay).await;
                // Update delay for the next retry.
                delay = delay
                    .mul_f32(RETRY_INTERVAL_MULTIPLIER)
                    .min(MAX_RETRY_INTERVAL);
            } else if retries > 0 {
                // Retry immediately, but still yield to avoid monopolizing the thread.
                tokio::task::yield_now().await;
            } else {
                // First attempt, reset delay for next retries but no waiting.
                delay = INITIAL_RETRY_INTERVAL;
            }
            retries += 1;

            let mut blocks = match network_client
                .subscribe_blocks(peer, last_received, MAX_RETRY_INTERVAL)
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
                        .with_label_values(&[peer_hostname, "success"])
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
                        .with_label_values(&[peer_hostname, "failure"])
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
                match blocks.next().await {
                    Some(block) => {
                        context
                            .metrics
                            .node_metrics
                            .subscribed_blocks
                            .with_label_values(&[peer_hostname])
                            .inc();
                        let result = authority_service
                            .handle_send_block(peer, block.clone())
                            .await;
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
                        // Reset retries when a block is received.
                        retries = 0;
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
    use anemo::async_trait;
    use bytes::Bytes;
    use futures::stream;

    use super::*;
    use crate::{
        block::BlockRef,
        commit::CommitRange,
        error::ConsensusResult,
        network::{test_network::TestService, BlockStream, ExtendedSerializedBlock},
        storage::mem_store::MemStore,
        VerifiedBlock,
    };

    struct SubscriberTestClient {}

    impl SubscriberTestClient {
        fn new() -> Self {
            Self {}
        }
    }

    #[async_trait]
    impl NetworkClient for SubscriberTestClient {
        const SUPPORT_STREAMING: bool = true;

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
            let block_stream = stream::unfold((), |_| async {
                sleep(Duration::from_millis(1)).await;
                let block = ExtendedSerializedBlock {
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
            _highest_accepted_rounds: Vec<Round>,
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
                    block: Bytes::from(vec![1u8; 8]),
                    excluded_ancestors: vec![]
                }
            );
        }
    }
}
