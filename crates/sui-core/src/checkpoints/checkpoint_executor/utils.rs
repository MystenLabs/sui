// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Instant};

use crate::checkpoints::CheckpointStore;
use crate::execution_cache::TransactionCacheRead;
use futures::{future::Either, Stream};
use mysten_common::fatal;
use std::time::Duration;
use strum::VariantNames;
use sui_types::{
    base_types::{TransactionDigest, TransactionEffectsDigest},
    message_envelope::Message,
    messages_checkpoint::{CheckpointSequenceNumber, CheckpointSummary, VerifiedCheckpoint},
};
use tokio::sync::watch;
use tracing::{debug, error, info, instrument, warn};

use super::metrics::CheckpointExecutorMetrics;

#[instrument(level = "debug", skip_all)]
pub(super) fn stream_synced_checkpoints(
    checkpoint_store: Arc<CheckpointStore>,
    start_seq: CheckpointSequenceNumber,
    stop_seq: Option<CheckpointSequenceNumber>,
) -> impl Stream<Item = VerifiedCheckpoint> + 'static {
    let scheduling_timeout_config = get_scheduling_timeout();
    let panic_timeout = scheduling_timeout_config.panic_timeout;
    let warning_timeout = scheduling_timeout_config.warning_timeout;

    struct State {
        current_seq: CheckpointSequenceNumber,
        checkpoint_store: Arc<CheckpointStore>,
        warning_timeout: Duration,
        panic_timeout: Option<Duration>,
        stop_seq: Option<CheckpointSequenceNumber>,
    }

    let state = State {
        current_seq: start_seq,
        checkpoint_store,
        warning_timeout,
        panic_timeout,
        stop_seq,
    };

    futures::stream::unfold(Some(state), |state| async move {
        match state {
            None => None,
            Some(state) if state.current_seq > state.stop_seq.unwrap_or(u64::MAX) => None,
            Some(mut state) => {
                let seq = state.current_seq;
                let checkpoint = wait_for_checkpoint(
                    &state.checkpoint_store,
                    seq,
                    state.warning_timeout,
                    state.panic_timeout,
                )
                .await;
                info!(
                    "received synced checkpoint: {:?}",
                    checkpoint.sequence_number
                );
                if checkpoint.end_of_epoch_data.is_some() {
                    Some((checkpoint, None))
                } else {
                    state.current_seq = seq + 1;
                    Some((checkpoint, Some(state)))
                }
            }
        }
    })
}

