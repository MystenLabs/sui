// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use object_store::aws::AmazonS3ConfigKey;
use object_store::path::Path;
use object_store::{ClientConfigKey, ClientOptions, ObjectStore, RetryConfig};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::Duration;
use sui_data_ingestion_core::Worker;
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
        let url = Url::parse(&config.url).expect("failed to parse remote store url");
        let mut builder = object_store::aws::AmazonS3Builder::new().with_url(url.as_str());
        for (key, value) in config.remote_store_options {
            builder = builder.with_config(
                AmazonS3ConfigKey::from_str(&key).expect("failed to parse config"),
                value,
            );
        }
        let retry_config = RetryConfig {
            max_retries: 0,
            retry_timeout: Duration::from_secs(10),
            ..Default::default()
        };
        builder = builder.with_retry(retry_config);
        builder = builder.with_client_options(
            ClientOptions::new().with_config(ClientConfigKey::Timeout, "15 seconds".to_string()),
        );
        let remote_store = Box::new(builder.build().expect("failed to create object store"));
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
