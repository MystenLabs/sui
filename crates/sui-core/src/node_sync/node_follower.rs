// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority_active::gossip::GossipMetrics, authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI, safe_client::SafeClient,
};
use sui_metrics::monitored_future;
use sui_storage::node_sync_store::NodeSyncStore;
use sui_types::{
    base_types::{AuthorityName, EpochId, ExecutionDigests},
    batch::{TxSequenceNumber, UpdateItem},
    error::{SuiError, SuiResult},
    messages::{BatchInfoRequest, BatchInfoResponseItem},
};

use async_trait::async_trait;
use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout, Duration, Instant};

use super::{NodeSyncHandle, SyncResult};

use tap::TapFallible;
use tracing::{debug, info, trace, warn};

// How long must a follower task run for for us to clear the exponential backoff for that peer?
const CLEAR_BACKOFF_DURATION: Duration = Duration::from_secs(30);
const MIN_RECONNECTION_DELAY: Duration = Duration::from_millis(500);
const NUM_ITEMS_PER_REQUEST: u64 = 1000;
const DRAIN_RESULTS_TIMEOUT: Duration = Duration::from_secs(1);

pub async fn node_sync_process<A>(
    node_sync_handle: NodeSyncHandle,
    node_sync_store: Arc<NodeSyncStore>,
    epoch_id: EpochId,
    aggregator: Arc<AuthorityAggregator<A>>,
    cancel_receiver: oneshot::Receiver<()>,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    follower_process(
        node_sync_handle.clone(),
        node_sync_store,
        epoch_id,
        aggregator,
        NUM_ITEMS_PER_REQUEST,
        &node_sync_handle.metrics,
        cancel_receiver,
    )
    .await;
}

async fn follower_process<A, Handler>(
    node_sync_handle: Handler,
    node_sync_store: Arc<NodeSyncStore>,
    epoch_id: EpochId,
    aggregator: Arc<AuthorityAggregator<A>>,
    max_stream_items: u64,
    metrics: &GossipMetrics,
    mut cancel_receiver: oneshot::Receiver<()>,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
    Handler: SyncHandler + Clone,
{
    debug!("starting follower process");

    let mut backoff = HashMap::new();

    loop {
        let mut follower_tasks = FuturesUnordered::new();
        let mut reconnects = FuturesUnordered::new();

        let start_one_task = |name, handle, store| {
            let start_time = Instant::now();
            let client = aggregator.clone_client(name);
            monitored_future!(async move {
                let result = follow_one_peer(
                    handle,
                    store,
                    epoch_id,
                    *name,
                    client,
                    max_stream_items,
                    metrics,
                )
                .await
                .tap_err(|e| warn!(peer=?name, "follower task exited with error {}", e));
                (result, start_time, name)
            })
        };

        for (name, _) in aggregator.committee.members() {
            follower_tasks.push(start_one_task(
                name,
                node_sync_handle.clone(),
                node_sync_store.clone(),
            ));
        }

        loop {
            tokio::select! {
                biased;

                _ = &mut cancel_receiver => {
                    info!("follower_process cancelled");
                    return;
                }

                Some((result, start, peer)) = follower_tasks.next() => {
                    let duration = Instant::now() - start;
                    info!(?peer, ?duration, "follower task completed");

                    if result.is_ok() || duration > CLEAR_BACKOFF_DURATION {
                        info!(?peer, "clearing backoff for peer");
                        backoff.remove(peer);
                    }

                    let mut delay = MIN_RECONNECTION_DELAY;
                    if result.is_err() {
                        let cur_backoff = backoff
                            .entry(peer)
                            .and_modify(|cur| *cur *= 2)
                            .or_insert(MIN_RECONNECTION_DELAY);
                        delay = std::cmp::max(delay, *cur_backoff);
                    }

                    info!(?peer, ?delay, "will restart task after delay");

                    reconnects.push(monitored_future!(async move {
                        sleep(delay).await;
                        peer
                    }));
                }

                Some(reconnect) = reconnects.next() => {
                    follower_tasks.push(start_one_task(
                        reconnect,
                        node_sync_handle.clone(),
                        node_sync_store.clone(),
                    ));
                }

                else => panic!()
            }
        }
    }
}

#[async_trait]
trait SyncHandler {
    async fn handle_digest(
        &self,
        epoch_id: EpochId,
        peer: AuthorityName,
        seq: TxSequenceNumber,
        digests: ExecutionDigests,
    ) -> SuiResult<BoxFuture<'static, SyncResult>>;
}

#[async_trait]
impl SyncHandler for NodeSyncHandle {
    async fn handle_digest(
        &self,
        epoch_id: EpochId,
        peer: AuthorityName,
        _seq: TxSequenceNumber,
        digests: ExecutionDigests,
    ) -> SuiResult<BoxFuture<'static, SyncResult>> {
        self.handle_sync_digest(epoch_id, peer, digests).await
    }
}

