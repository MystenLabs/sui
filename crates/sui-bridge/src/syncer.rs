// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The EthSyncer module is responsible for synchronizing Events emitted on Ethereum blockchain from
//! concerned contracts. Each contract is associated with a start block number, and the syncer will
//! only query from that block number onwards. The syncer also keeps track of the last finalized
//! block on Ethereum and will only query for events up to that block number.

use crate::error::BridgeResult;
use crate::eth_client::EthClient;
use mysten_metrics::spawn_logged_monitored_task;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::{self, Duration};
// use typed_store::rocks::DBMap;
// use typed_store::traits::{TableSummary, TypedStoreDebug};
// use typed_store_derive::DBMapUtils;
use crate::retry_with_max_delay;

const ETH_EVENTS_CHANNEL_SIZE: usize = 1000;
const FINALIZED_BLOCK_QUERY_INTERVAL: Duration = Duration::from_secs(2);

// #[derive(DBMapUtils)]
// pub struct WatermarkStore {
//     pub(crate) eth_block: DBMap<ethers::types::Address, i64>,
// }

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
            Self::run_finalized_block_refresh_task(last_finalized_block_tx, eth_client_clone,)
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
        Ok((task_handles, eth_events_rx))
    }

    async fn run_finalized_block_refresh_task(
        last_finalized_block_sender: watch::Sender<u64>,
        eth_client: Arc<EthClient<P>>,
    ) {
        // let mut last_value = retry_with_max_delay!(
        //     eth_client.get_last_finalized_block_id(),
        //     Duration::from_secs(10)
        // )
        // .expect("Failed to get last finalzied block from eth client after retry");
        // last_finalized_block_sender
        //     .send(last_value)
        //     .expect("last_finalized_block channel receiver is closed");
        tracing::info!("Starting finalized block refresh task.");
        let mut last_value = 0;
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
            if new_value > last_value {
                last_finalized_block_sender
                    .send(new_value)
                    .expect("last_finalized_block channel receiver is closed");
                tracing::info!("Observed new finalized eth block: {}", new_value);
                last_value = new_value;
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
    use std::str::FromStr;
    use prometheus::Registry;
    use ethers::{providers::MockProvider, types::{U64, Block, Address}};

    use super::*;

    #[tokio::test]
    async fn test_last_finalized_block() -> anyhow::Result<()> {
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);
        let mock_provider = MockProvider::new();
        let block = Block::<ethers::types::TxHash> {
            number: Some(U64::from(777)),
            ..Default::default()
        };
        mock_provider.push(block).unwrap();
        let client = EthClient::new_mocked(mock_provider.clone()).await?;
        let result = client.get_last_finalized_block_id().await.unwrap();
        assert_eq!(result, 777);

        // let eth_client = Arc::new(EthClient::new("https://eth.llama.com").unwrap());
        // let last_finalized_block = rt.block_on(eth_client.get_last_finalized_block_id()).unwrap();
        // println!("last_finalized_block: {}", last_finalized_block);

        let addresses = HashMap::from_iter(vec![
            ( Address::zero(), 100 ),
            // ( Address::from_str("0x00000000219ab540356cbb839cbe05303d7705fa").unwrap(), 200 )
        ]);

        let (_handle, rx) = EthSyncer::new(Arc::new(client), addresses).run().await.unwrap();
            
                    

        Ok(())
    }
}
