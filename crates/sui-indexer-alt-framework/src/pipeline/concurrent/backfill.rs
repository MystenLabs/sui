// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;
use std::ops::Range;
use std::time::Duration;

use sui_futures::service::Service;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio::time::Instant;
use tracing::info;

use crate::ingestion::ingestion_client::CheckpointEnvelope;
use crate::ingestion::ingestion_client::IngestionClient;

const BACKFILL_REQUEST_CHANNEL_SIZE: usize = 1024;

/// Configuration for [`BackfillService`].
#[derive(Clone)]
pub struct BackfillConfig {
    /// Bucket size, and also the maximum number of checkpoints any single
    /// [`BackfillHandle::next_chunk`] call returns.
    pub chunk_size: u64,

    /// Maximum distance above a bucket's upper bound for a handle to be considered an expected
    /// consumer of that bucket. This bounds cache retention to consumers likely to share the same
    /// completed fetch.
    pub expected_consumer_max_distance: u64,

    /// How long to keep a completed cache entry around for expected consumers.
    pub expected_consumer_wait_duration: Duration,

    /// Polling interval to retry fetching checkpoints that do not exist yet.
    pub retry_interval: Duration,
}

/// A source of checkpoint data for backfills.
pub struct BackfillService {
    backfill_config: BackfillConfig,
    ingestion_client: IngestionClient,
    pipeline_ranges: HashMap<BackfillHandleId, Range<u64>>,
    request_tx: mpsc::Sender<BackfillRequest>,
    request_rx: mpsc::Receiver<BackfillRequest>,
}

/// Per-pipeline subscription handle returned by [`BackfillService::subscribe`].
pub struct BackfillHandle {
    backfill_handle_id: BackfillHandleId,
    request_tx: mpsc::Sender<BackfillRequest>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct BackfillHandleId(usize);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CacheEntryId(usize);

enum BackfillRequest {
    SetPipelineLo {
        backfill_handle_id: BackfillHandleId,
        pipeline_lo: u64,
    },
    NextChunk {
        backfill_handle_id: BackfillHandleId,
        reply: oneshot::Sender<Option<Vec<CheckpointEnvelope>>>,
    },
}

struct CacheEntry {
    /// Generation ID for this `chunk_hi`, used to ignore stale fetch completions after eviction.
    cache_entry_id: CacheEntryId,
    /// Expected consumers keep a completed chunk warm without blocking forever on distant handles.
    remaining_backfill_handle_ids: HashSet<BackfillHandleId>,
    /// Requests wait here so concurrent consumers share one fetch fan-out.
    pending_handles: HashMap<BackfillHandleId, PendingHandle>,
    /// Ordered storage lets a shared cache entry serve per-handle sub-ranges deterministically.
    checkpoint_envelopes: BTreeMap<u64, CheckpointEnvelope>,
    /// A count avoids assuming fetch completions arrive contiguously or in order.
    expected_checkpoint_envelope_count: usize,
}

struct PendingHandle {
    reply: oneshot::Sender<Option<Vec<CheckpointEnvelope>>>,
    /// Each handle may need only the overlap between its subscription and the cached chunk.
    response_range: Range<u64>,
}

struct BackfillState {
    backfill_config: BackfillConfig,
    ingestion_client: IngestionClient,
    pipeline_ranges: HashMap<BackfillHandleId, Range<u64>>,
    /// Fixed upper bound captured at startup; completed or empty subscriptions do not shrink it.
    max_pipeline_hi: u64,
    /// Cache key is the chunk's exclusive upper bound.
    cache: HashMap<u64, CacheEntry>,
    /// Cache keys can repeat after eviction, so stale fetches need a generation discriminator.
    next_cache_entry_id: usize,
    /// Tasks run independently so one slow checkpoint or eviction timer does not block requests.
    tasks: JoinSet<BackfillTask>,
}

enum BackfillTask {
    Fetch {
        cache_entry_id: CacheEntryId,
        chunk_hi: u64,
        checkpoint_envelope: CheckpointEnvelope,
    },
    Evict {
        chunk_hi: u64,
        cache_entry_id: CacheEntryId,
    },
}

impl BackfillService {
    pub fn new(
        backfill_config: BackfillConfig,
        ingestion_client: IngestionClient,
    ) -> anyhow::Result<Self> {
        // Range alignment uses `next_multiple_of`, and zero-sized chunks would also make no
        // progress through a subscription.
        anyhow::ensure!(
            backfill_config.chunk_size > 0,
            "backfill chunk_size must be greater than zero"
        );

        let (request_tx, request_rx) = mpsc::channel(BACKFILL_REQUEST_CHANNEL_SIZE);
        Ok(Self {
            backfill_config,
            ingestion_client,
            pipeline_ranges: HashMap::new(),
            request_tx,
            request_rx,
        })
    }

