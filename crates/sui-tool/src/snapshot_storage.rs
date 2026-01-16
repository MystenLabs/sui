// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use clap::Args;
use object_store::aws::AmazonS3Builder;
use object_store::azure::MicrosoftAzureBuilder;
use object_store::gcp::GoogleCloudStorageBuilder;
use object_store::limit::LimitStore;
use object_store::local::LocalFileSystem;
use object_store::{ClientOptions, DynObjectStore};
use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_protocol_config::Chain;
use tracing::info;
use url::Url;

/// New-style snapshot source arguments using mutually exclusive flags.
/// Uses standard cloud provider environment variables for credentials.
#[derive(Args, Clone, Debug, Default)]
pub struct SnapshotSourceArgs {
    /// Fetch snapshot from AWS S3. Provide the bucket name.
    /// Credentials via env: AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_DEFAULT_REGION, AWS_ENDPOINT
    #[arg(long, group = "source")]
    pub s3: Option<String>,

    /// Fetch snapshot from Google Cloud Storage. Provide the bucket name.
    /// Credentials via env: GOOGLE_SERVICE_ACCOUNT_PATH
    #[arg(long, group = "source")]
    pub gcs: Option<String>,

    /// Fetch snapshot from Azure Blob Storage. Provide the container name.
    /// Credentials via env: AZURE_STORAGE_ACCOUNT_NAME, AZURE_STORAGE_ACCESS_KEY
    #[arg(long, group = "source")]
    pub azure: Option<String>,

    /// Fetch snapshot from an HTTP endpoint (unsigned/public access).
    #[arg(long, group = "source")]
    pub http: Option<Url>,

    /// Fetch snapshot from local filesystem. Provide the path to the snapshot directory.
    #[arg(long = "local-path", group = "source")]
    pub local_path: Option<PathBuf>,
}

impl SnapshotSourceArgs {
    pub fn has_explicit_source(&self) -> bool {
        self.s3.is_some()
            || self.gcs.is_some()
            || self.azure.is_some()
            || self.http.is_some()
            || self.local_path.is_some()
    }
}

/// Connection timeout arguments for snapshot downloads.
#[derive(Args, Clone, Debug, Default)]
pub struct StorageConnectionArgs {
    /// How long to wait for a snapshot file to be downloaded (milliseconds).
    /// Defaults to no timeout.
    #[arg(long)]
    pub snapshot_timeout_ms: Option<u64>,

    /// How long to wait when connecting to the snapshot store (milliseconds).
    /// Defaults to no timeout.
    #[arg(long)]
    pub snapshot_connection_timeout_ms: Option<u64>,

    /// Maximum number of concurrent connections to the object store.
    #[arg(long, default_value_t = 20)]
    pub object_store_connection_limit: usize,
}

impl From<&StorageConnectionArgs> for ClientOptions {
    fn from(args: &StorageConnectionArgs) -> ClientOptions {
        let mut opts = ClientOptions::new()
            .with_pool_idle_timeout(Duration::from_secs(300));

        opts = if let Some(timeout) = args.snapshot_timeout_ms {
            opts.with_timeout(Duration::from_millis(timeout))
        } else {
            opts.with_timeout_disabled()
        };

        opts = if let Some(timeout) = args.snapshot_connection_timeout_ms {
            opts.with_connect_timeout(Duration::from_millis(timeout))
        } else {
            opts.with_connect_timeout_disabled()
        };

        opts
    }
}

/// Legacy snapshot arguments (deprecated, for backward compatibility).
#[derive(Args, Clone, Debug, Default)]
pub struct LegacySnapshotArgs {
    /// [DEPRECATED] Use --s3, --gcs, --azure, --http, or --local-path instead.
    #[arg(long = "snapshot-bucket", hide = true)]
    pub snapshot_bucket: Option<String>,

    /// [DEPRECATED] Use --s3, --gcs, --azure, --http, or --local-path instead.
    #[arg(long = "snapshot-bucket-type", hide = true)]
    pub snapshot_bucket_type: Option<ObjectStoreType>,

    /// [DEPRECATED] Use --local-path instead.
    #[arg(long = "snapshot-path", hide = true)]
    pub snapshot_path: Option<PathBuf>,

    /// [DEPRECATED] Use --http with the appropriate public endpoint instead.
    #[arg(long = "no-sign-request", hide = true)]
    pub no_sign_request: bool,
}

impl LegacySnapshotArgs {
    pub fn is_used(&self) -> bool {
        self.snapshot_bucket.is_some()
            || self.snapshot_bucket_type.is_some()
            || self.snapshot_path.is_some()
            || self.no_sign_request
    }

