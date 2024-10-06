// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoint_fetcher::CheckpointFetcherTrait;
use crate::create_remote_store_client;
use object_store::path::Path;
use object_store::ObjectStore;
use std::sync::Arc;
use sui_storage::blob::Blob;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

/// Fetches checkpoint data from a remote archival store.
/// The store can be either an HTTP store or a cloud store (GCS or S3).
pub(crate) struct ArchivalFetcher {
    object_store: Box<dyn ObjectStore>,
}

impl ArchivalFetcher {
    pub fn new(
        url: String,
        remote_store_options: Vec<(String, String)>,
        timeout_secs: u64,
    ) -> anyhow::Result<Self> {
        let object_store = create_remote_store_client(url, remote_store_options, timeout_secs)?;
        Ok(Self { object_store })
    }
}

#[async_trait::async_trait]
impl CheckpointFetcherTrait for ArchivalFetcher {
    async fn fetch_checkpoint(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> anyhow::Result<(Arc<CheckpointData>, usize)> {
        let path = Path::from(format!("{}.chk", sequence_number));
        let response = self.object_store.get(&path).await?;
        let bytes = response.bytes().await?;
        let checkpoint_data = Blob::from_bytes::<Arc<CheckpointData>>(&bytes)?;
        Ok((checkpoint_data, bytes.len()))
    }
}
