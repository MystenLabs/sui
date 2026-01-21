// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ValidatorProxy;
use std::sync::Arc;
use std::time::Duration;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::base_types::EpochId;
use tokio::sync::oneshot::Sender;
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

#[derive(Debug)]
pub struct SystemStateObserver {
    pub state: Receiver<SystemState>,
    pub _sender: Sender<()>,
}

impl SystemStateObserver {
    pub fn new(proxy: Arc<dyn ValidatorProxy + Send + Sync>) -> Self {
        let (sender, mut recv) = tokio::sync::oneshot::channel();
        let mut interval = tokio::time::interval_at(Instant::now(), Duration::from_secs(60));
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        let (tx, rx) = watch::channel(SystemState {
            reference_gas_price: 1u64,
            protocol_config: None,
            epoch: 0,
        });
        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        match proxy.get_latest_system_state_object().await {
                            Ok(result) => {
                                let p = ProtocolConfig::get_for_version(ProtocolVersion::new(result.protocol_version), Chain::Unknown);
                                if tx.send(SystemState {
                                    reference_gas_price: result.reference_gas_price,
                                    protocol_config: Some(p),
                                    epoch: result.epoch,
                                }).is_ok() {
                                    info!("Reference gas price = {:?}, epoch = {}", result.reference_gas_price, result.epoch);
                                }
                            }
                            Err(err) => {
                                error!("Failed to get system state object: {:?}", err);
                            }
                        }
                    }
                    _ = &mut recv => break,
                }
            }
        });
        Self {
            state: rx,
            _sender: sender,
        }
    }
}
