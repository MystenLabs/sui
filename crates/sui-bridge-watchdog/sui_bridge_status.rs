// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The SuiBridgeStatus observable monitors whether the Sui Bridge is paused.

use crate::Observable;
use async_trait::async_trait;
use prometheus::IntGauge;
use std::sync::Arc;
use sui_bridge::sui_client::SuiBridgeClient;

use tokio::time::Duration;
use tracing::{error, info};

pub struct SuiBridgeStatus {
    sui_client: Arc<SuiBridgeClient>,
    metric: IntGauge,
}

impl SuiBridgeStatus {
    pub fn new(sui_client: Arc<SuiBridgeClient>, metric: IntGauge) -> Self {
        Self { sui_client, metric }
    }
}

#[async_trait]
impl Observable for SuiBridgeStatus {
    fn name(&self) -> &str {
        "SuiBridgeStatus"
    }

    async fn observe_and_report(&self) {
        let status = self.sui_client.is_bridge_paused().await;
        match status {
            Ok(status) => {
                self.metric.set(status as i64);
                info!("Sui Bridge Status: {:?}", status);
            }
            Err(e) => {
                error!("Error getting sui bridge status: {:?}", e);
            }
        }
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(2)
    }
}
