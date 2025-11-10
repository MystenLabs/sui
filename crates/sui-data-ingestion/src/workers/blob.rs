// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use object_store::ObjectStore;
use object_store::path::Path;
use serde::{Deserialize, Serialize};
use sui_data_ingestion_core::{Worker, create_remote_store_client};
use sui_storage::blob::{Blob, BlobEncoding};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BlobTaskConfig {
    pub url: String,
    pub remote_store_options: Vec<(String, String)>,
}

pub struct BlobWorker {
    remote_store: Box<dyn ObjectStore>,
}

impl BlobWorker {
    pub fn new(config: BlobTaskConfig) -> Self {
        Self {
            remote_store: create_remote_store_client(config.url, config.remote_store_options, 10)
                .expect("failed to create remote store client"),
        }
    }
}

#[async_trait]
impl Worker for BlobWorker {
    type Result = ();
    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> Result<()> {
        let bytes = Blob::encode(checkpoint, BlobEncoding::Bcs)?.to_bytes();
        let location = Path::from(format!(
            "{}.chk",
            checkpoint.checkpoint_summary.sequence_number
        ));
        self.remote_store
            .put(&location, Bytes::from(bytes).into())
            .await?;
        if checkpoint.checkpoint_summary.is_last_checkpoint_of_epoch() {
            let location = Path::from("epochs.json");
            let response = self.remote_store.get(&location).await?;
            let mut data: Vec<CheckpointSequenceNumber> =
                serde_json::from_slice(response.bytes().await?.as_ref())?;
            data.push(checkpoint.checkpoint_summary.sequence_number);
            data.sort();
            data.dedup();
            self.remote_store
                .put(&location, Bytes::from(serde_json::to_vec(&data)?).into())
                .await?;
        }
        Ok(())
    }
}
