// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Built-in [`RestoreSource`] that streams live objects from a
//! Sui formal snapshot.
//!
//! Each formal snapshot is a per-epoch directory of `.obj` /
//! `.ref` files indexed by `(bucket, partition)` pairs. This
//! source maps each *bucket* to one driver shard so per-bucket
//! `.obj` files run as one sequential stream and the driver
//! iterates buckets in parallel.
//!
//! # Cursor encoding
//!
//! Big-endian `u32` of the last successfully committed partition
//! index within the shard's bucket. `stream(shard_id, Some(c))`
//! yields only partitions whose index is `> c`. The first chunk's
//! cursor is the partition's own index (so after committing
//! partition `0`, the persisted cursor is `0` and a re-run
//! resumes at partition `1`).
//!
//! # Where the watermark comes from
//!
//! The snapshot store records files by `epoch`; the driver needs
//! a checkpoint sequence number for
//! [`target_checkpoint`](super::RestoreSource::target_checkpoint).
//! On construction we fetch the `end-of-epoch` checkpoint via the
//! supplied `remote_store_url` to anchor the snapshot at the
//! correct sequence number.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::Context as _;
use anyhow::bail;
use anyhow::ensure;
use async_trait::async_trait;
use backoff::Error as BE;
use backoff::ExponentialBackoff;
use bytes::Bytes;
use futures::StreamExt;
use futures::stream;
use futures::stream::BoxStream;
use object_store::ClientOptions;
use object_store::aws::AmazonS3Builder;
use object_store::azure::MicrosoftAzureBuilder;
use object_store::gcp::GoogleCloudStorageBuilder;
use object_store::http::HttpBuilder;
use object_store::local::LocalFileSystem;
use prometheus::Registry;
use sui_futures::future::with_slow_future_monitor;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientTrait;
use sui_indexer_alt_framework::ingestion::store_client::StoreIngestionClient;
use sui_indexer_alt_framework::types::full_checkpoint_content::Checkpoint;
use tracing::info;
use tracing::warn;
use url::Url;

use crate::ChainId;
use crate::Watermark;
use crate::restore::RestoreChunk;
use crate::restore::RestoreSource;
use crate::restore::format::EpochManifest;
use crate::restore::format::FileMetadata;
use crate::restore::format::FileType;
use crate::restore::format::LiveObjects;
use crate::restore::format::RootManifest;
use crate::restore::metrics::FormalSnapshotMetrics;
use crate::restore::storage::HttpStorage;
use crate::restore::storage::Storage;
use crate::restore::storage::StorageConnectionArgs;

/// Wait at most this long between retries while fetching files
/// from the snapshot.
const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(60);

/// If a single fetch takes longer than this, log a warning.
const SLOW_FETCH_THRESHOLD: Duration = Duration::from_secs(600);

/// Clap-style snapshot-source selector. One of `s3`, `gcs`,
/// `azure`, `http`, or `local` is required.
#[derive(clap::Args, Clone, Debug)]
#[group(required = true)]
pub struct FormalSnapshotArgs {
    /// Fetch formal snapshot from AWS S3. Provide the bucket
    /// name. (env: AWS_ENDPOINT, AWS_ACCESS_KEY_ID,
    /// AWS_SECRET_ACCESS_KEY, AWS_DEFAULT_REGION)
    #[arg(long, group = "source")]
    pub s3: Option<String>,

    /// Fetch formal snapshot from Google Cloud Storage. Provide
    /// the bucket name. (env: GOOGLE_SERVICE_ACCOUNT_PATH)
    #[arg(long, group = "source")]
    pub gcs: Option<String>,

    /// Fetch formal snapshot from Azure Blob Storage. Provide
    /// the container name. (env: AZURE_STORAGE_ACCOUNT_NAME,
    /// AZURE_STORAGE_ACCESS_KEY)
    #[arg(long, group = "source")]
    pub azure: Option<String>,

    /// Fetch formal snapshot from a generic HTTP endpoint.
    #[arg(long, group = "source")]
    pub http: Option<Url>,

    /// Fetch formal snapshot from local filesystem. Provide the
    /// path to the snapshot root directory.
    #[arg(long, group = "source")]
    pub local: Option<PathBuf>,

    /// URL of the remote checkpoint store. Used to fetch the
    /// epoch → end-of-epoch checkpoint mapping so the snapshot
    /// can be anchored at the right checkpoint sequence number.
    #[arg(long)]
    pub remote_store_url: Url,

    /// The epoch to restore from. Restores from the latest
    /// epoch in the snapshot if not specified. Required when
    /// `--path` is set, since per-prefix layouts may not include
    /// a `MANIFEST` to discover the latest epoch.
    #[arg(long)]
    pub epoch: Option<u64>,

