// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The EthSyncer module is responsible for synchronizing Events emitted on Ethereum blockchain from
//! concerned contracts. Each contract is associated with a start block number, and the syncer will
//! only query from that block number onwards. The syncer also keeps track of the last finalized
//! block on Ethereum and will only query for events up to that block number.

use crate::error::BridgeResult;
use crate::eth_client::EthClient;
use crate::metrics::BridgeMetrics;
use crate::retry_with_max_elapsed_time;
use crate::types::EthLog;
use ethers::types::Address as EthAddress;
use mysten_metrics::spawn_logged_monitored_task;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::{self, Duration, Instant};
use tracing::error;

const ETH_LOG_QUERY_MAX_BLOCK_RANGE: u64 = 1000;
const ETH_EVENTS_CHANNEL_SIZE: usize = 1000;
const FINALIZED_BLOCK_QUERY_INTERVAL: Duration = Duration::from_secs(5);

pub struct EthSyncer<P> {
    eth_client: Arc<EthClient<P>>,
    contract_addresses: EthTargetAddresses,
}

/// Map from contract address to their start block.
pub type EthTargetAddresses = HashMap<EthAddress, u64>;

#[allow(clippy::new_without_default)]
impl<P> EthSyncer<P>
where
    P: ethers::providers::JsonRpcClient + 'static,
{
    pub fn new(eth_client: Arc<EthClient<P>>, contract_addresses: EthTargetAddresses) -> Self {
        Self {
            eth_client,
            contract_addresses,
        }
    }

    pub async fn run(
        self,
        metrics: Arc<BridgeMetrics>,
    ) -> BridgeResult<(
        Vec<JoinHandle<()>>,
        mysten_metrics::metered_channel::Receiver<(EthAddress, u64, Vec<EthLog>)>,
        watch::Receiver<u64>,
    )> {
        let (eth_evnets_tx, eth_events_rx) = mysten_metrics::metered_channel::channel(
            ETH_EVENTS_CHANNEL_SIZE,
            &mysten_metrics::get_metrics()
                .unwrap()
                .channel_inflight
                .with_label_values(&["eth_events_queue"]),
        );
        let last_finalized_block = self.eth_client.get_last_finalized_block_id().await?;
        let (last_finalized_block_tx, last_finalized_block_rx) =
            watch::channel(last_finalized_block);
        let mut task_handles = vec![];
        let eth_client_clone = self.eth_client.clone();
        let metrics_clone = metrics.clone();
        task_handles.push(spawn_logged_monitored_task!(
            Self::run_finalized_block_refresh_task(
                last_finalized_block_tx,
                eth_client_clone,
                metrics_clone
            )
        ));
        for (contract_address, start_block) in self.contract_addresses {
            let eth_evnets_tx_clone = eth_evnets_tx.clone();
            let last_finalized_block_rx_clone = last_finalized_block_rx.clone();
            let eth_client_clone = self.eth_client.clone();
            let metrics_clone = metrics.clone();
            task_handles.push(spawn_logged_monitored_task!(
                Self::run_event_listening_task(
                    contract_address,
                    start_block,
                    last_finalized_block_rx_clone,
                    eth_evnets_tx_clone,
                    eth_client_clone,
                    metrics_clone,
                )
            ));
        }
        Ok((task_handles, eth_events_rx, last_finalized_block_rx))
    }

    async fn run_finalized_block_refresh_task(
        last_finalized_block_sender: watch::Sender<u64>,
        eth_client: Arc<EthClient<P>>,
        metrics: Arc<BridgeMetrics>,
    ) {
        tracing::info!("Starting finalized block refresh task.");
        let mut last_block_number = 0;
        let mut interval = time::interval(FINALIZED_BLOCK_QUERY_INTERVAL);
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            // TODO: allow to pass custom initial interval
            let Ok(Ok(new_value)) = retry_with_max_elapsed_time!(
                eth_client.get_last_finalized_block_id(),
                time::Duration::from_secs(600)
            ) else {
                error!("Failed to get last finalized block from eth client after retry");
                continue;
            };
            tracing::debug!("Last finalized block: {}", new_value);
            metrics.last_finalized_eth_block.set(new_value as i64);

            if new_value > last_block_number {
                last_finalized_block_sender
                    .send(new_value)
                    .expect("last_finalized_block channel receiver is closed");
                tracing::info!("Observed new finalized eth block: {}", new_value);
                last_block_number = new_value;
            }
        }
    }

    // TODO: define a type for block number for readability
    // TODO: add a metrics for current start block
    async fn run_event_listening_task(
        contract_address: EthAddress,
        mut start_block: u64,
        mut last_finalized_block_receiver: watch::Receiver<u64>,
        events_sender: mysten_metrics::metered_channel::Sender<(EthAddress, u64, Vec<EthLog>)>,
        eth_client: Arc<EthClient<P>>,
        metrics: Arc<BridgeMetrics>,
    ) {
        tracing::info!(contract_address=?contract_address, "Starting eth events listening task from block {start_block}");
        let contract_address_str = contract_address.to_string();
        let mut more_blocks = false;
        loop {
            // If no more known blocks, wait for the next finalized block.
            if !more_blocks {
                last_finalized_block_receiver
                    .changed()
                    .await
                    .expect("last_finalized_block channel sender is closed");
            }
            let new_finalized_block = *last_finalized_block_receiver.borrow();
            if new_finalized_block < start_block {
                tracing::info!(
                    contract_address=?contract_address,
                    "New finalized block {} is smaller than start block {}, ignore",
                    new_finalized_block,
                    start_block,
                );
                continue;
            }
            // Each query does at most ETH_LOG_QUERY_MAX_BLOCK_RANGE blocks.
            let end_block = std::cmp::min(
                start_block + ETH_LOG_QUERY_MAX_BLOCK_RANGE - 1,
                new_finalized_block,
            );
            more_blocks = end_block < new_finalized_block;
            let timer = Instant::now();
            let Ok(Ok(events)) = retry_with_max_elapsed_time!(
                eth_client.get_events_in_range(contract_address, start_block, end_block),
                Duration::from_secs(600)
            ) else {
                error!("Failed to get events from eth client after retry");
                continue;
            };
            tracing::debug!(
                ?contract_address,
                start_block,
                end_block,
                "Querying eth events took {:?}",
                timer.elapsed()
            );
            let len = events.len();
            let last_block = events.last().map(|e| e.block_number);

            // Note 1: we always events to the channel even when it is empty. This is because of
            // how `eth_getLogs` api is designed - we want cursor to move forward continuously.

            // Note 2: it's extremely critical to make sure the Logs we send via this channel
            // are complete per block height. Namely, we should never send a partial list
            // of events for a block. Otherwise, we may end up missing events.
            events_sender
                .send((contract_address, end_block, events))
                .await
                .expect("All Eth event channel receivers are closed");
            if len != 0 {
                tracing::info!(
                    ?contract_address,
                    start_block,
                    end_block,
                    "Observed {len} new Eth events",
                );
            }
            metrics
                .last_synced_eth_blocks
                .with_label_values(&[&contract_address_str])
                .set(last_block.unwrap_or(end_block) as i64);
            start_block = end_block + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, str::FromStr};

    use ethers::types::{Log, U256, U64};
    use prometheus::Registry;
    use tokio::sync::mpsc::error::TryRecvError;

    use crate::{
        eth_mock_provider::EthMockProvider,
        test_utils::{mock_get_logs, mock_last_finalized_block},
    };

    use super::*;
    use ethers::types::TxHash;

    #[tokio::test]
    async fn test_last_finalized_block() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);
        let mock_provider = EthMockProvider::new();
        mock_last_finalized_block(&mock_provider, 777);
        let client = EthClient::new_mocked(
            mock_provider.clone(),
            HashSet::from_iter(vec![EthAddress::zero()]),
        );
        let result = client.get_last_finalized_block_id().await.unwrap();
        assert_eq!(result, 777);

        let addresses = HashMap::from_iter(vec![(EthAddress::zero(), 100)]);
        let log = Log {
            address: EthAddress::zero(),
            transaction_hash: Some(TxHash::random()),
            block_number: Some(U64::from(777)),
            log_index: Some(U256::from(3)),
            ..Default::default()
        };
        let eth_log = EthLog {
            block_number: 777,
            tx_hash: log.transaction_hash.unwrap(),
            log_index_in_tx: 0,
            log: log.clone(),
        };
        mock_get_logs(
            &mock_provider,
            EthAddress::zero(),
            100,
            777,
            vec![log.clone()],
        );
        let (_handles, mut logs_rx, mut finalized_block_rx) =
            EthSyncer::new(Arc::new(client), addresses)
                .run(Arc::new(BridgeMetrics::new_for_testing()))
                .await
                .unwrap();

        // The latest finalized block stays at 777, event listener should not query again.
        finalized_block_rx.changed().await.unwrap();
        assert_eq!(*finalized_block_rx.borrow(), 777);
        let (contract_address, end_block, received_logs) = logs_rx.recv().await.unwrap();
        assert_eq!(contract_address, EthAddress::zero());
        assert_eq!(end_block, 777);
        assert_eq!(received_logs, vec![eth_log.clone()]);
        assert_eq!(logs_rx.try_recv().unwrap_err(), TryRecvError::Empty);

        mock_get_logs(
            &mock_provider,
            EthAddress::zero(),
            778,
            888,
            vec![log.clone()],
        );
        // The latest finalized block is updated to 888, event listener should query again.
        mock_last_finalized_block(&mock_provider, 888);
        finalized_block_rx.changed().await.unwrap();
        assert_eq!(*finalized_block_rx.borrow(), 888);
        let (contract_address, end_block, received_logs) = logs_rx.recv().await.unwrap();
        assert_eq!(contract_address, EthAddress::zero());
        assert_eq!(end_block, 888);
        assert_eq!(received_logs, vec![eth_log]);
        assert_eq!(logs_rx.try_recv().unwrap_err(), TryRecvError::Empty);

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_addresses() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let mock_provider = EthMockProvider::new();
        mock_last_finalized_block(&mock_provider, 198);

        let another_address =
            EthAddress::from_str("0x00000000219ab540356cbb839cbe05303d7705fa").unwrap();
        let client = EthClient::new_mocked(
            mock_provider.clone(),
            HashSet::from_iter(vec![another_address]),
        );

        let addresses = HashMap::from_iter(vec![(EthAddress::zero(), 100), (another_address, 200)]);

        let log1 = Log {
            address: EthAddress::zero(),
            transaction_hash: Some(TxHash::random()),
            block_number: Some(U64::from(101)),
            log_index: Some(U256::from(5)),
            ..Default::default()
        };
        let eth_log1 = EthLog {
            block_number: log1.block_number.unwrap().as_u64(),
            tx_hash: log1.transaction_hash.unwrap(),
            log_index_in_tx: 0,
            log: log1.clone(),
        };
        mock_get_logs(
            &mock_provider,
            EthAddress::zero(),
            100,
            198,
            vec![log1.clone()],
        );
        let log2 = Log {
            address: another_address,
            transaction_hash: Some(TxHash::random()),
            block_number: Some(U64::from(201)),
            log_index: Some(U256::from(6)),
            ..Default::default()
        };
        // Mock logs for another_address although it shouldn't be queried. We don't expect to
        // see log2 in the logs channel later on.
        mock_get_logs(
            &mock_provider,
            another_address,
            200,
            198,
            vec![log2.clone()],
        );

        let (_handles, mut logs_rx, mut finalized_block_rx) =
            EthSyncer::new(Arc::new(client), addresses)
                .run(Arc::new(BridgeMetrics::new_for_testing()))
                .await
                .unwrap();

        // The latest finalized block stays at 198.
        finalized_block_rx.changed().await.unwrap();
        assert_eq!(*finalized_block_rx.borrow(), 198);
        let (_contract_address, end_block, received_logs) = logs_rx.recv().await.unwrap();
        assert_eq!(end_block, 198);
        assert_eq!(received_logs, vec![eth_log1.clone()]);
        // log2 should not be received as another_address's start block is 200.
        assert_eq!(logs_rx.try_recv().unwrap_err(), TryRecvError::Empty);

        let log1 = Log {
            address: EthAddress::zero(),
            block_number: Some(U64::from(200)),
            transaction_hash: Some(TxHash::random()),
            log_index: Some(U256::from(7)),
            ..Default::default()
        };
        let eth_log1 = EthLog {
            block_number: log1.block_number.unwrap().as_u64(),
            tx_hash: log1.transaction_hash.unwrap(),
            log_index_in_tx: 0,
            log: log1.clone(),
        };
        mock_get_logs(
            &mock_provider,
            EthAddress::zero(),
            199,
            400,
            vec![log1.clone()],
        );
        let log2 = Log {
            address: another_address,
            transaction_hash: Some(TxHash::random()),
            block_number: Some(U64::from(201)),
            log_index: Some(U256::from(9)),
            ..Default::default()
        };
        let eth_log2 = EthLog {
            block_number: log2.block_number.unwrap().as_u64(),
            tx_hash: log2.transaction_hash.unwrap(),
            log_index_in_tx: 0,
            log: log2.clone(),
        };
        mock_get_logs(
            &mock_provider,
            another_address,
            200,
            400,
            vec![log2.clone()],
        );
        mock_last_finalized_block(&mock_provider, 400);

        finalized_block_rx.changed().await.unwrap();
        assert_eq!(*finalized_block_rx.borrow(), 400);
        let mut logs_set = HashSet::new();
        logs_rx.recv().await.unwrap().2.into_iter().for_each(|log| {
            logs_set.insert(format!("{:?}", log));
        });
        logs_rx.recv().await.unwrap().2.into_iter().for_each(|log| {
            logs_set.insert(format!("{:?}", log));
        });
        assert_eq!(
            logs_set,
            HashSet::from_iter(vec![format!("{:?}", eth_log1), format!("{:?}", eth_log2)])
        );
        // No more finalized block change, no more logs.
        assert_eq!(logs_rx.try_recv().unwrap_err(), TryRecvError::Empty);
        Ok(())
    }

    /// Test that the syncer will query for logs in multiple queries if the range is too big.
    #[tokio::test]
    async fn test_paginated_eth_log_query() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);
        let mock_provider = EthMockProvider::new();
        let start_block = 100;
        // range too big, we need two queries
        let last_finalized_block = start_block + ETH_LOG_QUERY_MAX_BLOCK_RANGE + 1;
        mock_last_finalized_block(&mock_provider, last_finalized_block);
        let client = EthClient::new_mocked(
            mock_provider.clone(),
            HashSet::from_iter(vec![EthAddress::zero()]),
        );
        let result = client.get_last_finalized_block_id().await.unwrap();
        assert_eq!(result, last_finalized_block);

        let addresses = HashMap::from_iter(vec![(EthAddress::zero(), start_block)]);
        let log = Log {
            address: EthAddress::zero(),
            transaction_hash: Some(TxHash::random()),
            block_number: Some(U64::from(start_block)),
            log_index: Some(U256::from(3)),
            ..Default::default()
        };
        let log2 = Log {
            address: EthAddress::zero(),
            transaction_hash: Some(TxHash::random()),
            block_number: Some(U64::from(last_finalized_block)),
            log_index: Some(U256::from(3)),
            ..Default::default()
        };
        let eth_log = EthLog {
            block_number: start_block,
            tx_hash: log.transaction_hash.unwrap(),
            log_index_in_tx: 0,
            log: log.clone(),
        };
        let eth_log2 = EthLog {
            block_number: last_finalized_block,
            tx_hash: log2.transaction_hash.unwrap(),
            log_index_in_tx: 0,
            log: log2.clone(),
        };
        // First query handles [start, start + ETH_LOG_QUERY_MAX_BLOCK_RANGE - 1]
        mock_get_logs(
            &mock_provider,
            EthAddress::zero(),
            start_block,
            start_block + ETH_LOG_QUERY_MAX_BLOCK_RANGE - 1,
            vec![log.clone()],
        );
        // Second query handles [start + ETH_LOG_QUERY_MAX_BLOCK_RANGE, last_finalized_block]
        mock_get_logs(
            &mock_provider,
            EthAddress::zero(),
            start_block + ETH_LOG_QUERY_MAX_BLOCK_RANGE,
            last_finalized_block,
            vec![log2.clone()],
        );

        let (_handles, mut logs_rx, mut finalized_block_rx) =
            EthSyncer::new(Arc::new(client), addresses)
                .run(Arc::new(BridgeMetrics::new_for_testing()))
                .await
                .unwrap();

        finalized_block_rx.changed().await.unwrap();
        assert_eq!(*finalized_block_rx.borrow(), last_finalized_block);
        let (contract_address, end_block, received_logs) = logs_rx.recv().await.unwrap();
        assert_eq!(contract_address, EthAddress::zero());
        assert_eq!(end_block, start_block + ETH_LOG_QUERY_MAX_BLOCK_RANGE - 1);
        assert_eq!(received_logs, vec![eth_log.clone()]);
        let (contract_address, end_block, received_logs) = logs_rx.recv().await.unwrap();
        assert_eq!(contract_address, EthAddress::zero());
        assert_eq!(end_block, last_finalized_block);
        assert_eq!(received_logs, vec![eth_log2.clone()]);
        Ok(())
    }
}
