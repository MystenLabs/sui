// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::sync::DropGuard;

use crate::RpcError;
use crate::metrics::RpcMetrics;

/// How a query stream ended, plus the exclusive range boundary the scan
/// reached. The boundary `(end_checkpoint, end_position)` lets the caller build
/// the terminal progress watermark when the scan completed naturally.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ChunkTerminal {
    pub(crate) reason: QueryEndReason,
    pub(crate) end_checkpoint: u64,
    pub(crate) end_position: u64,
}

pub(crate) struct ScanChunkDone<State, Item> {
    /// Frames to yield: rendered item frames plus, only when the chunk produced
    /// no items, at most one scan watermark frame.
    pub(crate) items: Vec<Item>,
    /// Count of real item frames in `items` (excludes any scan watermark).
    /// Drives the request item limit, not `items.len()`.
    pub(crate) produced: usize,
    pub(crate) next_state: Option<State>,
    pub(crate) terminal: ChunkTerminal,
    pub(crate) remaining_scan_budget: usize,
}

pub(crate) struct ScanChunkResult<State, Item> {
    pub(crate) result: Result<ScanChunkDone<State, Item>, RpcError>,
    pub(crate) blocking_queue_wait: Duration,
    pub(crate) blocking_work: Duration,
}

impl<State, Item> ScanChunkResult<State, Item> {
    pub(crate) fn new(
        result: Result<ScanChunkDone<State, Item>, RpcError>,
        blocking_queue_wait: Duration,
        blocking_work: Duration,
    ) -> Self {
        Self {
            result,
            blocking_queue_wait,
            blocking_work,
        }
    }
}

pub(crate) struct ChunkArgs {
    pub(crate) scan_budget: usize,
    pub(crate) chunk_item_limit: usize,
    pub(crate) remaining_request_item_limit: usize,
    pub(crate) cancel: CancellationToken,
}

pub(super) fn cancelled() -> RpcError {
    RpcError::new(tonic::Code::Cancelled, "request cancelled")
}

/// Bridges the async gRPC stream to blocking RocksDB reads.
///
/// Each request has at most one blocking chunk worker in flight. A worker builds
/// a whole chunk and returns it as a `Vec`, so the blocking task is never parked
/// behind gRPC stream backpressure. Once a chunk completes, the next worker is
/// scheduled before the async stream drains the completed chunk's buffered
/// items, letting slow clients delay response delivery without holding a Rocks
/// iterator or blocking thread open.
pub(crate) struct ChunkedScan<State, Item, Spawn>
where
    State: Send + 'static,
    Item: Send + 'static,
    Spawn: FnMut(State, ChunkArgs) -> JoinHandle<ScanChunkResult<State, Item>> + Send + 'static,
{
    current: Option<JoinHandle<ScanChunkResult<State, Item>>>,
    buffered: VecDeque<Item>,
    spawn: Spawn,
    produced: usize,
    scan_budget: usize,
    terminal: Option<ChunkTerminal>,
    chunks: usize,
    blocking_queue_wait: Duration,
    blocking_work: Duration,
    limit_items: usize,
    chunk_max: usize,
    cancel: CancellationToken,
    _cancel_guard: DropGuard,
}

impl<State, Item, Spawn> ChunkedScan<State, Item, Spawn>
where
    State: Send + 'static,
    Item: Send + 'static,
    Spawn: FnMut(State, ChunkArgs) -> JoinHandle<ScanChunkResult<State, Item>> + Send + 'static,
{
    pub(crate) fn new(
        initial_state: State,
        limit_items: usize,
        chunk_max: usize,
        scan_budget: usize,
        mut spawn: Spawn,
    ) -> Self {
        assert!(limit_items > 0, "chunked scan limit must be nonzero");
        assert!(chunk_max > 0, "chunked scan chunk size must be nonzero");
        let initial_chunk_item_limit = chunk_max.min(limit_items);
        let cancel = CancellationToken::new();
        let current = Some(spawn(
            initial_state,
            ChunkArgs {
                scan_budget,
                chunk_item_limit: initial_chunk_item_limit,
                remaining_request_item_limit: limit_items,
                cancel: cancel.clone(),
            },
        ));
        let _cancel_guard = cancel.clone().drop_guard();
        Self {
            current,
            buffered: VecDeque::new(),
            spawn,
            produced: 0,
            scan_budget,
            terminal: None,
            chunks: 0,
            blocking_queue_wait: Duration::ZERO,
            blocking_work: Duration::ZERO,
            limit_items,
            chunk_max,
            cancel,
            _cancel_guard,
        }
    }

    pub(crate) async fn next_item(&mut self) -> Result<Option<Item>, RpcError> {
        loop {
            if let Some(item) = self.buffered.pop_front() {
                return Ok(Some(item));
            }

            let Some(chunk) = self.current.take() else {
                return Ok(None);
            };

            let done = chunk
                .await
                .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
            self.chunks = self.chunks.saturating_add(1);
            self.blocking_queue_wait += done.blocking_queue_wait;
            self.blocking_work += done.blocking_work;
            let done = done.result?;
            self.apply_done(done);
        }
    }

    pub(crate) fn into_terminal(self) -> Option<ChunkTerminal> {
        self.terminal
    }

    /// Total real items produced across all chunks (excludes watermark frames).
    pub(crate) fn produced(&self) -> usize {
        self.produced
    }

    pub(crate) fn chunks(&self) -> usize {
        self.chunks
    }

    pub(crate) fn blocking_queue_wait(&self) -> Duration {
        self.blocking_queue_wait
    }

    pub(crate) fn blocking_work(&self) -> Duration {
        self.blocking_work
    }

    #[cfg(test)]
    pub(super) fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    fn apply_done(&mut self, done: ScanChunkDone<State, Item>) {
        self.terminal = Some(done.terminal);
        self.scan_budget = done.remaining_scan_budget;
        self.produced = self.produced.saturating_add(done.produced);
        self.buffered = done.items.into();
        if self.produced < self.limit_items
            && let Some(state) = done.next_state
        {
            let remaining_request_item_limit = self.limit_items - self.produced;
            let chunk_item_limit = self.chunk_max.min(remaining_request_item_limit);
            self.current = Some((self.spawn)(
                state,
                ChunkArgs {
                    scan_budget: self.scan_budget,
                    chunk_item_limit,
                    remaining_request_item_limit,
                    cancel: self.cancel.clone(),
                },
            ));
        }
    }
}

