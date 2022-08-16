use crate::{
    authority_aggregator::AuthorityAggregator, authority_client::AuthorityAPI,
    safe_client::SafeClient,
};
use sui_types::{
    base_types::AuthorityName,
    batch::{TxSequenceNumber, UpdateItem},
    committee::Committee,
    error::{SuiError, SuiResult},
    messages::{BatchInfoRequest, BatchInfoResponseItem},
};

use futures::{stream::FuturesUnordered, StreamExt};
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::time::{sleep, Duration};

use super::{NodeSyncHandle, NodeSyncState};

use tap::TapFallible;
use tracing::{debug, trace, warn};

pub async fn node_sync_process<A>(
    committee: Arc<Committee>,
    node_sync_handle: NodeSyncHandle,
    node_sync_state: Arc<NodeSyncState<A>>,
    aggregator: Arc<AuthorityAggregator<A>>,
    cancel_receiver: oneshot::Receiver<()>,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    follower_process(
        committee,
        node_sync_handle,
        node_sync_state,
        aggregator,
        cancel_receiver,
    )
    .await;
}

async fn follower_process<A>(
    committee: Arc<Committee>,
    node_sync_handle: NodeSyncHandle,
    node_sync_state: Arc<NodeSyncState<A>>,
    aggregator: Arc<AuthorityAggregator<A>>,
    mut cancel_receiver: oneshot::Receiver<()>,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    loop {
        let mut follower_tasks = FuturesUnordered::new();
        let mut reconnects = FuturesUnordered::new();

        let start_one_task = |name, handle, state| {
            let client = aggregator.clone_client(name);
            async move {
                let _ = follow_one_peer(handle, state, name, client)
                    .await
                    .tap_err(|e| warn!("follower task exited with error {}", e));
                name
            }
        };

        for (name, _) in committee.members() {
            follower_tasks.push(start_one_task(
                name,
                node_sync_handle.clone(),
                node_sync_state.clone(),
            ));
        }

        loop {
            tokio::select! {
                _ = &mut cancel_receiver => return,

                Some(finished) = follower_tasks.next() => {
                    reconnects.push(async move {
                        sleep(Duration::from_secs(1)).await;
                        finished
                    });
                }

                Some(reconnect) = reconnects.next() => {
                    follower_tasks.push(start_one_task(
                        reconnect,
                        node_sync_handle.clone(),
                        node_sync_state.clone(),
                    ));
                }

                else => panic!()
            }
        }
    }
}

async fn follow_one_peer<A>(
    node_sync_handle: NodeSyncHandle,
    node_sync_state: Arc<NodeSyncState<A>>,
    peer: &AuthorityName,
    client: SafeClient<A>,
) -> SuiResult
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let node_sync_store = node_sync_state.node_sync_store.clone();

    // Global timeout, we do not exceed this time in this task.
    let mut results = FuturesUnordered::new();

    let result_block = |fut, seq, digests| async move {
        fut.await?;
        trace!(?peer, ?seq, ?digests, "digest handler finished");
        Ok::<TxSequenceNumber, SuiError>(seq)
    };

    macro_rules! process_digest {
        ($seq: expr, $digests: expr) => {{
            let seq = $seq;
            let digests = $digests;
            let fut = node_sync_handle.handle_sync_digest(*peer, digests).await?;
            results.push(result_block(fut, seq, digests));
        }};
    }

    // Find the sequence to start streaming at.
    let start_seq = node_sync_store
        .latest_seq_for_peer(*peer)?
        .map(|seq| seq + 1)
        .unwrap_or(0);

    // Process everything currently in the db.
    for (seq, digests) in node_sync_store.batch_stream_iter(peer)? {
        process_digest!(seq, digests);
    }

    while let Some(result) = results.next().await {
        let seq = result?;
        node_sync_store.remove_batch_stream_item(*peer, seq)?;
    }

    client
        .metrics_seq_number_to_handle_batch_stream
        .set(start_seq as i64);

    let req = BatchInfoRequest {
        start: Some(start_seq),
        length: 1000,
    };

    let metrics = &node_sync_handle.metrics;
    let mut stream = Box::pin(client.handle_batch_stream(req).await?);

    loop {
        tokio::select! {
            Some(item) = &mut stream.next() => {
                match item {
                    Ok(BatchInfoResponseItem(UpdateItem::Batch(signed_batch))) => {
                        metrics.total_batch_received.inc();
                        println!("batch: {:?}", signed_batch);
                    }

                    Ok(BatchInfoResponseItem(UpdateItem::Transaction((seq, digests)))) => {
                        metrics.total_tx_received.inc();
                        node_sync_store.enqueue_execution_digests(*peer, seq, &digests)?;
                        process_digest!(seq, digests);
                    }

                    Err(err) => return Err(err),
                }
            }

            Some(result) = &mut results.next() => {
                let seq = result?;
                node_sync_store.remove_batch_stream_item(*peer, seq)?;
            }

            else => break
        }
    }

    Ok(())
}
