// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::panic;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use backoff::ExponentialBackoff;
use backoff::backoff::Backoff;
use sui_concurrency_limiter::stream::{AdaptiveStreamExt, Break};
use sui_concurrency_limiter::{Limiter, Outcome};
use sui_futures::service::Service;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_stream::wrappers::ReceiverStream;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

use crate::metrics::CheckpointLagMetricReporter;
use crate::metrics::IndexerMetrics;
use crate::pipeline::WatermarkPart;
use crate::pipeline::concurrent::BatchedRows;
use crate::pipeline::concurrent::Handler;
use crate::store::Store;

/// If the committer needs to retry a commit, it will wait this long initially.
const INITIAL_RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// If the committer needs to retry a commit, it will wait at most this long between retries.
const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(1);

/// The committer task is responsible for writing batches of rows to the database. It receives
/// batches on `rx` and writes them out to the `db` concurrently (the `concurrency` config
/// controls the degree of fan-out).
///
/// The writing of each batch will be repeatedly retried on an exponential back-off until it
/// succeeds. Once the write succeeds, the [WatermarkPart]s for that batch are sent on `tx` to the
/// watermark task.
///
/// This task will shutdown if its receiver or sender channels are closed.
pub(super) fn committer<H: Handler + 'static>(
    handler: Arc<H>,
    rx: mpsc::Receiver<BatchedRows<H>>,
    tx: mpsc::Sender<Vec<WatermarkPart>>,
    db: H::Store,
    metrics: Arc<IndexerMetrics>,
    limiter: Limiter,
    processor_peak_fill: Arc<AtomicUsize>,
    collector_peak_fill: Arc<AtomicUsize>,
    processor_capacity: usize,
    collector_capacity: usize,
) -> Service {
    Service::new().spawn_aborting(async move {
        info!(pipeline = H::NAME, "Starting committer");

        if H::CAPACITY_BATCHING {
            return rebatching_committer::<H>(
                handler,
                rx,
                tx,
                db,
                metrics,
                limiter,
                processor_peak_fill,
                collector_peak_fill,
                processor_capacity,
                collector_capacity,
            )
            .await;
        }

        let checkpoint_lag_reporter = CheckpointLagMetricReporter::new_for_pipeline::<H>(
            &metrics.partially_committed_checkpoint_timestamp_lag,
            &metrics.latest_partially_committed_checkpoint_timestamp_lag_ms,
            &metrics.latest_partially_committed_checkpoint,
        );

        let watermark_peak_fill = Arc::new(AtomicUsize::new(0));
        let stream_fut = ReceiverStream::new(rx).try_for_each_spawned_adaptive_with_retry_weighted(
            limiter.clone(),
            ExponentialBackoff {
                initial_interval: INITIAL_RETRY_INTERVAL,
                max_interval: MAX_RETRY_INTERVAL,
                max_elapsed_time: None,
                ..ExponentialBackoff::default()
            },
            |batched: &BatchedRows<H>| H::batch_weight(&batched.batch, batched.batch_len),
            // f: measured work (DB commit, retried on error)
            |BatchedRows {
                 batch,
                 batch_len,
                 watermark,
             }| {
                let batch: Arc<H::Batch> = Arc::new(batch);
                let handler = handler.clone();
                let db = db.clone();
                let metrics = metrics.clone();
                let checkpoint_lag_reporter = checkpoint_lag_reporter.clone();

                let highest_checkpoint = watermark.iter().map(|w| w.checkpoint()).max();
                let highest_checkpoint_timestamp = watermark.iter().map(|w| w.timestamp_ms()).max();

                move || {
                    let batch = batch.clone();
                    let handler = handler.clone();
                    let db = db.clone();
                    let metrics = metrics.clone();
                    let checkpoint_lag_reporter = checkpoint_lag_reporter.clone();
                    let watermark = watermark.clone();

                    async move {
                        if batch_len == 0 {
                            return Ok(watermark);
                        }

                        metrics
                            .total_committer_batches_attempted
                            .with_label_values(&[H::NAME])
                            .inc();

                        let guard = metrics
                            .committer_commit_latency
                            .with_label_values(&[H::NAME])
                            .start_timer();

                        let mut conn = match db.connect().await {
                            Ok(conn) => conn,
                            Err(e) => {
                                warn!(
                                    pipeline = H::NAME,
                                    "Committed failed to get connection for DB"
                                );

                                metrics
                                    .total_committer_batches_failed
                                    .with_label_values(&[H::NAME])
                                    .inc();

                                guard.stop_and_record();
                                return Err(Break::Err(e));
                            }
                        };

                        let affected = handler.commit(&batch, &mut conn).await;
                        let elapsed = guard.stop_and_record();

                        match affected {
                            Ok(affected) => {
                                debug!(
                                    pipeline = H::NAME,
                                    elapsed_ms = elapsed * 1000.0,
                                    affected,
                                    committed = batch_len,
                                    "Wrote batch",
                                );

                                checkpoint_lag_reporter.report_lag(
                                    highest_checkpoint.unwrap(),
                                    highest_checkpoint_timestamp.unwrap(),
                                );

                                metrics
                                    .total_committer_batches_succeeded
                                    .with_label_values(&[H::NAME])
                                    .inc();

                                metrics
                                    .total_committer_rows_committed
                                    .with_label_values(&[H::NAME])
                                    .inc_by(batch_len as u64);

                                metrics
                                    .total_committer_rows_affected
                                    .with_label_values(&[H::NAME])
                                    .inc_by(affected as u64);

                                metrics
                                    .committer_tx_rows
                                    .with_label_values(&[H::NAME])
                                    .observe(affected as f64);

                                Ok(watermark)
                            }

                            Err(e) => {
                                warn!(
                                    pipeline = H::NAME,
                                    elapsed_ms = elapsed * 1000.0,
                                    committed = batch_len,
                                    "Error writing batch: {e}",
                                );

                                metrics
                                    .total_committer_batches_failed
                                    .with_label_values(&[H::NAME])
                                    .inc();

                                Err(Break::Err(e))
                            }
                        }
                    }
                }
            },
            // g: unmeasured work (send watermark + channel metrics)
            {
                let tx = tx.clone();
                let watermark_peak_fill = watermark_peak_fill.clone();
                move |watermark: Vec<WatermarkPart>| {
                    let tx = tx.clone();
                    let watermark_peak_fill = watermark_peak_fill.clone();
                    async move {
                        if tx.send(watermark).await.is_err() {
                            info!(pipeline = H::NAME, "Watermark closed channel");
                            return Err(Break::Break);
                        }

                        let fill = tx.max_capacity() - tx.capacity();
                        watermark_peak_fill.fetch_max(fill, Ordering::Relaxed);

                        Ok(())
                    }
                }
            },
        );

        let metrics_for_timer = metrics.clone();
        match tokio::select! {
            result = stream_fut => result,
            _ = async {
                let mut interval = tokio::time::interval(Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    metrics_for_timer
                        .committer_write_concurrency
                        .with_label_values(&[H::NAME])
                        .set(limiter.current() as i64);
                    metrics_for_timer
                        .committer_write_peak_concurrency
                        .with_label_values(&[H::NAME])
                        .set(limiter.take_peak_limit() as i64);
                    metrics_for_timer
                        .committer_write_peak_inflight
                        .with_label_values(&[H::NAME])
                        .set(limiter.take_peak_inflight() as i64);
                    let processor_peak = processor_peak_fill.swap(0, Ordering::Relaxed);
                    metrics_for_timer
                        .processor_peak_channel_fill
                        .with_label_values(&[H::NAME])
                        .set(processor_peak as i64);
                    if processor_capacity > 0 {
                        metrics_for_timer
                            .processor_peak_channel_utilization
                            .with_label_values(&[H::NAME])
                            .set(processor_peak as f64 / processor_capacity as f64);
                    }

                    let collector_peak = collector_peak_fill.swap(0, Ordering::Relaxed);
                    metrics_for_timer
                        .collector_peak_channel_fill
                        .with_label_values(&[H::NAME])
                        .set(collector_peak as i64);
                    if collector_capacity > 0 {
                        metrics_for_timer
                            .collector_peak_channel_utilization
                            .with_label_values(&[H::NAME])
                            .set(collector_peak as f64 / collector_capacity as f64);
                    }

                    let watermark_peak = watermark_peak_fill.swap(0, Ordering::Relaxed);
                    metrics_for_timer
                        .committer_watermark_peak_channel_fill
                        .with_label_values(&[H::NAME])
                        .set(watermark_peak as i64);
                    let watermark_capacity = tx.max_capacity();
                    if watermark_capacity > 0 {
                        metrics_for_timer
                            .committer_watermark_peak_channel_utilization
                            .with_label_values(&[H::NAME])
                            .set(watermark_peak as f64 / watermark_capacity as f64);
                    }
                }
            } => unreachable!(),
        } {
            Ok(()) => {
                info!(pipeline = H::NAME, "Batches done, stopping committer");
                Ok(())
            }

            Err(Break::Break) => {
                info!(pipeline = H::NAME, "Channels closed, stopping committer");
                Ok(())
            }

            Err(Break::Err(e)) => {
                error!(pipeline = H::NAME, "Error from committer: {e}");
                Err(e.context(format!("Error from committer {}", H::NAME)))
            }
        }
    })
}