async fn wait_for_checkpoint(
    checkpoint_store: &CheckpointStore,
    seq: CheckpointSequenceNumber,
    warning_timeout: Duration,
    panic_timeout: Option<Duration>,
) -> VerifiedCheckpoint {
    debug!("waiting for checkpoint: {:?}", seq);
    loop {
        tokio::select! {
            checkpoint = checkpoint_store.notify_read_synced_checkpoint(seq) => {
                return checkpoint;
            }

            _ = tokio::time::sleep(warning_timeout) => {
                warn!(
                    "Received no new synced checkpoints for {warning_timeout:?}. Next checkpoint to be scheduled: {seq}",
                );
            }

            _ = panic_timeout
                        .map(|d| Either::Left(tokio::time::sleep(d)))
                        .unwrap_or_else(|| Either::Right(futures::future::pending())) => {
                fatal!("No new synced checkpoints received for {panic_timeout:?}");
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CheckpointTimeoutConfig {
    pub panic_timeout: Option<Duration>,
    pub warning_timeout: Duration,
}

// We use a thread local so that the config can be overridden on a per-test basis. This means
// that get_scheduling_timeout() can be called multiple times in a multithreaded context, but
// the function is still very cheap to call so this is okay.
thread_local! {
    static SCHEDULING_TIMEOUT: once_cell::sync::OnceCell<CheckpointTimeoutConfig> =
        const { once_cell::sync::OnceCell::new() };
}

#[cfg(msim)]
pub fn init_checkpoint_timeout_config(config: CheckpointTimeoutConfig) {
    SCHEDULING_TIMEOUT.with(|s| {
        s.set(config).expect("SchedulingTimeoutConfig already set");
    });
}

fn get_scheduling_timeout() -> CheckpointTimeoutConfig {
    fn inner() -> CheckpointTimeoutConfig {
        let panic_timeout: Option<Duration> = if cfg!(msim) {
            Some(Duration::from_secs(45))
        } else {
            std::env::var("NEW_CHECKPOINT_PANIC_TIMEOUT_MS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_millis)
        };

        let warning_timeout: Duration = std::env::var("NEW_CHECKPOINT_WARNING_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_secs(5));

        CheckpointTimeoutConfig {
            panic_timeout,
            warning_timeout,
        }
    }

    SCHEDULING_TIMEOUT.with(|s| *s.get_or_init(inner))
}

pub(super) fn assert_not_forked(
    checkpoint: &VerifiedCheckpoint,
    tx_digest: &TransactionDigest,
    expected_digest: &TransactionEffectsDigest,
    actual_effects_digest: &TransactionEffectsDigest,
    cache_reader: &dyn TransactionCacheRead,
) {
    if *expected_digest != *actual_effects_digest {
        let actual_effects = cache_reader
            .get_executed_effects(tx_digest)
            .expect("actual effects should exist");

        // log observed effects (too big for panic message) and then panic.
        error!(
            ?checkpoint,
            ?tx_digest,
            ?expected_digest,
            ?actual_effects,
            "fork detected!"
        );
        panic!(
            "When executing checkpoint {}, transaction {} \
            is expected to have effects digest {}, but got {}!",
            checkpoint.sequence_number(),
            tx_digest,
            expected_digest,
            actual_effects_digest,
        );
    }
}

pub(super) fn assert_checkpoint_not_forked(
    locally_built_checkpoint: &CheckpointSummary,
    verified_checkpoint: &VerifiedCheckpoint,
    checkpoint_store: &CheckpointStore,
) {
    assert_eq!(
        locally_built_checkpoint.sequence_number(),
        verified_checkpoint.sequence_number(),
        "Checkpoint sequence numbers must match"
    );

    if locally_built_checkpoint.digest() == *verified_checkpoint.digest() {
        return;
    }

    let verified_checkpoint_summary = verified_checkpoint.data();

    if locally_built_checkpoint.content_digest == verified_checkpoint_summary.content_digest {
        // fork is in the checkpoint header
        fatal!("Checkpoint fork detected in header! Locally built checkpoint: {:?}, verified checkpoint: {:?}",
            locally_built_checkpoint,
            verified_checkpoint
        );
    } else {
        let local_contents = checkpoint_store
            .get_checkpoint_contents(&locally_built_checkpoint.content_digest)
            .expect("db error")
            .expect("contents must exist if checkpoint was built locally!");

        let verified_contents = checkpoint_store
            .get_checkpoint_contents(&verified_checkpoint_summary.content_digest)
            .expect("db error")
            .expect("contents must exist if checkpoint has been synced!");

        // fork is in the checkpoint contents
        let mut local_contents_iter = local_contents.iter();
        let mut verified_contents_iter = verified_contents.iter();
        let mut pos = 0;

        loop {
            let local_digests = local_contents_iter.next();
            let verified_digests = verified_contents_iter.next();

            match (local_digests, verified_digests) {
                (Some(local_digests), Some(verified_digests)) => {
                    if local_digests != verified_digests {
                        fatal!("Checkpoint contents diverge at position {pos}! {local_digests:?} != {verified_digests:?}");
                    }
                }
                (None, Some(_)) | (Some(_), None) => {
                    fatal!("Checkpoint contents have different lengths! Locally built checkpoint: {:?}, verified checkpoint: {:?}",
                        locally_built_checkpoint,
                        verified_checkpoint
                    );
                }
                (None, None) => {
                    break;
                }
            }
            pos += 1;
        }

        fatal!("Checkpoint fork detected in contents! Locally built checkpoint: {:?}, verified checkpoint: {:?}",
            locally_built_checkpoint,
            verified_checkpoint
        );
    }
}

/// SequenceWatch is just a wrapper around a tokio::watch that can wait for a
/// specific sequence number, instead of waiting for any change at all.
struct SequenceWatch {
    watch: watch::Sender<CheckpointSequenceNumber>,
}

impl SequenceWatch {
    fn new(starting_seq: CheckpointSequenceNumber) -> Self {
        Self {
            watch: watch::channel(starting_seq).0,
        }
    }

    async fn wait_for(&self, seq: CheckpointSequenceNumber) {
        let mut ready_seq = self.watch.subscribe();
        while *ready_seq.borrow_and_update() < seq {
            ready_seq.changed().await.expect("sender cannot be dropped");
        }
    }

    fn signal(&self, new_ready: CheckpointSequenceNumber) {
        self.watch.send_modify(|prev| {
            assert_eq!(*prev + 1, new_ready);
            *prev = new_ready;
        });
    }
}

// Pipeline system for coordinating concurrent, pipelined execution without having to break code
// down into a one-stage-per-function style.
//
// Pipeline concurrency can be achieved when you have the following sort of execution dependency.
//
//      0   A → B → C → D
//          ↓   ↓   ↓   ↓
//      1   A → B → C → D
//          ↓   ↓   ↓   ↓
//      2   A → B → C → D
//          ↓   ↓   ↓   ↓
//      3   A → B → C → D
//
// In this case, we have the following requirements:
// - Stage A for seq i must complete before Stage A of seq i+1 begins.
// - Stage A for seq i must complete before Stage B of seq i begins.
// - (and likewise for B, C, D)
// - But, crucially, there is no dependency from (i, D) to (i+1, A)! In
//   other words, we can being sequence i+1 before i is completely finished.
//
// For CheckpointExecutor, we can see that this sort of pipelining can work. For instance,
// we require that the transaction outputs are committed in order by checkpoint sequence number.
// But there is no requirement that we commit outputs for seq i before we begin executing transactions
// for seq i+1.
//
// The code here allows you to write pipelined executions without breaking up every stage into
// its own function. Instead you can write monolithic functions like:
//
//       async fn execute_seq() {
//           pipeline.begin().await;
//           // do first stage
//           pipeline.finish_stage(FirstStage).await;
//           // do second stage
//           pipeline.finish_stage(SecondStage).await;
//           ...
//       }
//
// Then, simply run many instances of this function concurrently, and the pipeline ordering
// will be maintained.
//
// Currently this code is not generic with respect to the names of the stages. We can make the
// stage enum a generic param when we need to use this code in other places.

/// Names of the pipeline stages for CheckpointExecutor.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    strum::EnumIter,
    strum_macros::VariantNames,
    strum_macros::FromRepr,
    strum_macros::EnumCount,
)]

/// Names of the pipeline stages for CheckpointExecutor.
pub(crate) enum PipelineStage {
    ExecuteTransactions = 0,
    WaitForTransactions = 1,
    FinalizeTransactions = 2,
    ProcessCheckpointData = 3,
    BuildDbBatch = 4,
    CommitTransactionOutputs = 5,
    FinalizeCheckpoint = 6,
    UpdateRpcIndex = 7,
    BumpHighestExecutedCheckpoint = 8,
    End = 9,
}

impl PipelineStage {
    pub const fn first() -> Self {
        Self::ExecuteTransactions
    }

    fn next(self) -> Self {
        assert!(self < Self::End);
        Self::from_repr((self as usize) + 1).unwrap()
    }

    fn as_str(self) -> &'static str {
        Self::VARIANTS[self as usize]
    }
}

/// PipelineHandle is used to coordinate one iteration (sequence number) of the pipeline,
/// starting from the first stage and ending at the last stage.
pub(super) struct PipelineHandle {
    seq: CheckpointSequenceNumber,
    cur_stage: PipelineStage,
    stages: Arc<PipelineStages>,
    timer: Instant,
}

impl PipelineHandle {
    fn new(stages: Arc<PipelineStages>, seq: CheckpointSequenceNumber) -> Self {
        Self {
            seq,
            cur_stage: PipelineStage::first(),
            stages,
            timer: Instant::now(),
        }
    }

    /// Begin at the first stage.
    async fn begin(&mut self) {
        assert_eq!(self.cur_stage, PipelineStage::first(), "cannot begin twice");
        self.stages.begin(self.cur_stage, self.seq).await;
        self.timer = Instant::now();
    }

    /// Finish a given stage and begin the next one.
    pub async fn finish_stage(&mut self, finished: PipelineStage) {
        let duration = self.timer.elapsed();
        self.stages
            .metrics
            .stage_active_duration_ns
            .with_label_values(&[self.cur_stage.as_str()])
            .inc_by(duration.as_nanos() as u64);
        assert_eq!(finished, self.cur_stage, "cannot skip stages");

        self.stages.end(self.cur_stage, self.seq);

        self.cur_stage = self.cur_stage.next();
        if self.cur_stage != PipelineStage::End {
            self.stages.begin(self.cur_stage, self.seq).await;
        }
    }

    /// Skip to a given stage.
    pub async fn skip_to(&mut self, stage: PipelineStage) {
        assert!(self.cur_stage < stage);
        while self.cur_stage < stage {
            self.finish_stage(self.cur_stage).await;
        }
    }
}

/// Catch errors in which the PipelineHandle is dropped early.
/// Cannot use this check in the simulator because it drops tasks to simulate killing nodes.
#[cfg(not(msim))]
impl Drop for PipelineHandle {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert_eq!(
                self.cur_stage,
                PipelineStage::End,
                "PipelineHandle dropped without reaching end of pipeline"
            );
        }
    }
}

/// A collection of watches for each stage. These are the synchronization points
/// for the pipeline.
pub(super) struct PipelineStages {
    stages: [SequenceWatch; PipelineStage::End as usize],
    metrics: Arc<CheckpointExecutorMetrics>,
}

impl PipelineStages {
    pub fn new(
        starting_seq: CheckpointSequenceNumber,
        metrics: Arc<CheckpointExecutorMetrics>,
    ) -> Arc<Self> {
        Arc::new(Self {
            stages: std::array::from_fn(|_| SequenceWatch::new(starting_seq)),
            metrics,
        })
    }

    /// Create a new PipelineHandle for the given sequence number.
    pub async fn handle(self: &Arc<Self>, seq: CheckpointSequenceNumber) -> PipelineHandle {
        let mut handle = PipelineHandle::new(self.clone(), seq);
        handle.begin().await;
        handle
    }

    /// Wait until (stage, seq - 1) has been completed.
    async fn begin(&self, stage: PipelineStage, seq: CheckpointSequenceNumber) {
        debug!(?stage, ?seq, "begin stage");
        let start = Instant::now();
        self.stages[stage as usize].wait_for(seq).await;
        let duration = start.elapsed();
        self.metrics
            .stage_wait_duration_ns
            .with_label_values(&[stage.as_str()])
            .inc_by(duration.as_nanos() as u64);
    }

    /// Signal that (stage, seq) has been completed.
    fn end(&self, stage: PipelineStage, seq: CheckpointSequenceNumber) {
        debug!(?stage, ?seq, "end stage");
        self.stages[stage as usize].signal(seq + 1);
    }
}

#[derive(Default)]
pub(super) struct TPSEstimator {
    last_update: Option<Instant>,
    transaction_count: u64,
    tps: f64,
}

impl TPSEstimator {
    pub fn update(&mut self, now: Instant, transaction_count: u64) -> f64 {
        if let Some(last_update) = self.last_update {
            if now > last_update {
                let delta_t = now.duration_since(last_update);
                let delta_c = transaction_count - self.transaction_count;
                let tps = delta_c as f64 / delta_t.as_secs_f64();
                self.tps = self.tps * 0.9 + tps * 0.1;
            }
        }

        self.last_update = Some(now);
        self.transaction_count = transaction_count;
        self.tps
    }
}

#[cfg(test)]
mod test {
    use rand::{thread_rng, Rng};
    use std::collections::HashMap;
    use sui_macros::sim_test;

    use super::*;
    use futures::future::join_all;
    use parking_lot::Mutex;

    #[tokio::test]
    #[should_panic(expected = "cannot skip stages")]
    async fn test_skip_pipeline_stages() {
        let stages = PipelineStages::new(0, CheckpointExecutorMetrics::new_for_tests());
        let mut handle = stages.handle(0).await;
        handle
            .finish_stage(PipelineStage::WaitForTransactions)
            .await;
    }

    #[sim_test]
    async fn test_pipeline_stages() {
        let stages = PipelineStages::new(0, CheckpointExecutorMetrics::new_for_tests());

        let output_by_stage = Arc::new(Mutex::new(HashMap::new()));
        let output_by_order = Arc::new(Mutex::new(Vec::new()));

        let mut tasks = Vec::new();

        for seq in 0..30 {
            let stages = stages.clone();
            let output_by_stage = output_by_stage.clone();
            let output_by_order = output_by_order.clone();
            tasks.push(tokio::spawn(async move {
                let mut handle = stages.handle(seq).await;
                let mut val = 0;
                let mut get_next_val = || {
                    val += 1;
                    val
                };

                async fn finish_stage(handle: &mut PipelineHandle, stage: PipelineStage) {
                    handle.finish_stage(stage).await;
                    let sleep_time = Duration::from_millis(thread_rng().gen_range(0..10));
                    tokio::time::sleep(sleep_time).await;
                }

                async fn push_output(
                    seq: CheckpointSequenceNumber,
                    get_next_val: &mut impl FnMut() -> u64,
                    output_by_stage: &Arc<Mutex<HashMap<u64, Vec<CheckpointSequenceNumber>>>>,
                    output_by_order: &Arc<Mutex<Vec<u64>>>,
                ) {
                    let sleep_time = Duration::from_millis(thread_rng().gen_range(0..10));
                    tokio::time::sleep(sleep_time).await;
                    let val = get_next_val();
                    debug!("pushing output ({val}) for seq: {}", seq);
                    output_by_stage.lock().entry(val).or_default().push(seq);
                    output_by_order.lock().push(val);
                }

                push_output(seq, &mut get_next_val, &output_by_stage, &output_by_order).await;
                finish_stage(&mut handle, PipelineStage::ExecuteTransactions).await;
                push_output(seq, &mut get_next_val, &output_by_stage, &output_by_order).await;
                finish_stage(&mut handle, PipelineStage::WaitForTransactions).await;
                push_output(seq, &mut get_next_val, &output_by_stage, &output_by_order).await;
                finish_stage(&mut handle, PipelineStage::FinalizeTransactions).await;
                push_output(seq, &mut get_next_val, &output_by_stage, &output_by_order).await;
                finish_stage(&mut handle, PipelineStage::ProcessCheckpointData).await;
                push_output(seq, &mut get_next_val, &output_by_stage, &output_by_order).await;
                finish_stage(&mut handle, PipelineStage::BuildDbBatch).await;
                push_output(seq, &mut get_next_val, &output_by_stage, &output_by_order).await;
                finish_stage(&mut handle, PipelineStage::CommitTransactionOutputs).await;
                push_output(seq, &mut get_next_val, &output_by_stage, &output_by_order).await;
                finish_stage(&mut handle, PipelineStage::FinalizeCheckpoint).await;
                push_output(seq, &mut get_next_val, &output_by_stage, &output_by_order).await;
                finish_stage(&mut handle, PipelineStage::UpdateRpcIndex).await;
                push_output(seq, &mut get_next_val, &output_by_stage, &output_by_order).await;
                finish_stage(&mut handle, PipelineStage::BumpHighestExecutedCheckpoint).await;
            }));
        }

        join_all(tasks).await;

        let output_by_stage = output_by_stage.lock();
        let output_by_order = output_by_order.lock();
        // for each stage, assert that the sequences were done in order
        for (_, seqs) in output_by_stage.iter() {
            assert_eq!(seqs, &((0..30).collect::<Vec<_>>()));
        }

        // Verify that the output sequence shows evidence of concurrent execution
        // Because the output is random, this test could fail by chance, but the
        // probability is infinitesimal.
        let mut found_out_of_order = false;
        for window in output_by_order.windows(2) {
            if window[0] > window[1] {
                found_out_of_order = true;
                break;
            }
        }
        assert!(found_out_of_order, "Expected to find evidence of concurrent execution in output sequence, but all elements were in order");
    }
}
