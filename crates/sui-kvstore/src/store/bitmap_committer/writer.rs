// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use backoff::ExponentialBackoff;
use backoff::backoff::Backoff as _;
use bytes::Bytes;
use futures::StreamExt;
use rustc_hash::FxHashSet;
use sui_futures::stream::TrySpawnStreamExt;
use tokio::sync::mpsc;
use tracing::info;
use tracing::warn;

use crate::bigtable::client::BigTableClient;
use crate::bigtable::client::PartialWriteError;
use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::rate_limiter::CompositeRateLimiter;
use crate::tables;

use super::BitmapIndexMetrics;
use super::COMMIT_RETRY_BACKOFF;
use super::generation;

const MAX_ROW_WRITE_RETRY_BACKOFF: Duration = Duration::from_secs(1);

/// A single row write destined for BigTable, serialized on a shard task and
/// shipped to the writer.
pub(super) struct Row {
    pub(super) row_key: Bytes,
    pub(super) serialized: Bytes,
    pub(super) max_ts_ms: u64,
    /// Checkpoint generation that scheduled this row write. The writer
    /// reports durable rows by this value, so generation accounting is a simple
    /// counter.
    pub(super) generation_cp: u64,
}

/// Bounded unit sent from shards to the writer. Shards serialize rows
/// into batches so backpressure starts before a single shard can enqueue an
/// unbounded number of individual row writes.
pub(super) struct Batch {
    pub(super) rows: Vec<Row>,
}

pub(super) struct Writer {
    pub(super) pipeline: &'static str,
    pub(super) table: &'static str,
    pub(super) column: &'static str,
    pub(super) client: BigTableClient,
    pub(super) rate_limiter: Arc<CompositeRateLimiter>,
    pub(super) write_rx: mpsc::Receiver<Batch>,
    pub(super) generation_tx: mpsc::Sender<generation::Event>,
    pub(super) write_chunk_size: usize,
    pub(super) write_concurrency: usize,
    pub(super) rows_written: Arc<AtomicU64>,
    pub(super) metrics: Arc<BitmapIndexMetrics>,
}

struct Attempt {
    flushed: Vec<generation::RowsFlushed>,
    failed: Vec<Row>,
}

struct Chunk {
    rows: Vec<Row>,
}

#[derive(Clone)]
struct WriteContext {
    pipeline: &'static str,
    table: &'static str,
    column: &'static str,
    client: BigTableClient,
    rate_limiter: Arc<CompositeRateLimiter>,
    generation_tx: mpsc::Sender<generation::Event>,
    rows_written: Arc<AtomicU64>,
    metrics: Arc<BitmapIndexMetrics>,
}

impl Writer {
    pub(super) async fn run(self) {
        let Self {
            pipeline,
            table,
            column,
            client,
            rate_limiter,
            write_rx,
            generation_tx,
            write_chunk_size,
            write_concurrency,
            rows_written,
            metrics,
        } = self;

        info!(pipeline, "Bitmap row writer started");

        let write_chunk_size = write_chunk_size.max(1);
        let write_concurrency = write_concurrency.max(1);
        let context = WriteContext {
            pipeline,
            table,
            column,
            client,
            rate_limiter,
            generation_tx,
            rows_written,
            metrics,
        };

        let chunks = futures::stream::unfold(write_rx, |mut write_rx| async move {
            let batch = write_rx.recv().await?;
            Some((batch, write_rx))
        })
        .flat_map(|batch| futures::stream::iter(batch.rows))
        .ready_chunks(write_chunk_size);

        let result = chunks
            .try_for_each_spawned(write_concurrency, move |rows| {
                let context = context.clone();
                async move { context.write_chunk_retrying(Chunk::new(rows)).await }
            })
            .await;

        if result.is_err() {
            warn!(
                pipeline,
                "Bitmap row writer stopping after downstream channel closed"
            );
        }

        info!(pipeline, "Bitmap row writer exiting");
    }
}

impl WriteContext {
    async fn write_chunk_retrying(self, mut chunk: Chunk) -> Result<(), ()> {
        let mut backoff = row_write_backoff();

        loop {
            let result = self.attempt_write_chunk(chunk).await;

            let rows_written: u64 = result.flushed.iter().map(|r| r.count).sum();
            if rows_written != 0 {
                self.rows_written.fetch_add(rows_written, Ordering::Relaxed);
            }

            if !result.flushed.is_empty()
                && self
                    .generation_tx
                    .send(generation::Event::RowsFlushed {
                        counts: result.flushed,
                    })
                    .await
                    .is_err()
            {
                warn!(
                    self.pipeline,
                    "Generation task closed while reporting durable bitmap rows"
                );
                return Err(());
            }

            if result.failed.is_empty() {
                return Ok(());
            }

            let failed = result.failed.len();
            self.metrics.retry_rows.inc_by(failed as u64);
            chunk = Chunk {
                rows: result.failed,
            };
            let delay = backoff
                .next_backoff()
                .unwrap_or(MAX_ROW_WRITE_RETRY_BACKOFF);
            warn!(
                failed,
                ?delay,
                "Bitmap row write failed; retrying after backoff"
            );
            tokio::time::sleep(delay).await;
        }
    }

