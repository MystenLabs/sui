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
use tracing::{error, info};

#[derive(Debug, Clone)]
pub struct SystemState {
    pub reference_gas_price: u64,
    pub protocol_config: Option<ProtocolConfig>,
    pub epoch: EpochId,
}

/// Gas price for setup transactions: the reference gas price scaled by the configured
/// multiplier, rounded up so a multiplier above 1.0 always outbids the reference price.
pub fn setup_gas_price(reference_gas_price: u64, multiplier: f64) -> u64 {
    let scaled = (reference_gas_price as f64 * multiplier.max(1.0)).ceil();
    if scaled >= u64::MAX as f64 {
        u64::MAX
    } else {
        (scaled as u64).max(reference_gas_price)
    }
}

#[derive(Debug)]
pub struct SystemStateObserver {
    pub state: Receiver<SystemState>,
    setup_gas_price_multiplier: f64,
}

impl SystemStateObserver {
    /// Gas price for the benchmark's setup transactions.
    pub fn setup_gas_price(&self) -> u64 {
        setup_gas_price(
            self.state.borrow().reference_gas_price,
            self.setup_gas_price_multiplier,
        )
    }

    pub fn with_setup_gas_price_multiplier(mut self, multiplier: f64) -> Self {
        self.setup_gas_price_multiplier = multiplier;
        self
    }

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
                        let p = ProtocolConfig::get_for_version(
                            ProtocolVersion::new(result.protocol_version),
                            chain,
                        );
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
        Self {
            state: rx,
            setup_gas_price_multiplier: 1.0,
        }
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
        Self {
            state: rx,
            setup_gas_price_multiplier: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::setup_gas_price;

    #[test]
    fn setup_gas_price_scales_and_rounds_up() {
        assert_eq!(setup_gas_price(1000, 1.0), 1000);
        assert_eq!(setup_gas_price(1000, 2.0), 2000);
        assert_eq!(setup_gas_price(1000, 1.0005), 1001);
        assert_eq!(setup_gas_price(1000, 0.5), 1000);
        assert_eq!(setup_gas_price(u64::MAX, 2.0), u64::MAX);
    }
}
