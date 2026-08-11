// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Instant;

use anyhow::Result;
use anyhow::bail;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use tokio::sync::mpsc;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::store::BitmapInitialWatermarks;

use super::BucketId;
use super::NUM_SHARDS;
use super::shard;
use super::watermark;

/// Aggregated row-flush completions returned by the writer after BigTable
/// accepts row writes for a checkpoint generation.
pub(super) struct RowsFlushed {
    pub(super) checkpoint: u64,
    pub(super) count: u64,
}

/// Handle / shard / writer → generation task (BOUNDED).
///
/// Bounded so that a slow generation task applies backpressure to its producers,
/// which cascades up to the framework's adaptive ingestion controller.
/// Unbounded would let commit and row-flush messages pile up without limit when
/// the generation task falls behind.
pub(super) enum Event {
    /// `Handle::commit` registered a new checkpoint generation. This is sent
    /// before shard fanout so the generation task observes commits in
    /// sequential checkpoint order.
    GenerationStarted {
        watermark: CommitterWatermark,
        framework_commit_time: Instant,
    },
    /// One shard has merged its input and sent every row flush it discovered to
    /// the writer.
    ShardFlushesScheduled {
        checkpoint: u64,
        rows_scheduled: u64,
    },
    /// The writer persisted row writes for these checkpoint generations.
    /// These can race ahead of `ShardFlushesScheduled` for the same generation;
    /// promotion waits for both counters to line up.
    RowsFlushed { counts: Vec<RowsFlushed> },
}

pub(super) struct GenerationWorker {
    pub(super) pipeline: &'static str,
    pub(super) generation_rx: mpsc::Receiver<Event>,
    pub(super) shard_seal_senders: Vec<mpsc::UnboundedSender<shard::Seal>>,
    pub(super) watermark_commit_tx: mpsc::Sender<watermark::Commit>,
    pub(super) is_sealed: fn(u64, CommitterWatermark) -> bool,
    pub(super) initial_watermarks: BitmapInitialWatermarks,
}

struct GenerationState {
    watermark: CommitterWatermark,
    framework_commit_time: std::time::Instant,
    bucket_start_cp: u64,
    shards_scheduled: u16,
    rows_scheduled: u64,
    rows_flushed: u64,
}

struct GenerationLoopState {
    // Bucket containing the most recent observed watermark's `tx_hi`.
    current_bucket_id: BucketId,
    // Replay floor for `current_bucket_id`: `init_watermark` resumes from
    // just before this checkpoint so the active bucket is rebuilt after a
    // crash. This can be the checkpoint that made the bucket active, even if
    // the first transaction in that bucket appears in the next checkpoint.
    current_bucket_start_cp: u64,
    next_bucket_to_evict: BucketId,
    generations: BTreeMap<u64, GenerationState>,
}

impl GenerationWorker {
    pub(super) async fn run(mut self) -> Result<()> {
        info!(self.pipeline, "Bitmap generation task started");

        let Some(mut state) = self.initialize().await? else {
            return Ok(());
        };

        while let Some(msg) = self.generation_rx.recv().await {
            self.handle_generation_event(&mut state, msg);
            self.try_promote(&mut state).await;
        }

        info!(self.pipeline, "Bitmap generation task exiting");
        Ok(())
    }

    async fn initialize(&mut self) -> Result<Option<GenerationLoopState>> {
        // The framework runs `init_watermark` before the first `commit()`, and
        // `commit()` always sends `GenerationStarted` before any shard can send row
        // flush accounting. Treat that first generation as the init barrier.
        let Some(first_msg) = self.generation_rx.recv().await else {
            info!(
                self.pipeline,
                "Bitmap generation task exiting before first generation"
            );
            return Ok(None);
        };
        let Event::GenerationStarted {
            watermark,
            framework_commit_time,
        } = first_msg
        else {
            bail!(
                "bitmap generation task for `{}` received accounting event \
                 before first GenerationStarted",
                self.pipeline,
            );
        };

        let startup = self.initial_watermarks.get(self.pipeline)?;
        let startup_watermark = startup.watermark;
        let startup_bucket_start_cp = startup.bucket_start_cp;
        info!(
            self.pipeline,
            startup_tx_hi = startup_watermark.tx_hi,
            startup_cp_hi = startup_watermark.checkpoint_hi_inclusive,
            startup_bucket_start_cp,
            "Bitmap generation task initial watermark loaded"
        );

        let mut state =
            GenerationLoopState::new(startup_watermark, startup_bucket_start_cp, self.is_sealed);
        state.record_generation_started(watermark, framework_commit_time, self.is_sealed);
        Ok(Some(state))
    }

