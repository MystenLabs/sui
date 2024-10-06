// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoint_fetcher::CheckpointFetcherTrait;
use std::sync::Arc;
use sui_rest_api::Client;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

/// Fetches checkpoint data from the REST API.
pub(crate) struct RestFetcher {
    client: Client,
}

impl RestFetcher {
    pub fn new(url: String) -> Self {
        let client = Client::new(url);
        Self { client }
    }
}

#[async_trait::async_trait]
impl CheckpointFetcherTrait for RestFetcher {
    async fn fetch_checkpoint(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> anyhow::Result<(Arc<CheckpointData>, usize)> {
        let checkpoint = self.client.get_full_checkpoint(sequence_number).await?;
        let size = bcs::serialized_size(&checkpoint)?;
        Ok((Arc::new(checkpoint), size))
    }
}
