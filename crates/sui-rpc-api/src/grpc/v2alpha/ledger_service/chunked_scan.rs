// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;

use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc_cursor::Position;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::sync::DropGuard;

use crate::RpcError;
use crate::ledger_history::query_options::RangeExhaustion;
use crate::ledger_history::watermark::ScanTerminal;

/// Chunk terminal for a scan that either exhausted its request scan budget
/// (authoritative frontier watermark, built lazily) or ended at the resolved
/// range boundary.
pub(crate) fn scan_limit_or_range<Frontier>(
    request_scan_limit_reached: bool,
    exhaustion: RangeExhaustion,
    position: Position,
    frontier_watermark: Frontier,
) -> Result<ScanTerminal, RpcError>
where
    Frontier: FnOnce() -> Result<Watermark, RpcError>,
{
    if request_scan_limit_reached {
        Ok(ScanTerminal::ScanLimit {
            watermark: frontier_watermark()?,
        })
    } else {
        Ok(ScanTerminal::Range {
            exhaustion,
            position,
        })
    }
}

pub(crate) struct ScanChunkDone<State, Item> {
    /// Frames to yield: rendered item frames plus, only when the chunk produced
    /// no items, at most one scan watermark frame.
    pub(crate) items: Vec<Item>,
    /// Count of real item frames in `items` (excludes any scan watermark).
    /// Drives the request item limit, not `items.len()`.
    pub(crate) produced: usize,
    pub(crate) next_state: Option<State>,
    pub(crate) terminal: ScanTerminal,
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
    Spawn: FnMut(State, ChunkArgs) -> JoinHandle<Result<ScanChunkDone<State, Item>, RpcError>>
        + Send
        + 'static,
{
    current: Option<JoinHandle<Result<ScanChunkDone<State, Item>, RpcError>>>,
    buffered: VecDeque<Item>,
    spawn: Spawn,
    produced: usize,
    scan_budget: usize,
    terminal: Option<ScanTerminal>,
    limit_items: usize,
    chunk_max: usize,
    cancel: CancellationToken,
    _cancel_guard: DropGuard,
}

impl<State, Item, Spawn> ChunkedScan<State, Item, Spawn>
where
    State: Send + 'static,
    Item: Send + 'static,
    Spawn: FnMut(State, ChunkArgs) -> JoinHandle<Result<ScanChunkDone<State, Item>, RpcError>>
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

            let done = chunk
                .await
                .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))??;
            self.apply_done(done);
        }
    }

    pub(crate) fn into_terminal(self) -> Option<ScanTerminal> {
        self.terminal
    }

    /// Total real items produced across all chunks (excludes watermark frames).
    pub(crate) fn produced(&self) -> usize {
        self.produced
    }

    /// True when no buffered frame remains and no further chunk is in flight —
    /// the next `next_item()` call would return `None`.
    pub(crate) fn exhausted(&self) -> bool {
        self.buffered.is_empty() && self.current.is_none()
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
                Ok(ScanChunkDone {
                    produced: items.len(),
                    items,
                    next_state: (state < 2).then_some(state + 1),
                    terminal: ScanTerminal::Range {
                        exhaustion: RangeExhaustion::CheckpointBound,
                        position: Position::Transactions {
                            checkpoint: 0,
                            tx_seq: 0,
                        },
                    },
                    remaining_scan_budget: scan_budget - 1,
                })
            })
        });

        let mut items = Vec::new();
        while let Some(item) = scan.next_item().await.unwrap() {
            items.push(item);
        }

        assert_eq!(items, vec![0, 1, 10, 11, 20]);
        assert_eq!(
            scan.into_terminal(),
            Some(ScanTerminal::Range {
                exhaustion: RangeExhaustion::CheckpointBound,
                position: Position::Transactions {
                    checkpoint: 0,
                    tx_seq: 0
                },
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
                Ok(ScanChunkDone::<usize, usize> {
                    items: Vec::new(),
                    produced: 0,
                    next_state: None,
                    terminal: ScanTerminal::Range {
                        exhaustion: RangeExhaustion::CheckpointBound,
                        position: Position::Transactions {
                            checkpoint: 0,
                            tx_seq: 0,
                        },
                    },
                    remaining_scan_budget: args.scan_budget,
                })
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
            tokio::task::spawn_blocking(move || -> Result<ScanChunkDone<usize, usize>, RpcError> {
                while !args.cancel.is_cancelled() {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(cancelled())
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

    #[tokio::test]
    async fn limit_reaching_final_item_exhausts_without_scheduling_another_chunk() {
        use std::sync::atomic::AtomicUsize;
        use std::sync::atomic::Ordering;

        let spawn_count = Arc::new(AtomicUsize::new(0));
        let spawn_count_for_worker = spawn_count.clone();
        let mut scan = ChunkedScan::new(0usize, 2, 2, 10, move |_state, args: ChunkArgs| {
            spawn_count_for_worker.fetch_add(1, Ordering::SeqCst);
            tokio::task::spawn_blocking(move || {
                Ok(ScanChunkDone {
                    items: vec!["first", "final"],
                    produced: 2,
                    next_state: Some(1),
                    terminal: ScanTerminal::ScanLimit {
                        watermark: {
                            let mut watermark = Watermark::default();
                            watermark.cursor = Some(b"scan-frontier".to_vec().into());
                            watermark
                        },
                    },
                    remaining_scan_budget: args.scan_budget - 1,
                })
            })
        });

        assert_eq!(scan.next_item().await.unwrap(), Some("first"));
        assert_eq!(scan.produced(), 2);
        assert!(!scan.exhausted(), "the final item is still buffered");

        assert_eq!(scan.next_item().await.unwrap(), Some("final"));
        assert_eq!(scan.produced(), 2);
        assert!(
            scan.exhausted(),
            "the item limit leaves no trailing frame or chunk in flight"
        );
        assert_eq!(scan.next_item().await.unwrap(), None);
        let terminal = scan.into_terminal().expect("worker terminal is retained");
        let ScanTerminal::ScanLimit { watermark } = terminal else {
            panic!("expected scan-limit terminal");
        };
        assert!(
            watermark.cursor.is_some(),
            "scan-limit terminal must own a resume cursor"
        );
        assert_eq!(
            spawn_count.load(Ordering::SeqCst),
            1,
            "next_state must not schedule another chunk at the item limit"
        );
    }
}
