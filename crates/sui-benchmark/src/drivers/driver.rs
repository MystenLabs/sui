// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::drivers::Interval;
use crate::ValidatorProxy;
use async_trait::async_trait;
use prometheus::Registry;

use crate::workloads::workload::WorkloadInfo;

#[async_trait]
pub trait Driver<T> {
    async fn run(
        &self,
        workload: Vec<WorkloadInfo>,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        registry: &Registry,
        show_progress: bool,
        run_duration: Interval,
    ) -> Result<T, anyhow::Error>;
}