    pub fn subscribe(&mut self, pipeline_backfill_range: Range<u64>) -> BackfillHandle {
        // Subscriptions are fixed before `run`, so a dense local ID is enough to correlate requests.
        let id = BackfillHandleId(self.pipeline_ranges.len());
        self.pipeline_ranges.insert(id, pipeline_backfill_range);
        BackfillHandle {
            backfill_handle_id: id,
            request_tx: self.request_tx.clone(),
        }
    }

    pub fn run(self) -> Service {
        Service::new().spawn_aborting(async move {
            let BackfillService {
                backfill_config,
                ingestion_client,
                pipeline_ranges,
                request_tx: _,
                mut request_rx,
            } = self;

            let Some(mut state) =
                BackfillState::new(backfill_config, ingestion_client, pipeline_ranges)
            else {
                info!("No backfill subscriptions, stopping backfill service");
                return Ok(());
            };

            loop {
                tokio::select! {
                    request = request_rx.recv() => {
                        let Some(request) = request else {
                            // All handles have been dropped; pending tasks no longer have consumers.
                            break;
                        };
                        match request {
                            BackfillRequest::SetPipelineLo { backfill_handle_id, pipeline_lo } => state.handle_set_pipeline_lo(backfill_handle_id, pipeline_lo),
                            BackfillRequest::NextChunk { backfill_handle_id, reply } => state.handle_next_chunk(backfill_handle_id, reply),
                        }
                    }

                    Some(task) = state.tasks.join_next() => {
                        // Task completions are driven even when no handle sends another request.
                        match task? {
                            BackfillTask::Fetch {
                                cache_entry_id,
                                chunk_hi,
                                checkpoint_envelope,
                            } => {
                                state.handle_fetch(cache_entry_id, chunk_hi, checkpoint_envelope);
                            }
                            BackfillTask::Evict {
                                chunk_hi,
                                cache_entry_id,
                            } => {
                                state.handle_evict(chunk_hi, cache_entry_id);
                            }
                        }
                    }

                    else => {
                        break;
                    }
                }
            }

            state.close_pending_handles();

            Ok(())
        })
    }
}

impl BackfillState {
    fn new(
        backfill_config: BackfillConfig,
        ingestion_client: IngestionClient,
        mut pipeline_ranges: HashMap<BackfillHandleId, Range<u64>>,
    ) -> Option<Self> {
        // Empty subscriptions would otherwise keep the service alive and widen the fixed checkpoint
        // bound even though no caller can consume their range.
        pipeline_ranges.retain(|_, pipeline_range| !pipeline_range.is_empty());

        // The upper bound is fixed so later progress updates cannot make earlier chunks reach into
        // checkpoints no subscriber originally requested.
        let max_pipeline_hi = pipeline_ranges
            .values()
            .map(|pipeline_range| pipeline_range.end)
            .max()?;

        let state = Self {
            backfill_config,
            ingestion_client,
            pipeline_ranges,
            max_pipeline_hi,
            cache: HashMap::new(),
            next_cache_entry_id: 0,
            tasks: JoinSet::new(),
        };
        Some(state)
    }

    fn handle_set_pipeline_lo(&mut self, id: BackfillHandleId, pipeline_lo: u64) {
        if let Some(pipeline_range) = self.pipeline_ranges.get_mut(&id) {
            // Pipelines report their committed low watermark asynchronously; once it catches the
            // high end there is no remaining backfill work for that handle.
            pipeline_range.start = pipeline_lo;
            if pipeline_range.is_empty() {
                self.pipeline_ranges.remove(&id);
            }
        }
    }

