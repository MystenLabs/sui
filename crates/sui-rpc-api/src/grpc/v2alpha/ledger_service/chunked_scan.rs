// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;

use futures::future::BoxFuture;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use tokio_util::sync::CancellationToken;
use tokio_util::sync::DropGuard;

use crate::RpcError;

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
    Spawn: FnMut(State, ChunkArgs) -> BoxFuture<'static, Result<ScanChunkDone<State, Item>, RpcError>>
        + Send
        + 'static,
{
    current: Option<BoxFuture<'static, Result<ScanChunkDone<State, Item>, RpcError>>>,
    buffered: VecDeque<Item>,
    spawn: Spawn,
    produced: usize,
    scan_budget: usize,
    terminal: Option<ChunkTerminal>,
    limit_items: usize,
    chunk_max: usize,
    cancel: CancellationToken,
    _cancel_guard: DropGuard,
}

impl<State, Item, Spawn> ChunkedScan<State, Item, Spawn>
where
    State: Send + 'static,
    Item: Send + 'static,
    Spawn: FnMut(State, ChunkArgs) -> BoxFuture<'static, Result<ScanChunkDone<State, Item>, RpcError>>
        + Send
        + 'static,
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

            let done = chunk.await?;
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex;

    use super::*;
    use futures::FutureExt;

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
            async move {
                let items = match state {
                    0 => vec![0, 1],
                    1 => vec![10, 11],
                    2 => vec![20],
                    _ => Vec::new(),
                };
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
                })
            }
            .boxed()
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
            async move {
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
                })
            }
            .boxed()
        });

        let token = captured.lock().unwrap().clone().expect("token captured");
        assert!(!token.is_cancelled());
        drop(scan);
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn chunked_scan_worker_observes_cancel_and_returns_cancelled_error() {
        let mut scan = ChunkedScan::new(0usize, 5, 2, 10, move |_state, args: ChunkArgs| {
            let handle = tokio::task::spawn_blocking(
                move || -> Result<ScanChunkDone<usize, usize>, RpcError> {
                    while !args.cancel.is_cancelled() {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(cancelled())
                },
            );
            async move { handle.await.expect("join") }.boxed()
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
