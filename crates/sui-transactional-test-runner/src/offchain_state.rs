// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use std::time::Duration;

pub struct TestResponse {
    pub response_body: String,
    pub http_headers: Option<http::HeaderMap>,
    pub service_version: Option<String>,
}

#[async_trait]
pub trait OffchainStateReader: Send + Sync + 'static {
    async fn wait_for_objects_snapshot_catchup(&self, base_timeout: Duration);
    async fn wait_for_checkpoint_catchup(&self, checkpoint: u64, base_timeout: Duration);
    async fn wait_for_pruned_checkpoint(&self, checkpoint: u64, base_timeout: Duration);
    async fn execute_graphql(
        &self,
        query: String,
        show_usage: bool,
    ) -> Result<TestResponse, anyhow::Error>;
}
