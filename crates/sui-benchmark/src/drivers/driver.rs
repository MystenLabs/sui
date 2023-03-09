// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::drivers::Interval;
use crate::system_state_observer::SystemStateObserver;
use crate::ValidatorProxy;
use async_trait::async_trait;
use prometheus::Registry;

use crate::workloads::workload::WorkloadInfo;

#[async_trait]
pub trait Driver<T> {
    async fn run(
        &self,
        proxy_workloads: Vec<(Arc<dyn ValidatorProxy + Send + Sync>, Vec<WorkloadInfo>)>,
        system_state_observer: Arc<SystemStateObserver>,
        registry: &Registry,
        show_progress: bool,
        run_duration: Interval,
    ) -> Result<T, anyhow::Error>;
}
