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

/// Recover the gap `lo..=hi_inclusive` by reading checkpoints from kv-rpc, processing them
/// through the existing streaming pipeline, and broadcasting in order. Each chunk waits for the
/// kv-rpc indexer to reach the chunk's upper bound before fetching, so partial progress is
/// broadcast as the indexer catches up rather than waiting for the entire gap to be available.
pub(crate) async fn recover_gap<F: CheckpointFetcher>(
    fetcher: &F,
    watermarks_rx: &watch::Receiver<Arc<Watermarks>>,
    sender: &broadcast::Sender<Arc<ProcessedCheckpoint>>,
    lo: u64,
    hi_inclusive: u64,
    chunk_size: usize,
) -> anyhow::Result<()> {
    if lo > hi_inclusive {
        return Ok(());
    }

    let mask = checkpoint_field_mask();
    let chunk_size = chunk_size.max(1) as u64;
    let mut cursor = lo;
    let mut watermarks_rx = watermarks_rx.clone();

    while cursor <= hi_inclusive {
        let chunk_hi_inclusive = (cursor + chunk_size - 1).min(hi_inclusive);

        wait_for_pipelines_catching_up_at(chunk_hi_inclusive, &mut watermarks_rx).await?;

        let processed = fetch_and_process(fetcher, &mask, cursor..=chunk_hi_inclusive).await?;
        for cp in processed {
            // Ignore send errors: no active subscribers is a normal state during recovery.
            let _ = sender.send(cp);
        }

        cursor = chunk_hi_inclusive + 1;
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
    watermarks_rx: &mut watch::Receiver<Arc<Watermarks>>,
) -> anyhow::Result<()> {
    watermarks_rx
        .wait_for(|w| {
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

/// Fetch every checkpoint in `range` from kv-rpc in parallel and parse each into a
/// `ProcessedCheckpoint`, returning them in input order.
pub(super) async fn fetch_and_process<F: CheckpointFetcher>(
    fetcher: &F,
    mask: &FieldMask,
    range: RangeInclusive<u64>,
) -> anyhow::Result<Vec<Arc<ProcessedCheckpoint>>> {
    fetch_chunk(fetcher, mask, range)
        .await?
        .into_iter()
        .map(|p| process_checkpoint(p).map(Arc::new))
        .collect()
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
pub(super) async fn fetch_one_with_retry<F: CheckpointFetcher>(
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
    use std::time::Duration;

    use tokio::time::timeout;

    use super::*;
    use crate::task::streaming::test_utils::FetcherBehavior;
    use crate::task::streaming::test_utils::MockFetcher;

    fn fetcher(setup: &[(u64, FetcherBehavior)]) -> MockFetcher {
        MockFetcher::from_setup(setup)
    }

    fn recovery_watermarks(
        hi_inclusive: u64,
    ) -> (
        watch::Sender<Arc<Watermarks>>,
        watch::Receiver<Arc<Watermarks>>,
    ) {
        watch::channel(Arc::new(Watermarks::for_test(&[
            (LEDGER_GRPC_PIPELINE, hi_inclusive),
            (KV_PACKAGES_PIPELINE, hi_inclusive),
        ])))
    }

    fn empty_mask() -> FieldMask {
        FieldMask::default()
    }

    fn drain_broadcast(receiver: &mut broadcast::Receiver<Arc<ProcessedCheckpoint>>) -> Vec<u64> {
        let mut received = Vec::new();
        while let Ok(cp) = receiver.try_recv() {
            received.push(cp.summary.sequence_number);
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
