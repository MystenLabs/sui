// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, ensure, Context as _};
use bytes::Bytes;
use object_store::{
    aws::AmazonS3Builder, azure::MicrosoftAzureBuilder, gcp::GoogleCloudStorageBuilder,
    local::LocalFileSystem,
};
use std::{path::PathBuf, sync::Arc};
use sui_indexer_alt_framework::{
    ingestion::remote_client::RemoteIngestionClient, types::full_checkpoint_content::CheckpointData,
};
use sui_storage::blob::Blob;
use tracing::info;
use url::Url;

use crate::{db::Watermark, restore::storage::HttpStorage};

use super::{
    format::{FileMetadata, FileType, RootManifest},
    storage::{Storage, StorageConnectionArgs},
};

#[derive(clap::Args, Clone, Debug)]
#[group(required = true)]
pub struct FormalSnapshotArgs {
    /// Fetch formal snapshot from AWS S3. Provide the bucket name or endpoint-and-bucket.
    /// (env: AWS_ENDPOINT, AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_DEFAULT_REGION)
    #[arg(long, group = "source")]
    pub s3: Option<String>,

    /// Fetch formal snapshot from Google Cloud Storage. Provide the bucket name.
    /// (env: GOOGLE_SERVICE_ACCOUNT_PATH).
    #[arg(long, group = "source")]
    pub gcs: Option<String>,

    /// Fetch formal snapshot from Azure Blob Storage. Provide the container name.
    /// (env: AZURE_STORAGE_ACCOUNT_NAME, AZURE_STORAGE_ACCESS_KEY)
    #[arg(long, group = "source")]
    pub azure: Option<String>,

    /// Fetch formal snapshot from a generic HTTP endpoint.
    #[arg(long, group = "source")]
    pub http: Option<Url>,

    /// Fetch formal snapshot from local filesystem. Provide the path to the snapshot directory.
    #[arg(long, group = "source")]
    pub path: Option<PathBuf>,

    /// URL of the remote checkpoint store. Used to fetch the mapping between epochs and
    /// checkpoints.
    #[arg(long)]
    pub remote_store_url: Url,

    /// The epoch to restore from. Restores from the latest epoch if not specified.
    #[arg(long)]
    pub epoch: Option<u64>,
}

/// Interface over a formal snapshot at a specific epoch.
#[derive(Clone)]
pub(super) struct FormalSnapshot {
    /// The source of files related to the formal snapshot.
    source: Arc<dyn Storage + Send + Sync + 'static>,

    /// The point in time this snapshot is from.
    watermark: Watermark,
}

