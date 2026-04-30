// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::RangeInclusive;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use anyhow::anyhow;
use backoff::ExponentialBackoff;
use futures::future::try_join_all;
use prost_types::FieldMask;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcReader;
use sui_rpc::proto::sui::rpc::v2::Checkpoint as ProtoCheckpoint;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use tokio::sync::broadcast;
use tokio::sync::watch;
use tracing::warn;

use super::ProcessedCheckpoint;
use super::checkpoint_stream_task::checkpoint_field_mask;
use super::checkpoint_stream_task::process_checkpoint;
use crate::task::watermark::KV_PACKAGES_PIPELINE;
use crate::task::watermark::Watermarks;

/// Pipeline name under which `WatermarkTask` tracks the kv-rpc / LedgerService source.
const LEDGER_GRPC_PIPELINE: &str = "ledger_grpc";

/// Abstraction over the source that gap recovery fetches checkpoints from. The production
/// implementation talks to kv-rpc via `LedgerGrpcReader`; tests use an in-memory mock.
///
/// `Ok(None)` covers both `NotFound` and empty-payload responses (treated equivalently as
/// "not available, retry"). `Err` covers transport-level failures.
pub(crate) trait CheckpointFetcher {
    async fn fetch_checkpoint(
        &self,
        seq: u64,
        mask: &FieldMask,
    ) -> anyhow::Result<Option<ProtoCheckpoint>>;
}