    pub fn print_deprecation_warning(&self) {
        eprintln!("WARNING: The following flags are deprecated:");
        eprintln!("  --snapshot-bucket, --snapshot-bucket-type, --snapshot-path, --no-sign-request");
        eprintln!();
        eprintln!("Please use the new flags instead:");
        eprintln!("  --s3 <bucket>       for AWS S3");
        eprintln!("  --gcs <bucket>      for Google Cloud Storage");
        eprintln!("  --azure <container> for Azure Blob Storage");
        eprintln!("  --http <url>        for public HTTP access (replaces --no-sign-request)");
        eprintln!("  --local-path <path> for local filesystem");
        eprintln!();
        eprintln!("For more information, see: https://docs.sui.io/guides/operator/snapshots");
        eprintln!();
    }
}

/// Network-specific default values for snapshot locations.
pub struct NetworkDefaults {
    pub formal_bucket: &'static str,
    pub formal_http_endpoint: &'static str,
    pub db_bucket: &'static str,
    pub db_http_endpoint: &'static str,
}

impl NetworkDefaults {
    pub fn for_network(network: Chain) -> Option<Self> {
        match network {
            Chain::Mainnet => Some(Self {
                formal_bucket: "mysten-mainnet-formal",
                formal_http_endpoint: "https://formal-snapshot.mainnet.sui.io",
                db_bucket: "mysten-mainnet-snapshots",
                db_http_endpoint: "https://db-snapshot.mainnet.sui.io",
            }),
            Chain::Testnet => Some(Self {
                formal_bucket: "mysten-testnet-formal",
                formal_http_endpoint: "https://formal-snapshot.testnet.sui.io",
                db_bucket: "mysten-testnet-snapshots",
                db_http_endpoint: "https://db-snapshot.testnet.sui.io",
            }),
            Chain::Unknown => None,
        }
    }
}

/// Snapshot type for selecting appropriate defaults.
#[derive(Clone, Copy, Debug)]
pub enum SnapshotType {
    Formal,
    Db,
}

/// Build an object store client from the new-style source arguments.
/// Uses *Builder::from_env() to read credentials from standard environment variables.
/// Returns Arc<DynObjectStore> which implements ObjectStoreGetExt.
pub async fn build_object_store(
    source: &SnapshotSourceArgs,
    connection: &StorageConnectionArgs,
) -> Result<Arc<DynObjectStore>> {
    let client_options: ClientOptions = connection.into();
    let limit = connection.object_store_connection_limit;

    if let Some(bucket) = &source.s3 {
        info!(bucket, "S3 storage");
        let store = AmazonS3Builder::from_env()
            .with_client_options(client_options)
            .with_imdsv1_fallback()
            .with_bucket_name(bucket)
            .with_allow_http(true)
            .build()?;
        Ok(Arc::new(LimitStore::new(store, limit)))
    } else if let Some(bucket) = &source.gcs {
        info!(bucket, "GCS storage");
        let store = GoogleCloudStorageBuilder::from_env()
            .with_client_options(client_options)
            .with_bucket_name(bucket)
            .build()?;
        Ok(Arc::new(LimitStore::new(store, limit)))
    } else if let Some(container) = &source.azure {
        info!(container, "Azure storage");
        let store = MicrosoftAzureBuilder::from_env()
            .with_client_options(client_options)
            .with_container_name(container)
            .build()?;
        Ok(Arc::new(LimitStore::new(store, limit)))
    } else if let Some(endpoint) = &source.http {
        info!(endpoint = %endpoint, "HTTP storage (unsigned)");
        let config = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::S3),
            aws_endpoint: Some(endpoint.to_string()),
            aws_virtual_hosted_style_request: true,
            no_sign_request: true,
            object_store_connection_limit: limit,
            ..Default::default()
        };
        config.make()
    } else if let Some(path) = &source.local_path {
        info!(path = %path.display(), "Local filesystem storage");
        let store = LocalFileSystem::new_with_prefix(path)?;
        Ok(Arc::new(store))
    } else {
        Err(anyhow!("No snapshot source specified"))
    }
}

/// Resolve the effective source, handling defaults for unspecified sources.
pub fn resolve_source(
    source: &SnapshotSourceArgs,
    legacy: &LegacySnapshotArgs,
    network: Chain,
    snapshot_type: SnapshotType,
) -> Result<SnapshotSourceArgs> {
    if legacy.is_used() {
        legacy.print_deprecation_warning();
        return legacy_to_source(legacy, network, snapshot_type);
    }

    if source.has_explicit_source() {
        return Ok(source.clone());
    }

    let defaults = NetworkDefaults::for_network(network)
        .ok_or_else(|| anyhow!("Cannot determine default snapshot location for unknown network. Please specify a source with --s3, --gcs, --azure, --http, or --local-path"))?;

    let endpoint = match snapshot_type {
        SnapshotType::Formal => defaults.formal_http_endpoint,
        SnapshotType::Db => defaults.db_http_endpoint,
    };

    Ok(SnapshotSourceArgs {
        http: Some(Url::parse(endpoint)?),
        ..Default::default()
    })
}