    /// Path prefix inside the storage backend that contains
    /// per-epoch subdirectories. When set, the source looks for
    /// `<path>/epoch_<N>/` and skips both the root `MANIFEST`
    /// and the per-epoch `_SUCCESS` marker — the operator
    /// vouches for the snapshot being complete. Requires
    /// `--epoch`.
    #[arg(long, requires = "epoch")]
    pub path: Option<String>,
}

/// Built-in [`RestoreSource`] backed by a Sui formal snapshot.
///
/// Construct via [`FormalSnapshot::new`]; the connection to the
/// underlying object store and the watermark anchoring round
/// trip happen during construction so subsequent
/// [`stream`](RestoreSource::stream) calls are pure file I/O.
pub struct FormalSnapshot {
    /// Underlying storage backend (S3/GCS/Azure/HTTP/local).
    source: Arc<dyn Storage + Send + Sync + 'static>,

    /// Path of the epoch's subdirectory within `source`.
    epoch_dir: String,

    /// Watermark this snapshot is anchored at (epoch, checkpoint,
    /// tx count, timestamp). The driver writes this into
    /// `__watermark` so tip indexing resumes from the right
    /// place once restore finishes.
    target_watermark: Watermark,

    /// Chain identifier the snapshot belongs to (digest of the
    /// chain's genesis checkpoint). Pinned into `__chain_id`
    /// on finalise so tip indexing rejects checkpoints from a
    /// different chain.
    target_chain_id: ChainId,

    /// Every `.obj` partition across all buckets, flattened and
    /// sorted by `(bucket, partition)`. Each entry is one driver
    /// shard, so `shard_id` indexes directly into this vector and
    /// the shard count equals the partition count. Exposing one
    /// shard per partition (rather than one per bucket) lets the
    /// driver fetch partitions concurrently and makes its
    /// `restore_shards_done` gauge track partition-level progress —
    /// snapshots commonly ship a single bucket, which would
    /// otherwise be one giant sequential shard.
    partitions: Vec<FileMetadata>,

    metrics: Arc<FormalSnapshotMetrics>,
}

impl FormalSnapshot {
    /// Connect to the snapshot store described by `args` /
    /// `connection_args`, fetch the epoch manifest, resolve the
    /// anchor checkpoint, and group partitions by bucket.
    pub async fn new(
        args: FormalSnapshotArgs,
        connection_args: StorageConnectionArgs,
        metrics: Arc<FormalSnapshotMetrics>,
    ) -> anyhow::Result<Self> {
        let store = connect_storage(&args, connection_args.clone())?;

        let (epoch, epoch_dir) = resolve_epoch(&args, &store).await?;
        info!(epoch, epoch_dir, "Connected to valid formal snapshot");

        let (target_watermark, target_chain_id) =
            anchor_watermark_and_chain_id(&args.remote_store_url, epoch).await?;
        info!(
            epoch,
            target_checkpoint = target_watermark.checkpoint_hi_inclusive,
            "Anchored snapshot at end-of-epoch checkpoint",
        );

        let manifest_bytes = store
            .get(format!("{epoch_dir}/MANIFEST").into())
            .await
            .context("Failed to fetch epoch manifest")?;
        let manifest = EpochManifest::read(&manifest_bytes)?;

        // Collect every `.obj` file (one partition each), dropping
        // `.ref` entries (those carry historical-version metadata,
        // not live objects). Sort by `(bucket, partition)` so the
        // shard ordering is deterministic across runs, which keeps
        // resume cursors stable.
        let mut partitions: Vec<FileMetadata> = manifest
            .metadata()
            .iter()
            .filter(|m| matches!(m.file_type, FileType::Object))
            .cloned()
            .collect();
        partitions.sort_by_key(|m| (m.bucket, m.partition));

        metrics.total_partitions.set(partitions.len() as i64);
        info!(
            shards = partitions.len(),
            partitions = partitions.len(),
            "Discovered formal-snapshot partitions",
        );

        Ok(Self {
            source: store,
            epoch_dir,
            target_watermark,
            target_chain_id,
            partitions,
            metrics,
        })
    }

    /// Encode a partition index as a 4-byte big-endian cursor. Each
    /// shard is a single partition, so the cursor is only ever
    /// present (the partition's chunk committed) or absent (not yet);
    /// the encoded value is recorded for debugging the `__restore`
    /// state rather than for resuming mid-partition.
    fn encode_cursor(partition: u32) -> Bytes {
        Bytes::copy_from_slice(&partition.to_be_bytes())
    }
}

#[async_trait]
impl RestoreSource for FormalSnapshot {
    fn target_checkpoint(&self) -> u64 {
        self.target_watermark.checkpoint_hi_inclusive
    }

