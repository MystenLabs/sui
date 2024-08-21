// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::object_store::http::{get, DEFAULT_USER_AGENT};
use crate::object_store::ObjectStoreGetExt;
use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use object_store::path::Path;
use object_store::{BackoffConfig, GetResult, RetryConfig};
use percent_encoding::{percent_encode, utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use std::fmt;
use std::sync::Arc;
use sui_config::object_storage_config::retry_config;

#[derive(Debug)]
struct GoogleCloudStorageClient {
    client: ClientWithMiddleware,
    bucket_name_encoded: String,
}

impl GoogleCloudStorageClient {
    pub fn new(bucket: &str) -> Result<Self> {
        let RetryConfig {
            backoff:
                BackoffConfig {
                    init_backoff,
                    max_backoff,
                    base,
                },
            max_retries,
            ..
        } = retry_config();
        let retry_policy = ExponentialBackoff::builder()
            .retry_bounds(init_backoff, max_backoff)
            .base(base as u32)
            .build_with_max_retries(max_retries as u32);
        let reqwest_client = reqwest::Client::builder()
            .user_agent(DEFAULT_USER_AGENT)
            .https_only(false)
            .build()?;
        let middleware_client = ClientBuilder::new(reqwest_client)
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();
        let bucket_name_encoded = percent_encode(bucket.as_bytes(), NON_ALPHANUMERIC).to_string();

        Ok(Self {
            client: middleware_client,
            bucket_name_encoded,
        })
    }

    async fn get(&self, path: &Path) -> Result<GetResult> {
        let url = self.object_url(path);
        get(&url, "gcs", path, &self.client).await
    }

    fn object_url(&self, path: &Path) -> String {
        let encoded = utf8_percent_encode(path.as_ref(), NON_ALPHANUMERIC);
        format!(
            "https://storage.googleapis.com/{}/{}",
            self.bucket_name_encoded, encoded
        )
    }
}

/// Interface for [Google Cloud Storage](https://cloud.google.com/storage/).
#[derive(Debug)]
pub struct GoogleCloudStorage {
    client: Arc<GoogleCloudStorageClient>,
}

impl GoogleCloudStorage {
    pub fn new(bucket: &str) -> Result<Self> {
        let gcs_client = GoogleCloudStorageClient::new(bucket)?;
        Ok(GoogleCloudStorage {
            client: Arc::new(gcs_client),
        })
    }
}

impl fmt::Display for GoogleCloudStorage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "gcs:{}", self.client.bucket_name_encoded)
    }
}

#[async_trait]
impl ObjectStoreGetExt for GoogleCloudStorage {
    async fn get_bytes(&self, location: &Path) -> Result<Bytes> {
        let result = self.client.get(location).await?;
        let bytes = result.bytes().await?;
        Ok(bytes)
    }
}
