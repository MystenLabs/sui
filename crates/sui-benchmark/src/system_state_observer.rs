// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ValidatorProxy;
use std::sync::Arc;
use std::time::Duration;
use sui_types::{sui_system_state::SuiSystemState, SUI_SYSTEM_STATE_OBJECT_ID};
use tokio::sync::oneshot::Sender;
use tokio::sync::watch;
use tokio::sync::watch::Receiver;
use tokio::time;
use tokio::time::Instant;
use tracing::info;

pub struct SystemStateObserver {
    pub reference_gas_price: Receiver<u64>,
    pub _sender: Sender<()>,
}

impl SystemStateObserver {
    pub fn new(proxy: Arc<dyn ValidatorProxy + Send + Sync>) -> Self {
        let (sender, mut recv) = tokio::sync::oneshot::channel();
        let mut interval = tokio::time::interval_at(Instant::now(), Duration::from_secs(60));
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        let (tx, rx) = watch::channel(1u64);
        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Ok(system_state) = proxy.get_object(SUI_SYSTEM_STATE_OBJECT_ID).await {
                            let move_obj = system_state.data.try_as_move().unwrap();
                            if let Ok(result) = bcs::from_bytes::<SuiSystemState>(move_obj.contents()) {
                                if tx.send(result.reference_gas_price).is_ok() {
                                    info!("Reference gas price = {:?}", result.reference_gas_price);
                                }
                            }
                        }
                    }
                    _ = &mut recv => break,
                }
            }
        });
        Self {
            reference_gas_price: rx,
            _sender: sender,
        }
    }
}