    fn target_watermark(&self) -> Watermark {
        self.target_watermark
    }

    fn target_chain_id(&self) -> ChainId {
        self.target_chain_id
    }

    fn shards(&self) -> u32 {
        self.partitions.len() as u32
    }

    fn stream(
        &self,
        shard_id: u32,
        cursor: Option<Bytes>,
    ) -> BoxStream<'_, anyhow::Result<RestoreChunk>> {
        let idx = shard_id as usize;
        if idx >= self.partitions.len() {
            let shard_id_for_msg = shard_id;
            let shards = self.partitions.len();
            return stream::once(async move {
                bail!(
                    "formal-snapshot shard_id {shard_id_for_msg} out of range \
                     (have {shards} shards)",
                )
            })
            .boxed();
        }

        // A shard is a single partition fetched and committed as one
        // chunk. A non-empty cursor means that chunk was already
        // committed (the driver persists the cursor atomically with
        // the data), so there is nothing left to stream — re-yielding
        // would double-apply additive index writes such as `balance`.
        if cursor.is_some() {
            return stream::empty().boxed();
        }

        let meta = self.partitions[idx].clone();
        let metrics = self.metrics.clone();
        let source = self.source.clone();
        let epoch_dir = self.epoch_dir.clone();

        stream::once(async move {
            let path = format!("{}/{}_{}.obj", epoch_dir, meta.bucket, meta.partition);
            let live = fetch_objects(source.as_ref(), &path, &meta, metrics.as_ref()).await?;
            Ok(RestoreChunk {
                objects: live.objects,
                cursor: FormalSnapshot::encode_cursor(meta.partition),
            })
        })
        .boxed()
    }
}

/// Connect to the underlying object store described by `args`.
fn connect_storage(
    args: &FormalSnapshotArgs,
    connection_args: StorageConnectionArgs,
) -> anyhow::Result<Arc<dyn Storage + Send + Sync + 'static>> {
    if let Some(bucket) = &args.s3 {
        info!(bucket, "S3 storage");
        return Ok(Arc::new(
            AmazonS3Builder::from_env()
                .with_client_options(connection_args.into())
                .with_imdsv1_fallback()
                .with_bucket_name(bucket)
                .build()?,
        ));
    }
    if let Some(bucket) = &args.gcs {
        info!(bucket, "GCS storage");
        return Ok(Arc::new(
            GoogleCloudStorageBuilder::from_env()
                .with_client_options(connection_args.into())
                .with_bucket_name(bucket)
                .build()?,
        ));
    }
    if let Some(container) = &args.azure {
        info!(container, "Azure storage");
        return Ok(Arc::new(
            MicrosoftAzureBuilder::from_env()
                .with_client_options(connection_args.into())
                .with_container_name(container)
                .build()?,
        ));
    }
    if let Some(endpoint) = &args.http {
        info!(endpoint = %endpoint, "HTTP storage");
        return Ok(Arc::new(HttpStorage::new(
            endpoint.clone(),
            connection_args,
        )?));
    }
    if let Some(path) = &args.local {
        info!(path = %path.display(), "Local storage");
        return Ok(Arc::new(LocalFileSystem::new_with_prefix(path)?));
    }
    bail!("No formal snapshot source provided");
}

/// Resolve `(epoch, epoch_dir)` honouring the `--path` /
/// `--epoch` overrides. When neither is set we consult the root
/// `MANIFEST` for the latest epoch and check its `_SUCCESS`
/// marker.
async fn resolve_epoch(
    args: &FormalSnapshotArgs,
    store: &Arc<dyn Storage + Send + Sync + 'static>,
) -> anyhow::Result<(u64, String)> {
    if let Some(path) = &args.path {
        let epoch = args
            .epoch
            .expect("clap requires --epoch when --path is set");
        return Ok((epoch, format!("{path}/epoch_{epoch}")));
    }

    let root_manifest = RootManifest::read(
        store
            .get("MANIFEST".into())
            .await
            .context("Failed to fetch root manifest")?
            .as_ref(),
    )?;

    let epoch = args
        .epoch
        .or_else(|| root_manifest.latest())
        .context("No epochs available in the snapshot store")?;

    ensure!(
        root_manifest.contains(epoch),
        "Requested epoch {epoch} is not available in the snapshot store",
    );

    let epoch_dir = format!("epoch_{epoch}");
    let is_complete = store
        .get(format!("{epoch_dir}/_SUCCESS").into())
        .await
        .is_ok();
    ensure!(is_complete, "Snapshot for epoch {epoch} is not complete");

    Ok((epoch, epoch_dir))
}

