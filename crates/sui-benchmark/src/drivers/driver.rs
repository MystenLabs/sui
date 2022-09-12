// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
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
        aggregator: AuthorityAggregator<NetworkAuthorityClient>,
        registry: &Registry,
    ) -> Result<T, anyhow::Error>;
}
