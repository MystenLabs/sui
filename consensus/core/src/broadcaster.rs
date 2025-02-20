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
    block::{BlockAPI as _, ExtendedBlock, VerifiedBlock},
    context::Context,
    core::CoreSignalsReceivers,
    error::ConsensusResult,
    network::NetworkClient,
};

/// Number of Blocks that can be inflight sending to a peer.
const BROADCAST_CONCURRENCY: usize = 10;

/// Broadcaster sends newly created blocks to each peer over the network.
///
/// For a peer that lags behind or is disconnected, blocks are buffered and retried until
/// a limit is reached, then old blocks will get dropped from the buffer.
pub(crate) struct Broadcaster {
    // Background tasks listening for new blocks and pushing them to peers.
    senders: JoinSet<()>,
}

impl Broadcaster {
    const LAST_BLOCK_RETRY_INTERVAL: Duration = Duration::from_secs(2);
    const MIN_SEND_BLOCK_NETWORK_TIMEOUT: Duration = Duration::from_secs(5);

    pub(crate) fn new<C: NetworkClient>(
        context: Arc<Context>,
        network_client: Arc<C>,
        signals_receiver: &CoreSignalsReceivers,
    ) -> Self {
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
        mut rx_block_broadcast: broadcast::Receiver<ExtendedBlock>,
        peer: AuthorityIndex,
    ) {
        let peer_hostname = &context.committee.authority(peer).hostname;

        // Record the last block to be broadcasted, to retry in case no new block is produced for awhile.
        // Even if the peer has acknowledged the last block, the block might have been dropped afterwards
        // if the peer crashed.
        let mut last_block: Option<VerifiedBlock> = None;

        // Retry last block with an interval.
        let mut retry_timer = tokio::time::interval(Self::LAST_BLOCK_RETRY_INTERVAL);
        retry_timer.reset_after(Self::LAST_BLOCK_RETRY_INTERVAL);
        retry_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

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
            // Use a minimum timeout of 5s so the receiver does not terminate the request too early.
            let network_timeout =
                std::cmp::max(req_timeout, Broadcaster::MIN_SEND_BLOCK_NETWORK_TIMEOUT);
            let resp = timeout(
                req_timeout,
                network_client.send_block(peer, &block, network_timeout),
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
                        // Other info from ExtendedBlock are ignored, because Broadcaster is not used in production.
                        Ok(block) => block.block,
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
                    requests.push(send_block(network_client.clone(), peer, rtt_estimate, block.clone()));
                    if last_block.is_none() || last_block.as_ref().unwrap().round() < block.round() {
                        last_block = Some(block);
                    }
                }

                Some((resp, start, block)) = requests.next() => {
                    match resp {
                        Ok(Ok(_)) => {
                            let now = Instant::now();
                            rtt_estimate = rtt_estimate.mul_f64(RTT_ESTIMATE_DECAY) + (now - start).mul_f64(1.0 - RTT_ESTIMATE_DECAY);
                            // Avoid immediately retrying a successfully sent block.
                            // Resetting timer is unnecessary otherwise because there are
                            // additional inflight requests.
                            retry_timer.reset_after(Self::LAST_BLOCK_RETRY_INTERVAL);
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

                _ = retry_timer.tick() => {
                    if requests.is_empty() {
                        if let Some(block) = last_block.clone() {
                            requests.push(send_block(network_client.clone(), peer, rtt_estimate, block));
                        }
                    }
                }
            };

            // Limit RTT estimate to be between 5ms and 5s.
            rtt_estimate = min(rtt_estimate, Duration::from_secs(5));
            rtt_estimate = max(rtt_estimate, Duration::from_millis(5));
            context
                .metrics
                .node_metrics
                .broadcaster_rtt_estimate_ms
                .with_label_values(&[peer_hostname])
                .set(rtt_estimate.as_millis() as i64);
        }
    }
}

#[cfg(test)]
mod test {
    use std::{collections::BTreeMap, ops::DerefMut, time::Duration};

    use async_trait::async_trait;
    use bytes::Bytes;
    use parking_lot::Mutex;
    use tokio::time::sleep;

    use super::*;
    use crate::{
        block::{BlockRef, ExtendedBlock, TestBlock},
        commit::CommitRange,
        core::CoreSignals,
        network::BlockStream,
        Round,
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
            let mut blocks_sent = self.blocks_sent.lock();
            let result = std::mem::take(blocks_sent.deref_mut());
            blocks_sent.clear();
            result
        }
    }

    #[async_trait]
    impl NetworkClient for FakeNetworkClient {
        const SUPPORT_STREAMING: bool = false;

        async fn send_block(
            &self,
            peer: AuthorityIndex,
            block: &VerifiedBlock,
            _timeout: Duration,
        ) -> ConsensusResult<()> {
            let mut blocks_sent = self.blocks_sent.lock();
            let blocks = blocks_sent.entry(peer).or_default();
            blocks.push(block.serialized().clone());
            Ok(())
        }

        async fn subscribe_blocks(
            &self,
            _peer: AuthorityIndex,
            _last_received: Round,
            _timeout: Duration,
        ) -> ConsensusResult<BlockStream> {
            unimplemented!("Unimplemented")
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
    async fn test_broadcaster() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let network_client = Arc::new(FakeNetworkClient::new());
        let (core_signals, signals_receiver) = CoreSignals::new(context.clone());
        let _broadcaster =
            Broadcaster::new(context.clone(), network_client.clone(), &signals_receiver);

        let block = VerifiedBlock::new_for_test(TestBlock::new(9, 1).build());
        assert!(
            core_signals
                .new_block(ExtendedBlock {
                    block: block.clone(),
                    excluded_ancestors: vec![],
                })
                .is_ok(),
            "No subscriber active to receive the block"
        );

        // block should be broadcasted immediately to all peers.
        sleep(Duration::from_millis(1)).await;
        let blocks_sent = network_client.blocks_sent();
        for (index, _) in context.committee.authorities() {
            if index == context.own_index {
                continue;
            }
            assert_eq!(blocks_sent.get(&index).unwrap(), &vec![block.serialized()]);
        }

        // block should not be re-broadcasted ...
        sleep(Broadcaster::LAST_BLOCK_RETRY_INTERVAL / 2).await;
        let blocks_sent = network_client.blocks_sent();
        for (index, _) in context.committee.authorities() {
            if index == context.own_index {
                continue;
            }
            assert!(!blocks_sent.contains_key(&index));
        }

        // ... until LAST_BLOCK_RETRY_INTERVAL
        sleep(Broadcaster::LAST_BLOCK_RETRY_INTERVAL / 2).await;
        let blocks_sent = network_client.blocks_sent();
        for (index, _) in context.committee.authorities() {
            if index == context.own_index {
                continue;
            }
            assert_eq!(blocks_sent.get(&index).unwrap(), &vec![block.serialized()]);
        }
    }
}
