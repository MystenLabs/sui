// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoint_fetcher::archival_fetcher::ArchivalFetcher;
use crate::checkpoint_fetcher::rest_fetcher::RestFetcher;
use crate::checkpoint_fetcher::CheckpointFetcherTrait;
use std::sync::Arc;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

/// The HybridFetcher first tries to fetch the checkpoint from the REST API.
/// If that fails, it falls back to fetching the checkpoint from the archival store.
pub(crate) struct HybridFetcher {
    archival_fetcher: ArchivalFetcher,
    rest_fetcher: RestFetcher,
}

impl HybridFetcher {
    pub fn new(archival_fetcher: ArchivalFetcher, rest_fetcher: RestFetcher) -> Self {
        Self {
            archival_fetcher,
            rest_fetcher,
        }
    }
}

#[async_trait::async_trait]
impl CheckpointFetcherTrait for HybridFetcher {
    async fn fetch_checkpoint(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> anyhow::Result<(Arc<CheckpointData>, usize)> {
        match self.rest_fetcher.fetch_checkpoint(sequence_number).await {
            Ok(result) => Ok(result),
            Err(_) => {
                self.archival_fetcher
                    .fetch_checkpoint(sequence_number)
                    .await
            }
        }
    }
}
