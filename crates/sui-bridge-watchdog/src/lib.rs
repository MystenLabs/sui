// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The BridgeWatchDog module is responsible for monitoring the health
//! of the bridge by periodically running various observables and
//! reporting the results.

use anyhow::Result;
use async_trait::async_trait;
use mysten_metrics::spawn_logged_monitored_task;
use std::sync::Arc;
use tokio::time::Duration;
use tokio::time::MissedTickBehavior;
use tracing::{error_span, info, Instrument};

pub mod eth_bridge_status;
pub mod eth_vault_balance;
pub mod metrics;
pub mod sui_bridge_status;

pub struct BridgeWatchDog {
    observables: Vec<Arc<dyn Observable + Send + Sync>>,
}

impl BridgeWatchDog {
    pub fn new(observables: Vec<Arc<dyn Observable + Send + Sync>>) -> Self {
        Self { observables }
    }

    pub async fn run(self) {
        let mut handles = vec![];
        for observable in self.observables.into_iter() {
            let handle = spawn_logged_monitored_task!(Self::run_observable(observable));
            handles.push(handle);
        }
        // Return when any task returns an error or all tasks exit.
        futures::future::try_join_all(handles).await.unwrap();
        unreachable!("watch dog tasks should not exit");
    }

    async fn run_observable(observable: Arc<dyn Observable + Send + Sync>) -> Result<()> {
        let mut interval = tokio::time::interval(observable.interval());
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let name = observable.name();
        let span = error_span!("observable", name);
        loop {
            info!("Running observable {}", name);
            observable
                .observe_and_report()
                .instrument(span.clone())
                .await;
            interval.tick().await;
        }
    }
}

#[async_trait]
pub trait Observable {
    fn name(&self) -> &str;
    async fn observe_and_report(&self);
    fn interval(&self) -> Duration;
}
