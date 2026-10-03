// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ValidatorProxy;
use std::sync::Arc;
use std::time::Duration;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
use sui_types::{
    base_types::EpochId,
    sui_system_state::{SuiSystemState, SuiSystemStateTrait},
};
use test_cluster::TestCluster;
use tokio::sync::watch;
use tokio::sync::watch::Receiver;
use tokio::time;
use tokio::time::Instant;
use tracing::{error, info, warn};

#[derive(Debug, Clone)]
pub struct SystemState {
    pub reference_gas_price: u64,
    pub protocol_config: Option<ProtocolConfig>,
    pub epoch: EpochId,
}

#[derive(Debug)]
pub struct SystemStateObserver {
    pub state: Receiver<SystemState>,
}

impl SystemStateObserver {
    pub fn new(proxy: Arc<dyn ValidatorProxy + Send + Sync>) -> Self {
        let mut interval = tokio::time::interval_at(Instant::now(), Duration::from_secs(60));
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        let (tx, rx) = watch::channel(SystemState {
            reference_gas_price: 1u64,
            protocol_config: None,
            epoch: 0,
        });
        let chain = proxy.get_chain_identifier().chain();
        tokio::task::spawn(async move {
            loop {
                interval.tick().await;
                match proxy.get_latest_system_state_object().await {
                    Ok(result) => {
                        if result.protocol_version == 0 {
                            warn!("Received protocol version 0 from network, ignoring");
                            continue;
                        }

                        let network_version = ProtocolVersion::new(result.protocol_version);
                        let version = if network_version > ProtocolVersion::MAX {
                            warn!(
                                "Network protocol version {} exceeds maximum supported version {}, capping",
                                network_version.as_u64(),
                                ProtocolVersion::MAX.as_u64()
                            );
                            ProtocolVersion::MAX
                        } else {
                            network_version
                        };

                        let p = ProtocolConfig::get_for_version(version, chain);
                        if tx
                            .send(SystemState {
                                reference_gas_price: result.reference_gas_price,
                                protocol_config: Some(p),
                                epoch: result.epoch,
                            })
                            .is_ok()
                        {
                            info!(
                                "Reference gas price = {:?}, epoch = {}",
                                result.reference_gas_price, result.epoch
                            );
                        }
                    }
                    Err(err) => {
                        error!("Failed to get system state object: {:?}", err);
                    }
                }
            }
        });
        Self { state: rx }
    }

    pub fn new_from_test_cluster(test_cluster: &TestCluster) -> Self {
        let chain = test_cluster.get_chain_identifier().chain();
        let convert_system_state = move |system_state: SuiSystemState| -> SystemState {
            SystemState {
                reference_gas_price: system_state.reference_gas_price(),
                protocol_config: Some(ProtocolConfig::get_for_version(
                    ProtocolVersion::new(system_state.protocol_version()),
                    chain,
                )),
                epoch: system_state.epoch(),
            }
        };

        let initial_system_state = convert_system_state(test_cluster.get_sui_system_state());
        let (tx, rx) = watch::channel(initial_system_state.clone());
        let mut receiver = test_cluster.subscribe_to_epoch_change();
        tx.send_modify(move |state| *state = initial_system_state);

        tokio::task::spawn(async move {
            loop {
                let Ok(system_state) = receiver.recv().await else {
                    info!("Epoch change receiver closed, exiting task");
                    break;
                };
                let system_state = convert_system_state(system_state);
                // send new system state only if the epoch has changed
                tx.send_if_modified(|state| {
                    if state.epoch != system_state.epoch {
                        *state = system_state;
                        true
                    } else {
                        false
                    }
                });
            }
        });
        Self { state: rx }
    }
}