pub(crate) struct ListRequestMetrics {
    metrics: Option<Arc<RpcMetrics>>,
    method: &'static str,
    started: Instant,
    chunks: usize,
    items_emitted: usize,
    blocking_queue_wait: Duration,
    blocking_work: Duration,
    recorded: bool,
}

impl ListRequestMetrics {
    pub(crate) fn new(metrics: Option<Arc<RpcMetrics>>, method: &'static str) -> Self {
        Self {
            metrics,
            method,
            started: Instant::now(),
            chunks: 0,
            items_emitted: 0,
            blocking_queue_wait: Duration::ZERO,
            blocking_work: Duration::ZERO,
            recorded: false,
        }
    }

    pub(crate) fn observe_construction_error(
        metrics: Option<Arc<RpcMetrics>>,
        method: &'static str,
        started: Instant,
        error: &RpcError,
    ) {
        let mut request_metrics = Self {
            metrics,
            method,
            started,
            chunks: 0,
            items_emitted: 0,
            blocking_queue_wait: Duration::ZERO,
            blocking_work: Duration::ZERO,
            recorded: false,
        };
        request_metrics.record(rpc_error_outcome(error), "none");
    }

    pub(crate) fn update(
        &mut self,
        chunks: usize,
        items_emitted: usize,
        blocking_queue_wait: Duration,
        blocking_work: Duration,
    ) {
        self.chunks = chunks;
        self.items_emitted = items_emitted;
        self.blocking_queue_wait = blocking_queue_wait;
        self.blocking_work = blocking_work;
    }

    pub(crate) fn finish_ok(&mut self, end_reason: QueryEndReason) {
        self.record("ok", query_end_reason_label(end_reason));
    }

    pub(crate) fn finish_error(&mut self, error: &RpcError) {
        self.record(rpc_error_outcome(error), "none");
    }

    fn record(&mut self, outcome: &'static str, end_reason: &'static str) {
        if self.recorded {
            return;
        }
        self.recorded = true;
        let Some(metrics) = &self.metrics else {
            return;
        };

        let elapsed = self.started.elapsed();
        let accounted = self.blocking_queue_wait + self.blocking_work;
        let unaccounted = elapsed.saturating_sub(accounted);

        metrics
            .list_request_seconds
            .with_label_values(&[self.method])
            .observe(elapsed.as_secs_f64());
        metrics
            .list_request_chunks
            .with_label_values(&[self.method])
            .observe(self.chunks as f64);
        metrics
            .list_request_items
            .with_label_values(&[self.method])
            .observe(self.items_emitted as f64);
        metrics
            .list_request_blocking_queue_wait_seconds
            .with_label_values(&[self.method])
            .observe(self.blocking_queue_wait.as_secs_f64());
        metrics
            .list_request_blocking_work_seconds
            .with_label_values(&[self.method])
            .observe(self.blocking_work.as_secs_f64());
        metrics
            .list_request_unaccounted_seconds
            .with_label_values(&[self.method])
            .observe(unaccounted.as_secs_f64());
        metrics
            .list_request_outcomes
            .with_label_values(&[self.method, outcome, end_reason])
            .inc();
    }
}

impl Drop for ListRequestMetrics {
    fn drop(&mut self) {
        self.record("dropped", "none");
    }
}

