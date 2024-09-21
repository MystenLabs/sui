// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context, Result};

use clap::*;
use object_store::aws::AmazonS3Builder;
use object_store::{ClientOptions, DynObjectStore};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::{env, fs};
use tracing::info;

/// Object-store type.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize, ValueEnum)]
pub enum ObjectStoreType {
    /// Local file system
    File,
    /// AWS S3
    S3,
    /// Google Cloud Store
    GCS,
    /// Azure Blob Store
    Azure,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize, Args)]
#[serde(rename_all = "kebab-case")]
pub struct ObjectStoreConfig {
    /// Which object storage to use. If not specified, defaults to local file system.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(value_enum)]
    pub object_store: Option<ObjectStoreType>,
    /// Path of the local directory. Only relevant is `--object-store` is File
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(long)]
    pub directory: Option<PathBuf>,
    /// Name of the bucket to use for the object store. Must also set
    /// `--object-store` to a cloud object storage to have any effect.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(long)]
    pub bucket: Option<String>,
    /// When using Amazon S3 as the object store, set this to an access key that
    /// has permission to read from and write to the specified S3 bucket.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(long)]
    pub aws_access_key_id: Option<String>,
    /// When using Amazon S3 as the object store, set this to the secret access
    /// key that goes with the specified access key ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(long)]
    pub aws_secret_access_key: Option<String>,
    /// When using Amazon S3 as the object store, set this to bucket endpoint
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(long)]
    pub aws_endpoint: Option<String>,
    /// When using Amazon S3 as the object store, set this to the region
    /// that goes with the specified bucket
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(long)]
    pub aws_region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(long)]
    pub aws_profile: Option<String>,
    /// Enable virtual hosted style requests
    #[serde(default)]
    #[arg(long, default_value_t = true)]
    pub aws_virtual_hosted_style_request: bool,
    /// Allow unencrypted HTTP connection to AWS.
    #[serde(default)]
    #[arg(long, default_value_t = true)]
    pub aws_allow_http: bool,
    /// When using Google Cloud Storage as the object store, set this to the
    /// path to the JSON file that contains the Google credentials.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(long)]
    pub google_service_account: Option<String>,
    /// When using Google Cloud Storage as the object store and writing to a
    /// bucket with Requester Pays enabled, set this to the project_id
    /// you want to associate the write cost with.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(long)]
    pub google_project_id: Option<String>,
    /// When using Microsoft Azure as the object store, set this to the
    /// azure account name
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(long)]
    pub azure_storage_account: Option<String>,
    /// When using Microsoft Azure as the object store, set this to one of the
    /// keys in storage account settings
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(long)]
    pub azure_storage_access_key: Option<String>,
    #[serde(default = "default_object_store_connection_limit")]
    #[arg(long, default_value_t = 20)]
    pub object_store_connection_limit: usize,
    #[serde(default)]
    #[arg(long, default_value_t = false)]
    pub no_sign_request: bool,
}

fn default_object_store_connection_limit() -> usize {
    20
}

fn no_timeout_options() -> ClientOptions {
    ClientOptions::new()
        .with_timeout_disabled()
        .with_connect_timeout_disabled()
        .with_pool_idle_timeout(std::time::Duration::from_secs(300))
}

