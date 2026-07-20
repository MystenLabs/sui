// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use std::{
    cell::RefCell,
    fs::File,
    os::fd::AsRawFd,
    sync::{
        Once,
        atomic::{AtomicBool, Ordering},
    },
};

use sui_rpc::proto::sui::rpc::v2::Watermark;
use sui_rpc_cursor::Position;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::sync::DropGuard;

use crate::RpcError;
use crate::ledger_history::query_options::RangeExhaustion;
use crate::ledger_history::watermark::ScanTerminal;
use crate::metrics::{ListChunkSetupTimer, ListStreamMetrics};

/// Chunk terminal for a scan that either exhausted its request scan budget
/// (authoritative frontier watermark, built lazily) or ended at the resolved
/// range boundary. Chunks only run over nonempty intervals, so the range
/// terminal is never `interval_empty`.
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
        Ok(ScanTerminal::from_range_exhaustion(
            exhaustion, position, false,
        ))
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
#[cfg(unix)]
fn thread_cpu_time() -> Option<Duration> {
    let mut value = std::mem::MaybeUninit::<libc::timespec>::uninit();
    // SAFETY: `clock_gettime` initializes `value` on success.
    if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, value.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: the successful call above initialized the timespec.
    let value = unsafe { value.assume_init() };
    let seconds = u64::try_from(value.tv_sec).ok()?;
    let nanoseconds = u32::try_from(value.tv_nsec).ok()?;
    (nanoseconds < 1_000_000_000).then(|| Duration::new(seconds, nanoseconds))
}

#[cfg(not(unix))]
fn thread_cpu_time() -> Option<Duration> {
    None
}

#[cfg(target_os = "linux")]
static SCHEDSTAT_AVAILABLE: AtomicBool = AtomicBool::new(true);
#[cfg(target_os = "linux")]
static SCHEDSTAT_BENCHMARK: Once = Once::new();

#[cfg(target_os = "linux")]
struct ThreadSchedstat {
    chunks_seen: u64,
    file: Option<File>,
}

#[cfg(target_os = "linux")]
thread_local! {
    static THREAD_SCHEDSTAT: RefCell<ThreadSchedstat> = RefCell::new(ThreadSchedstat {
        chunks_seen: 0,
        file: None,
    });
}

#[cfg(target_os = "linux")]
fn disable_schedstat() {
    SCHEDSTAT_AVAILABLE.store(false, Ordering::Relaxed);
}

#[cfg(target_os = "linux")]
fn parse_schedstat(bytes: &[u8]) -> Option<(u64, u64)> {
    let mut values = [0_u64; 2];
    let mut offset = 0;
    for value in &mut values {
        while offset < bytes.len() && bytes[offset].is_ascii_whitespace() {
            offset += 1;
        }
        let start = offset;
        while offset < bytes.len() && bytes[offset].is_ascii_digit() {
            *value = value
                .checked_mul(10)?
                .checked_add(u64::from(bytes[offset] - b'0'))?;
            offset += 1;
        }
        if offset == start {
            return None;
        }
    }
    Some((values[0], values[1]))
}