/// RAII guard that decrements `active` by `weight` on drop, ensuring the inflight
/// mutation count is released even when a spawned task is cancelled.
struct WeightGuard(Arc<AtomicUsize>, usize);

impl Drop for WeightGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(self.1, Ordering::Relaxed);
    }
}

/// Capacity-based rebatching committer. Buffers incoming collector batches and drains
/// entries based on available limiter capacity, merging them into optimally-sized
/// commit batches. The limiter tracks inflight mutations (not batches), giving the
/// algorithm fine-grained visibility into backend load.
async fn rebatching_committer<H: Handler + 'static>(
    handler: Arc<H>,
    mut rx: mpsc::Receiver<BatchedRows<H>>,
    tx: mpsc::Sender<Vec<WatermarkPart>>,
    db: H::Store,
    metrics: Arc<IndexerMetrics>,
    limiter: Limiter,
    processor_peak_fill: Arc<AtomicUsize>,
    collector_peak_fill: Arc<AtomicUsize>,
    processor_capacity: usize,
    collector_capacity: usize,
) -> anyhow::Result<()> {
    let checkpoint_lag_reporter = CheckpointLagMetricReporter::new_for_pipeline::<H>(
        &metrics.partially_committed_checkpoint_timestamp_lag,
        &metrics.latest_partially_committed_checkpoint_timestamp_lag_ms,
        &metrics.latest_partially_committed_checkpoint,
    );

    let active = Arc::new(AtomicUsize::new(0));
    let mut buffer: VecDeque<BatchedRows<H>> = VecDeque::new();
    let mut buffer_weight: usize = 0;
    let mut join_set: JoinSet<Result<(), Break<anyhow::Error>>> = JoinSet::new();
    let mut draining = false;
    let mut error: Option<Break<anyhow::Error>> = None;

    let watermark_peak_fill = Arc::new(AtomicUsize::new(0));
    let mut metrics_interval = tokio::time::interval(Duration::from_secs(30));

    loop {
        // Eagerly dispatch merged batches while capacity and data are available.
        while error.is_none() && buffer_weight > 0 {
            let current_limit = limiter.current();
            let inflight = active.load(Ordering::Relaxed);
            if inflight >= current_limit {
                break;
            }
            let available = current_limit - inflight;
            let cap = available.min(H::MAX_BATCH_WEIGHT);

            let mut dest_batch = H::Batch::default();
            let mut dest_watermarks: Vec<WatermarkPart> = Vec::new();
            let mut dest_weight: usize = 0;
            let mut dest_count: usize = 0;

            while dest_weight < cap && !buffer.is_empty() {
                let front = buffer.front_mut().unwrap();
                let remaining = cap - dest_weight;
                let (weight, count) = H::drain_batch(&mut front.batch, &mut dest_batch, remaining);
                if weight == 0 {
                    break;
                }

                front.batch_len -= count;
                let wms = front.take_watermarks(count);
                dest_watermarks.extend(wms);
                dest_weight += weight;
                dest_count += count;
                buffer_weight -= weight;

                if front.is_empty() {
                    // Take any remaining watermarks from the fully-drained batch.
                    dest_watermarks.append(&mut buffer.pop_front().unwrap().watermark);
                }
            }

            if dest_weight == 0 {
                break;
            }

            active.fetch_add(dest_weight, Ordering::Relaxed);
            let guard = WeightGuard(active.clone(), dest_weight);
            let handler = handler.clone();
            let db = db.clone();
            let limiter = limiter.clone();
            let tx = tx.clone();
            let metrics = metrics.clone();
            let checkpoint_lag_reporter = checkpoint_lag_reporter.clone();
            let watermark_peak_fill = watermark_peak_fill.clone();

            let highest_checkpoint = dest_watermarks.iter().map(|w| w.checkpoint()).max();
            let highest_checkpoint_timestamp =
                dest_watermarks.iter().map(|w| w.timestamp_ms()).max();
            let batch_len = dest_count;

            join_set.spawn(async move {
                let _guard = guard;
                let batch = Arc::new(dest_batch);
                let mut backoff = ExponentialBackoff {
                    initial_interval: INITIAL_RETRY_INTERVAL,
                    max_interval: MAX_RETRY_INTERVAL,
                    max_elapsed_time: None,
                    ..ExponentialBackoff::default()
                };

                loop {
                    let token = limiter.acquire_weighted(dest_weight);

                    metrics
                        .total_committer_batches_attempted
                        .with_label_values(&[H::NAME])
                        .inc();

                    let guard = metrics
                        .committer_commit_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    let mut conn = match db.connect().await {
                        Ok(conn) => conn,
                        Err(e) => {
                            warn!(
                                pipeline = H::NAME,
                                "Committer failed to get connection for DB"
                            );
                            metrics
                                .total_committer_batches_failed
                                .with_label_values(&[H::NAME])
                                .inc();
                            guard.stop_and_record();
                            token.record_sample(Outcome::Dropped);
                            match backoff.next_backoff() {
                                Some(d) => {
                                    tokio::time::sleep(d).await;
                                    continue;
                                }
                                None => return Err(Break::Err(e)),
                            }
                        }
                    };

                    let affected = handler.commit(&batch, &mut conn).await;
                    let elapsed = guard.stop_and_record();

                    match affected {
                        Ok(affected) => {
                            token.record_sample(Outcome::Success);

                            debug!(
                                pipeline = H::NAME,
                                elapsed_ms = elapsed * 1000.0,
                                affected,
                                committed = batch_len,
                                mutations = dest_weight,
                                "Wrote batch",
                            );

                            if let Some(cp) = highest_checkpoint {
                                checkpoint_lag_reporter
                                    .report_lag(cp, highest_checkpoint_timestamp.unwrap());
                            }

                            metrics
                                .total_committer_batches_succeeded
                                .with_label_values(&[H::NAME])
                                .inc();
                            metrics
                                .total_committer_rows_committed
                                .with_label_values(&[H::NAME])
                                .inc_by(batch_len as u64);
                            metrics
                                .total_committer_rows_affected
                                .with_label_values(&[H::NAME])
                                .inc_by(affected as u64);
                            metrics
                                .committer_tx_rows
                                .with_label_values(&[H::NAME])
                                .observe(affected as f64);

                            if tx.send(dest_watermarks).await.is_err() {
                                info!(pipeline = H::NAME, "Watermark closed channel");
                                return Err(Break::Break);
                            }

                            let fill = tx.max_capacity() - tx.capacity();
                            watermark_peak_fill.fetch_max(fill, Ordering::Relaxed);

                            return Ok(());
                        }

                        Err(e) => {
                            warn!(
                                pipeline = H::NAME,
                                elapsed_ms = elapsed * 1000.0,
                                committed = batch_len,
                                mutations = dest_weight,
                                "Error writing batch: {e}",
                            );
                            metrics
                                .total_committer_batches_failed
                                .with_label_values(&[H::NAME])
                                .inc();
                            token.record_sample(Outcome::Dropped);
                            match backoff.next_backoff() {
                                Some(d) => {
                                    tokio::time::sleep(d).await;
                                    continue;
                                }
                                None => return Err(Break::Err(e)),
                            }
                        }
                    }
                }
            });
        }

        tokio::select! {
            biased;

            Some(res) = join_set.join_next() => {
                match res {
                    Ok(Err(e)) if error.is_none() => {
                        error = Some(e);
                        draining = true;
                    }
                    Ok(_) => {}
                    Err(e) if e.is_panic() => {
                        panic::resume_unwind(e.into_panic())
                    }
                    Err(e) => {
                        assert!(e.is_cancelled());
                        draining = true;
                    }
                }
            }

            next = rx.recv(), if !draining => {
                match next {
                    Some(batched) => {
                        let w = H::batch_weight(&batched.batch, batched.batch_len);
                        if w == 0 {
                            // Empty batch: forward watermarks immediately without
                            // going through the limiter or buffer.
                            if !batched.watermark.is_empty() {
                                if tx.send(batched.watermark).await.is_err() {
                                    info!(pipeline = H::NAME, "Watermark closed channel");
                                    draining = true;
                                }
                            }
                        } else {
                            buffer_weight += w;
                            buffer.push_back(batched);
                        }
                    }
                    None => {
                        draining = true;
                    }
                }
            }

            _ = metrics_interval.tick() => {
                metrics
                    .committer_write_concurrency
                    .with_label_values(&[H::NAME])
                    .set(limiter.current() as i64);
                metrics
                    .committer_write_peak_concurrency
                    .with_label_values(&[H::NAME])
                    .set(limiter.take_peak_limit() as i64);
                metrics
                    .committer_write_peak_inflight
                    .with_label_values(&[H::NAME])
                    .set(limiter.take_peak_inflight() as i64);

                let processor_peak = processor_peak_fill.swap(0, Ordering::Relaxed);
                metrics
                    .processor_peak_channel_fill
                    .with_label_values(&[H::NAME])
                    .set(processor_peak as i64);
                if processor_capacity > 0 {
                    metrics
                        .processor_peak_channel_utilization
                        .with_label_values(&[H::NAME])
                        .set(processor_peak as f64 / processor_capacity as f64);
                }

                let collector_peak = collector_peak_fill.swap(0, Ordering::Relaxed);
                metrics
                    .collector_peak_channel_fill
                    .with_label_values(&[H::NAME])
                    .set(collector_peak as i64);
                if collector_capacity > 0 {
                    metrics
                        .collector_peak_channel_utilization
                        .with_label_values(&[H::NAME])
                        .set(collector_peak as f64 / collector_capacity as f64);
                }

                let watermark_peak = watermark_peak_fill.swap(0, Ordering::Relaxed);
                metrics
                    .committer_watermark_peak_channel_fill
                    .with_label_values(&[H::NAME])
                    .set(watermark_peak as i64);
                let watermark_capacity = tx.max_capacity();
                if watermark_capacity > 0 {
                    metrics
                        .committer_watermark_peak_channel_utilization
                        .with_label_values(&[H::NAME])
                        .set(watermark_peak as f64 / watermark_capacity as f64);
                }
            }

            else => {
                if active.load(Ordering::Relaxed) == 0 && draining && buffer.is_empty() {
                    break;
                }
            }
        }
    }

    match error {
        Some(Break::Break) => {
            info!(pipeline = H::NAME, "Channels closed, stopping committer");
            Ok(())
        }
        Some(Break::Err(e)) => {
            error!(pipeline = H::NAME, "Error from committer: {e}");
            Err(e.context(format!("Error from committer {}", H::NAME)))
        }
        None => {
            info!(pipeline = H::NAME, "Batches done, stopping committer");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use anyhow::ensure;
    use async_trait::async_trait;
    use sui_types::full_checkpoint_content::Checkpoint;
    use tokio::sync::mpsc;

    use crate::FieldCount;
    use crate::metrics::IndexerMetrics;
    use crate::mocks::store::*;
    use crate::pipeline::Processor;
    use crate::pipeline::WatermarkPart;
    use crate::pipeline::concurrent::BatchStatus;
    use crate::pipeline::concurrent::BatchedRows;
    use crate::pipeline::concurrent::Handler;
    use crate::store::CommitterWatermark;

    use super::*;
    use sui_concurrency_limiter::Limiter as ConcurrencyLimiter;

    #[derive(Clone, FieldCount, Default)]
    pub struct StoredData {
        pub cp_sequence_number: u64,
        pub tx_sequence_numbers: Vec<u64>,
        /// Tracks remaining commit failures for testing retry logic.
        /// The committer spawns concurrent tasks that call H::commit,
        /// so this needs to be thread-safe (hence Arc<AtomicUsize>).
        pub commit_failure_remaining: Arc<AtomicUsize>,
        pub commit_delay_ms: u64,
    }

    pub struct DataPipeline;

    #[async_trait]
    impl Processor for DataPipeline {
        const NAME: &'static str = "data";

        type Value = StoredData;

        async fn process(&self, _checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl Handler for DataPipeline {
        type Store = MockStore;
        type Batch = Vec<Self::Value>;

        fn batch(
            &self,
            batch: &mut Self::Batch,
            values: &mut std::vec::IntoIter<Self::Value>,
        ) -> BatchStatus {
            batch.extend(values);
            BatchStatus::Pending
        }

        async fn commit<'a>(
            &self,
            batch: &Self::Batch,
            conn: &mut MockConnection<'a>,
        ) -> anyhow::Result<usize> {
            for value in batch {
                // If there's a delay, sleep for that duration
                if value.commit_delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(value.commit_delay_ms)).await;
                }

                // If there are remaining failures, fail the commit and decrement the counter
                {
                    let remaining = value
                        .commit_failure_remaining
                        .fetch_sub(1, Ordering::Relaxed);
                    ensure!(
                        remaining == 0,
                        "Commit failed, remaining failures: {}",
                        remaining - 1
                    );
                }

                conn.0
                    .commit_data(
                        DataPipeline::NAME,
                        value.cp_sequence_number,
                        value.tx_sequence_numbers.clone(),
                    )
                    .await?;
            }
            Ok(batch.len())
        }
    }

    struct TestSetup {
        store: MockStore,
        batch_tx: mpsc::Sender<BatchedRows<DataPipeline>>,
        watermark_rx: mpsc::Receiver<Vec<WatermarkPart>>,
        committer: Service,
    }

    /// Creates and spawns a committer task with the provided mock store, along with
    /// all necessary channels and configuration. The committer runs in the background
    /// and can be interacted with through the returned channels.
    ///
    /// # Arguments
    /// * `store` - The mock store to use for testing
    async fn setup_test(store: MockStore) -> TestSetup {
        let metrics = IndexerMetrics::new(None, &Default::default());

        let (batch_tx, batch_rx) = mpsc::channel::<BatchedRows<DataPipeline>>(10);
        let (watermark_tx, watermark_rx) = mpsc::channel(10);

        let store_clone = store.clone();
        let handler = Arc::new(DataPipeline);
        let committer = committer(
            handler,
            batch_rx,
            watermark_tx,
            store_clone,
            metrics,
            ConcurrencyLimiter::fixed(5),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
            0,
            0,
        );

        TestSetup {
            store,
            batch_tx,
            watermark_rx,
            committer,
        }
    }

    #[tokio::test]
    async fn test_concurrent_batch_processing() {
        let mut setup = setup_test(MockStore::default()).await;

        // Send batches
        let batch1 = BatchedRows::from_vec(
            vec![
                StoredData {
                    cp_sequence_number: 1,
                    tx_sequence_numbers: vec![1, 2, 3],
                    ..Default::default()
                },
                StoredData {
                    cp_sequence_number: 2,
                    tx_sequence_numbers: vec![4, 5, 6],
                    ..Default::default()
                },
            ],
            vec![
                WatermarkPart {
                    watermark: CommitterWatermark {
                        epoch_hi_inclusive: 0,
                        checkpoint_hi_inclusive: 1,
                        tx_hi: 3,
                        timestamp_ms_hi_inclusive: 1000,
                    },
                    batch_rows: 1,
                    total_rows: 1, // Total rows from checkpoint 1
                },
                WatermarkPart {
                    watermark: CommitterWatermark {
                        epoch_hi_inclusive: 0,
                        checkpoint_hi_inclusive: 2,
                        tx_hi: 6,
                        timestamp_ms_hi_inclusive: 2000,
                    },
                    batch_rows: 1,
                    total_rows: 1, // Total rows from checkpoint 2
                },
            ],
        );

        let batch2 = BatchedRows::from_vec(
            vec![StoredData {
                cp_sequence_number: 3,
                tx_sequence_numbers: vec![7, 8, 9],
                ..Default::default()
            }],
            vec![WatermarkPart {
                watermark: CommitterWatermark {
                    epoch_hi_inclusive: 0,
                    checkpoint_hi_inclusive: 3,
                    tx_hi: 9,
                    timestamp_ms_hi_inclusive: 3000,
                },
                batch_rows: 1,
                total_rows: 1, // Total rows from checkpoint 3
            }],
        );

        setup.batch_tx.send(batch1).await.unwrap();
        setup.batch_tx.send(batch2).await.unwrap();

        // Verify watermarks. Blocking until committer has processed.
        let watermark1 = setup.watermark_rx.recv().await.unwrap();
        let watermark2 = setup.watermark_rx.recv().await.unwrap();
        assert_eq!(watermark1.len(), 2);
        assert_eq!(watermark2.len(), 1);

        // Verify data was committed
        {
            let data = setup.store.data.get(DataPipeline::NAME).unwrap();
            assert_eq!(data.len(), 3);
            assert_eq!(data.get(&1).unwrap().value(), &vec![1, 2, 3]);
            assert_eq!(data.get(&2).unwrap().value(), &vec![4, 5, 6]);
            assert_eq!(data.get(&3).unwrap().value(), &vec![7, 8, 9]);
        }
    }

    #[tokio::test]
    async fn test_commit_with_retries_for_commit_failure() {
        let mut setup = setup_test(MockStore::default()).await;

        // Create a batch with a single item that will fail once before succeeding
        let batch = BatchedRows::from_vec(
            vec![StoredData {
                cp_sequence_number: 1,
                tx_sequence_numbers: vec![1, 2, 3],
                commit_failure_remaining: Arc::new(AtomicUsize::new(1)),
                commit_delay_ms: 1_000, // Long commit delay for testing state between retry
            }],
            vec![WatermarkPart {
                watermark: CommitterWatermark {
                    epoch_hi_inclusive: 0,
                    checkpoint_hi_inclusive: 1,
                    tx_hi: 3,
                    timestamp_ms_hi_inclusive: 1000,
                },
                batch_rows: 1,
                total_rows: 1,
            }],
        );

        // Send the batch
        setup.batch_tx.send(batch).await.unwrap();

        // Wait for the first attempt to fail and before the retry succeeds
        tokio::time::sleep(Duration::from_millis(1_500)).await;

        // Verify state before retry succeeds
        {
            let data = setup.store.data.get(DataPipeline::NAME);
            assert!(
                data.is_none(),
                "Data should not be committed before retry succeeds"
            );
        }
        assert!(
            setup.watermark_rx.try_recv().is_err(),
            "No watermark should be received before retry succeeds"
        );

        // Wait for the retry to succeed
        tokio::time::sleep(Duration::from_millis(1_500)).await;

        // Verify state after retry succeeds
        {
            let data = setup.store.data.get(DataPipeline::NAME).unwrap();

            assert_eq!(data.get(&1).unwrap().value(), &vec![1, 2, 3]);
        }
        let watermark = setup.watermark_rx.recv().await.unwrap();
        assert_eq!(watermark.len(), 1);
    }

    #[tokio::test]
    async fn test_commit_with_retries_for_connection_failure() {
        // Create a batch with a single item
        let store = MockStore {
            connection_failure: Arc::new(Mutex::new(ConnectionFailure {
                connection_failure_attempts: 1,
                connection_delay_ms: 1_000, // Long connection delay for testing state between retry
                ..Default::default()
            })),
            ..Default::default()
        };
        let mut setup = setup_test(store).await;

        let batch = BatchedRows::from_vec(
            vec![StoredData {
                cp_sequence_number: 1,
                tx_sequence_numbers: vec![1, 2, 3],
                ..Default::default()
            }],
            vec![WatermarkPart {
                watermark: CommitterWatermark {
                    epoch_hi_inclusive: 0,
                    checkpoint_hi_inclusive: 1,
                    tx_hi: 3,
                    timestamp_ms_hi_inclusive: 1000,
                },
                batch_rows: 1,
                total_rows: 1,
            }],
        );

        // Send the batch
        setup.batch_tx.send(batch).await.unwrap();

        // Wait for the first attempt to fail and before the retry succeeds
        tokio::time::sleep(Duration::from_millis(1_500)).await;

        // Verify state before retry succeeds
        {
            let data = setup.store.data.get(DataPipeline::NAME);
            assert!(
                data.is_none(),
                "Data should not be committed before retry succeeds"
            );
        }
        assert!(
            setup.watermark_rx.try_recv().is_err(),
            "No watermark should be received before retry succeeds"
        );

        // Wait for the retry to succeed
        tokio::time::sleep(Duration::from_millis(1_500)).await;

        // Verify state after retry succeeds
        {
            let data = setup.store.data.get(DataPipeline::NAME).unwrap();
            assert_eq!(data.get(&1).unwrap().value(), &vec![1, 2, 3]);
        }
        let watermark = setup.watermark_rx.recv().await.unwrap();
        assert_eq!(watermark.len(), 1);
    }

    #[tokio::test]
    async fn test_empty_batch_handling() {
        let mut setup = setup_test(MockStore::default()).await;

        let empty_batch = BatchedRows::from_vec(
            vec![], // Empty batch
            vec![WatermarkPart {
                watermark: CommitterWatermark {
                    epoch_hi_inclusive: 0,
                    checkpoint_hi_inclusive: 1,
                    tx_hi: 0,
                    timestamp_ms_hi_inclusive: 1000,
                },
                batch_rows: 0,
                total_rows: 0,
            }],
        );

        // Send the empty batch
        setup.batch_tx.send(empty_batch).await.unwrap();

        // Verify watermark is still sent (empty batches should still produce watermarks)
        let watermark = setup.watermark_rx.recv().await.unwrap();
        assert_eq!(watermark.len(), 1);
        assert_eq!(watermark[0].batch_rows, 0);
        assert_eq!(watermark[0].total_rows, 0);

        // Verify no data was committed (since batch was empty)
        {
            let data = setup.store.data.get(DataPipeline::NAME);
            assert!(
                data.is_none(),
                "No data should be committed for empty batch"
            );
        }
    }

    #[tokio::test]
    async fn test_watermark_channel_closed() {
        let setup = setup_test(MockStore::default()).await;

        let batch = BatchedRows::from_vec(
            vec![StoredData {
                cp_sequence_number: 1,
                tx_sequence_numbers: vec![1, 2, 3],
                ..Default::default()
            }],
            vec![WatermarkPart {
                watermark: CommitterWatermark {
                    epoch_hi_inclusive: 0,
                    checkpoint_hi_inclusive: 1,
                    tx_hi: 3,
                    timestamp_ms_hi_inclusive: 1000,
                },
                batch_rows: 1,
                total_rows: 1,
            }],
        );

        // Send the batch
        setup.batch_tx.send(batch).await.unwrap();

        // Wait for processing.
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Close the watermark channel by dropping the receiver
        drop(setup.watermark_rx);

        // Wait a bit more for the committer to handle the channel closure
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify data was still committed despite watermark channel closure
        {
            let data = setup.store.data.get(DataPipeline::NAME).unwrap();
            assert_eq!(data.get(&1).unwrap().value(), &vec![1, 2, 3]);
        }

        // Close the batch channel to allow the committer to terminate
        drop(setup.batch_tx);

        // Verify the committer task has terminated due to watermark channel closure
        // The task should exit gracefully when it can't send watermarks (returns Break::Cancel)
        setup.committer.shutdown().await.unwrap();
    }
}
