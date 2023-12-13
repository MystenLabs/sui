// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `BridgeOrchestrator` is the component that:
//! 1. monitors Sui and Ethereum events with the help of `SuiSyncer` and `EthSyncer`
//! 2. updates WAL table and cursor tables
//! 2. hands actions to `BridgeExecutor` for execution

use crate::abi::EthBridgeEvent;
use crate::error::BridgeResult;
use crate::events::SuiBridgeEvent;
use crate::storage::BridgeOrchestratorTables;
use crate::sui_client::{SuiClient, SuiClientInner};
use crate::types::BridgeCommittee;
use arc_swap::ArcSwap;
use mysten_metrics::spawn_logged_monitored_task;
use std::sync::Arc;
use sui_json_rpc_types::SuiEvent;
use sui_types::Identifier;
use tokio::task::JoinHandle;
use tracing::{info, warn};

pub struct BridgeOrchestrator<C> {
    sui_client: Arc<SuiClient<C>>,
    sui_events_rx: mysten_metrics::metered_channel::Receiver<(Identifier, Vec<SuiEvent>)>,
    eth_events_rx: mysten_metrics::metered_channel::Receiver<(
        ethers::types::Address,
        Vec<ethers::types::Log>,
    )>,
    store: Arc<BridgeOrchestratorTables>,
}

impl<C> BridgeOrchestrator<C>
where
    C: SuiClientInner + 'static,
{
    pub async fn new(
        sui_client: Arc<SuiClient<C>>,
        sui_events_rx: mysten_metrics::metered_channel::Receiver<(Identifier, Vec<SuiEvent>)>,
        eth_events_rx: mysten_metrics::metered_channel::Receiver<(
            ethers::types::Address,
            Vec<ethers::types::Log>,
        )>,
        store: Arc<BridgeOrchestratorTables>,
    ) -> BridgeResult<Self> {
        Ok(Self {
            sui_client,
            sui_events_rx,
            eth_events_rx,
            store,
        })
    }

    pub async fn run(self) -> BridgeResult<Vec<JoinHandle<()>>> {
        let bridge_committee = self.sui_client.get_bridge_committee().await?;
        tracing::info!("Bridge committee: {:?}", bridge_committee);
        let bridge_committee = Arc::new(ArcSwap::from_pointee(bridge_committee));
        let mut task_handles = vec![];
        let bridge_committee_clone = bridge_committee.clone();
        let store_clone = self.store.clone();
        task_handles.push(spawn_logged_monitored_task!(Self::run_sui_watcher(
            store_clone,
            self.sui_events_rx,
            bridge_committee_clone,
        )));
        let bridge_committee_clone = bridge_committee.clone();
        let store_clone = self.store.clone();
        task_handles.push(spawn_logged_monitored_task!(Self::run_eth_watcher(
            store_clone,
            self.eth_events_rx,
            bridge_committee_clone,
        )));

        // TODO: spawn bridge committee change watcher task
        Ok(task_handles)
    }

    async fn run_sui_watcher(
        store: Arc<BridgeOrchestratorTables>,
        mut sui_events_rx: mysten_metrics::metered_channel::Receiver<(Identifier, Vec<SuiEvent>)>,
        _bridge_committee: Arc<ArcSwap<BridgeCommittee>>,
    ) {
        info!("Starting sui watcher task");
        while let Some((identifier, events)) = sui_events_rx.recv().await {
            if events.is_empty() {
                continue;
            }

            // TODO: skip events that are already processed (in DB and on chain)

            let bridge_events = events
                .iter()
                .map(SuiBridgeEvent::try_from_sui_event)
                .collect::<BridgeResult<Vec<_>>>()
                .expect("Sui Event could not be deserialzed to SuiBridgeEvent");

            let mut actions = vec![];
            for (sui_event, opt_bridge_event) in events.iter().zip(bridge_events) {
                if opt_bridge_event.is_none() {
                    // TODO: we probably should not miss any events, warn for now.
                    warn!("Sui event not recognized: {:?}", sui_event);
                }
                let bridge_event: SuiBridgeEvent = opt_bridge_event.unwrap();

                if let Some(action) = bridge_event
                    .try_into_bridge_action(sui_event.id.tx_digest, sui_event.id.event_seq as u16)
                {
                    actions.push(action);
                }
                // TODO: handle non Action events
            }

            if !actions.is_empty() {
                // Write action to pending WAL
                store
                    .insert_pending_actions(&actions)
                    .expect("Store operation should not fail");

                // TODO: ask executor to execute, who calls `remove_pending_actions` after
                // confirming the action is done.
            }

            // TODO: add tests for storage
            // Unwrap safe: in the beginning of the loop we checked that events is not empty
            let cursor = events.last().unwrap().id.tx_digest;
            store
                .update_sui_event_cursor(identifier, cursor)
                .expect("Store operation should not fail");
        }
        panic!("Sui event channel was closed");
    }

    async fn run_eth_watcher(
        _store: Arc<BridgeOrchestratorTables>,
        mut eth_events_rx: mysten_metrics::metered_channel::Receiver<(
            ethers::types::Address,
            Vec<ethers::types::Log>,
        )>,
        _bridge_committee: Arc<ArcSwap<BridgeCommittee>>,
    ) {
        info!("Starting eth watcher task");
        while let Some((_contract, logs)) = eth_events_rx.recv().await {
            // TODO: skip events that are not already processed (in DB and on chain)

            let bridge_events = logs
                .iter()
                .map(EthBridgeEvent::try_from_eth_log)
                .collect::<Vec<_>>();

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
