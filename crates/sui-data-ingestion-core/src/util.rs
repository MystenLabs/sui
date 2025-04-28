// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use object_store::aws::AmazonS3ConfigKey;
use object_store::gcp::GoogleConfigKey;
use object_store::{ClientOptions, ObjectStore, RetryConfig};
use std::str::FromStr;
use std::time::Duration;
use url::Url;

pub fn create_remote_store_client(
    url: String,
    remote_store_options: Vec<(String, String)>,
    timeout_secs: u64,
) -> Result<Box<dyn ObjectStore>> {
    let retry_config = RetryConfig {
        max_retries: 0,
        retry_timeout: Duration::from_secs(timeout_secs + 1),
        ..Default::default()
    };
    let client_options = ClientOptions::new()
        .with_timeout(Duration::from_secs(timeout_secs))
        .with_allow_http(true);
    let url = Url::parse(&url)?;
    match url.scheme() {
        "http" | "https" => {
            let http_store = object_store::http::HttpBuilder::new()
                .with_url(url)
                .with_client_options(client_options)
                .with_retry(retry_config)
                .build()?;
            Ok(Box::new(http_store))
        }
        "gs" => {
            let mut builder = object_store::gcp::GoogleCloudStorageBuilder::new()
                .with_url(url.as_str())
                .with_retry(retry_config)
                .with_client_options(client_options);
            for (key, value) in remote_store_options {
                builder = builder.with_config(GoogleConfigKey::from_str(&key)?, value);
            }
            Ok(Box::new(builder.build()?))
        }
        "s3" => {
            let mut builder = object_store::aws::AmazonS3Builder::new()
                .with_url(url.as_str())
                .with_retry(retry_config)
                .with_client_options(client_options);
            for (key, value) in remote_store_options {
                builder = builder.with_config(AmazonS3ConfigKey::from_str(&key)?, value);
            }
            Ok(Box::new(builder.build()?))
        }
        "file" => Ok(Box::new(
            object_store::local::LocalFileSystem::new_with_prefix(url.path())?,
        )),
        _ => Err(anyhow::anyhow!("Unsupported URL scheme: {}", url.scheme())),
    }
}