    async fn attempt_write_chunk(&self, chunk: Chunk) -> Attempt {
        self.rate_limiter.acquire(chunk.rows.len()).await;
        let entries = make_entries(&chunk.rows, self.column);

        let write_start = Instant::now();
        let mut client = self.client.clone();
        let result = client.write_entries(self.table, entries).await;
        self.metrics
            .write_chunk_latency
            .observe(write_start.elapsed().as_secs_f64());

        split_write_result(result, chunk)
    }
}

impl Chunk {
    fn new(rows: Vec<Row>) -> Self {
        Self { rows }
    }
}

fn row_write_backoff() -> ExponentialBackoff {
    ExponentialBackoff {
        initial_interval: COMMIT_RETRY_BACKOFF,
        current_interval: COMMIT_RETRY_BACKOFF,
        max_interval: MAX_ROW_WRITE_RETRY_BACKOFF,
        max_elapsed_time: None,
        ..Default::default()
    }
}

fn split_write_result(result: anyhow::Result<()>, chunk: Chunk) -> Attempt {
    match result {
        Ok(()) => Attempt {
            flushed: rows_flushed_by_generation(chunk.rows),
            failed: Vec::new(),
        },
        Err(e) => {
            if let Some(partial) = e.downcast_ref::<PartialWriteError>() {
                let failed_keys: FxHashSet<&Bytes> =
                    partial.failed_keys.iter().map(|f| &f.key).collect();
                let Chunk { rows } = chunk;
                let mut flushed_by_checkpoint = BTreeMap::<u64, u64>::new();
                let mut failed = Vec::new();
                for row in rows {
                    if failed_keys.contains(&row.row_key) {
                        failed.push(row);
                    } else {
                        *flushed_by_checkpoint.entry(row.generation_cp).or_default() += 1;
                    }
                }
                Attempt {
                    flushed: flushed_by_checkpoint
                        .into_iter()
                        .map(|(checkpoint, count)| generation::RowsFlushed { checkpoint, count })
                        .collect(),
                    failed,
                }
            } else {
                Attempt {
                    flushed: Vec::new(),
                    failed: chunk.rows,
                }
            }
        }
    }
}

fn rows_flushed_by_generation(rows: Vec<Row>) -> Vec<generation::RowsFlushed> {
    let mut flushed_by_checkpoint = BTreeMap::<u64, u64>::new();
    for row in rows {
        *flushed_by_checkpoint.entry(row.generation_cp).or_default() += 1;
    }
    flushed_by_checkpoint
        .into_iter()
        .map(|(checkpoint, count)| generation::RowsFlushed { checkpoint, count })
        .collect()
}

fn make_entries(rows: &[Row], column: &'static str) -> Vec<Entry> {
    rows.iter()
        .map(|r| {
            tables::make_entry(
                r.row_key.clone(),
                [(column, r.serialized.clone())],
                Some(r.max_ts_ms),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use bytes::Bytes;

    use crate::bigtable::client::MutationError;
    use crate::bigtable::proto::bigtable::v2::mutation;

    use super::*;

    fn row(key: &'static [u8], generation_cp: u64) -> Row {
        Row {
            row_key: Bytes::from_static(key),
            serialized: Bytes::from_static(b"bitmap"),
            max_ts_ms: generation_cp * 1_000,
            generation_cp,
        }
    }

    #[test]
    fn rows_flushed_are_grouped_by_generation() {
        let flushed = rows_flushed_by_generation(vec![row(b"a", 2), row(b"b", 1), row(b"c", 2)]);

        let counts: Vec<_> = flushed
            .into_iter()
            .map(|r| (r.checkpoint, r.count))
            .collect();
        assert_eq!(counts, vec![(1, 1), (2, 2)]);
    }

    #[test]
    fn partial_write_retries_only_failed_rows_and_counts_successes() {
        let chunk = Chunk::new(vec![row(b"a", 1), row(b"b", 1), row(b"c", 2)]);
        let result: anyhow::Result<()> = Err(PartialWriteError {
            failed_keys: vec![MutationError {
                key: Bytes::from_static(b"b"),
                code: 8,
                message: "injected".to_string(),
            }],
        }
        .into());

        let attempt = split_write_result(result, chunk);

        assert_eq!(attempt.failed.len(), 1);
        assert_eq!(attempt.failed[0].row_key, Bytes::from_static(b"b"));
        let flushed: Vec<_> = attempt
            .flushed
            .into_iter()
            .map(|r| (r.checkpoint, r.count))
            .collect();
        assert_eq!(flushed, vec![(1, 1), (2, 1)]);
    }

    #[test]
    fn non_partial_write_error_retries_entire_chunk() {
        let chunk = Chunk::new(vec![row(b"a", 1), row(b"b", 2)]);

        let attempt = split_write_result(Err(anyhow!("rpc unavailable")), chunk);

        assert!(attempt.flushed.is_empty());
        assert_eq!(attempt.failed.len(), 2);
    }

    #[test]
    fn row_timestamp_becomes_bigtable_cell_version() {
        let rows = vec![Row {
            row_key: Bytes::from_static(b"a"),
            serialized: Bytes::from_static(b"bitmap"),
            max_ts_ms: 123,
            generation_cp: 1,
        }];

        let entries = make_entries(&rows, "bitmap");

        let cell = match &entries[0].mutations[0].mutation {
            Some(mutation::Mutation::SetCell(cell)) => cell,
            other => panic!("expected SetCell mutation, got {other:?}"),
        };
        assert_eq!(cell.timestamp_micros, 123_000);
        assert_eq!(cell.value, Bytes::from_static(b"bitmap"));
    }
}
