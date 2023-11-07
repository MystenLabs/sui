// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::object_store::downloader::{
    get, Downloader, DEFAULT_USER_AGENT, STRICT_PATH_ENCODE_SET,
};
use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use object_store::path::Path;
use object_store::GetResult;
use percent_encoding::{utf8_percent_encode, PercentEncode};
use reqwest::Client;
use reqwest::ClientBuilder;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) struct S3Client {
    endpoint: String,
    client: Client,
}

impl S3Client {
    pub fn new(bucket: &str, region: &str, transfer_accelerated: bool) -> Result<Self> {
        let mut builder = ClientBuilder::new();
        builder = builder.user_agent(DEFAULT_USER_AGENT);
        let endpoint = if transfer_accelerated {
            format!("https://{bucket}.s3-accelerate.amazonaws.com")
        } else {
            // only use virtual hosted style requests
            format!("https://{bucket}.s3.{region}.amazonaws.com")
        };
        let client = builder.https_only(false).build()?;
        Ok(Self { endpoint, client })
    }
    pub fn new_with_endpoint(endpoint: String) -> Result<Self> {
        let mut builder = ClientBuilder::new();
        builder = builder.user_agent(DEFAULT_USER_AGENT);
        let client = builder.https_only(false).build()?;
        Ok(Self { endpoint, client })
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
    pub fn new(bucket: &str, region: &str, transfer_accelerated: bool) -> Result<Self> {
        let s3_client = S3Client::new(bucket, region, transfer_accelerated)?;
        Ok(AmazonS3 {
            client: Arc::new(s3_client),
        })
    }
}

#[async_trait]
impl Downloader for AmazonS3 {
    async fn get(&self, location: &Path) -> Result<Bytes> {
        let result = self.client.get(location).await?;
        let bytes = result.bytes().await?;
        Ok(bytes)
    }
}