impl FormalSnapshot {
    /// Establish a connection to a formal snapshot source, and validate that it can serve data for
    /// a formal snapshot at the specified epoch (or the latest available epoch if none is
    /// specified).
    pub(super) async fn new(
        snapshot_args: FormalSnapshotArgs,
        connection_args: StorageConnectionArgs,
    ) -> anyhow::Result<Self> {
        // Connect to the formal snapshot source.
        let store: Arc<dyn Storage + Send + Sync + 'static> = if let Some(bucket) = snapshot_args.s3
        {
            info!(bucket, "S3 storage");
            AmazonS3Builder::from_env()
                .with_client_options(connection_args.into())
                .with_imdsv1_fallback()
                .with_bucket_name(bucket)
                .build()
                .map(Arc::new)?
        } else if let Some(bucket) = snapshot_args.gcs {
            info!(bucket, "GCS storage");
            GoogleCloudStorageBuilder::from_env()
                .with_client_options(connection_args.into())
                .with_bucket_name(bucket)
                .build()
                .map(Arc::new)?
        } else if let Some(container) = snapshot_args.azure {
            info!(container, "Azure storage");
            MicrosoftAzureBuilder::from_env()
                .with_client_options(connection_args.into())
                .with_container_name(container)
                .build()
                .map(Arc::new)?
        } else if let Some(endpoint) = snapshot_args.http {
            info!(endpoint = %endpoint, "HTTP storage");
            HttpStorage::new(endpoint, connection_args).map(Arc::new)?
        } else if let Some(path) = snapshot_args.path {
            info!(path = %path.display(), "Directory storage");
            LocalFileSystem::new_with_prefix(path).map(Arc::new)?
        } else {
            bail!("No formal snapshot source provided");
        };

        // Fetch details about all the epochs it has available, and use that to confirm and
        // validate the epoch to restore from (it is available and complete at the source).
        let root_manifest = RootManifest::read(
            store
                .get("MANIFEST".into())
                .await
                .context("Failed to fetch root manifest")?
                .as_ref(),
        )?;

        // If an epoch has not been specified, restore from the latest available epoch.
        let epoch = snapshot_args
            .epoch
            .or_else(|| root_manifest.latest())
            .context("No epochs available in the snapshot store")?;

        ensure!(
            root_manifest.contains(epoch),
            "Requested epoch {epoch} is not available in the snapshot store",
        );

        let is_complete = store
            .get(format!("epoch_{epoch}/_SUCCESS").into())
            .await
            .is_ok();

        ensure!(is_complete, "Snapshot for epoch {epoch} is not complete");

        info!(epoch, "Connected to valid formal snapshot");

        // Use the remote store to associate the epoch with a watermark pointing to its last
        // transaction.
        let client = RemoteIngestionClient::new(snapshot_args.remote_store_url)
            .context("Failed to connect to remote checkpoint store")?;

        let end_of_epoch_checkpoints: Vec<_> = client
            .end_of_epoch_checkpoints()
            .await
            .context("Failed to fetch end-of-epoch checkpoints")?
            .json()
            .await
            .context("Failed to parse end-of-epoch checkpoints")?;

        let checkpoint = end_of_epoch_checkpoints
            .get(epoch as usize)
            .cloned()
            .with_context(|| format!("Cannot find end-of-epoch checkpoint for epoch {epoch}"))?;

        let CheckpointData {
            checkpoint_summary, ..
        } = Blob::from_bytes(
            &client
                .checkpoint(checkpoint)
                .await
                .context("Failed to fetch end-of-epoch checkpoint")?
                .bytes()
                .await
                .context("Failed to read end-of-epoch checkpoint bytes")?,
        )
        .context("Failed to deserialize end-of-epoch checkpoint")?;

        ensure!(
            checkpoint_summary.epoch == epoch,
            "End-of-epoch checkpoint {checkpoint} does not belong to epoch {epoch}",
        );

        let watermark = Watermark {
            epoch_hi_inclusive: epoch,
            checkpoint_hi_inclusive: checkpoint_summary.sequence_number,
            tx_hi: checkpoint_summary.network_total_transactions,
            timestamp_ms_hi_inclusive: checkpoint_summary.timestamp_ms,
        };

        info!(?watermark, "Anchored formal snapshot");

        Ok(Self {
            source: store,
            watermark,
        })
    }

    /// The watermark this snapshot was taken from.
    pub(super) fn watermark(&self) -> Watermark {
        self.watermark
    }

    /// Load the manifest for this snapshot, detailing all its files.
    pub(super) async fn manifest(&self) -> anyhow::Result<Bytes> {
        let path = format!("epoch_{}/MANIFEST", self.watermark.epoch_hi_inclusive);

        self.source
            .get(path.into())
            .await
            .context("Failed to fetch epoch manifest")
    }

    /// Load a file from this snapshot, given its metadata.
    pub(super) async fn file(&self, metadata: &FileMetadata) -> anyhow::Result<Bytes> {
        let ext = match metadata.file_type {
            FileType::Object => "obj",
            FileType::Reference => "ref",
        };

        let path = format!(
            "epoch_{}/{}_{}.{ext}",
            self.watermark.epoch_hi_inclusive, metadata.bucket, metadata.partition
        );

        self.source
            .get(path.into())
            .await
            .context("Failed to fetch object file")
    }
}
