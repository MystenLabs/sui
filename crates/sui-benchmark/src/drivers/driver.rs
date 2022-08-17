// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::workloads::workload::Payload;
use crate::workloads::workload::Workload;
use async_trait::async_trait;
use prometheus::Registry;
use sui_core::authority_aggregator::AuthorityAggregator;
use sui_core::authority_client::NetworkAuthorityClient;

#[async_trait]
pub trait Driver<T> {
    async fn run(
        &self,
        workload: Box<dyn Workload<dyn Payload>>,
        aggregator: AuthorityAggregator<NetworkAuthorityClient>,
        registry: &Registry,
    ) -> Result<T, anyhow::Error>;
}