impl CheckpointFetcher for LedgerGrpcReader {
    async fn fetch_checkpoint(
        &self,
        seq: u64,
        mask: &FieldMask,
    ) -> anyhow::Result<Option<ProtoCheckpoint>> {
        let request = GetCheckpointRequest::by_sequence_number(seq).with_read_mask(mask.clone());
        match self.get_checkpoint(request).await {
            Ok(response) => Ok(response.checkpoint),
            Err(status) if status.code() == tonic::Code::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// Recover the gap `lo..=hi` by reading checkpoints from kv-rpc, processing them through the
/// existing streaming pipeline, and broadcasting in order. Each chunk waits for the kv-rpc
/// indexer to reach the chunk's upper bound before fetching, so partial progress is broadcast
/// as the indexer catches up rather than waiting for the entire gap to be available.
pub(crate) async fn recover_gap<F: CheckpointFetcher>(
    fetcher: &F,
    watermarks_rx: &watch::Receiver<Arc<Watermarks>>,
    sender: &broadcast::Sender<Arc<ProcessedCheckpoint>>,
    lo: u64,
    hi: u64,
    chunk_size: usize,
) -> anyhow::Result<()> {
    if lo > hi {
        return Ok(());
    }

    let mask = checkpoint_field_mask();
    let chunk_size = chunk_size.max(1) as u64;
    let mut cursor = lo;

    while cursor <= hi {
        let chunk_hi = (cursor + chunk_size - 1).min(hi);

        wait_for_pipelines_catching_up_at(chunk_hi, watermarks_rx).await?;

        let chunk = fetch_chunk(fetcher, &mask, cursor..=chunk_hi).await?;

        for proto in chunk {
            let processed = process_checkpoint(proto)?;
            // Ignore send errors: no active subscribers is a normal state during recovery.
            let _ = sender.send(Arc::new(processed));
        }

        cursor = chunk_hi + 1;
    }

    Ok(())
}

/// Block until both indexer pipelines that gap recovery depends on have caught up to
/// `target`: `ledger_grpc` (kv-rpc, serves checkpoint contents) and `kv_packages`
/// (Postgres, serves package resolution from the DB). Recovered checkpoints don't go
/// through `index_and_broadcast`, so subscribers resolving their packages fall through
/// to the DB and need `kv_packages` to be ready.
async fn wait_for_pipelines_catching_up_at(
    target: u64,
    watermarks_rx: &watch::Receiver<Arc<Watermarks>>,
) -> anyhow::Result<()> {
    let mut rx = watermarks_rx.clone();
    rx.wait_for(|w| {
        let pipelines = w.per_pipeline();
        let caught_up = |name| {
            pipelines
                .get(name)
                .is_some_and(|p| p.hi().checkpoint() >= target)
        };
        caught_up(LEDGER_GRPC_PIPELINE) && caught_up(KV_PACKAGES_PIPELINE)
    })
    .await
    .ok()
    .context("Watermark task shut down before pipelines caught up")?;
    Ok(())
}

/// Fetch every checkpoint in `range` concurrently. Each call retries independently and
/// indefinitely on any error; successful results are held locally until the full chunk
/// resolves.
async fn fetch_chunk<F: CheckpointFetcher>(
    fetcher: &F,
    mask: &FieldMask,
    range: RangeInclusive<u64>,
) -> anyhow::Result<Vec<ProtoCheckpoint>> {
    let futures = range.map(|seq| fetch_one_with_retry(fetcher, mask, seq));
    try_join_all(futures).await
}

/// Fetch one checkpoint via `GetCheckpoint`, retrying every error as transient with exponential
/// backoff and no overall deadline.
///
/// All gRPC errors are treated as transient on purpose, including `NotFound` and empty payloads.
/// After the per-chunk watermark gate, `NotFound` would mean either below-retention or a
/// watermark-data inconsistency on kv-rpc. Neither case is fixed by retrying, but neither is
/// fixed by crashing the streaming server either: the server is long-lived with many
/// subscribers, and tearing it down on a single bad fetch is worse than staying up and
/// surfacing the issue through observability so an operator can intervene (manual restart,
/// retention bump, etc.).
///
/// TODO: Emit metrics so ops can alert on stuck recovery.
async fn fetch_one_with_retry<F: CheckpointFetcher>(
    fetcher: &F,
    mask: &FieldMask,
    seq: u64,
) -> anyhow::Result<ProtoCheckpoint> {
    let backoff = ExponentialBackoff {
        initial_interval: Duration::from_millis(100),
        max_interval: Duration::from_secs(5),
        max_elapsed_time: None,
        ..Default::default()
    };

    backoff::future::retry(backoff, || async {
        match fetcher.fetch_checkpoint(seq, mask).await {
            Ok(Some(cp)) => Ok(cp),
            Ok(None) => {
                warn!(seq, "Retrying: checkpoint not yet available from kv-rpc");
                Err(backoff::Error::transient(anyhow!(
                    "checkpoint {seq} not available"
                )))
            }
            Err(e) => {
                warn!(seq, "Retrying gap fetch after error: {e}");
                Err(backoff::Error::transient(e))
            }
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::Duration;

    use sui_rpc::proto::sui::rpc::v2 as grpc;
    use sui_sdk_types::Bitmap;
    use sui_sdk_types::Bls12381Signature;
    use sui_sdk_types::ValidatorAggregatedSignature as SdkValidatorAggregatedSignature;
    use sui_types::crypto::AggregateAuthoritySignature;
    use sui_types::gas::GasCostSummary;
    use sui_types::messages_checkpoint::CheckpointContents as NativeCheckpointContents;
    use sui_types::messages_checkpoint::CheckpointSummary as NativeCheckpointSummary;
    use tokio::time::timeout;

    use super::*;

    /// Build a fully deserializable test `ProtoCheckpoint` with empty contents and a zero
    /// signature. `process_checkpoint` parses (but does not verify) the signature, so the
    /// all-zero bytes are accepted.
    fn make_test_proto_checkpoint(seq: u64) -> ProtoCheckpoint {
        let contents = NativeCheckpointContents::new_with_digests_only_for_tests(vec![]);
        let summary = NativeCheckpointSummary {
            epoch: 0,
            sequence_number: seq,
            network_total_transactions: 0,
            content_digest: *contents.digest(),
            previous_digest: None,
            epoch_rolling_gas_cost_summary: GasCostSummary::default(),
            timestamp_ms: 0,
            checkpoint_commitments: vec![],
            end_of_epoch_data: None,
            version_specific_data: vec![],
        };
        // Use the default `AggregateAuthoritySignature` bytes rather than all-zeros: the
        // proto → sui_types conversion in `process_checkpoint` calls
        // `AggregateAuthoritySignature::from_bytes` and expects the BLS encoding to round-trip,
        // which all-zero bytes do not satisfy.
        let default_bls_bytes: [u8; 48] = AggregateAuthoritySignature::default()
            .as_ref()
            .try_into()
            .expect("BLS aggregate signature is 48 bytes");
        let sdk_sig = SdkValidatorAggregatedSignature {
            epoch: 0,
            signature: Bls12381Signature::new(default_bls_bytes),
            bitmap: Bitmap::default(),
        };

        let mut summary_bcs = grpc::Bcs::default();
        summary_bcs.value = Some(bcs::to_bytes(&summary).unwrap().into());
        let mut summary_proto = grpc::CheckpointSummary::default();
        summary_proto.bcs = Some(summary_bcs);

        let mut contents_bcs = grpc::Bcs::default();
        contents_bcs.value = Some(bcs::to_bytes(&contents).unwrap().into());
        let mut contents_proto = grpc::CheckpointContents::default();
        contents_proto.bcs = Some(contents_bcs);

        let mut cp = ProtoCheckpoint::default();
        cp.sequence_number = Some(seq);
        cp.summary = Some(summary_proto);
        cp.contents = Some(contents_proto);
        cp.signature = Some(sdk_sig.into());
        cp
    }

    /// Per-key behavior of the mock fetcher.
    #[derive(Debug, Clone)]
    enum FetcherBehavior {
        /// Always return `Ok(Some(make_test_proto_checkpoint(seq)))`.
        Success,
        /// Return `Err` for the first N calls, then `Ok(Some(...))` afterward.
        ErrorThenSuccess(usize),
        /// Return `Ok(None)` for the first N calls, then `Ok(Some(...))` afterward.
        NoneThenSuccess(usize),
    }

    struct MockFetcher {
        state: Mutex<HashMap<u64, (FetcherBehavior, usize)>>,
    }

    impl MockFetcher {
        fn new(setup: HashMap<u64, FetcherBehavior>) -> Self {
            Self {
                state: Mutex::new(setup.into_iter().map(|(k, v)| (k, (v, 0))).collect()),
            }
        }

        fn calls_for(&self, seq: u64) -> usize {
            self.state.lock().unwrap().get(&seq).map_or(0, |(_, c)| *c)
        }
    }

    impl CheckpointFetcher for MockFetcher {
        async fn fetch_checkpoint(
            &self,
            seq: u64,
            _mask: &FieldMask,
        ) -> anyhow::Result<Option<ProtoCheckpoint>> {
            let (behavior, calls) = {
                let mut state = self.state.lock().unwrap();
                let entry = state
                    .get_mut(&seq)
                    .unwrap_or_else(|| panic!("MockFetcher: unconfigured key {seq}"));
                entry.1 += 1;
                (entry.0.clone(), entry.1)
            };

            match behavior {
                FetcherBehavior::Success => Ok(Some(make_test_proto_checkpoint(seq))),
                FetcherBehavior::ErrorThenSuccess(n) => {
                    if calls <= n {
                        Err(anyhow!("simulated transient error for cp {seq}"))
                    } else {
                        Ok(Some(make_test_proto_checkpoint(seq)))
                    }
                }
                FetcherBehavior::NoneThenSuccess(n) => {
                    if calls <= n {
                        Ok(None)
                    } else {
                        Ok(Some(make_test_proto_checkpoint(seq)))
                    }
                }
            }
        }
    }

    fn fetcher(setup: &[(u64, FetcherBehavior)]) -> MockFetcher {
        MockFetcher::new(setup.iter().cloned().collect::<HashMap<_, _>>())
    }

    fn recovery_watermarks(
        hi: u64,
    ) -> (
        watch::Sender<Arc<Watermarks>>,
        watch::Receiver<Arc<Watermarks>>,
    ) {
        watch::channel(Arc::new(Watermarks::for_test(&[
            (LEDGER_GRPC_PIPELINE, hi),
            (KV_PACKAGES_PIPELINE, hi),
        ])))
    }

    fn empty_mask() -> FieldMask {
        FieldMask::default()
    }

    fn drain_broadcast(receiver: &mut broadcast::Receiver<Arc<ProcessedCheckpoint>>) -> Vec<u64> {
        let mut received = Vec::new();
        while let Ok(cp) = receiver.try_recv() {
            received.push(cp.sequence_number);
        }
        received
    }

    // --- fetch_one_with_retry ---

    #[tokio::test]
    async fn fetch_one_with_retry_succeeds_first_try() {
        let mock = fetcher(&[(42, FetcherBehavior::Success)]);
        let cp = fetch_one_with_retry(&mock, &empty_mask(), 42)
            .await
            .unwrap();
        assert_eq!(cp.sequence_number, Some(42));
        assert_eq!(mock.calls_for(42), 1);
    }

    #[tokio::test]
    async fn fetch_one_with_retry_recovers_from_transient_errors() {
        let mock = fetcher(&[(42, FetcherBehavior::ErrorThenSuccess(3))]);
        let cp = fetch_one_with_retry(&mock, &empty_mask(), 42)
            .await
            .unwrap();
        assert_eq!(cp.sequence_number, Some(42));
        assert_eq!(mock.calls_for(42), 4);
    }

    #[tokio::test]
    async fn fetch_one_with_retry_recovers_from_not_available() {
        let mock = fetcher(&[(42, FetcherBehavior::NoneThenSuccess(2))]);
        let cp = fetch_one_with_retry(&mock, &empty_mask(), 42)
            .await
            .unwrap();
        assert_eq!(cp.sequence_number, Some(42));
        assert_eq!(mock.calls_for(42), 3);
    }

    // --- fetch_chunk ---

    #[tokio::test]
    async fn fetch_chunk_all_succeed() {
        let mock = fetcher(&[
            (1, FetcherBehavior::Success),
            (2, FetcherBehavior::Success),
            (3, FetcherBehavior::Success),
        ]);
        let chunk = fetch_chunk(&mock, &empty_mask(), 1..=3).await.unwrap();
        assert_eq!(
            chunk.iter().map(|c| c.sequence_number).collect::<Vec<_>>(),
            vec![Some(1), Some(2), Some(3)],
        );
        for seq in 1..=3 {
            assert_eq!(mock.calls_for(seq), 1);
        }
    }

    #[tokio::test]
    async fn fetch_chunk_failing_call_does_not_refetch_successful_ones() {
        let mock = fetcher(&[
            (1, FetcherBehavior::Success),
            (2, FetcherBehavior::ErrorThenSuccess(2)),
            (3, FetcherBehavior::Success),
        ]);
        let chunk = fetch_chunk(&mock, &empty_mask(), 1..=3).await.unwrap();
        assert_eq!(chunk.len(), 3);
        assert_eq!(mock.calls_for(1), 1);
        assert_eq!(mock.calls_for(3), 1);
        assert_eq!(mock.calls_for(2), 3);
    }

    // --- recover_gap with watermark progression ---

    #[tokio::test]
    async fn recover_gap_lo_greater_than_hi_returns_immediately() {
        let mock = fetcher(&[]);
        let (_tx, rx) = recovery_watermarks(0);
        let (sender, _rx) = broadcast::channel(16);
        recover_gap(&mock, &rx, &sender, 5, 4, 10).await.unwrap();
        // No keys configured; if anything was fetched, MockFetcher would panic.
    }

    #[tokio::test]
    async fn recover_gap_progresses_chunk_by_chunk_with_watermark() {
        let mock = fetcher(&[
            (1, FetcherBehavior::Success),
            (2, FetcherBehavior::Success),
            (3, FetcherBehavior::Success),
            (4, FetcherBehavior::Success),
            (5, FetcherBehavior::Success),
            (6, FetcherBehavior::Success),
        ]);
        // Initial watermark below the first chunk's hi (3), so recover_gap must wait.
        let (tx, rx) = recovery_watermarks(0);
        let (sender, mut receiver) = broadcast::channel(16);

        let mock_arc = Arc::new(mock);
        let mock_for_task = mock_arc.clone();
        let task =
            tokio::spawn(async move { recover_gap(&*mock_for_task, &rx, &sender, 1, 6, 3).await });

        // Should not progress while watermark is at 0. 200ms is comfortably more than the
        // task scheduling overhead so the spawned `recover_gap` reaches its watermark wait.
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(drain_broadcast(&mut receiver), Vec::<u64>::new());
        assert_eq!(mock_arc.calls_for(1), 0, "first chunk not yet fetched");

        // Advance to 3: first chunk completes (1, 2, 3).
        tx.send(Arc::new(Watermarks::for_test(&[
            (LEDGER_GRPC_PIPELINE, 3),
            (KV_PACKAGES_PIPELINE, 3),
        ])))
        .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(drain_broadcast(&mut receiver), vec![1, 2, 3]);
        assert_eq!(
            mock_arc.calls_for(4),
            0,
            "second chunk waiting on watermark"
        );

        // Advance to 6: second chunk completes (4, 5, 6) and recover_gap returns.
        tx.send(Arc::new(Watermarks::for_test(&[
            (LEDGER_GRPC_PIPELINE, 6),
            (KV_PACKAGES_PIPELINE, 6),
        ])))
        .unwrap();
        timeout(Duration::from_secs(1), task)
            .await
            .expect("recover_gap did not finish")
            .unwrap()
            .unwrap();
        assert_eq!(drain_broadcast(&mut receiver), vec![4, 5, 6]);
    }

    #[tokio::test]
    async fn recover_gap_waits_for_both_pipelines() {
        let mock = fetcher(&[
            (1, FetcherBehavior::Success),
            (2, FetcherBehavior::Success),
            (3, FetcherBehavior::Success),
        ]);
        let (tx, rx) = recovery_watermarks(0);
        let (sender, mut receiver) = broadcast::channel(16);

        let mock_arc = Arc::new(mock);
        let mock_for_task = mock_arc.clone();
        let task =
            tokio::spawn(async move { recover_gap(&*mock_for_task, &rx, &sender, 1, 3, 3).await });

        // ledger_grpc has caught up but kv_packages is still at 0. Recovery must
        // not progress because subscribers would resolve packages from a DB that
        // hasn't indexed them yet.
        tx.send(Arc::new(Watermarks::for_test(&[
            (LEDGER_GRPC_PIPELINE, 3),
            (KV_PACKAGES_PIPELINE, 0),
        ])))
        .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(drain_broadcast(&mut receiver), Vec::<u64>::new());
        assert_eq!(mock_arc.calls_for(1), 0, "fetch waiting on kv_packages");

        // Advance kv_packages: both pipelines caught up, recovery completes.
        tx.send(Arc::new(Watermarks::for_test(&[
            (LEDGER_GRPC_PIPELINE, 3),
            (KV_PACKAGES_PIPELINE, 3),
        ])))
        .unwrap();
        timeout(Duration::from_secs(1), task)
            .await
            .expect("recover_gap did not finish")
            .unwrap()
            .unwrap();
        assert_eq!(drain_broadcast(&mut receiver), vec![1, 2, 3]);
    }
}
