// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The SuiBridgeStatus observable monitors whether the Sui Bridge is paused.

use crate::sui_bridge_watchdog::Observable;
use async_trait::async_trait;
use prometheus::IntGaugeVec;
use std::collections::BTreeMap;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoRequest;

use tokio::time::Duration;
use tracing::{error, info};

pub struct TotalSupplies {
    sui_client: sui_rpc::Client,
    coins: BTreeMap<String, String>,
    metric: IntGaugeVec,
}

impl TotalSupplies {
    pub fn new(
        sui_client: sui_rpc::Client,
        coins: BTreeMap<String, String>,
        metric: IntGaugeVec,
    ) -> Self {
        Self {
            sui_client,
            coins,
            metric,
        }
    }
}

#[async_trait]
impl Observable for TotalSupplies {
    fn name(&self) -> &str {
        "TotalSupplies"
    }

    async fn observe_and_report(&self) {
        for (coin_name, coin_type) in &self.coins {
            let resp = self
                .sui_client
                .clone()
                .state_client()
                .get_coin_info(GetCoinInfoRequest::default().with_coin_type(coin_type))
                .await;
            match resp {
                Ok(resp) => {
                    let supply = resp.into_inner().treasury().total_supply();
                    self.metric
                        .with_label_values(&[coin_name])
                        .set(supply as i64);
                    info!("Total supply for {coin_type}: {}", supply);
                }
                Err(e) => {
                    error!("Error getting total supply for coin {coin_type}: {:?}", e);
                }
            }
        }
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(10)
    }
}