    fn handle_next_chunk(
        &mut self,
        backfill_handle_id: BackfillHandleId,
        reply: oneshot::Sender<Option<Vec<CheckpointEnvelope>>>,
    ) {
        let Some(pipeline_range) = self.pipeline_ranges.get_mut(&backfill_handle_id) else {
            // This handle already completed or was filtered out at startup.
            let _ = reply.send(None);
            return;
        };

        let BackfillConfig {
            chunk_size,
            expected_consumer_max_distance,
            retry_interval,
            ..
        } = self.backfill_config;
        // Align upward so overlapping subscriptions converge on the same shared chunk, then clamp
        // to the startup maximum so the final chunk does not wait for unrequested checkpoints.
        let chunk_hi = pipeline_range
            .end
            .next_multiple_of(chunk_size)
            .min(self.max_pipeline_hi);
        // Genesis-aligned chunks have no lower bucket to borrow from, so avoid underflow.
        let chunk_lo = chunk_hi.saturating_sub(chunk_size);
        // A cache entry can hold a full chunk while each caller receives only its requested overlap.
        let response_lo = pipeline_range.start.max(chunk_lo);
        let response_hi = pipeline_range.end.min(chunk_hi);

        // Move this caller out of the expected-consumer set before it is computed below.
        pipeline_range.end = response_lo;
        if pipeline_range.is_empty() {
            self.pipeline_ranges.remove(&backfill_handle_id);
        }

        let pending_handle = PendingHandle {
            reply,
            response_range: response_lo..response_hi,
        };

        let cache_entry = match self.cache.entry(chunk_hi) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                // Fetch completions can arrive after this entry is evicted and recreated.
                let cache_entry_id = CacheEntryId(self.next_cache_entry_id);
                self.next_cache_entry_id += 1;

                for checkpoint in chunk_lo..chunk_hi {
                    let ingestion_client = self.ingestion_client.clone();
                    self.tasks.spawn(async move {
                        let checkpoint_envelope = ingestion_client
                            .wait_for(checkpoint, retry_interval)
                            .await
                            .unwrap_or_else(|e| {
                                panic!("checkpoint {checkpoint} fetch failed during backfill: {e}")
                            });

                        BackfillTask::Fetch {
                            cache_entry_id,
                            chunk_hi,
                            checkpoint_envelope,
                        }
                    });
                }

                // Keep the completed chunk warm only for consumers close enough to plausibly ask
                // for this same chunk next.
                let remaining_backfill_handle_ids = self
                    .pipeline_ranges
                    .iter()
                    .filter_map(|(&handle_id, &Range { start, end })| {
                        // Range is after chunk.
                        if start >= chunk_hi
                            // Range is before chunk.
                            || end <= chunk_lo
                            // Range overlaps, but range end is too far away from chunk start.
                            || end.saturating_sub(chunk_hi) > expected_consumer_max_distance
                        {
                            return None;
                        }

                        Some(handle_id)
                    })
                    .collect::<HashSet<_>>();

                entry.insert(CacheEntry {
                    cache_entry_id,
                    remaining_backfill_handle_ids,
                    pending_handles: HashMap::new(),
                    checkpoint_envelopes: BTreeMap::new(),
                    expected_checkpoint_envelope_count: (chunk_hi - chunk_lo) as usize,
                })
            }
        };
        cache_entry
            .remaining_backfill_handle_ids
            .remove(&backfill_handle_id);
        cache_entry
            .pending_handles
            .insert(backfill_handle_id, pending_handle);

        if cache_entry.has_all_checkpoint_envelopes()
            && cache_entry.remaining_backfill_handle_ids.is_empty()
        {
            self.dispatch_pending(chunk_hi);
        }
    }

    fn handle_fetch(
        &mut self,
        cache_entry_id: CacheEntryId,
        chunk_hi: u64,
        checkpoint_envelope: CheckpointEnvelope,
    ) {
        let Some(cache_entry) = self.cache.get_mut(&chunk_hi) else {
            // The cache entry was evicted before this fetch completed.
            return;
        };
        if cache_entry.cache_entry_id != cache_entry_id {
            // The chunk key was reused after this fetch was spawned.
            return;
        }

        let sequence_number = *checkpoint_envelope.checkpoint.summary.sequence_number();
        // Duplicate completion would mean two tasks raced to fill the same checkpoint slot.
        assert!(
            cache_entry
                .checkpoint_envelopes
                .insert(sequence_number, checkpoint_envelope)
                .is_none(),
            "checkpoint {sequence_number} fetch slot must not be completed twice"
        );

        if cache_entry.has_all_checkpoint_envelopes() {
            if cache_entry.remaining_backfill_handle_ids.is_empty() {
                self.dispatch_pending(chunk_hi);
            } else {
                // Expected consumers can reduce duplicate fetches, but they must not block current
                // waiters beyond the configured grace period.
                let evict_at =
                    Instant::now() + self.backfill_config.expected_consumer_wait_duration;
                let cache_entry_id = cache_entry.cache_entry_id;
                self.tasks.spawn(async move {
                    tokio::time::sleep_until(evict_at).await;
                    BackfillTask::Evict {
                        chunk_hi,
                        cache_entry_id,
                    }
                });
            }
        }
    }

    fn handle_evict(&mut self, chunk_hi: u64, cache_entry_id: CacheEntryId) {
        if self
            .cache
            .get(&chunk_hi)
            .is_some_and(|cache_entry| cache_entry.cache_entry_id == cache_entry_id)
        {
            // Expired retention must still answer current waiters; otherwise they can hang after
            // the cache entry leaves the map.
            self.dispatch_pending(chunk_hi);
        }
    }

    fn dispatch_pending(&mut self, chunk_hi: u64) {
        let Some(mut cache_entry) = self.cache.remove(&chunk_hi) else {
            return;
        };

        for (_, pending_handle) in cache_entry.pending_handles.drain() {
            let PendingHandle {
                reply,
                response_range,
            } = pending_handle;
            let checkpoint_envelopes = cache_entry
                .checkpoint_envelopes
                .range(response_range)
                .map(|(_, v)| v.clone())
                .collect();
            // A dropped receiver means the caller no longer needs the backfill result.
            let _ = reply.send(Some(checkpoint_envelopes));
        }
    }

    fn close_pending_handles(self) {
        for cache_entry in self.cache.into_values() {
            for pending_handle in cache_entry.pending_handles.into_values() {
                let PendingHandle { reply, .. } = pending_handle;
                // Shutdown should resolve callers instead of leaving their oneshot receivers open.
                let _ = reply.send(None);
            }
        }
    }
}