fn query_end_reason_label(reason: QueryEndReason) -> &'static str {
    match reason {
        QueryEndReason::Unspecified => "unspecified",
        QueryEndReason::ItemLimit => "item_limit",
        QueryEndReason::ScanLimit => "scan_limit",
        QueryEndReason::LedgerTip => "ledger_tip",
        QueryEndReason::CheckpointBound => "checkpoint_bound",
        QueryEndReason::CursorBound => "cursor_bound",
        _ => "unknown",
    }
}

fn rpc_error_outcome(error: &RpcError) -> &'static str {
    match error.code() {
        tonic::Code::Ok => "ok",
        tonic::Code::Cancelled => "cancelled",
        tonic::Code::Unknown => "unknown",
        tonic::Code::InvalidArgument => "invalid_argument",
        tonic::Code::DeadlineExceeded => "deadline",
        tonic::Code::NotFound => "not_found",
        tonic::Code::AlreadyExists => "already_exists",
        tonic::Code::PermissionDenied => "permission_denied",
        tonic::Code::ResourceExhausted => "resource_exhausted",
        tonic::Code::FailedPrecondition => "failed_precondition",
        tonic::Code::Aborted => "aborted",
        tonic::Code::OutOfRange => "out_of_range",
        tonic::Code::Unimplemented => "unimplemented",
        tonic::Code::Internal => "internal",
        tonic::Code::Unavailable => "unavailable",
        tonic::Code::DataLoss => "data_loss",
        tonic::Code::Unauthenticated => "unauthenticated",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex;

    use super::*;

    #[tokio::test]
    async fn chunked_scan_drains_vector_chunks_in_order() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let spawn_calls = calls.clone();
        let mut scan = ChunkedScan::new(0usize, 5, 2, 10, move |state, args: ChunkArgs| {
            spawn_calls.lock().unwrap().push((
                state,
                args.scan_budget,
                args.chunk_item_limit,
                args.remaining_request_item_limit,
            ));
            let scan_budget = args.scan_budget;
            let _ = args.cancel;
            tokio::task::spawn_blocking(move || {
                let items = match state {
                    0 => vec![0, 1],
                    1 => vec![10, 11],
                    2 => vec![20],
                    _ => Vec::new(),
                };
                ScanChunkResult::new(
                    Ok(ScanChunkDone {
                        produced: items.len(),
                        items,
                        next_state: (state < 2).then_some(state + 1),
                        terminal: ChunkTerminal {
                            reason: QueryEndReason::CheckpointBound,
                            end_checkpoint: 0,
                            end_position: 0,
                        },
                        remaining_scan_budget: scan_budget - 1,
                    }),
                    Duration::from_millis(1),
                    Duration::from_millis(2),
                )
            })
        });

        let mut items = Vec::new();
        while let Some(item) = scan.next_item().await.unwrap() {
            items.push(item);
        }

        assert_eq!(items, vec![0, 1, 10, 11, 20]);
        assert_eq!(
            scan.into_terminal(),
            Some(ChunkTerminal {
                reason: QueryEndReason::CheckpointBound,
                end_checkpoint: 0,
                end_position: 0,
            })
        );
        assert_eq!(
            *calls.lock().unwrap(),
            vec![(0, 10, 2, 5), (1, 9, 2, 3), (2, 8, 1, 1)]
        );
    }

    #[tokio::test]
    async fn chunked_scan_drop_cancels_token_seen_by_workers() {
        let captured: Arc<Mutex<Option<CancellationToken>>> = Arc::new(Mutex::new(None));
        let captured_for_spawn = captured.clone();
        let scan = ChunkedScan::new(0usize, 5, 2, 10, move |_state, args: ChunkArgs| {
            *captured_for_spawn.lock().unwrap() = Some(args.cancel.clone());
            tokio::task::spawn_blocking(move || {
                ScanChunkResult::new(
                    Ok(ScanChunkDone::<usize, usize> {
                        items: Vec::new(),
                        produced: 0,
                        next_state: None,
                        terminal: ChunkTerminal {
                            reason: QueryEndReason::CheckpointBound,
                            end_checkpoint: 0,
                            end_position: 0,
                        },
                        remaining_scan_budget: args.scan_budget,
                    }),
                    Duration::ZERO,
                    Duration::ZERO,
                )
            })
        });

        let token = captured.lock().unwrap().clone().expect("token captured");
        assert!(!token.is_cancelled());
        drop(scan);
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn chunked_scan_worker_observes_cancel_and_returns_cancelled_error() {
        let mut scan = ChunkedScan::new(0usize, 5, 2, 10, move |_state, args: ChunkArgs| {
            tokio::task::spawn_blocking(move || -> ScanChunkResult<usize, usize> {
                while !args.cancel.is_cancelled() {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                ScanChunkResult::new(Err(cancelled()), Duration::ZERO, Duration::ZERO)
            })
        });

        scan.cancel_token().cancel();
        let err = scan
            .next_item()
            .await
            .expect_err("expected cancelled error");
        let status = tonic::Status::from(err);
        assert_eq!(status.code(), tonic::Code::Cancelled);
    }
}
