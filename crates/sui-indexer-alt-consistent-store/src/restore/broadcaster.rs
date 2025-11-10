// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::Context as _;
use backoff::{Error as BE, ExponentialBackoff};
use futures::{future::try_join_all, stream};
use sui_indexer_alt_framework::task::{TrySpawnStreamExt, with_slow_future_monitor};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::db::Db;

use super::{
    Break, FormalSnapshot, LiveObjects, RestorerMetrics,
    format::{EpochManifest, FileMetadata, FileType},
};

/// Wait at most this long between retries while fetching files from the snapshot.
const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(60);

/// If a fetch takes longer than this, log a warning.
const SLOW_FETCH_THRESHOLD: Duration = Duration::from_secs(600);

/// The broadcaster task is responsible for consulting the formal snapshot's metadata file,
/// fetching object files from the formal snapshot and disseminating them to all subscribers in
/// `subscribers`.
///
/// The task will shut down if the `cancel` token is signalled, or if all object files have been
/// restored across all subscribers. Returns `Ok(_)` if all object files were successfully fetched
/// and passed to subscribers, or `Err(_)` otherwise.
pub(super) fn broadcaster(
    object_file_concurrency: usize,
    subscribers: BTreeMap<String, mpsc::Sender<Arc<LiveObjects>>>,
    db: Arc<Db>,
    snapshot: FormalSnapshot,
    metrics: Arc<RestorerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<Result<(), ()>> {
    tokio::spawn(async move {
        info!("Starting broadcaster");

        let manifest = match snapshot
            .manifest()
            .await
            .and_then(|bs| EpochManifest::read(&bs))
        {
            Ok(manifest) => manifest,
            Err(e) => {
                error!("Failed to read snapshot manifest: {e:#}");
                cancel.cancel();
                return Err(());
            }
        };

        let metadata: Vec<_> = manifest
            .metadata()
            .iter()
            .filter(|m| matches!(m.file_type, FileType::Object))
            .cloned()
            .collect();

        metrics.total_partitions.set(metadata.len() as i64);
        info!(partitions = metadata.len(), "Restoring partitions");

        match stream::iter(metadata)
            .try_for_each_spawned(object_file_concurrency, |metadata| {
                let subscribers = subscribers.clone();
                let db = db.clone();
                let snapshot = snapshot.clone();
                let metrics = metrics.clone();

                let supervisor_cancel = cancel.clone();
                let cancel = cancel.clone();

                async move {
                    let restored = db
                        .is_restored(
                            metadata.bucket,
                            metadata.partition,
                            subscribers.keys().map(|p| p.as_str()),
                        )
                        .context("Failed to check restored markers")?;

                    // If all the pipelines have restored this partition, it can be skipped.
                    if restored.iter().all(|r| *r) {
                        info!(
                            bucket = metadata.bucket,
                            partition = metadata.partition,
                            "Skipping",
                        );
                        metrics.total_partitions_skipped.inc();
                        return Ok(());
                    } else {
                        info!(
                            bucket = metadata.bucket,
                            partition = metadata.partition,
                            "Fetching",
                        );
                    }

                    // Download the object file.
                    let objects = tokio::select! {
                        objects = fetch_objects(snapshot, metadata, metrics.as_ref()) => Arc::new(objects?),
                        _ = cancel.cancelled() => {
                            return Err(Break::Cancel);
                        }
                    };

                    // Send it to all subscribers who are not restored yet.
                    let futures = subscribers
                        .iter()
                        .zip(restored)
                        .filter(|(_, restored)| !*restored)
                        .map(|((_, s), _)| s.send(objects.clone()));

                    if try_join_all(futures).await.is_err() {
                        info!("Subscription dropped, signalling shutdown");
                        supervisor_cancel.cancel();
                        Err(Break::Cancel)
                    } else {
                        metrics.total_partitions_broadcast.inc();
                        Ok(())
                    }
                }
            })
            .await
        {
            Ok(()) => {
                info!("Live objects done, stopping broadcaster");
                Ok(())
            }

            Err(Break::Cancel) => {
                info!("Shutdown received, stopping broadcaster");
                Err(())
            }

            Err(Break::Err(e)) => {
                error!("Error from broadcaster: {e:#}");
                Err(())
            }
        }
    })
}

/// Fetch the file described by `metadata` from `snapshot` as a live objects file.
///
/// This function will repeatedly retry the fetch, with exponential backoff, until it succeeds. It
/// also monitors for individual fetches that seem slower than expected, logging a warning if one
/// is found.
async fn fetch_objects(
    snapshot: FormalSnapshot,
    metadata: FileMetadata,
    metrics: &RestorerMetrics,
) -> anyhow::Result<LiveObjects> {
    let _guard = metrics.objects_fetch_latency.start_timer();

    let attempts = AtomicUsize::new(1);
    let request = || async {
        let attempt = attempts.fetch_add(1, Ordering::Relaxed);

        let future = async {
            snapshot
                .file(&metadata)
                .await
                .and_then(|bs| Ok((bs.len(), LiveObjects::read(&bs, &metadata)?)))
        };

        match with_slow_future_monitor(future, SLOW_FETCH_THRESHOLD, || {
            warn!(
                attempt,
                bucket = metadata.bucket,
                partition = metadata.partition,
                "Fetch slow"
            );
        })
        .await
        {
            Ok((bytes, objects)) => {
                metrics.total_bytes_fetched.inc_by(bytes as u64);
                metrics.total_partitions_fetched.inc();
                Ok(objects)
            }

            Err(e) => {
                warn!(
                    attempt,
                    bucket = metadata.bucket,
                    partition = metadata.partition,
                    "Fetch error: {e:#}"
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
