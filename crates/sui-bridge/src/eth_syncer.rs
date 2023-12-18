// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The EthSyncer module is responsible for synchronizing Events emitted on Ethereum blockchain from
//! concerned contracts. Each contract is associated with a start block number, and the syncer will
//! only query from that block number onwards. The syncer also keeps track of the last finalized
//! block on Ethereum and will only query for events up to that block number.

use crate::error::BridgeResult;
use crate::eth_client::EthClient;
use crate::retry_with_max_delay;
use crate::types::EthLog;
use mysten_metrics::spawn_logged_monitored_task;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::{self, Duration};
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::Retry;
use tracing::error;

const ETH_EVENTS_CHANNEL_SIZE: usize = 1000;
const FINALIZED_BLOCK_QUERY_INTERVAL: Duration = Duration::from_secs(2);

pub struct EthSyncer<P> {
    eth_client: Arc<EthClient<P>>,
    contract_addresses: EthTargetAddresses,
}

/// Map from contract address to their start block.
pub type EthTargetAddresses = HashMap<ethers::types::Address, u64>;

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
    ) -> BridgeResult<(
        Vec<JoinHandle<()>>,
        mysten_metrics::metered_channel::Receiver<(ethers::types::Address, Vec<EthLog>)>,
        watch::Receiver<u64>,
    )> {
        let (eth_evnets_tx, eth_events_rx) = mysten_metrics::metered_channel::channel(
            ETH_EVENTS_CHANNEL_SIZE,
            &mysten_metrics::get_metrics()
                .unwrap()
                .channels
                .with_label_values(&["eth_events_queue"]),
        );
        let last_finalized_block = self.eth_client.get_last_finalized_block_id().await?;
        let (last_finalized_block_tx, last_finalized_block_rx) =
            watch::channel(last_finalized_block);
        let mut task_handles = vec![];
        let eth_client_clone = self.eth_client.clone();
        task_handles.push(spawn_logged_monitored_task!(
            Self::run_finalized_block_refresh_task(last_finalized_block_tx, eth_client_clone)
        ));
        for (contract_address, start_block) in self.contract_addresses {
            let eth_evnets_tx_clone = eth_evnets_tx.clone();
            let last_finalized_block_rx_clone = last_finalized_block_rx.clone();
            let eth_client_clone = self.eth_client.clone();
            task_handles.push(spawn_logged_monitored_task!(
                Self::run_event_listening_task(
                    contract_address,
                    start_block,
                    last_finalized_block_rx_clone,
                    eth_evnets_tx_clone,
                    eth_client_clone,
                )
            ));
        }
        Ok((task_handles, eth_events_rx, last_finalized_block_rx))
    }

    async fn run_finalized_block_refresh_task(
        last_finalized_block_sender: watch::Sender<u64>,
        eth_client: Arc<EthClient<P>>,
    ) {
        tracing::info!("Starting finalized block refresh task.");
        let mut last_block_number = 0;
        let mut interval = time::interval(FINALIZED_BLOCK_QUERY_INTERVAL);
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let Ok(new_value) = retry_with_max_delay!(
                eth_client.get_last_finalized_block_id(),
                Duration::from_secs(600)
            ) else {
                error!("Failed to get last finalized block from eth client after retry");
                continue;
            };
            tracing::debug!("Last finalized block: {}", new_value);

            // TODO add a metrics for the last finalized block

            if new_value > last_block_number {
                last_finalized_block_sender
                    .send(new_value)
                    .expect("last_finalized_block channel receiver is closed");
                tracing::info!("Observed new finalized eth block: {}", new_value);
                last_block_number = new_value;
            }
        }
    }

    async fn run_event_listening_task(
        contract_address: ethers::types::Address,
        mut start_block: u64,
        mut last_finalized_block_receiver: watch::Receiver<u64>,
        events_sender: mysten_metrics::metered_channel::Sender<(
            ethers::types::Address,
            Vec<EthLog>,
        )>,
        eth_client: Arc<EthClient<P>>,
    ) {
        tracing::info!(contract_address=?contract_address, "Starting eth events listening task from block {start_block}");
        loop {
            last_finalized_block_receiver
                .changed()
                .await
                .expect("last_finalized_block channel sender is closed");
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
            let Ok(events) = retry_with_max_delay!(
                eth_client.get_events_in_range(contract_address, start_block, new_finalized_block),
                Duration::from_secs(600)
            ) else {
                error!("Failed to get events from eth client after retry");
                continue;
            };
            let len = events.len();
            // TODO: convert Log to a custom Log struct that contains block number, tx hash and log index in tx.
            if !events.is_empty() {
                // Note: it's extremely critical to make sure the Logs we send via this channel
                // are complete per block height. Namely, we should never send a partial list
                // of events for a block. Otherwise, we may end up missing events.
                events_sender
                    .send((contract_address, events))
                    .await
                    .expect("All Eth event channel receivers are closed");
                tracing::info!(?contract_address, "Observed {len} new Eth events",);
            }
            start_block = new_finalized_block + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, str::FromStr};

    use ethers::types::{Address, Log, U256, U64};
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
        let client = EthClient::new_mocked(mock_provider.clone());
        let result = client.get_last_finalized_block_id().await.unwrap();
        assert_eq!(result, 777);

        let addresses = HashMap::from_iter(vec![(Address::zero(), 100)]);
        let log = Log {
            address: Address::zero(),
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
        mock_get_logs(&mock_provider, Address::zero(), 100, 777, vec![log.clone()]);
        let (_handles, mut logs_rx, mut finalized_block_rx) =
            EthSyncer::new(Arc::new(client), addresses)
                .run()
                .await
                .unwrap();

        // The latest finalized block stays at 777, event listener should not query again.
        finalized_block_rx.changed().await.unwrap();
        assert_eq!(*finalized_block_rx.borrow(), 777);
        let (contract_address, received_logs) = logs_rx.recv().await.unwrap();
        assert_eq!(contract_address, Address::zero());
        assert_eq!(received_logs, vec![eth_log.clone()]);
        assert_eq!(logs_rx.try_recv().unwrap_err(), TryRecvError::Empty);

        mock_get_logs(&mock_provider, Address::zero(), 778, 888, vec![log.clone()]);
        // The latest finalized block is updated to 888, event listener should query again.
        mock_last_finalized_block(&mock_provider, 888);
        finalized_block_rx.changed().await.unwrap();
        assert_eq!(*finalized_block_rx.borrow(), 888);
        let (contract_address, received_logs) = logs_rx.recv().await.unwrap();
        assert_eq!(contract_address, Address::zero());
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
        let client = EthClient::new_mocked(mock_provider.clone());

        let another_address =
            Address::from_str("0x00000000219ab540356cbb839cbe05303d7705fa").unwrap();
        let addresses = HashMap::from_iter(vec![(Address::zero(), 100), (another_address, 200)]);

        let log1 = Log {
            address: Address::zero(),
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
            Address::zero(),
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
                .run()
                .await
                .unwrap();

        // The latest finalized block stays at 198.
        finalized_block_rx.changed().await.unwrap();
        assert_eq!(*finalized_block_rx.borrow(), 198);
        assert_eq!(logs_rx.recv().await.unwrap().1, vec![eth_log1.clone()]);
        // log2 should not be received as another_address's start block is 200.
        assert_eq!(logs_rx.try_recv().unwrap_err(), TryRecvError::Empty);

        let log1 = Log {
            address: Address::zero(),
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
            Address::zero(),
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
        logs_rx.recv().await.unwrap().1.into_iter().for_each(|log| {
            logs_set.insert(format!("{:?}", log));
        });
        logs_rx.recv().await.unwrap().1.into_iter().for_each(|log| {
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
}
