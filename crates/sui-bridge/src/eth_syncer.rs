// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The EthSyncer module is responsible for synchronizing Events emitted on Ethereum blockchain from
//! concerned contracts. Each contract is associated with a start block number, and the syncer will
//! only query from that block number onwards. The syncer also keeps track of the last finalized
//! block on Ethereum and will only query for events up to that block number.

use crate::error::BridgeResult;
use crate::eth_client::EthClient;
use crate::retry_with_max_delay;
use mysten_metrics::spawn_logged_monitored_task;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::{self, Duration};

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
        mysten_metrics::metered_channel::Receiver<Vec<ethers::types::Log>>,
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
            let new_value = retry_with_max_delay!(
                eth_client.get_last_finalized_block_id(),
                Duration::from_secs(600)
            )
            .expect("Failed to get last finalzied block from eth client after retry");
            tracing::debug!("Last finalized block: {}", new_value);
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
        events_sender: mysten_metrics::metered_channel::Sender<Vec<ethers::types::Log>>,
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
            let events = retry_with_max_delay!(
                eth_client.get_events_in_range(contract_address, start_block, new_finalized_block),
                Duration::from_secs(600)
            )
            .expect("Failed to get events from eth client after retry");
            let len = events.len();
            if !events.is_empty() {
                events_sender
                    .send(events)
                    .await
                    .expect("All Eth event channel receivers are closed");
                tracing::info!(
                    contract_address=?contract_address,
                    "Observed {len} new events",
                );
            }
            start_block = new_finalized_block + 1;
        }
    }
}

use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::Retry;

#[macro_export]
macro_rules! retry_with_max_delay {
    ($func:expr, $max_delay:expr) => {{
        let retry_strategy = ExponentialBackoff::from_millis(100)
            .max_delay($max_delay)
            .map(jitter);
        Retry::spawn(retry_strategy, || $func).await
    }};
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, str::FromStr};

    use ethers::types::{
        Address, Block, BlockNumber, Filter, FilterBlockOption, Log, ValueOrArray, U64,
    };
    use prometheus::Registry;
    use tokio::sync::mpsc::error::TryRecvError;

    use crate::eth_mock_provider::EthMockProvider;

    use super::*;

    #[tokio::test]
    async fn test_last_finalized_block() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);
        let mock_provider = EthMockProvider::new();
        mock_last_finalized_block(&mock_provider, 777);
        let client = EthClient::new_mocked(mock_provider.clone()).await?;
        let result = client.get_last_finalized_block_id().await.unwrap();
        assert_eq!(result, 777);

        let addresses = HashMap::from_iter(vec![(Address::zero(), 100)]);
        let log = Log {
            address: Address::zero(),
            ..Default::default()
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
        assert_eq!(logs_rx.recv().await.unwrap(), vec![log.clone()]);
        assert_eq!(logs_rx.try_recv().unwrap_err(), TryRecvError::Empty);

        mock_get_logs(&mock_provider, Address::zero(), 778, 888, vec![log.clone()]);
        // The latest finalized block is updated to 888, event listener should query again.
        mock_last_finalized_block(&mock_provider, 888);
        finalized_block_rx.changed().await.unwrap();
        assert_eq!(*finalized_block_rx.borrow(), 888);
        assert_eq!(logs_rx.recv().await.unwrap(), vec![log]);
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
        let client = EthClient::new_mocked(mock_provider.clone()).await?;

        let another_address =
            Address::from_str("0x00000000219ab540356cbb839cbe05303d7705fa").unwrap();
        let addresses = HashMap::from_iter(vec![(Address::zero(), 100), (another_address, 200)]);

        let log1 = Log {
            address: Address::zero(),
            ..Default::default()
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
        assert_eq!(logs_rx.recv().await.unwrap(), vec![log1.clone()]);
        // log2 should not be received as another_address's start block is 200.
        assert_eq!(logs_rx.try_recv().unwrap_err(), TryRecvError::Empty);

        let log1 = Log {
            address: Address::zero(),
            block_number: Some(U64::from(200)),
            ..Default::default()
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
            block_number: Some(U64::from(201)),
            ..Default::default()
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
        logs_rx.recv().await.unwrap().into_iter().for_each(|log| {
            logs_set.insert(format!("{:?}", log));
        });
        logs_rx.recv().await.unwrap().into_iter().for_each(|log| {
            logs_set.insert(format!("{:?}", log));
        });
        assert_eq!(
            logs_set,
            HashSet::from_iter(vec![format!("{:?}", log1), format!("{:?}", log2)])
        );
        // No more finalized block change, no more logs.
        assert_eq!(logs_rx.try_recv().unwrap_err(), TryRecvError::Empty);
        Ok(())
    }

    fn mock_last_finalized_block(mock_provider: &EthMockProvider, block_number: u64) {
        let block = Block::<ethers::types::TxHash> {
            number: Some(U64::from(block_number)),
            ..Default::default()
        };
        mock_provider
            .add_response("eth_getBlockByNumber", ("finalized", false), block)
            .unwrap();
    }

    fn mock_get_logs(
        mock_provider: &EthMockProvider,
        address: Address,
        from_block: u64,
        to_block: u64,
        logs: Vec<Log>,
    ) {
        mock_provider.add_response::<[ethers::types::Filter; 1], Vec<ethers::types::Log>, Vec<ethers::types::Log>>(
            "eth_getLogs",
            [
                Filter {
                    block_option: FilterBlockOption::Range {
                        from_block: Some(BlockNumber::Number(U64::from(from_block))),
                        to_block: Some(BlockNumber::Number(U64::from(to_block))),
                    },
                    address: Some(ValueOrArray::Value(address)),
                    topics: [None, None, None, None],
                }
            ],
            logs,
        ).unwrap();
    }
}
