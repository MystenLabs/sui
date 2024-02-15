// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cmp::{max, min},
    sync::Arc,
    time::Duration,
};

use consensus_config::AuthorityIndex;
use futures::{stream::FuturesUnordered, StreamExt as _};
use tokio::{
    sync::broadcast,
    task::JoinSet,
    time::{error::Elapsed, sleep_until, timeout, Instant},
};
use tracing::{trace, warn};

use crate::{
    block::VerifiedBlock, context::Context, core::CoreSignalsReceivers, error::ConsensusResult,
    network::NetworkClient,
};

/// Number of Blocks that can be inflight sending to a peer.
const BROADCAST_CONCURRENCY: usize = 10;

/// Broadcaster sends newly created blocks to each peer over the network.
///
/// For a peer that lags behind or is disconnected, blocks are buffered and retried until
/// a limit is reached, then old blocks will get dropped from the buffer.
pub(crate) struct Broadcaster {
    // Background tasks sending blocks to peers.
    senders: JoinSet<()>,
}

impl Broadcaster {
    pub(crate) fn new<C: NetworkClient>(
        context: Arc<Context>,
        network_client: Arc<C>,
        signals_receiver: &CoreSignalsReceivers,
    ) -> Self {
        // Initialize sender tasks.
        let mut senders = JoinSet::new();
        for (index, _authority) in context.committee.authorities() {
            // Skip sending Block to self.
            if index == context.own_index {
                continue;
            }
            senders.spawn(Self::push_blocks(
                context.clone(),
                network_client.clone(),
                signals_receiver.block_broadcast_receiver(),
                index,
            ));
        }
        Self { senders }
    }

    pub(crate) fn stop(&mut self) {
        // Intentionally not waiting for senders to exit, to speed up shutdown.
        self.senders.abort_all();
    }

    /// Runs a loop that continously pushes new blocks received from the rx_block_broadcast
    /// channel to the target peer.
    ///
    /// The loop does not exit until the validator is shutting down.
    async fn push_blocks<C: NetworkClient>(
        context: Arc<Context>,
        network_client: Arc<C>,
        mut rx_block_broadcast: broadcast::Receiver<VerifiedBlock>,
        peer: AuthorityIndex,
    ) {
        let peer_hostname = context.committee.authority(peer).hostname.clone();

        // Use a simple exponential-decay RTT estimator to adjust the timeout for each block sent.
        // The estimation logic will be removed once the underlying transport switches to use
        // streaming and the streaming implementation can be relied upon for retries.
        const RTT_ESTIMATE_DECAY: f64 = 0.95;
        const TIMEOUT_THRESHOLD_MULTIPLIER: f64 = 2.0;
        const TIMEOUT_RTT_INCREMENT_FACTOR: f64 = 1.6;
        let mut rtt_estimate = Duration::from_millis(200);

        let mut requests = FuturesUnordered::new();

        async fn send_block<C: NetworkClient>(
            network_client: Arc<C>,
            peer: AuthorityIndex,
            rtt_estimate: Duration,
            block: VerifiedBlock,
        ) -> (Result<ConsensusResult<()>, Elapsed>, Instant, VerifiedBlock) {
            let start = Instant::now();
            let req_timeout = rtt_estimate.mul_f64(TIMEOUT_THRESHOLD_MULTIPLIER);
            let resp = timeout(
                req_timeout,
                network_client.send_block(peer, block.serialized()),
            )
            .await;
            if matches!(resp, Ok(Err(_))) {
                // Add a delay before retrying.
                sleep_until(start + req_timeout).await;
            }
            (resp, start, block)
        }

        loop {
            tokio::select! {
                result = rx_block_broadcast.recv(), if requests.len() < BROADCAST_CONCURRENCY => {
                    let block = match result {
                        Ok(block) => block,
                        Err(broadcast::error::RecvError::Closed) => {
                            trace!("Sender to {peer} is shutting down!");
                            return;
                        }
                        Err(broadcast::error::RecvError::Lagged(e)) => {
                            warn!("Sender to {peer} is lagging! {e}");
                            // Re-run the loop to receive again.
                            continue;
                        }
                    };
                    requests.push(send_block(network_client.clone(), peer, rtt_estimate, block));
                }
                Some((resp, start, block)) = requests.next() => {
                    match resp {
                        Ok(Ok(_)) => {
                            let now = Instant::now();
                            rtt_estimate = rtt_estimate.mul_f64(RTT_ESTIMATE_DECAY) + (now - start).mul_f64(1.0 - RTT_ESTIMATE_DECAY);
                        },
                        Err(Elapsed { .. }) => {
                            rtt_estimate = rtt_estimate.mul_f64(TIMEOUT_RTT_INCREMENT_FACTOR);
                            requests.push(send_block(network_client.clone(), peer, rtt_estimate, block));
                        },
                        Ok(Err(_)) => {
                            requests.push(send_block(network_client.clone(), peer, rtt_estimate, block));
                        },
                    };
                }
            };
            // Limit RTT estimate to be between 5ms and 5s.
            rtt_estimate = min(rtt_estimate, Duration::from_secs(5));
            rtt_estimate = max(rtt_estimate, Duration::from_millis(5));
            context
                .metrics
                .node_metrics
                .broadcaster_rtt_estimate_ms
                .with_label_values(&[&peer_hostname])
                .set(rtt_estimate.as_millis() as i64);
        }
    }
}

#[cfg(test)]
mod test {
    use std::{collections::BTreeMap, time::Duration};

    use async_trait::async_trait;
    use bytes::Bytes;
    use parking_lot::Mutex;
    use tokio::time::sleep;

    use super::*;
    use crate::{
        block::{BlockRef, TestBlock},
        core::CoreSignals,
    };

    struct FakeNetworkClient {
        blocks_sent: Mutex<BTreeMap<AuthorityIndex, Vec<Bytes>>>,
    }

    impl FakeNetworkClient {
        fn new() -> Self {
            Self {
                blocks_sent: Mutex::new(BTreeMap::new()),
            }
        }

        fn blocks_sent(&self) -> BTreeMap<AuthorityIndex, Vec<Bytes>> {
            self.blocks_sent.lock().clone()
        }
    }

    #[async_trait]
    impl NetworkClient for FakeNetworkClient {
        async fn send_block(
            &self,
            peer: AuthorityIndex,
            serialized_block: &Bytes,
        ) -> ConsensusResult<()> {
            let mut blocks_sent = self.blocks_sent.lock();
            let blocks = blocks_sent.entry(peer).or_default();
            blocks.push(serialized_block.clone());
            Ok(())
        }

        async fn fetch_blocks(
            &self,
            _peer: AuthorityIndex,
            _block_refs: Vec<BlockRef>,
        ) -> ConsensusResult<Vec<Bytes>> {
            unimplemented!("Unimplemented")
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_broadcaster() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let network_client = Arc::new(FakeNetworkClient::new());
        let (core_signals, signals_receiver) = CoreSignals::new();
        let _broadcaster =
            Broadcaster::new(context.clone(), network_client.clone(), &signals_receiver);

        let block = VerifiedBlock::new_for_test(TestBlock::new(9, 1).build());
        core_signals.new_block(block.clone()).unwrap();

        sleep(Duration::from_secs(1)).await;

        let blocks_sent = network_client.blocks_sent();
        for (index, _) in context.committee.authorities() {
            if index == context.own_index {
                continue;
            }
            assert_eq!(blocks_sent.get(&index).unwrap(), &vec![block.serialized()]);
        }
    }
}