impl BackfillHandle {
    pub async fn set_pipeline_lo(&self, pipeline_lo: u64) -> anyhow::Result<bool> {
        if self
            .request_tx
            .send(BackfillRequest::SetPipelineLo {
                backfill_handle_id: self.backfill_handle_id,
                pipeline_lo,
            })
            .await
            .is_err()
        {
            // The service has stopped, so the low watermark was not applied.
            return Ok(false);
        }

        Ok(true)
    }

    pub async fn next_chunk(&mut self) -> anyhow::Result<Option<Vec<CheckpointEnvelope>>> {
        let (reply, receive) = oneshot::channel();
        if self
            .request_tx
            .send(BackfillRequest::NextChunk {
                backfill_handle_id: self.backfill_handle_id,
                reply,
            })
            .await
            .is_err()
        {
            // The service has stopped, so no chunk can be returned.
            return Ok(None);
        }

        let Ok(Some(chunk)) = receive.await else {
            // The service closed the pending reply or returned no chunk.
            return Ok(None);
        };

        Ok(Some(chunk))
    }
}

impl CacheEntry {
    fn has_all_checkpoint_envelopes(&self) -> bool {
        self.checkpoint_envelopes.len() == self.expected_checkpoint_envelope_count
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;
    use std::sync::Arc;
    use std::time::Duration;

    use sui_futures::service::Service;
    use sui_types::digests::CheckpointDigest;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
    use tokio::task::JoinHandle;
    use tokio::time::timeout;

    use crate::ingestion::ingestion_client::tests::MockIngestionClient;
    use crate::metrics::tests::test_ingestion_metrics;

    use super::*;

    const CHUNK_SIZE: u64 = 3;
    const EXPECTED_CONSUMER_MAX_DISTANCE: u64 = 1;

    const EXPECTED_CONSUMER_WAIT_DURATION: Duration = Duration::from_secs(60);
    const RETRY_INTERVAL: Duration = Duration::from_millis(1);
    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    fn checkpoint_envelope_with_timestamp(
        sequence_number: u64,
        timestamp_ms: u64,
    ) -> CheckpointEnvelope {
        CheckpointEnvelope {
            checkpoint: Arc::new(
                TestCheckpointBuilder::new(sequence_number)
                    .with_timestamp_ms(timestamp_ms)
                    .build_checkpoint(),
            ),
            chain_id: CheckpointDigest::new([1; 32]).into(),
        }
    }

    fn checkpoint_envelope(sequence_number: u64) -> CheckpointEnvelope {
        checkpoint_envelope_with_timestamp(sequence_number, 1_000 + sequence_number)
    }

    fn checkpoint_sequences(checkpoints: &[CheckpointEnvelope]) -> Vec<u64> {
        checkpoints
            .iter()
            .map(|envelope| *envelope.checkpoint.summary.sequence_number())
            .collect()
    }

    fn checkpoint_timestamps(checkpoints: &[CheckpointEnvelope]) -> Vec<u64> {
        checkpoints
            .iter()
            .map(|envelope| envelope.checkpoint.summary.timestamp_ms)
            .collect()
    }

    fn backfill_service() -> (BackfillService, Arc<MockIngestionClient>) {
        backfill_service_with_expected_consumer_wait_duration(EXPECTED_CONSUMER_WAIT_DURATION)
    }

    fn backfill_service_with_expected_consumer_wait_duration(
        expected_consumer_wait_duration: Duration,
    ) -> (BackfillService, Arc<MockIngestionClient>) {
        let mock_client = Arc::new(MockIngestionClient::default());
        let ingestion_client =
            IngestionClient::new_impl(mock_client.clone(), test_ingestion_metrics());

        let service = BackfillService::new(
            BackfillConfig {
                chunk_size: CHUNK_SIZE,
                expected_consumer_max_distance: EXPECTED_CONSUMER_MAX_DISTANCE,
                expected_consumer_wait_duration,
                retry_interval: RETRY_INTERVAL,
            },
            ingestion_client,
        )
        .expect("test backfill config must be valid");

        (service, mock_client)
    }

    fn insert_mock_checkpoints(mock_client: &MockIngestionClient, checkpoints: Range<u64>) {
        for checkpoint in checkpoints {
            mock_client.checkpoints.insert(
                checkpoint,
                checkpoint_envelope(checkpoint).checkpoint.as_ref().clone(),
            );
        }
    }

    fn insert_mock_checkpoints_with_timestamp_base(
        mock_client: &MockIngestionClient,
        checkpoints: Range<u64>,
        timestamp_base: u64,
    ) {
        for checkpoint in checkpoints {
            mock_client.checkpoints.insert(
                checkpoint,
                checkpoint_envelope_with_timestamp(checkpoint, timestamp_base + checkpoint)
                    .checkpoint
                    .as_ref()
                    .clone(),
            );
        }
    }

    fn test_service(
        pipeline_ranges: impl IntoIterator<Item = Range<u64>>,
    ) -> (Service, Vec<BackfillHandle>, Arc<MockIngestionClient>) {
        let (mut backfill_service, mock_client) = backfill_service();
        let handles = pipeline_ranges
            .into_iter()
            .map(|range| backfill_service.subscribe(range))
            .collect();
        let service = backfill_service.run();

        (service, handles, mock_client)
    }

    fn test_service_with_expected_consumer_wait_duration(
        pipeline_ranges: impl IntoIterator<Item = Range<u64>>,
        expected_consumer_wait_duration: Duration,
    ) -> (Service, Vec<BackfillHandle>, Arc<MockIngestionClient>) {
        let (mut backfill_service, mock_client) =
            backfill_service_with_expected_consumer_wait_duration(expected_consumer_wait_duration);
        let handles = pipeline_ranges
            .into_iter()
            .map(|range| backfill_service.subscribe(range))
            .collect();
        let service = backfill_service.run();

        (service, handles, mock_client)
    }

    async fn expect_next_chunk(
        handle_name: &str,
        handle: &mut BackfillHandle,
    ) -> Vec<CheckpointEnvelope> {
        timeout(TEST_TIMEOUT, handle.next_chunk())
            .await
            .unwrap_or_else(|_| panic!("{handle_name} next_chunk should return"))
            .unwrap_or_else(|error| panic!("{handle_name} next_chunk should not fail: {error}"))
            .unwrap_or_else(|| panic!("{handle_name} chunk should be returned"))
    }

    async fn expect_next_chunk_sequences(
        handle_name: &str,
        handle: &mut BackfillHandle,
    ) -> Vec<u64> {
        checkpoint_sequences(&expect_next_chunk(handle_name, handle).await)
    }

    async fn expect_no_next_chunk(handle_name: &str, handle: &mut BackfillHandle) {
        let chunk = timeout(TEST_TIMEOUT, handle.next_chunk())
            .await
            .unwrap_or_else(|_| panic!("{handle_name} next_chunk should return"))
            .unwrap_or_else(|error| panic!("{handle_name} next_chunk should not fail: {error}"));

        assert!(
            chunk.is_none(),
            "{handle_name} chunk should not be returned"
        );
    }

    async fn expect_spawned_next_chunk_result(
        task_name: &str,
        task: JoinHandle<anyhow::Result<Option<Vec<CheckpointEnvelope>>>>,
    ) -> Option<Vec<CheckpointEnvelope>> {
        timeout(TEST_TIMEOUT, task)
            .await
            .unwrap_or_else(|_| panic!("{task_name} handle should finish"))
            .unwrap_or_else(|error| panic!("{task_name} task should not panic: {error}"))
            .unwrap_or_else(|error| panic!("{task_name} next_chunk should not fail: {error}"))
    }

    async fn expect_spawned_next_chunk(
        task_name: &str,
        task: JoinHandle<anyhow::Result<Option<Vec<CheckpointEnvelope>>>>,
    ) -> Vec<CheckpointEnvelope> {
        expect_spawned_next_chunk_result(task_name, task)
            .await
            .unwrap_or_else(|| panic!("{task_name} chunk should be returned"))
    }

    async fn expect_service_join(mut service: Service) {
        timeout(TEST_TIMEOUT, service.join())
            .await
            .expect("service should stop")
            .expect("service should not fail");
    }

    async fn expect_service_shutdown(service: Service) {
        service.shutdown().await.expect("service should shut down");
    }

    async fn yield_to_service() {
        // Some retention tests need the first request parked in the service before mutating time or
        // replacing mock checkpoint data.
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }
    }