#[cfg(target_os = "linux")]
fn read_schedstat(file: &File) -> Option<u64> {
    let mut buffer = [0_u8; 128];
    // SAFETY: `buffer` is writable for its full length and `file` remains open
    // for the duration of the positional read.
    let bytes_read = unsafe {
        libc::pread(
            file.as_raw_fd(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            0,
        )
    };
    if bytes_read <= 0 {
        return None;
    }
    let (runtime_ns, run_delay_ns) = parse_schedstat(&buffer[..bytes_read as usize])?;
    // A completely zero record cannot provide a meaningful delta.
    (runtime_ns != 0 || run_delay_ns != 0).then_some(run_delay_ns)
}

#[cfg(target_os = "linux")]
fn kernel_schedstats_enabled() -> bool {
    let Ok(file) = File::open("/proc/sys/kernel/sched_schedstats") else {
        return false;
    };
    let mut buffer = [0_u8; 8];
    // SAFETY: `buffer` is writable for its full length and `file` remains open
    // for the duration of the positional read.
    let bytes_read = unsafe {
        libc::pread(
            file.as_raw_fd(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            0,
        )
    };
    bytes_read > 0
        && buffer[..bytes_read as usize]
            .iter()
            .find(|byte| !byte.is_ascii_whitespace())
            .is_some_and(|byte| *byte == b'1')
}

#[cfg(target_os = "linux")]
fn benchmark_schedstat_probe(file: &File) {
    SCHEDSTAT_BENCHMARK.call_once(|| {
        if !kernel_schedstats_enabled() {
            disable_schedstat();
            return;
        }
        const PROBES: u128 = 100;
        let started = Instant::now();
        for _ in 0..PROBES {
            if read_schedstat(file).is_none() {
                disable_schedstat();
                return;
            }
        }
        tracing::info!(
            schedstat_probe_ns = started.elapsed().as_nanos() / PROBES,
            "List chunk schedstat probe cost"
        );
    });
}

#[cfg(target_os = "linux")]
fn start_schedstat_sample(sample_every: u64) -> Option<u64> {
    if !SCHEDSTAT_AVAILABLE.load(Ordering::Relaxed) {
        return None;
    }
    THREAD_SCHEDSTAT.with(|thread_state| {
        let mut thread_state = thread_state.borrow_mut();
        let chunk_index = thread_state.chunks_seen;
        thread_state.chunks_seen = thread_state.chunks_seen.wrapping_add(1);
        if chunk_index % sample_every != 0 {
            return None;
        }
        if thread_state.file.is_none() {
            // `/proc/thread-self` binds to this blocking thread when opened.
            thread_state.file = File::open("/proc/thread-self/schedstat").ok();
            if thread_state.file.is_none() {
                disable_schedstat();
                return None;
            }
        }
        let file = thread_state.file.as_ref().expect("checked above");
        benchmark_schedstat_probe(file);
        if !SCHEDSTAT_AVAILABLE.load(Ordering::Relaxed) {
            return None;
        }
        read_schedstat(file).or_else(|| {
            disable_schedstat();
            None
        })
    })
}

#[cfg(target_os = "linux")]
fn finish_schedstat_sample() -> Option<u64> {
    THREAD_SCHEDSTAT.with(|thread_state| {
        let thread_state = thread_state.borrow();
        let file = thread_state.file.as_ref()?;
        read_schedstat(file).or_else(|| {
            disable_schedstat();
            None
        })
    })
}

#[cfg(not(target_os = "linux"))]
fn start_schedstat_sample(_sample_every: u64) -> Option<u64> {
    None
}

#[cfg(not(target_os = "linux"))]
fn finish_schedstat_sample() -> Option<u64> {
    None
}

pub(crate) fn spawn_list_chunk<T, F>(
    metrics: Option<ListStreamMetrics>,
    work: F,
) -> JoinHandle<Result<T, RpcError>>
where
    T: Send + 'static,
    F: FnOnce(Option<&ListStreamMetrics>, &mut ListChunkSetupTimer) -> Result<T, RpcError>
        + Send
        + 'static,
{
    match metrics {
        Some(metrics) => {
            let queue_timer = metrics.start_queue_timer();
            tokio::task::spawn_blocking(move || {
                queue_timer.stop_and_record();
                let run_delay_started = metrics
                    .schedstat_sample_every()
                    .and_then(start_schedstat_sample);
                // Each clock read is one syscall with no allocation or parsing,
                // about 0.001% of median chunk wall time. The load-test ladder
                // still includes an instrumentation-off control arm.
                let cpu_started = thread_cpu_time();
                let mut setup_timer = metrics.start_setup_timer();
                let work_started = Instant::now();
                let result = work(Some(&metrics), &mut setup_timer);
                let work_elapsed = work_started.elapsed();
                let cpu_elapsed =
                    cpu_started.and_then(|started| thread_cpu_time()?.checked_sub(started));
                let run_delay_elapsed = run_delay_started.and_then(|started| {
                    finish_schedstat_sample()
                        .map(|finished| Duration::from_nanos(finished.saturating_sub(started)))
                });

                metrics.observe_chunk_work(work_elapsed);
                if let Some(cpu_elapsed) = cpu_elapsed {
                    metrics.observe_chunk_work_cpu(cpu_elapsed);
                }
                if let Some(run_delay_elapsed) = run_delay_elapsed {
                    metrics.observe_chunk_run_delay(run_delay_elapsed);
                }
                result
            })
        }
        None => tokio::task::spawn_blocking(move || {
            work(None, &mut ListChunkSetupTimer::disabled())
        }),
    }
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
    initial_scan_budget: usize,
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
            initial_scan_budget: scan_budget,
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

    pub(crate) fn bitmap_buckets_evaluated(&self) -> usize {
        self.initial_scan_budget - self.scan_budget
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
    use crate::ledger_history::watermark::NaturalRangeEnd;
    use std::time::Instant;

    use prometheus::Registry;
    use sui_rpc::proto::sui::rpc::v2::QueryEndReason;

    use crate::metrics::{ListApiMetrics, ListRequestMetrics};

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
                    terminal: ScanTerminal::from_range_exhaustion(
                        RangeExhaustion::CheckpointBound,
                        Position::Transactions {
                            checkpoint: 0,
                            tx_seq: 0,
                        },
                        false,
                    ),
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
            Some(ScanTerminal::NaturalRange {
                end: NaturalRangeEnd::CheckpointBound,
                position: Position::Transactions {
                    checkpoint: 0,
                    tx_seq: 0,
                },
                interval_empty: false,
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
                    terminal: ScanTerminal::from_range_exhaustion(
                        RangeExhaustion::CheckpointBound,
                        Position::Transactions {
                            checkpoint: 0,
                            tx_seq: 0,
                        },
                        false,
                    ),
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
    #[tokio::test]
    async fn spawn_list_chunk_observes_queue_and_work_on_success_and_error() {
        let registry = Registry::new();
        let metrics = ListApiMetrics::new(&registry).stream_metrics("list_checkpoints", "summary");

        let success = spawn_list_chunk(Some(metrics.clone()), |_, _| Ok(()));
        let error = spawn_list_chunk::<(), _>(Some(metrics), |_, _| {
            Err(RpcError::new(
                tonic::Code::Internal,
                "synthetic chunk error",
            ))
        });

        assert!(success.await.expect("success worker joined").is_ok());
        assert!(error.await.expect("error worker joined").is_err());

        let families = registry.gather();
        let family = families
            .iter()
            .find(|family| family.name() == "list_chunk_seconds")
            .expect("list chunk metric family");
        assert_eq!(family.get_metric().len(), 4);
        for phase in ["queue", "setup", "work"] {
            let metric = family
                .get_metric()
                .iter()
                .find(|metric| {
                    metric
                        .get_label()
                        .iter()
                        .any(|label| label.name() == "phase" && label.value() == phase)
                })
                .unwrap_or_else(|| panic!("missing {phase} metric"));
            assert_eq!(metric.get_histogram().get_sample_count(), 2, "{phase}");
        }
        let read = family
            .get_metric()
            .iter()
            .find(|metric| {
                metric
                    .get_label()
                    .iter()
                    .any(|label| label.name() == "phase" && label.value() == "read")
            })
            .expect("missing read metric");
        assert_eq!(read.get_histogram().get_sample_count(), 0, "read");
    }

    #[tokio::test]
    async fn chunked_scan_accumulates_bitmap_buckets_across_chunks() {
        let mut scan = ChunkedScan::new(0usize, 1, 1, 10, move |state, args: ChunkArgs| {
            tokio::task::spawn_blocking(move || {
                let buckets_evaluated = if state == 0 { 2 } else { 3 };
                Ok(ScanChunkDone::<usize, usize> {
                    items: Vec::new(),
                    produced: 0,
                    next_state: (state == 0).then_some(1),
                    terminal: ScanTerminal::from_range_exhaustion(
                        RangeExhaustion::CheckpointBound,
                        Position::Transactions {
                            checkpoint: 0,
                            tx_seq: 0,
                        },
                        false,
                    ),
                    remaining_scan_budget: args.scan_budget - buckets_evaluated,
                })
            })
        });

        assert_eq!(scan.next_item().await.unwrap(), None);
        assert_eq!(scan.bitmap_buckets_evaluated(), 5);

        let filtered_registry = Registry::new();
        let mut filtered_metrics = ListRequestMetrics::new(
            Some(
                ListApiMetrics::new(&filtered_registry)
                    .stream_metrics("list_checkpoints", "summary"),
            ),
            Instant::now(),
        );
        filtered_metrics.finish_success(QueryEndReason::LedgerTip, Some(5));
        let filtered_family = filtered_registry
            .gather()
            .into_iter()
            .find(|family| family.name() == "list_bitmap_buckets_evaluated")
            .expect("filtered bitmap metric family");
        assert_eq!(filtered_family.get_metric().len(), 1);
        let filtered_histogram = filtered_family.get_metric()[0].get_histogram();
        assert_eq!(filtered_histogram.get_sample_count(), 1);
        assert_eq!(filtered_histogram.get_sample_sum(), 5.0);

        let unfiltered_registry = Registry::new();
        let mut unfiltered_metrics = ListRequestMetrics::new(
            Some(
                ListApiMetrics::new(&unfiltered_registry)
                    .stream_metrics("list_checkpoints", "summary"),
            ),
            Instant::now(),
        );
        unfiltered_metrics.finish_success(QueryEndReason::LedgerTip, None);
        let unfiltered_family = unfiltered_registry
            .gather()
            .into_iter()
            .find(|family| family.name() == "list_bitmap_buckets_evaluated")
            .expect("unfiltered bitmap metric family");
        assert_eq!(
            unfiltered_family.get_metric()[0]
                .get_histogram()
                .get_sample_count(),
            0
        );
    }
}
