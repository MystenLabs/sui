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
                            if let Ok(system_state) = bcs::from_bytes::<SuiSystemState>(move_obj.contents()) {
                                let reference_gas_price = compute_reference_gas_price(&system_state);
                                if tx.send(reference_gas_price).is_ok() {
                                    info!("Reference gas price = {:?}", reference_gas_price);
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

// Temporary fix-up of reference gas price.
fn compute_reference_gas_price(system_state: &SuiSystemState) -> u64 {
    let mut gas_prices: Vec<_> = system_state
        .validators
        .active_validators
        .iter()
        .map(|v| (v.gas_price, v.voting_power))
        .collect();

    gas_prices.sort();
    let mut votes = 0;
    let mut reference_gas_price = 0;
    const VOTING_THRESHOLD: u64 = 3_333;
    for (price, vote) in gas_prices.into_iter().rev() {
        if votes >= VOTING_THRESHOLD {
            break;
        }

        reference_gas_price = price;
        votes += vote;
    }
    reference_gas_price
}
