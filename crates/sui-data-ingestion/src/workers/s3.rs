// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Worker;
use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3 as s3;
use aws_sdk_s3::config::{Credentials, Region};
use serde::{Deserialize, Serialize};
use sui_storage::blob::{Blob, BlobEncoding};
use sui_types::full_checkpoint_content::CheckpointData;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct S3TaskConfig {
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub aws_region: String,
    pub bucket_name: String,
}

#[derive(Clone)]
pub struct S3Worker {
    client: s3::Client,
    bucket_name: String,
}

impl S3Worker {
    pub async fn new(config: S3TaskConfig) -> Self {
        let credentials = Credentials::new(
            &config.aws_access_key_id,
            &config.aws_secret_access_key,
            None,
            None,
            "s3",
        );
        let aws_config = aws_config::from_env()
            .credentials_provider(credentials)
            .region(Region::new(config.aws_region))
            .load()
            .await;
        let client = s3::Client::new(&aws_config);
        Self {
            client,
            bucket_name: config.bucket_name,
        }
    }
}

#[async_trait]
impl Worker for S3Worker {
    async fn process_checkpoint(&self, checkpoint: CheckpointData) -> Result<()> {
        let bytes = Blob::encode(&checkpoint, BlobEncoding::Bcs)?.to_bytes();
        self.client
            .put_object()
            .bucket(self.bucket_name.clone())
            .key(format!(
                "{}.chk",
                checkpoint.checkpoint_summary.sequence_number
            ))
            .body(bytes.into())
            .send()
            .await?;
        Ok(())
    }
}