    fn handle_generation_event(&self, state: &mut GenerationLoopState, msg: Event) {
        match msg {
            Event::GenerationStarted {
                watermark,
                framework_commit_time,
            } => {
                state.record_generation_started(watermark, framework_commit_time, self.is_sealed);
            }
            Event::ShardFlushesScheduled {
                checkpoint,
                rows_scheduled,
            } => {
                if let Some(entry) = state.generations.get_mut(&checkpoint) {
                    entry.record_shard_flushes_scheduled(rows_scheduled);
                } else {
                    warn!(
                        self.pipeline,
                        checkpoint, "ShardFlushesScheduled for unknown checkpoint"
                    );
                }
            }
            Event::RowsFlushed { counts } => {
                for flushed in counts {
                    if let Some(entry) = state.generations.get_mut(&flushed.checkpoint) {
                        entry.record_rows_flushed(flushed.count);
                    } else {
                        warn!(
                            self.pipeline,
                            checkpoint = flushed.checkpoint,
                            "RowsFlushed for unknown checkpoint"
                        );
                    }
                }
            }
        }
    }

    /// Walks generations in checkpoint order, dispatches a watermark for the newest
    /// contiguous generation whose rows are durable, and drops every generation covered
    /// by that request.
    async fn try_promote(&self, state: &mut GenerationLoopState) {
        let mut candidate_cp: Option<u64> = None;
        for (&cp, entry) in state.generations.iter() {
            if !entry.all_flushes_complete() {
                break;
            }
            candidate_cp = Some(cp);
        }

        let Some(cp) = candidate_cp else {
            return;
        };
        let (watermark, bucket_start_cp, framework_commit_time) = {
            let entry = state
                .generations
                .get(&cp)
                .expect("candidate generation exists");
            (
                entry.watermark,
                entry.bucket_start_cp,
                entry.framework_commit_time,
            )
        };

        let previous_bucket_to_evict = state.next_bucket_to_evict;
        while (self.is_sealed)(state.next_bucket_to_evict, watermark) {
            state.next_bucket_to_evict += 1;
        }
        if state.next_bucket_to_evict > previous_bucket_to_evict {
            for tx in &self.shard_seal_senders {
                let _ = tx.send(shard::Seal {
                    bucket_id_exclusive: state.next_bucket_to_evict,
                });
            }
        }

        let req = watermark::Commit {
            watermark,
            bucket_start_cp,
            framework_commit_time,
        };
        if self.watermark_commit_tx.send(req).await.is_err() {
            warn!(self.pipeline, "Watermark writer closed; cannot promote");
            return;
        }

        while let Some((&front_cp, _)) = state.generations.first_key_value()
            && front_cp <= cp
        {
            state.generations.pop_first();
        }

        debug!(
            self.pipeline,
            cp,
            tx_hi = watermark.tx_hi,
            bucket_start_cp,
            "Watermark dispatched to writer"
        );
    }
}

impl GenerationState {
    fn new(
        watermark: CommitterWatermark,
        framework_commit_time: std::time::Instant,
        bucket_start_cp: u64,
    ) -> Self {
        Self {
            watermark,
            framework_commit_time,
            bucket_start_cp,
            shards_scheduled: 0,
            rows_scheduled: 0,
            rows_flushed: 0,
        }
    }

