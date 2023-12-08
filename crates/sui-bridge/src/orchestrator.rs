// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `BridgeOrchestrator` is the component that monitors Sui and Ethereum events
//! with the help of `SuiSyncer` and `EthSyncer` and process them by quorum
//! driving among bridge committee.

use crate::abi::EthBridgeEvent;
use crate::error::BridgeResult;
use crate::events::SuiBridgeEvent;
use crate::sui_client::{SuiClient, SuiClientInner};
use crate::types::BridgeCommittee;
use arc_swap::ArcSwap;
use mysten_metrics::spawn_logged_monitored_task;
use std::sync::Arc;
use sui_json_rpc_types::SuiEvent;
use tokio::task::JoinHandle;
use tracing::{info, warn};

pub struct BridgeOrchestrator<C> {
    sui_client: Arc<SuiClient<C>>,
    sui_events_rx: mysten_metrics::metered_channel::Receiver<Vec<SuiEvent>>,
    eth_events_rx: mysten_metrics::metered_channel::Receiver<Vec<ethers::types::Log>>,
}

impl<C> BridgeOrchestrator<C>
where
    C: SuiClientInner + 'static,
{
    pub async fn new(
        sui_client: Arc<SuiClient<C>>,
        sui_events_rx: mysten_metrics::metered_channel::Receiver<Vec<SuiEvent>>,
        eth_events_rx: mysten_metrics::metered_channel::Receiver<Vec<ethers::types::Log>>,
    ) -> BridgeResult<Self> {
        Ok(Self {
            sui_client,
            sui_events_rx,
            eth_events_rx,
        })
    }

    pub async fn run(self) -> BridgeResult<Vec<JoinHandle<()>>> {
        let bridge_committee = self.sui_client.get_bridge_committee().await?;
        tracing::info!("Bridge committee: {:?}", bridge_committee);
        let bridge_committee = Arc::new(ArcSwap::from_pointee(bridge_committee));
        let mut task_handles = vec![];
        let bridge_committee_clone = bridge_committee.clone();
        task_handles.push(spawn_logged_monitored_task!(Self::run_sui_watcher(
            self.sui_events_rx,
            bridge_committee_clone,
        )));
        let bridge_committee_clone = bridge_committee.clone();
        task_handles.push(spawn_logged_monitored_task!(Self::run_eth_watcher(
            self.eth_events_rx,
            bridge_committee_clone,
        )));

        // TODO: spawn bridge change watcher task
        Ok(task_handles)
    }

    async fn run_sui_watcher(
        mut sui_events_rx: mysten_metrics::metered_channel::Receiver<Vec<SuiEvent>>,
        bridge_committee: Arc<ArcSwap<BridgeCommittee>>,
    ) {
        info!("Starting sui watcher task");
        while let Some(events) = sui_events_rx.recv().await {
            // TODO: skip events that are already processed (in DB and on chain)

            let bridge_events = events
                .iter()
                .map(SuiBridgeEvent::try_from_sui_event)
                .collect::<BridgeResult<Vec<_>>>()
                .expect("Sui Event could not be deserialzed to SuiBridgeEvent");

            // Load committee upfront to avoid weird edge cases where committee changes in between
            let _committee = bridge_committee.load().clone();

            // TODO: optimize handling of multiple events
            for (sui_event, opt_bridge_event) in events.iter().zip(bridge_events) {
                if opt_bridge_event.is_none() {
                    // TODO: we probably should not miss any events, warn for now.
                    warn!("Sui event not recognized: {:?}", sui_event);
                }
                let _bridge_event = opt_bridge_event.unwrap();

                // TODO: handle all bridge events
            }
        }
        panic!("Sui event channel was closed");
    }

    async fn run_eth_watcher(
        mut eth_events_rx: mysten_metrics::metered_channel::Receiver<Vec<ethers::types::Log>>,
        bridge_committee: Arc<ArcSwap<BridgeCommittee>>,
    ) {
        info!("Starting eth watcher task");
        while let Some(logs) = eth_events_rx.recv().await {
            // TODO: skip events that are not already processed (in DB and on chain)

            let bridge_events = logs
                .iter()
                .map(EthBridgeEvent::try_from_eth_log)
                .collect::<Vec<_>>();

            // Load committee upfront to avoid weird edge cases where committee changes in between
            let _committee = bridge_committee.load().clone();

            for (log, opt_bridge_event) in logs.iter().zip(bridge_events) {
                if opt_bridge_event.is_none() {
                    // TODO: we probably should not miss any events, warn for now.
                    warn!("Eth event not recognized: {:?}", log);
                }
                let _bridge_event = opt_bridge_event.unwrap();
                // TODO: handle all bridge events
            }
        }
        panic!("Eth event channel was closed");
    }
}