impl ObjectStoreConfig {
    fn new_local_fs(&self) -> Result<Arc<DynObjectStore>, anyhow::Error> {
        info!(directory=?self.directory, object_store_type="File", "Object Store");
        if let Some(path) = &self.directory {
            fs::create_dir_all(path).context(anyhow!(
                "Failed to create local directory: {}",
                path.display()
            ))?;
            let store = object_store::local::LocalFileSystem::new_with_prefix(path)
                .context(anyhow!("Failed to create local object store"))?;
            Ok(Arc::new(store))
        } else {
            Err(anyhow!("No directory provided for local fs storage"))
        }
    }
    fn new_s3(&self) -> Result<Arc<DynObjectStore>, anyhow::Error> {
        use object_store::limit::LimitStore;

        info!(bucket=?self.bucket, object_store_type="S3", "Object Store");

        let mut builder = AmazonS3Builder::new()
            .with_client_options(no_timeout_options())
            .with_imdsv1_fallback();

        if self.aws_virtual_hosted_style_request {
            builder = builder.with_virtual_hosted_style_request(true);
        }
        if self.aws_allow_http {
            builder = builder.with_allow_http(true);
        }
        if let Some(region) = &self.aws_region {
            builder = builder.with_region(region);
        }
        if let Some(bucket) = &self.bucket {
            builder = builder.with_bucket_name(bucket);
        }

        if let Some(key_id) = &self.aws_access_key_id {
            builder = builder.with_access_key_id(key_id);
        } else if let Ok(secret) = env::var("ARCHIVE_READ_AWS_ACCESS_KEY_ID") {
            builder = builder.with_access_key_id(secret);
        } else if let Ok(secret) = env::var("FORMAL_SNAPSHOT_WRITE_AWS_ACCESS_KEY_ID") {
            builder = builder.with_access_key_id(secret);
        } else if let Ok(secret) = env::var("DB_SNAPSHOT_READ_AWS_ACCESS_KEY_ID") {
            builder = builder.with_access_key_id(secret);
        }

        if let Some(secret) = &self.aws_secret_access_key {
            builder = builder.with_secret_access_key(secret);
        } else if let Ok(secret) = env::var("ARCHIVE_READ_AWS_SECRET_ACCESS_KEY") {
            builder = builder.with_secret_access_key(secret);
        } else if let Ok(secret) = env::var("FORMAL_SNAPSHOT_WRITE_AWS_SECRET_ACCESS_KEY") {
            builder = builder.with_secret_access_key(secret);
        } else if let Ok(secret) = env::var("DB_SNAPSHOT_READ_AWS_SECRET_ACCESS_KEY") {
            builder = builder.with_secret_access_key(secret);
        }

        if let Some(endpoint) = &self.aws_endpoint {
            builder = builder.with_endpoint(endpoint);
        }
        Ok(Arc::new(LimitStore::new(
            builder.build().context("Invalid s3 config")?,
            self.object_store_connection_limit,
        )))
    }
    fn new_gcs(&self) -> Result<Arc<DynObjectStore>, anyhow::Error> {
        use object_store::gcp::GoogleCloudStorageBuilder;
        use object_store::limit::LimitStore;

        info!(bucket=?self.bucket, object_store_type="GCS", "Object Store");

        let mut builder = GoogleCloudStorageBuilder::new();

        if let Some(bucket) = &self.bucket {
            builder = builder.with_bucket_name(bucket);
        }
        if let Some(account) = &self.google_service_account {
            builder = builder.with_service_account_path(account);
        }

        let mut client_options = no_timeout_options();
        if let Some(google_project_id) = &self.google_project_id {
            let x_project_header = HeaderName::from_static("x-goog-user-project");
            let iam_req_header = HeaderName::from_static("userproject");

            let mut headers = HeaderMap::new();
            headers.insert(x_project_header, HeaderValue::from_str(google_project_id)?);
            headers.insert(iam_req_header, HeaderValue::from_str(google_project_id)?);
            client_options = client_options.with_default_headers(headers);
        }
        builder = builder.with_client_options(client_options);

        Ok(Arc::new(LimitStore::new(
            builder.build().context("Invalid gcs config")?,
            self.object_store_connection_limit,
        )))
    }
    fn new_azure(&self) -> Result<Arc<DynObjectStore>, anyhow::Error> {
        use object_store::azure::MicrosoftAzureBuilder;
        use object_store::limit::LimitStore;

        info!(bucket=?self.bucket, account=?self.azure_storage_account,
          object_store_type="Azure", "Object Store");

        let mut builder = MicrosoftAzureBuilder::new().with_client_options(no_timeout_options());

        if let Some(bucket) = &self.bucket {
            builder = builder.with_container_name(bucket);
        }
        if let Some(account) = &self.azure_storage_account {
            builder = builder.with_account(account)
        }
        if let Some(key) = &self.azure_storage_access_key {
            builder = builder.with_access_key(key)
        }

        Ok(Arc::new(LimitStore::new(
            builder.build().context("Invalid azure config")?,
            self.object_store_connection_limit,
        )))
    }
    pub fn make(&self) -> Result<Arc<DynObjectStore>, anyhow::Error> {
        match &self.object_store {
            Some(ObjectStoreType::File) => self.new_local_fs(),
            Some(ObjectStoreType::S3) => self.new_s3(),
            Some(ObjectStoreType::GCS) => self.new_gcs(),
            Some(ObjectStoreType::Azure) => self.new_azure(),
            _ => Err(anyhow!("At least one storage backend should be provided")),
        }
    }
}
