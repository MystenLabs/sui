// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::object_store::http::{get, DEFAULT_USER_AGENT, STRICT_PATH_ENCODE_SET};
use crate::object_store::ObjectStoreGetExt;
use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use object_store::path::Path;
use object_store::GetResult;
use percent_encoding::{utf8_percent_encode, PercentEncode};
use reqwest::Client;
use reqwest::ClientBuilder;
use std::fmt;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) struct S3Client {
    endpoint: String,
    client: Client,
}

impl S3Client {
    pub fn new(endpoint: &str) -> Result<Self> {
        let mut builder = ClientBuilder::new();
        builder = builder
            .user_agent(DEFAULT_USER_AGENT)
            .pool_idle_timeout(None);
        let client = builder.https_only(false).build()?;

        Ok(Self {
            endpoint: endpoint.to_string(),
            client,
        })
    }
    async fn get(&self, location: &Path) -> Result<GetResult> {
        let url = self.path_url(location);
        get(&url, "s3", location, &self.client).await
    }
    fn path_url(&self, path: &Path) -> String {
        format!("{}/{}", self.endpoint, Self::encode_path(path))
    }
    fn encode_path(path: &Path) -> PercentEncode<'_> {
        utf8_percent_encode(path.as_ref(), &STRICT_PATH_ENCODE_SET)
    }
}

/// Interface for [Amazon S3](https://aws.amazon.com/s3/).
#[derive(Debug)]
pub struct AmazonS3 {
    client: Arc<S3Client>,
}

impl AmazonS3 {
    pub fn new(endpoint: &str) -> Result<Self> {
        let s3_client = S3Client::new(endpoint)?;
        Ok(AmazonS3 {
            client: Arc::new(s3_client),
        })
    }
}

impl fmt::Display for AmazonS3 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "s3:{}", self.client.endpoint)
    }
}

#[async_trait]
impl ObjectStoreGetExt for AmazonS3 {
    async fn get_bytes(&self, location: &Path) -> Result<Bytes> {
        let result = self.client.get(location).await?;
        let bytes = result.bytes().await?;
        Ok(bytes)
    }
}
