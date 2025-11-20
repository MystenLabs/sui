// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use sui_futures::stream::TrySpawnStreamExt as _;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::{db::Db, store::Schema};

use super::{Break, LiveObjects, Restore, RestorerMetrics};

/// A worker that processes live objects from a single bucket and partition, for a given pipeline.
///
/// Returns `Ok(_)` if it was able to process all live objects it was given, or `Err(_)` otherwise.
pub(super) fn worker<S: Schema + Send + Sync + 'static, R: Restore<S>>(
    rx: mpsc::Receiver<Arc<LiveObjects>>,
    db: Arc<Db>,
    schema: Arc<S>,
    metrics: Arc<RestorerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<Result<(), ()>> {
    tokio::spawn(async move {
        info!(pipeline = R::NAME, "Starting worker");

        match ReceiverStream::new(rx)
            .try_for_each_spawned(<R as Restore<S>>::FANOUT, |objects| {
                let db = db.clone();
                let schema = schema.clone();
                let metrics = metrics.clone();
                let cancel = cancel.clone();

                async move {
                    info!(
                        pipeline = R::NAME,
                        bucket = objects.bucket,
                        partition = objects.partition,
                        objects = objects.objects.len(),
                        "Recv"
                    );

                    metrics
                        .total_partitions_received
                        .with_label_values(&[R::NAME])
                        .inc();

                    metrics
                        .total_objects_received
                        .with_label_values(&[R::NAME])
                        .inc_by(objects.objects.len() as u64);

                    let _guard = metrics
                        .worker_partition_latency
                        .with_label_values(&[R::NAME])
                        .start_timer();

                    let mut batch = rocksdb::WriteBatch::default();
                    for object in &objects.objects {
                        if cancel.is_cancelled() {
                            return Err(Break::Cancel);
                        }

                        R::restore(&schema, object, &mut batch).with_context(|| {
                            format!(
                                "Error restoring {}, {}, {} in bucket {}, partition {}",
                                object.id(),
                                object.version(),
                                object.digest(),
                                objects.bucket,
                                objects.partition
                            )
                        })?;
                    }

                    db.restore(objects.bucket, objects.partition, R::NAME, batch)
                        .with_context(|| {
                            format!(
                                "Failed to write batch for bucket {}, partition {}",
                                objects.bucket, objects.partition
                            )
                        })?;

                    info!(
                        pipeline = R::NAME,
                        bucket = objects.bucket,
                        partition = objects.partition,
                        "DONE",
                    );

                    metrics
                        .total_partitions_restored
                        .with_label_values(&[R::NAME])
                        .inc();

                    metrics
                        .total_objects_restored
                        .with_label_values(&[R::NAME])
                        .inc_by(objects.objects.len() as u64);

                    Ok(())
                }
            })
            .await
        {
            Ok(()) => {
                info!(pipeline = R::NAME, "Live objects done, stopping worker");
                Ok(())
            }

            Err(Break::Cancel) => {
                info!(pipeline = R::NAME, "Shutdown received, stopping worker");
                Err(())
            }

            Err(Break::Err(e)) => {
                error!(pipeline = R::NAME, "Error from worker: {e:#}");
                cancel.cancel();
                Err(())
            }
        }
    })
}
