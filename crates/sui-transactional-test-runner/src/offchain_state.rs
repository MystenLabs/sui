// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use std::time::Duration;

pub struct TestResponse {
    pub response_body: String,
    pub http_headers: Option<http::HeaderMap>,
    pub service_version: Option<String>,
}

/// Trait for interacting with the offchain state of the Sui network. To reduce test flakiness,
/// these methods are used in the `RunGraphqlCommand` to stabilize the off-chain indexed state.
#[async_trait]
pub trait OffchainStateReader: Send + Sync + 'static {
    /// Polls the objects snapshot table until it is within the allowed lag from the latest
    /// checkpoint.
    async fn wait_for_objects_snapshot_catchup(&self, base_timeout: Duration);
    /// Polls the checkpoint table until the given checkpoint is committed.
    async fn wait_for_checkpoint_catchup(&self, checkpoint: u64, base_timeout: Duration);
    /// Polls the checkpoint table until the given checkpoint is pruned.
    async fn wait_for_pruned_checkpoint(&self, checkpoint: u64, base_timeout: Duration);
    /// Executes a GraphQL query and returns the response.
    async fn execute_graphql(
        &self,
        query: String,
        show_usage: bool,
    ) -> Result<TestResponse, anyhow::Error>;
}