    fn record_shard_flushes_scheduled(&mut self, rows_scheduled: u64) {
        self.shards_scheduled += 1;
        self.rows_scheduled += rows_scheduled;
        debug_assert!(
            self.shards_scheduled <= NUM_SHARDS as u16,
            "generation scheduled more shards than exist"
        );
    }

    fn record_rows_flushed(&mut self, count: u64) {
        // Writer completions can race ahead of a shard's
        // `ShardFlushesScheduled` message for the same generation because the
        // shard sends row batches before it sends its scheduled count.
        self.rows_flushed += count;
    }

    fn all_flushes_complete(&self) -> bool {
        let all_shards_scheduled = self.shards_scheduled as usize == NUM_SHARDS;
        if all_shards_scheduled {
            debug_assert!(
                self.rows_flushed <= self.rows_scheduled,
                "generation flushed more rows than were scheduled"
            );
        }
        all_shards_scheduled && self.rows_flushed == self.rows_scheduled
    }
}

impl GenerationLoopState {
    fn new(
        startup_watermark: CommitterWatermark,
        startup_bucket_start_cp: u64,
        is_sealed: fn(u64, CommitterWatermark) -> bool,
    ) -> Self {
        let current_bucket_id = super::bucket_of(startup_watermark, is_sealed);
        Self {
            current_bucket_id,
            current_bucket_start_cp: startup_bucket_start_cp,
            next_bucket_to_evict: current_bucket_id,
            generations: BTreeMap::new(),
        }
    }

