// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::drivers::Interval;
use async_trait::async_trait;
use prometheus::Registry;
use sui_core::authority_aggregator::AuthorityAggregator;
use sui_core::authority_client::NetworkAuthorityClient;

use crate::workloads::workload::WorkloadInfo;

#[async_trait]
pub trait Driver<T> {
    async fn run(
        &self,
        workload: Vec<WorkloadInfo>,
        aggregator: Arc<AuthorityAggregator<NetworkAuthorityClient>>,
        registry: &Registry,
        show_progress: bool,
        run_duration: Interval,
    ) -> Result<T, anyhow::Error>;
}