    #[test]
    fn new_rejects_zero_chunk_size() {
        let mock_client = Arc::new(MockIngestionClient::default());
        let ingestion_client =
            IngestionClient::new_impl(mock_client.clone(), test_ingestion_metrics());

        let result = BackfillService::new(
            BackfillConfig {
                chunk_size: 0,
                expected_consumer_max_distance: EXPECTED_CONSUMER_MAX_DISTANCE,
                expected_consumer_wait_duration: EXPECTED_CONSUMER_WAIT_DURATION,
                retry_interval: RETRY_INTERVAL,
            },
            ingestion_client,
        );

        let error = match result {
            Ok(_) => panic!("zero chunk size config must fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("chunk_size"));
    }

    #[tokio::test]
    async fn run_without_subscriptions_exits() {
        let (backfill_service, _) = backfill_service();
        let service = backfill_service.run();

        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn handle_methods_return_closed_results_after_service_stops() {
        let (mut backfill_service, _) = backfill_service();
        let mut handle = backfill_service.subscribe(0..CHUNK_SIZE);
        let service = backfill_service.run();

        expect_service_shutdown(service).await;

        assert!(!handle.set_pipeline_lo(4).await.unwrap());
        expect_no_next_chunk("closed handle", &mut handle).await;
    }

    #[tokio::test]
    async fn next_chunk_returns_none_for_empty_subscription() {
        let (service, mut handles, _) = test_service(std::iter::once(1..1));
        let mut handle = handles.pop().unwrap();

        expect_no_next_chunk("empty handle", &mut handle).await;

        expect_service_shutdown(service).await;
    }

    #[tokio::test]
    async fn next_chunk_fetches_chunk_through_service() {
        let (service, mut handles, mock_client) = test_service(std::iter::once(0..CHUNK_SIZE));
        insert_mock_checkpoints(&mock_client, 0..CHUNK_SIZE);
        let mut handle = handles.pop().unwrap();

        let sequences = expect_next_chunk_sequences("handle", &mut handle).await;

        assert_eq!(sequences, vec![0, 1, 2]);

        drop(handle);
        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn next_chunk_reduces_subscription_range_until_empty() {
        let (service, mut handles, mock_client) = test_service(std::iter::once(0..CHUNK_SIZE * 2));
        insert_mock_checkpoints(&mock_client, 0..CHUNK_SIZE * 2);
        let mut handle = handles.pop().unwrap();

        let first_sequences = expect_next_chunk_sequences("first", &mut handle).await;
        let second_sequences = expect_next_chunk_sequences("second", &mut handle).await;

        assert_eq!(first_sequences, vec![3, 4, 5]);
        assert_eq!(second_sequences, vec![0, 1, 2]);

        expect_no_next_chunk("completed handle", &mut handle).await;

        drop(handle);
        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn concurrent_handles_receive_same_pending_chunk() {
        let (service, mut handles, mock_client) = test_service([0..CHUNK_SIZE, 0..CHUNK_SIZE]);
        let mut second_handle = handles.pop().unwrap();
        let mut first_handle = handles.pop().unwrap();

        let first = tokio::spawn(async move { first_handle.next_chunk().await });
        let second = tokio::spawn(async move { second_handle.next_chunk().await });

        tokio::task::yield_now().await;
        insert_mock_checkpoints(&mock_client, 0..CHUNK_SIZE);

        let first = expect_spawned_next_chunk("first", first).await;
        let second = expect_spawned_next_chunk("second", second).await;

        assert_eq!(checkpoint_sequences(&first), vec![0, 1, 2]);
        assert_eq!(checkpoint_sequences(&second), vec![0, 1, 2]);

        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn concurrent_handles_receive_filtered_pending_chunk() {
        let (service, mut handles, mock_client) = test_service([0..4, 0..CHUNK_SIZE * 2]);
        let mut wide_handle = handles.pop().unwrap();
        let mut narrow_handle = handles.pop().unwrap();

        let narrow = tokio::spawn(async move { narrow_handle.next_chunk().await });
        let wide = tokio::spawn(async move { wide_handle.next_chunk().await });

        tokio::task::yield_now().await;
        insert_mock_checkpoints(&mock_client, CHUNK_SIZE..CHUNK_SIZE * 2);

        let narrow = expect_spawned_next_chunk("narrow", narrow).await;
        let wide = expect_spawned_next_chunk("wide", wide).await;

        assert_eq!(checkpoint_sequences(&narrow), vec![3]);
        assert_eq!(checkpoint_sequences(&wide), vec![3, 4, 5]);

        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn set_pipeline_lo_can_mark_subscription_complete_before_next_chunk() {
        let (service, mut handles, _) = test_service(std::iter::once(0..CHUNK_SIZE));
        let mut handle = handles.pop().unwrap();

        assert!(handle.set_pipeline_lo(CHUNK_SIZE).await.unwrap());
        expect_no_next_chunk("completed handle", &mut handle).await;

        expect_service_shutdown(service).await;
    }

    #[tokio::test]
    async fn next_chunk_returns_none_when_service_drops_pending_reply() {
        let (service, mut handles, _) = test_service(std::iter::once(0..CHUNK_SIZE));
        let mut handle = handles.pop().unwrap();
        let pending = tokio::spawn(async move { handle.next_chunk().await });

        tokio::task::yield_now().await;
        expect_service_shutdown(service).await;

        let chunk = expect_spawned_next_chunk_result("pending", pending).await;

        assert!(chunk.is_none());
    }

    #[tokio::test]
    async fn service_stops_when_last_handle_drops_with_pending_fetches() {
        let (service, mut handles, mock_client) = test_service(std::iter::once(0..CHUNK_SIZE));
        mock_client.not_found_failures.insert(0, usize::MAX);
        let mut handle = handles.pop().unwrap();
        let pending = tokio::spawn(async move { handle.next_chunk().await });

        timeout(TEST_TIMEOUT, async {
            loop {
                if mock_client
                    .not_found_failures
                    .get(&0)
                    .is_some_and(|remaining| *remaining < usize::MAX)
                {
                    // Wait until the fetch task is parked in the service before aborting the
                    // caller, otherwise the test can pass without exercising shutdown cleanup.
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("fetch should start");

        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());

        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn completed_chunks_remain_cached_during_expected_consumer_wait_duration() {
        let (service, mut handles, mock_client) = test_service_with_expected_consumer_wait_duration(
            [0..CHUNK_SIZE, 0..CHUNK_SIZE],
            EXPECTED_CONSUMER_WAIT_DURATION,
        );
        let mut expected_handle = handles.pop().unwrap();
        let mut first_handle = handles.pop().unwrap();

        insert_mock_checkpoints(&mock_client, 0..CHUNK_SIZE);
        let first = tokio::spawn(async move { first_handle.next_chunk().await });
        yield_to_service().await;

        // Distinct timestamps let the test distinguish cache reuse from a fresh fetch.
        insert_mock_checkpoints_with_timestamp_base(&mock_client, 0..CHUNK_SIZE, 10_000);
        let cached_chunk = expect_next_chunk("expected consumer", &mut expected_handle).await;
        let first_chunk = expect_spawned_next_chunk("first", first).await;

        assert_eq!(
            checkpoint_timestamps(&first_chunk),
            vec![1_000, 1_001, 1_002]
        );
        assert_eq!(
            checkpoint_timestamps(&cached_chunk),
            vec![1_000, 1_001, 1_002]
        );

        drop(expected_handle);
        expect_service_join(service).await;
    }

    #[tokio::test(start_paused = true)]
    async fn completed_chunks_are_evicted_after_expected_consumer_wait_duration() {
        let (service, mut handles, mock_client) = test_service_with_expected_consumer_wait_duration(
            [0..CHUNK_SIZE, 0..CHUNK_SIZE],
            Duration::from_millis(1),
        );
        let mut expected_handle = handles.pop().unwrap();
        let mut first_handle = handles.pop().unwrap();

        insert_mock_checkpoints(&mock_client, 0..CHUNK_SIZE);
        let first = tokio::spawn(async move { first_handle.next_chunk().await });
        yield_to_service().await;

        tokio::time::advance(Duration::from_millis(10)).await;
        // The first waiter only completes after the retention timer has released the cached entry.
        let first_chunk = expect_spawned_next_chunk("first", first).await;

        insert_mock_checkpoints_with_timestamp_base(&mock_client, 0..CHUNK_SIZE, 10_000);
        let refreshed_chunk = expect_next_chunk("expected consumer", &mut expected_handle).await;
        assert_eq!(
            checkpoint_timestamps(&first_chunk),
            vec![1_000, 1_001, 1_002]
        );
        assert_eq!(
            checkpoint_timestamps(&refreshed_chunk),
            vec![10_000, 10_001, 10_002]
        );

        drop(expected_handle);
        expect_service_join(service).await;
    }

    #[tokio::test(start_paused = true)]
    async fn completed_chunk_waiter_is_released_by_evict_task() {
        let (service, mut handles, mock_client) = test_service_with_expected_consumer_wait_duration(
            [0..CHUNK_SIZE, 0..CHUNK_SIZE],
            Duration::from_millis(1),
        );
        let expected_handle = handles.pop().unwrap();
        let mut first_handle = handles.pop().unwrap();

        insert_mock_checkpoints(&mock_client, 0..CHUNK_SIZE);
        let first = tokio::spawn(async move { first_handle.next_chunk().await });
        yield_to_service().await;

        // This guards the timer path specifically: no later request should be needed to wake a
        // waiter once retention expires.
        tokio::time::advance(Duration::from_millis(10)).await;
        let first_chunk = expect_spawned_next_chunk("first", first).await;

        assert_eq!(
            checkpoint_timestamps(&first_chunk),
            vec![1_000, 1_001, 1_002]
        );

        drop(expected_handle);
        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn empty_subscription_does_not_extend_highest_subscription_end() {
        let (service, mut handles, mock_client) = test_service([0..4, 10..10]);
        let mut empty_handle = handles.pop().unwrap();
        let mut handle = handles.remove(0);

        expect_no_next_chunk("empty handle", &mut empty_handle).await;

        insert_mock_checkpoints(&mock_client, 1..4);

        let sequences = expect_next_chunk_sequences("handle", &mut handle).await;

        assert_eq!(sequences, vec![1, 2, 3]);

        drop(empty_handle);
        drop(handle);
        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn set_pipeline_lo_removes_empty_subscription_without_changing_highest_end() {
        let (service, mut handles, mock_client) = test_service([0..4, 0..10]);
        let mut high_handle = handles.pop().unwrap();
        let mut handle = handles.pop().unwrap();

        assert!(high_handle.set_pipeline_lo(10).await.unwrap());
        expect_no_next_chunk("completed handle", &mut high_handle).await;

        insert_mock_checkpoints(&mock_client, 3..6);

        let sequences = expect_next_chunk_sequences("handle", &mut handle).await;

        assert_eq!(sequences, vec![3]);

        drop(high_handle);
        drop(handle);
        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn next_chunk_filters_aligned_chunk_by_pipeline_hi() {
        let (service, mut handles, mock_client) = test_service([0..4, 9..10]);
        let mut handle = handles.remove(0);
        drop(handles);
        insert_mock_checkpoints(&mock_client, 3..6);

        let sequences = expect_next_chunk_sequences("handle", &mut handle).await;

        assert_eq!(sequences, vec![3]);

        drop(handle);
        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn next_chunk_filters_final_chunk_by_pipeline_lo() {
        let (service, mut handles, mock_client) = test_service(std::iter::once(2..4));
        let mut handle = handles.pop().unwrap();
        insert_mock_checkpoints(&mock_client, 1..4);

        let sequences = expect_next_chunk_sequences("handle", &mut handle).await;

        assert_eq!(sequences, vec![2, 3]);

        drop(handle);
        expect_service_join(service).await;
    }

    #[tokio::test]
    async fn final_chunk_is_clamped_to_highest_subscription_end() {
        let (service, mut handles, mock_client) = test_service(std::iter::once(0..2));
        let mut handle = handles.pop().unwrap();
        insert_mock_checkpoints(&mock_client, 0..2);

        let sequences = expect_next_chunk_sequences("handle", &mut handle).await;

        assert_eq!(sequences, vec![0, 1]);

        drop(handle);
        expect_service_join(service).await;
    }
}
