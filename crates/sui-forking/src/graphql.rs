// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_graphql::{CheckpointResponse, Client};

pub struct GraphQLQueryClient {
    client: Client,
}

impl GraphQLQueryClient {
    pub fn new(endpoint: &str) -> anyhow::Result<Self> {
        let client = Client::new(endpoint)?;
        Ok(Self { client })
    }

    /// Fetch a checkpoint from GraphQL. If `sequence_number` is `None`, fetch the latest
    /// checkpoint.
    pub async fn fetch_checkpoint(
        &self,
        sequence_number: Option<u64>,
    ) -> anyhow::Result<Option<CheckpointResponse>> {
        self.client
            .get_checkpoint(sequence_number)
            .await
            .map_err(|e| e.into())
    }

    pub async fn fetch_protocol_version(&self) -> anyhow::Result<u64> {
        Ok(self.client.protocol_version().await?)
    }
}
