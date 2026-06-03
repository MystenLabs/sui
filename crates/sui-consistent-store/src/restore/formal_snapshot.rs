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

use std::collections::BTreeMap;
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
use sui_indexer_alt_framework::ingestion::store_client::StoreIngestionClient;
use sui_indexer_alt_framework::types::full_checkpoint_content::Checkpoint;
use tracing::info;
use tracing::warn;
use url::Url;

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

    /// Checkpoint sequence number this snapshot is anchored at.
    target_checkpoint: u64,

    /// Dense `shard_id -> bucket_id` mapping. The number of
    /// shards is `shard_buckets.len()`.
    shard_buckets: Vec<u32>,

    /// Per-shard list of `.obj` partitions, sorted by partition
    /// index. `shard_partitions[shard_id]` is the partitions for
    /// `shard_buckets[shard_id]`.
    shard_partitions: Vec<Vec<FileMetadata>>,

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

        let target_checkpoint = anchor_checkpoint(&args.remote_store_url, epoch).await?;
        info!(
            epoch,
            target_checkpoint, "Anchored snapshot at end-of-epoch checkpoint",
        );

        let manifest_bytes = store
            .get(format!("{epoch_dir}/MANIFEST").into())
            .await
            .context("Failed to fetch epoch manifest")?;
        let manifest = EpochManifest::read(&manifest_bytes)?;

        // Group `.obj` files by bucket, dropping `.ref` entries
        // (those carry historical-version metadata, not live
        // objects).
        let mut by_bucket: BTreeMap<u32, Vec<FileMetadata>> = BTreeMap::new();
        let mut total_partitions = 0u64;
        for meta in manifest.metadata() {
            if !matches!(meta.file_type, FileType::Object) {
                continue;
            }
            by_bucket.entry(meta.bucket).or_default().push(meta.clone());
            total_partitions += 1;
        }
        for parts in by_bucket.values_mut() {
            parts.sort_by_key(|m| m.partition);
        }

        let shard_buckets: Vec<u32> = by_bucket.keys().copied().collect();
        let shard_partitions: Vec<Vec<FileMetadata>> = by_bucket.into_values().collect();

        metrics.total_partitions.set(total_partitions as i64);
        info!(
            shards = shard_buckets.len(),
            partitions = total_partitions,
            "Discovered formal-snapshot buckets",
        );

        Ok(Self {
            source: store,
            epoch_dir,
            target_checkpoint,
            shard_buckets,
            shard_partitions,
            metrics,
        })
    }

    /// Encode a partition index as a 4-byte big-endian cursor.
    fn encode_cursor(partition: u32) -> Bytes {
        Bytes::copy_from_slice(&partition.to_be_bytes())
    }

    /// Decode a cursor previously produced by
    /// [`encode_cursor`](Self::encode_cursor). Returns `None`
    /// for an empty / missing cursor and an error for malformed
    /// bytes.
    fn decode_cursor(cursor: Option<&Bytes>) -> anyhow::Result<Option<u32>> {
        let Some(cursor) = cursor else {
            return Ok(None);
        };
        ensure!(
            cursor.len() == 4,
            "formal-snapshot cursor must be 4 bytes, got {}",
            cursor.len(),
        );
        let mut buf = [0u8; 4];
        buf.copy_from_slice(cursor);
        Ok(Some(u32::from_be_bytes(buf)))
    }
}

#[async_trait]
impl RestoreSource for FormalSnapshot {
    fn target_checkpoint(&self) -> u64 {
        self.target_checkpoint
    }

    fn shards(&self) -> u32 {
        self.shard_buckets.len() as u32
    }

    fn stream(
        &self,
        shard_id: u32,
        cursor: Option<Bytes>,
    ) -> BoxStream<'_, anyhow::Result<RestoreChunk>> {
        let resume_after = match Self::decode_cursor(cursor.as_ref()) {
            Ok(c) => c,
            Err(e) => return stream::once(async move { Err(e) }).boxed(),
        };

        let idx = shard_id as usize;
        if idx >= self.shard_partitions.len() {
            let shard_id_for_msg = shard_id;
            let shards = self.shard_buckets.len();
            return stream::once(async move {
                bail!(
                    "formal-snapshot shard_id {shard_id_for_msg} out of range \
                     (have {shards} shards)",
                )
            })
            .boxed();
        }

        let bucket = self.shard_buckets[idx];
        let partitions = self.shard_partitions[idx].clone();
        let metrics = self.metrics.clone();
        let source = self.source.clone();
        let epoch_dir = self.epoch_dir.clone();

        let chunks = stream::iter(
            partitions
                .into_iter()
                .filter(move |m| resume_after.is_none_or(|after| m.partition > after)),
        )
        .then(move |meta| {
            let metrics = metrics.clone();
            let source = source.clone();
            let path = format!("{}/{}_{}.obj", epoch_dir, meta.bucket, meta.partition);
            let _ = bucket; // captured only for tracing context in callers
            async move {
                let live = fetch_objects(source.as_ref(), &path, &meta, metrics.as_ref()).await?;
                Ok(RestoreChunk {
                    objects: live.objects,
                    cursor: FormalSnapshot::encode_cursor(meta.partition),
                })
            }
        });
        chunks.boxed()
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

/// Hit the remote checkpoint store at `remote_store_url` to map
/// `epoch` to its end-of-epoch checkpoint sequence number.
async fn anchor_checkpoint(remote_store_url: &Url, epoch: u64) -> anyhow::Result<u64> {
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

    Ok(summary.sequence_number)
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
    fn cursor_roundtrip() {
        for partition in [0u32, 1, 42, 1_000_000, u32::MAX - 1, u32::MAX] {
            let encoded = FormalSnapshot::encode_cursor(partition);
            let decoded = FormalSnapshot::decode_cursor(Some(&encoded)).unwrap();
            assert_eq!(decoded, Some(partition), "round-trip for {partition}");
        }
    }

    #[test]
    fn cursor_none_decodes_to_none() {
        assert_eq!(FormalSnapshot::decode_cursor(None).unwrap(), None);
    }

    #[test]
    fn cursor_wrong_length_errors() {
        let bad = Bytes::from_static(&[0u8, 1, 2]);
        let err = FormalSnapshot::decode_cursor(Some(&bad)).unwrap_err();
        assert!(
            format!("{err:#}").contains("must be 4 bytes"),
            "expected length error, got: {err:#}",
        );
    }
}