    fn record_generation_started(
        &mut self,
        watermark: CommitterWatermark,
        framework_commit_time: std::time::Instant,
        is_sealed: fn(u64, CommitterWatermark) -> bool,
    ) {
        let cp = watermark.checkpoint_hi_inclusive;
        while is_sealed(self.current_bucket_id, watermark) {
            self.current_bucket_id += 1;
            self.current_bucket_start_cp = cp;
        }
        self.generations.insert(
            cp,
            GenerationState::new(
                watermark,
                framework_commit_time,
                self.current_bucket_start_cp,
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Instant;

    use sui_indexer_alt_framework_store_traits::InitWatermark;

    use crate::store::BitmapInitialWatermark;
    use crate::store::PipelineInitResult;

    use super::*;

    const PIPELINE: &str = "test_bitmap";

    fn is_sealed(bucket_id: u64, watermark: CommitterWatermark) -> bool {
        watermark.tx_hi >= (bucket_id + 1) * 10
    }

    fn watermark(cp: u64, tx_hi: u64) -> CommitterWatermark {
        CommitterWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: cp,
            tx_hi,
            timestamp_ms_hi_inclusive: cp * 1_000,
        }
    }

    fn complete_generation(state: &mut GenerationState, rows: u64) {
        state.record_rows_flushed(rows);
        for _ in 0..NUM_SHARDS - 1 {
            state.record_shard_flushes_scheduled(0);
        }
        state.record_shard_flushes_scheduled(rows);
    }

    fn worker(
        watermark_commit_tx: mpsc::Sender<watermark::Commit>,
        shard_seal_senders: Vec<mpsc::UnboundedSender<shard::Seal>>,
    ) -> GenerationWorker {
        let (_generation_tx, generation_rx) = mpsc::channel(1);
        let mut init_results = HashMap::new();
        init_results.insert(
            PIPELINE.to_string(),
            PipelineInitResult {
                init: Some(InitWatermark {
                    checkpoint_hi_inclusive: None,
                    reader_lo: Some(0),
                }),
                bitmap: BitmapInitialWatermark {
                    watermark: CommitterWatermark::default(),
                    bucket_start_cp: 0,
                },
            },
        );

        GenerationWorker {
            pipeline: PIPELINE,
            generation_rx,
            shard_seal_senders,
            watermark_commit_tx,
            is_sealed,
            initial_watermarks: BitmapInitialWatermarks {
                init_results: Arc::new(Mutex::new(init_results)),
            },
        }
    }

    fn seal_channel() -> (
        mpsc::UnboundedSender<shard::Seal>,
        mpsc::UnboundedReceiver<shard::Seal>,
    ) {
        #[allow(clippy::disallowed_methods)]
        mpsc::unbounded_channel()
    }

    #[test]
    fn generation_waits_for_all_shards_and_rows() {
        let mut state = GenerationState::new(watermark(1, 5), Instant::now(), 0);

        state.record_rows_flushed(2);
        for _ in 0..NUM_SHARDS - 1 {
            state.record_shard_flushes_scheduled(0);
        }
        assert!(!state.all_flushes_complete());

        state.record_shard_flushes_scheduled(2);
        assert!(state.all_flushes_complete());
    }

    #[test]
    fn empty_generation_completes_after_all_shards_report_zero_rows() {
        let mut state = GenerationState::new(watermark(1, 5), Instant::now(), 0);

        for _ in 0..NUM_SHARDS {
            state.record_shard_flushes_scheduled(0);
        }

        assert!(state.all_flushes_complete());
    }

    #[test]
    fn bucket_start_cp_tracks_commit_that_crosses_bucket_boundary() {
        let mut state = GenerationLoopState::new(CommitterWatermark::default(), 0, is_sealed);

        state.record_generation_started(watermark(1, 5), Instant::now(), is_sealed);
        assert_eq!(state.current_bucket_id, 0);
        assert_eq!(state.generations.get(&1).unwrap().bucket_start_cp, 0);

        state.record_generation_started(watermark(2, 25), Instant::now(), is_sealed);

        assert_eq!(state.current_bucket_id, 2);
        assert_eq!(state.current_bucket_start_cp, 2);
        assert_eq!(state.generations.get(&2).unwrap().bucket_start_cp, 2);
    }

    #[tokio::test]
    async fn promote_dispatches_latest_contiguous_watermark_and_bucket_seal() {
        let (watermark_tx, mut watermark_rx) = mpsc::channel(1);
        let (seal_tx, mut seal_rx) = seal_channel();
        let worker = worker(watermark_tx, vec![seal_tx]);
        let mut state = GenerationLoopState::new(CommitterWatermark::default(), 0, is_sealed);

        state.record_generation_started(watermark(1, 5), Instant::now(), is_sealed);
        state.record_generation_started(watermark(2, 25), Instant::now(), is_sealed);
        state.record_generation_started(watermark(3, 35), Instant::now(), is_sealed);
        complete_generation(state.generations.get_mut(&1).unwrap(), 1);
        complete_generation(state.generations.get_mut(&2).unwrap(), 0);

        worker.try_promote(&mut state).await;

        let req = watermark_rx.try_recv().unwrap();
        assert_eq!(req.watermark.checkpoint_hi_inclusive, 2);
        assert_eq!(req.bucket_start_cp, 2);
        let seal = seal_rx.try_recv().unwrap();
        assert_eq!(seal.bucket_id_exclusive, 2);
        assert!(!state.generations.contains_key(&1));
        assert!(!state.generations.contains_key(&2));
        assert!(state.generations.contains_key(&3));
    }

    #[tokio::test]
    async fn promote_waits_for_contiguous_prefix() {
        let (watermark_tx, mut watermark_rx) = mpsc::channel(1);
        let (seal_tx, _seal_rx) = seal_channel();
        let worker = worker(watermark_tx, vec![seal_tx]);
        let mut state = GenerationLoopState::new(CommitterWatermark::default(), 0, is_sealed);

        state.record_generation_started(watermark(1, 5), Instant::now(), is_sealed);
        state.record_generation_started(watermark(2, 25), Instant::now(), is_sealed);
        complete_generation(state.generations.get_mut(&2).unwrap(), 0);

        worker.try_promote(&mut state).await;

        assert!(watermark_rx.try_recv().is_err());
        assert!(state.generations.contains_key(&1));
        assert!(state.generations.contains_key(&2));
    }
}
