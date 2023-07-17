// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context};
use clap::*;
use object_store::aws::AmazonS3Builder;
use object_store::DynObjectStore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

pub mod util;

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
    #[clap(value_enum)]
    pub object_store: Option<ObjectStoreType>,
    /// Path of the local directory. Only relevant is `--object-store` is File
    #[serde(skip_serializing_if = "Option::is_none")]
    #[clap(long)]
    pub directory: Option<PathBuf>,
    /// Name of the bucket to use for the object store. Must also set
    /// `--object-store` to a cloud object storage to have any effect.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[clap(long)]
    pub bucket: Option<String>,
    /// When using Amazon S3 as the object store, set this to an access key that
    /// has permission to read from and write to the specified S3 bucket.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[clap(long)]
    pub aws_access_key_id: Option<String>,
    /// When using Amazon S3 as the object store, set this to the secret access
    /// key that goes with the specified access key ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[clap(long)]
    pub aws_secret_access_key: Option<String>,
    /// When using Amazon S3 as the object store, set this to the region
    /// that goes with the specified bucket
    #[serde(skip_serializing_if = "Option::is_none")]
    #[clap(long)]
    pub aws_region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[clap(long)]
    pub aws_profile: Option<String>,
    /// Allow unencrypted HTTP connection to AWS.
    #[serde(default)]
    #[clap(long, default_value_t = false)]
    pub aws_allow_http: bool,
    /// When using Google Cloud Storage as the object store, set this to the
    /// path to the JSON file that contains the Google credentials.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[clap(long)]
    pub google_service_account: Option<String>,
    /// When using Microsoft Azure as the object store, set this to the
    /// azure account name
    #[serde(skip_serializing_if = "Option::is_none")]
    #[clap(long)]
    pub azure_storage_account: Option<String>,
    /// When using Microsoft Azure as the object store, set this to one of the
    /// keys in storage account settings
    #[serde(skip_serializing_if = "Option::is_none")]
    #[clap(long)]
    pub azure_storage_access_key: Option<String>,
    #[serde(default = "default_object_store_connection_limit")]
    #[clap(long, default_value_t = 20)]
    pub object_store_connection_limit: usize,
}

fn default_object_store_connection_limit() -> usize {
    20
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
            .with_allow_http(self.aws_allow_http)
            .with_imdsv1_fallback();

        if let Some(region) = &self.aws_region {
            builder = builder.with_region(region);
        }
        if let Some(bucket) = &self.bucket {
            builder = builder.with_bucket_name(bucket);
        }
        if let Some(key_id) = &self.aws_access_key_id {
            builder = builder.with_access_key_id(key_id);
        }
        if let Some(secret) = &self.aws_secret_access_key {
            builder = builder.with_secret_access_key(secret);
        }
        if let Some(profile) = &self.aws_profile {
            builder = builder.with_profile(profile);
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

        let mut builder = MicrosoftAzureBuilder::new();

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
