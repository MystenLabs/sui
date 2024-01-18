// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Worker;
use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use object_store::path::Path;
use object_store::{parse_url_opts, ObjectStore};
use serde::{Deserialize, Serialize};
use sui_storage::blob::{Blob, BlobEncoding};
use sui_types::full_checkpoint_content::CheckpointData;
use url::Url;

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
        let remote_store = parse_url_opts(
            &Url::parse(&config.url).expect("failed to parse remote store url"),
            config.remote_store_options,
        )
        .expect("failed to parse remote store config")
        .0;
        Self { remote_store }
    }
}

#[async_trait]
impl Worker for BlobWorker {
    async fn process_checkpoint(&self, checkpoint: CheckpointData) -> Result<()> {
        let bytes = Blob::encode(&checkpoint, BlobEncoding::Bcs)?.to_bytes();
        let location = Path::from(format!(
            "{}.chk",
            checkpoint.checkpoint_summary.sequence_number
        ));
        self.remote_store.put(&location, Bytes::from(bytes)).await?;
        Ok(())
    }
}