/// Hit the remote checkpoint store at `remote_store_url` to
/// resolve `epoch` to its end-of-epoch checkpoint and return a
/// fully populated [`Watermark`] alongside the chain id
/// (digest of checkpoint 0).
async fn anchor_watermark_and_chain_id(
    remote_store_url: &Url,
    epoch: u64,
) -> anyhow::Result<(Watermark, ChainId)> {
    let client = StoreIngestionClient::new(
        HttpBuilder::new()
            .with_url(remote_store_url.to_string())
            .with_client_options(ClientOptions::new().with_allow_http(true))
            .build()
            .map(Arc::new)
            .context("Failed to connect to remote checkpoint store")?,
        None,
    );

    let end_of_epoch_checkpoints: Vec<u64> = client
        .end_of_epoch_checkpoints()
        .await
        .context("Failed to fetch end-of-epoch checkpoints")?;

    let checkpoint = end_of_epoch_checkpoints
        .get(epoch as usize)
        .copied()
        .with_context(|| format!("Cannot find end-of-epoch checkpoint for epoch {epoch}"))?;

    let Checkpoint { summary, .. } = client
        .checkpoint(checkpoint)
        .await
        .context("Failed to fetch end-of-epoch checkpoint")?;

    ensure!(
        summary.epoch == epoch,
        "End-of-epoch checkpoint {checkpoint} does not belong to epoch {epoch}",
    );

    let chain_identifier = client
        .chain_id()
        .await
        .context("Failed to fetch chain id from remote checkpoint store")?;

    let watermark = Watermark {
        epoch_hi_inclusive: summary.epoch,
        checkpoint_hi_inclusive: summary.sequence_number,
        tx_hi: summary.network_total_transactions,
        timestamp_ms_hi_inclusive: summary.timestamp_ms,
    };
    Ok((watermark, ChainId(*chain_identifier.as_bytes())))
}

/// Fetch one `.obj` file with exponential backoff, decode it,
/// and tick the relevant metrics. Retries forever (matching the
/// previous `Restorer`'s behaviour) — the caller is responsible
/// for setting upstream timeouts if it needs to bound total
/// wait time.
async fn fetch_objects(
    source: &(dyn Storage + Send + Sync + 'static),
    path: &str,
    meta: &FileMetadata,
    metrics: &FormalSnapshotMetrics,
) -> anyhow::Result<LiveObjects> {
    let _guard = metrics.objects_fetch_latency.start_timer();

    let attempts = AtomicUsize::new(1);
    let request = || async {
        let attempt = attempts.fetch_add(1, Ordering::Relaxed);

        let future = async {
            let bytes = source
                .get(path.to_string().into())
                .await
                .with_context(|| format!("Failed to fetch {path}"))?;
            let bytes_len = bytes.len();
            let live = LiveObjects::read(bytes.as_ref(), meta)
                .with_context(|| format!("Failed to decode {path}"))?;
            Ok::<_, anyhow::Error>((bytes_len, live))
        };

        match with_slow_future_monitor(future, SLOW_FETCH_THRESHOLD, || {
            warn!(
                attempt,
                bucket = meta.bucket,
                partition = meta.partition,
                "Fetch slow",
            );
        })
        .await
        {
            Ok((bytes, live)) => {
                metrics.total_bytes_fetched.inc_by(bytes as u64);
                metrics.total_partitions_fetched.inc();
                Ok(live)
            }
            Err(e) => {
                warn!(
                    attempt,
                    bucket = meta.bucket,
                    partition = meta.partition,
                    "Fetch error: {e:#}",
                );
                metrics.total_objects_fetch_retries.inc();
                Err(BE::transient(e))
            }
        }
    };

    let backoff = ExponentialBackoff {
        max_interval: MAX_RETRY_INTERVAL,
        max_elapsed_time: None,
        ..Default::default()
    };

    backoff::future::retry(backoff, request).await
}

/// Convenience for callers that don't want to register their
/// own metrics: produce a [`FormalSnapshotMetrics`] against the
/// given prometheus registry.
pub fn metrics(prefix: Option<&str>, registry: &Registry) -> Arc<FormalSnapshotMetrics> {
    FormalSnapshotMetrics::new(prefix, registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_encodes_partition_as_4_byte_be() {
        for partition in [0u32, 1, 42, 1_000_000, u32::MAX - 1, u32::MAX] {
            let encoded = FormalSnapshot::encode_cursor(partition);
            assert_eq!(encoded.len(), 4, "cursor for {partition} must be 4 bytes");
            assert_eq!(
                encoded.as_ref(),
                partition.to_be_bytes(),
                "cursor for {partition} must be big-endian",
            );
        }
    }
}