#[derive(Default, Debug)]
struct FollowResult {
    items_from_db: u64,
    items_from_stream: u64,
}

async fn follow_one_peer<A, Handler>(
    sync_handle: Handler,
    node_sync_store: Arc<NodeSyncStore>,
    epoch_id: EpochId,
    peer: AuthorityName,
    client: SafeClient<A>,
    max_stream_items: u64,
    metrics: &GossipMetrics,
) -> SuiResult<FollowResult>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
    Handler: SyncHandler,
{
    debug!(?peer, "follow_one_peer");

    // Global timeout, we do not exceed this time in this task.
    let mut results = FuturesUnordered::new();

    let result_block = |fut, seq, digests| {
        monitored_future!(async move {
            fut.await?;
            trace!(?peer, ?seq, ?digests, "digest handler finished");
            Ok::<TxSequenceNumber, SuiError>(seq)
        })
    };

    // Using a macro to avoid duplicating code - was much too difficult to satisfy the borrow
    // checker with closures.
    macro_rules! process_digest {
        ($seq: expr, $digests: expr) => {{
            let seq = $seq;
            let digests = $digests;
            let fut = sync_handle
                .handle_digest(epoch_id, peer, seq, digests)
                .await?;
            results.push(result_block(fut, seq, digests));
        }};
    }

    let remove_from_seq_store = |seq| {
        trace!(?peer, ?seq, "removing completed batch stream item");
        node_sync_store.remove_batch_stream_item(epoch_id, peer, seq)
    };

    let mut follow_result = FollowResult::default();

    // Process everything currently in the db.
    let mut first = true;
    for (seq, digests) in node_sync_store.batch_stream_iter(epoch_id, &peer)? {
        if first {
            first = false;
            debug!(
                ?peer,
                start_seq = ?seq,
                "processing persisted batch stream items"
            );
        }
        process_digest!(seq, digests);
        follow_result.items_from_db += 1;
    }

    // Find the sequence to start streaming at.
    let start_seq = node_sync_store
        .latest_seq_for_peer(epoch_id, &peer)?
        .map(|seq| seq + 1)
        .unwrap_or(0);

    debug!(
        ?peer,
        ?start_seq,
        ?max_stream_items,
        "requesting new batch stream items"
    );
    client
        .metrics_seq_number_to_handle_batch_stream
        .set(start_seq as i64);

    let req = BatchInfoRequest {
        start: Some(start_seq),
        length: max_stream_items,
    };

    let mut stream = Box::pin(client.handle_batch_stream(req).await?);

    loop {
        tokio::select! {
            biased;

            Some(result) = &mut results.next() => {
                remove_from_seq_store(result?)?;
            }

            next = &mut stream.next() => {
                match next {
                    Some(Ok(BatchInfoResponseItem(UpdateItem::Batch(signed_batch)))) => {
                        let batch_next_seq = signed_batch.data().next_sequence_number;
                        debug!(?peer, ?batch_next_seq, "Received signed batch");
                        metrics.total_batch_received.inc();
                    }

                    Some(Ok(BatchInfoResponseItem(UpdateItem::Transaction((seq, digests))))) => {
                        trace!(?peer, ?digests, ?seq, "received tx from peer");
                        metrics.total_tx_received.inc();
                        node_sync_store.enqueue_execution_digests(epoch_id, peer, seq, &digests)?;
                        process_digest!(seq, digests);

                        follow_result.items_from_stream += 1;
                    }

                    Some(Err(err)) => {
                        debug!(?peer, "handle_batch_stream error: {}", err);
                        return Err(err);
                    }

                    // stream is closed
                    None => {
                        debug!(?peer, "stream closed");
                        break;
                    }
                }
            }

        }
    }

    let drain_results = async move {
        debug!(?peer, "draining results");
        while let Some(result) = results.next().await {
            remove_from_seq_store(result?)?;
        }
        debug!(?peer, "finished draining results");
        Ok::<(), SuiError>(())
    };

    match timeout(DRAIN_RESULTS_TIMEOUT, drain_results).await {
        // a timeout is not an error for our purposes.
        Err(_) => debug!(?peer, "timed out draining results"),
        // errors from drain_results should be propagated however.
        Ok(res) => res?,
    }

    Ok(follow_result)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        authority_active::gossip::GossipMetrics, authority_aggregator::AuthorityAggregatorBuilder,
        authority_client::NetworkAuthorityClient, node_sync::SyncStatus,
    };
    use std::sync::{Arc, Mutex};
    use sui_macros::sim_test;
    use sui_types::{
        base_types::ObjectID,
        crypto::get_account_key_pair,
        messages::{ExecutionStatus, VerifiedTransaction},
        object::Object,
    };
    use test_utils::{
        authority::{spawn_test_authorities, test_and_configure_authority_configs},
        messages::make_transfer_sui_transaction,
    };
    use tokio::{sync::broadcast, time::Instant};

    #[derive(Clone)]
    struct TestNodeSyncHandler {
        break_after: Arc<Mutex<Option<TxSequenceNumber>>>,
        // sent whenever a failure is triggered.
        failure_tx: broadcast::Sender<()>,
        _failure_rx: Arc<broadcast::Receiver<()>>,
    }

    impl TestNodeSyncHandler {
        pub fn new() -> Self {
            let (failure_tx, failure_rx) = broadcast::channel(10000);
            Self {
                break_after: Default::default(),
                failure_tx,
                _failure_rx: Arc::new(failure_rx),
            }
        }

        pub fn break_after(self, after: TxSequenceNumber) -> Self {
            self.set_break_after(after);
            self
        }

        pub fn set_break_after(&self, after: TxSequenceNumber) {
            *self.break_after.lock().unwrap() = Some(after);
        }

        pub fn inc_break_after(&self) {
            if let Some(cur) = &mut *self.break_after.lock().unwrap() {
                *cur += 1;
            }
        }

        pub fn clear_break_after(&self) {
            *self.break_after.lock().unwrap() = None;
        }
    }

    #[async_trait]
    impl SyncHandler for TestNodeSyncHandler {
        async fn handle_digest(
            &self,
            epoch_id: EpochId,
            peer: AuthorityName,
            seq: TxSequenceNumber,
            digests: ExecutionDigests,
        ) -> SuiResult<BoxFuture<'static, SyncResult>> {
            debug!(?peer, ?digests, ?epoch_id, "handle_digest");
            if let Some(after) = *self.break_after.lock().unwrap() {
                if seq > after {
                    // reset so that the handler can make progress after follower is restarted.
                    debug!(?after, "SyncHandler returning error");
                    self.failure_tx.send(()).unwrap();
                    return Err(SuiError::GenericAuthorityError { error: "".into() });
                }
            }
            Ok(Box::pin(async move {
                Ok::<SyncStatus, SuiError>(SyncStatus::CertExecuted)
            }))
        }
    }

    async fn execute_transactions(
        aggregator: &AuthorityAggregator<NetworkAuthorityClient>,
        transactions: &[VerifiedTransaction],
    ) {
        for transaction in transactions {
            let (_, effects) = aggregator
                .clone()
                .execute_transaction(transaction)
                .await
                .unwrap();

            assert!(matches!(
                effects.data().status,
                ExecutionStatus::Success { .. }
            ));
        }
    }

    fn new_sync_store() -> Arc<NodeSyncStore> {
        let working_dir = tempfile::tempdir().unwrap();
        let db_path = working_dir.path().join("sync_store");
        Arc::new(NodeSyncStore::open_tables_read_write(db_path, None, None))
    }

    #[sim_test]
    async fn test_follower() {
        telemetry_subscribers::init_for_testing();

        let (sender, keypair) = get_account_key_pair();
        let (receiver, _) = get_account_key_pair();

        let objects: Vec<_> = (0..10)
            .map(|_| Object::with_id_owner_for_testing(ObjectID::random(), sender))
            .collect();

        let transactions: Vec<_> = objects
            .iter()
            .map(|obj| {
                make_transfer_sui_transaction(
                    obj.compute_object_reference(),
                    receiver,
                    Some(100),
                    sender,
                    &keypair,
                )
            })
            .collect();

        // Set up an authority
        let config = test_and_configure_authority_configs(1);
        let authorities = spawn_test_authorities(objects, &config).await;
        let (agg, _) = AuthorityAggregatorBuilder::from_network_config(&config)
            .build()
            .unwrap();
        let net = Arc::new(agg);

        execute_transactions(&net, &transactions).await;

        {
            debug!("testing follow_one_peer");
            let sync_store = new_sync_store();
            let test_handler = TestNodeSyncHandler::new();

            let peer = authorities[0].with(|node| node.state().name);
            let metrics = GossipMetrics::new_for_tests();
            follow_one_peer(
                test_handler.clone().break_after(1),
                sync_store.clone(),
                0,
                peer,
                net.clone_client(&peer),
                3,
                &metrics,
            )
            .await
            .unwrap_err();

            test_handler.clear_break_after();

            let result = follow_one_peer(
                test_handler.clone(),
                sync_store.clone(),
                0,
                peer,
                net.clone_client(&peer),
                3,
                &metrics,
            )
            .await
            .unwrap();

            // One item was processed from the db because the first one failed.
            assert_eq!(result.items_from_db, 1);

            // At least 3 more were fetched from the stream - this number can't be checked exactly
            // because the authority returns everything in the current batch even if fewer items
            // were requested, and batch boundaries are non-deterministic.
            assert!(result.items_from_stream >= 3 && result.items_from_stream <= 10);
        }

        // test follower_process
        {
            debug!("testing follower_process");
            let sync_store = new_sync_store();
            let test_handler = TestNodeSyncHandler::new();

            let metrics = GossipMetrics::new_for_tests();

            let (_cancel_tx, cancel_rx) = oneshot::channel();

            let test_handler_clone = test_handler.clone();
            let net_clone = net.clone();
            let follower_task = tokio::task::spawn(async move {
                let _ = timeout(
                    Duration::from_secs(60),
                    follower_process(
                        test_handler_clone.break_after(2),
                        sync_store,
                        0,
                        net_clone,
                        5,
                        &metrics,
                        cancel_rx,
                    ),
                )
                .await;
            });

            let intervals: Arc<Mutex<Vec<Duration>>> = Default::default();
            let intervals_clone = intervals.clone();

            let mut failure_rx = test_handler.failure_tx.subscribe();
            let monitor_task = tokio::task::spawn(async move {
                let start = Instant::now();
                loop {
                    failure_rx.recv().await.unwrap();
                    test_handler.inc_break_after();
                    let delay = Instant::now() - start;
                    intervals_clone.lock().unwrap().push(delay);
                    debug!("failure triggered {:?}", delay)
                }
            });

            follower_task.await.unwrap();
            monitor_task.abort();

            let ivs = intervals.lock().unwrap();
            // The follow should restart at least this many times because it is configured to fail
            // after every 2 items.
            assert_eq!(ivs.len(), 7);

            // The exact delay times due to backoff are non-deterministic, so we can't test them
            // exactly, but we should verify that the final interval is at least 30 seconds long.
            assert!(*ivs.last().unwrap() > Duration::from_secs(30));
        }
    }
}