/// Convert legacy arguments to the new source format.
fn legacy_to_source(
    legacy: &LegacySnapshotArgs,
    network: Chain,
    snapshot_type: SnapshotType,
) -> Result<SnapshotSourceArgs> {
    if legacy.no_sign_request {
        let defaults = NetworkDefaults::for_network(network)
            .ok_or_else(|| anyhow!("Cannot use --no-sign-request with unknown network"))?;
        let endpoint = match snapshot_type {
            SnapshotType::Formal => defaults.formal_http_endpoint,
            SnapshotType::Db => defaults.db_http_endpoint,
        };
        return Ok(SnapshotSourceArgs {
            http: Some(Url::parse(endpoint)?),
            ..Default::default()
        });
    }

    if let Some(path) = &legacy.snapshot_path {
        return Ok(SnapshotSourceArgs {
            local_path: Some(path.clone()),
            ..Default::default()
        });
    }

    let bucket = legacy.snapshot_bucket.clone().ok_or_else(|| {
        anyhow!("Legacy mode requires --snapshot-bucket or --no-sign-request")
    })?;

    let bucket_type = legacy.snapshot_bucket_type.ok_or_else(|| {
        anyhow!("Legacy mode requires --snapshot-bucket-type when --snapshot-bucket is specified")
    })?;

    match bucket_type {
        ObjectStoreType::S3 => Ok(SnapshotSourceArgs {
            s3: Some(bucket),
            ..Default::default()
        }),
        ObjectStoreType::GCS => Ok(SnapshotSourceArgs {
            gcs: Some(bucket),
            ..Default::default()
        }),
        ObjectStoreType::Azure => Ok(SnapshotSourceArgs {
            azure: Some(bucket),
            ..Default::default()
        }),
        ObjectStoreType::File => Err(anyhow!(
            "Use --local-path instead of --snapshot-bucket-type=FILE"
        )),
    }
}

/// Build object store from legacy arguments (for backward compatibility).
/// This preserves the old behavior including custom environment variable handling.
pub async fn build_object_store_legacy(
    legacy: &LegacySnapshotArgs,
    network: Chain,
    snapshot_type: SnapshotType,
    connection: &StorageConnectionArgs,
) -> Result<Arc<DynObjectStore>> {
    let source = legacy_to_source(legacy, network, snapshot_type)?;
    build_object_store(&source, connection).await
}

/// Convert SnapshotSourceArgs to ObjectStoreConfig for use with existing download functions.
/// This is a bridge to maintain compatibility with StateSnapshotReaderV1.
pub fn source_to_object_store_config(
    source: &SnapshotSourceArgs,
    connection: &StorageConnectionArgs,
) -> Result<ObjectStoreConfig> {
    if let Some(bucket) = &source.s3 {
        Ok(ObjectStoreConfig {
            object_store: Some(ObjectStoreType::S3),
            bucket: Some(bucket.clone()),
            object_store_connection_limit: connection.object_store_connection_limit,
            ..Default::default()
        })
    } else if let Some(bucket) = &source.gcs {
        Ok(ObjectStoreConfig {
            object_store: Some(ObjectStoreType::GCS),
            bucket: Some(bucket.clone()),
            object_store_connection_limit: connection.object_store_connection_limit,
            ..Default::default()
        })
    } else if let Some(container) = &source.azure {
        Ok(ObjectStoreConfig {
            object_store: Some(ObjectStoreType::Azure),
            bucket: Some(container.clone()),
            object_store_connection_limit: connection.object_store_connection_limit,
            ..Default::default()
        })
    } else if let Some(endpoint) = &source.http {
        Ok(ObjectStoreConfig {
            object_store: Some(ObjectStoreType::S3),
            aws_endpoint: Some(endpoint.to_string()),
            aws_virtual_hosted_style_request: true,
            no_sign_request: true,
            object_store_connection_limit: connection.object_store_connection_limit,
            ..Default::default()
        })
    } else if let Some(path) = &source.local_path {
        Ok(ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(path.clone()),
            object_store_connection_limit: connection.object_store_connection_limit,
            ..Default::default()
        })
    } else {
        Err(anyhow!("No snapshot source specified"))
    }
}
