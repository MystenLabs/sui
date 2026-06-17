// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Narrow mock BigTable server for tests that need deterministic write behavior.
//!
//! Supported surface:
//! - `MutateRows`: optional row-key expectations, per-entry injected failures,
//!   pause gates, and successful `SetCell` persistence with latest-timestamp
//!   semantics.
//! - `ReadRows`: explicit row-key lookups with an optional row limit, with no
//!   filter or `CellsPerColumnLimitFilter(1)`. Row ranges, reverse scans, and
//!   other filters are intentionally unsupported.
//! - `CheckAndMutateRow`: `PassAllFilter(true)` and the CAS helper shape used
//!   by this crate (`Chain` of family regex, column qualifier regex, optional
//!   value range, and optional cells-per-column limit).
//!
//! Anything outside that subset should return `UNIMPLEMENTED` so tests do not
//! accidentally rely on BigTable behavior this mock does not model.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use anyhow::Result;
use bytes::Bytes;
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tokio_stream::Stream;
use tonic::Request;
use tonic::Response;
use tonic::Status;
use tonic::transport::Server;

use crate::bigtable::proto::bigtable::v2::CheckAndMutateRowRequest;
use crate::bigtable::proto::bigtable::v2::CheckAndMutateRowResponse;
use crate::bigtable::proto::bigtable::v2::ExecuteQueryRequest;
use crate::bigtable::proto::bigtable::v2::ExecuteQueryResponse;
use crate::bigtable::proto::bigtable::v2::GenerateInitialChangeStreamPartitionsRequest;
use crate::bigtable::proto::bigtable::v2::GenerateInitialChangeStreamPartitionsResponse;
use crate::bigtable::proto::bigtable::v2::MutateRowRequest;
use crate::bigtable::proto::bigtable::v2::MutateRowResponse;
use crate::bigtable::proto::bigtable::v2::MutateRowsRequest;
use crate::bigtable::proto::bigtable::v2::MutateRowsResponse;
use crate::bigtable::proto::bigtable::v2::PingAndWarmRequest;
use crate::bigtable::proto::bigtable::v2::PingAndWarmResponse;
use crate::bigtable::proto::bigtable::v2::PrepareQueryRequest;
use crate::bigtable::proto::bigtable::v2::PrepareQueryResponse;
use crate::bigtable::proto::bigtable::v2::ReadChangeStreamRequest;
use crate::bigtable::proto::bigtable::v2::ReadChangeStreamResponse;
use crate::bigtable::proto::bigtable::v2::ReadModifyWriteRowRequest;
use crate::bigtable::proto::bigtable::v2::ReadModifyWriteRowResponse;
use crate::bigtable::proto::bigtable::v2::ReadRowsRequest;
use crate::bigtable::proto::bigtable::v2::ReadRowsResponse;
use crate::bigtable::proto::bigtable::v2::SampleRowKeysRequest;
use crate::bigtable::proto::bigtable::v2::SampleRowKeysResponse;
use crate::bigtable::proto::bigtable::v2::bigtable_server::Bigtable;
use crate::bigtable::proto::bigtable::v2::bigtable_server::BigtableServer;
use crate::bigtable::proto::bigtable::v2::mutate_rows_response::Entry;
use crate::bigtable::proto::bigtable::v2::mutation;
use crate::bigtable::proto::bigtable::v2::read_rows_response::CellChunk;
use crate::bigtable::proto::bigtable::v2::read_rows_response::cell_chunk::RowStatus;
use crate::bigtable::proto::bigtable::v2::row_filter::Filter;
use crate::bigtable::proto::bigtable::v2::value_range::EndValue;
use crate::bigtable::proto::bigtable::v2::value_range::StartValue;
use crate::bigtable::proto::rpc::Status as RpcStatus;

/// Expected call: which row keys should arrive, and which entry indices should fail.
#[derive(Clone)]
pub struct ExpectedCall {
    /// Expected row keys in order. Panics if the actual call doesn't match.
    pub row_keys: Vec<&'static [u8]>,
    /// Map of entry index -> gRPC status code (non-zero = failure).
    pub failures: HashMap<usize, i32>,
}

struct Cell {
    value: Bytes,
    timestamp_micros: i64,
}

/// In-memory row: (family, column_qualifier) -> latest cell version.
type Row = HashMap<(String, Bytes), Cell>;

enum GateMatcher {
    Any,
    TimestampMicros(i64),
}

struct MutateRowsGateRegistration {
    matcher: GateMatcher,
    skip_matches: usize,
    gate: Arc<MutateRowsGateInner>,
}

impl MutateRowsGateRegistration {
    fn matches(&self, request: &MutateRowsRequest) -> bool {
        match self.matcher {
            GateMatcher::Any => true,
            GateMatcher::TimestampMicros(timestamp_micros) => request.entries.iter().any(|entry| {
                entry.mutations.iter().any(|mutation| {
                    matches!(
                        &mutation.mutation,
                        Some(mutation::Mutation::SetCell(cell))
                            if cell.timestamp_micros == timestamp_micros
                    )
                })
            }),
        }
    }
}

struct MutateRowsGateInner {
    arrived: AtomicBool,
    released: AtomicBool,
    arrived_notify: Notify,
    released_notify: Notify,
}

impl MutateRowsGateInner {
    fn new() -> Self {
        Self {
            arrived: AtomicBool::new(false),
            released: AtomicBool::new(false),
            arrived_notify: Notify::new(),
            released_notify: Notify::new(),
        }
    }

    fn mark_arrived(&self) {
        self.arrived.store(true, Ordering::SeqCst);
        self.arrived_notify.notify_waiters();
    }

    async fn wait_until_released(&self) {
        loop {
            let notified = self.released_notify.notified();
            if self.released.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }
}

#[derive(Clone)]
pub struct MutateRowsGate {
    inner: Arc<MutateRowsGateInner>,
}

impl MutateRowsGate {
    pub async fn wait_for_arrival(&self) {
        tokio::time::timeout(tokio::time::Duration::from_secs(5), async {
            loop {
                let notified = self.inner.arrived_notify.notified();
                if self.inner.arrived.load(Ordering::SeqCst) {
                    return;
                }
                notified.await;
            }
        })
        .await
        .expect("timed out waiting for MutateRows gate arrival");
    }

    pub fn release(&self) {
        self.inner.released.store(true, Ordering::SeqCst);
        self.inner.released_notify.notify_waiters();
    }
}

/// Shared state for the mock server.
#[derive(Default)]
struct MockState {
    expectations: Vec<ExpectedCall>,
    mutate_rows_gates: VecDeque<MutateRowsGateRegistration>,
    /// In-memory row storage keyed by (table_name_suffix, row_key).
    /// Table name suffix is the part after the /tables/ prefix.
    rows: HashMap<(String, Bytes), Row>,
}

/// Mock BigTable server with injectable failures and call recording.
#[derive(Clone)]
pub struct MockBigtableServer {
    state: Arc<Mutex<MockState>>,
    /// When true, PingAndWarm returns an error.
    pub ping_should_fail: Arc<AtomicBool>,
    /// Total number of requests received across all RPCs.
    pub request_count: Arc<AtomicUsize>,
    /// Total number of MutateRows requests received.
    pub mutate_rows_count: Arc<AtomicUsize>,
    /// Counter of CheckAndMutateRow calls to fail with `Status::unavailable`
    /// before serving normally. Decremented on each consumed failure.
    check_and_mutate_failures: Arc<AtomicUsize>,
}

impl MockBigtableServer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockState::default())),
            ping_should_fail: Arc::new(AtomicBool::new(false)),
            request_count: Arc::new(AtomicUsize::new(0)),
            mutate_rows_count: Arc::new(AtomicUsize::new(0)),
            check_and_mutate_failures: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Fail the next `n` CheckAndMutateRow calls with `Status::unavailable`.
    /// Subsequent calls (or any call after the budget is exhausted) succeed
    /// normally.
    pub fn fail_next_n_check_and_mutate(&self, n: usize) {
        self.check_and_mutate_failures.store(n, Ordering::Relaxed);
    }

    pub async fn pause_next_mutate_rows(&self) -> MutateRowsGate {
        self.pause_next_mutate_rows_matching(GateMatcher::Any).await
    }

    pub async fn pause_next_mutate_rows_with_timestamp(
        &self,
        timestamp_micros: i64,
    ) -> MutateRowsGate {
        self.pause_nth_mutate_rows_with_timestamp(0, timestamp_micros)
            .await
    }

    pub async fn pause_nth_mutate_rows_with_timestamp(
        &self,
        skip_matches: usize,
        timestamp_micros: i64,
    ) -> MutateRowsGate {
        self.pause_mutate_rows_matching(
            GateMatcher::TimestampMicros(timestamp_micros),
            skip_matches,
        )
        .await
    }

    async fn pause_next_mutate_rows_matching(&self, matcher: GateMatcher) -> MutateRowsGate {
        self.pause_mutate_rows_matching(matcher, 0).await
    }

    async fn pause_mutate_rows_matching(
        &self,
        matcher: GateMatcher,
        skip_matches: usize,
    ) -> MutateRowsGate {
        let gate = Arc::new(MutateRowsGateInner::new());
        self.state
            .lock()
            .await
            .mutate_rows_gates
            .push_back(MutateRowsGateRegistration {
                matcher,
                skip_matches,
                gate: gate.clone(),
            });
        MutateRowsGate { inner: gate }
    }

    /// Add an expectation for the next MutateRows call.
    /// The server will panic if the actual row keys don't match.
    pub async fn expect(&self, expected: ExpectedCall) {
        self.state.lock().await.expectations.push(expected);
    }

    /// Read a cell value from the in-memory store (for test assertions).
    pub async fn get_cell(
        &self,
        table: &str,
        row_key: &[u8],
        family: &str,
        column: &[u8],
    ) -> Option<Bytes> {
        let state = self.state.lock().await;
        let row = state
            .rows
            .get(&(table.to_string(), Bytes::copy_from_slice(row_key)))?;
        row.get(&(family.to_string(), Bytes::copy_from_slice(column)))
            .map(|cell| cell.value.clone())
    }

    /// Start the mock server on a random available port.
    /// Returns the socket address the server is listening on.
    pub async fn start(&self) -> Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let mock = self.clone();

        let handle = tokio::spawn(async move {
            let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
            Server::builder()
                .add_service(BigtableServer::new(mock))
                .serve_with_incoming(incoming)
                .await
                .ok();
        });

        // Small delay to ensure server is ready
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        Ok((addr, handle))
    }
}

impl Default for MockBigtableServer {
    fn default() -> Self {
        Self::new()
    }
}

/// The CAS helpers wrap literal column names as `^<literal>$`. Strip the anchors so
/// the mock can match the column qualifier against stored cell keys verbatim.
fn strip_regex_anchors(qualifier: &Bytes) -> &[u8] {
    let bytes = qualifier.as_ref();
    let stripped = bytes.strip_prefix(b"^").unwrap_or(bytes);
    stripped.strip_suffix(b"$").unwrap_or(stripped)
}

/// Evaluate a ValueRangeFilter against a raw cell value. Unbounded sides are treated
/// as open-ended. Handles both closed and open range endpoints.
fn value_in_range(value: &Bytes, vr: &crate::bigtable::proto::bigtable::v2::ValueRange) -> bool {
    let lo_ok = match &vr.start_value {
        Some(StartValue::StartValueClosed(s)) => value.as_ref() >= s.as_ref(),
        Some(StartValue::StartValueOpen(s)) => value.as_ref() > s.as_ref(),
        None => true,
    };
    let hi_ok = match &vr.end_value {
        Some(EndValue::EndValueClosed(e)) => value.as_ref() <= e.as_ref(),
        Some(EndValue::EndValueOpen(e)) => value.as_ref() < e.as_ref(),
        None => true,
    };
    lo_ok && hi_ok
}

/// Stream type for streaming responses.
type BoxStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

/// Stream type for other streaming methods (not implemented).
type UnimplementedStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

#[tonic::async_trait]
impl Bigtable for MockBigtableServer {
    type ReadRowsStream = BoxStream<ReadRowsResponse>;
    type SampleRowKeysStream = UnimplementedStream<SampleRowKeysResponse>;
    type MutateRowsStream = BoxStream<MutateRowsResponse>;
    type GenerateInitialChangeStreamPartitionsStream =
        UnimplementedStream<GenerateInitialChangeStreamPartitionsResponse>;
    type ReadChangeStreamStream = UnimplementedStream<ReadChangeStreamResponse>;
    type ExecuteQueryStream = UnimplementedStream<ExecuteQueryResponse>;

    async fn mutate_rows(
        &self,
        request: Request<MutateRowsRequest>,
    ) -> Result<Response<Self::MutateRowsStream>, Status> {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        self.mutate_rows_count.fetch_add(1, Ordering::Relaxed);
        let req = request.into_inner();

        let gate = {
            let mut state = self.state.lock().await;
            let mut gate = None;
            let mut remove_idx = None;
            for (idx, registered) in state.mutate_rows_gates.iter_mut().enumerate() {
                if registered.matches(&req) {
                    if registered.skip_matches > 0 {
                        registered.skip_matches -= 1;
                    } else {
                        gate = Some(registered.gate.clone());
                        remove_idx = Some(idx);
                    }
                    break;
                }
            }
            if let Some(idx) = remove_idx {
                state.mutate_rows_gates.remove(idx);
            }
            gate
        };
        if let Some(gate) = gate {
            gate.mark_arrived();
            gate.wait_until_released().await;
        }

        let mut state = self.state.lock().await;

        let table = req
            .table_name
            .rsplit_once("/tables/")
            .map(|(_, t)| t.to_string())
            .unwrap_or_default();

        // If callers registered expectations, enforce them (used by tests that
        // inject failures). Otherwise, accept and persist every mutation — the
        // default permissive behavior that bitmap-handler tests rely on to
        // read back what was written.
        let expected = if state.expectations.is_empty() {
            None
        } else {
            let expected = state.expectations.remove(0);
            let actual_keys: Vec<&[u8]> = req.entries.iter().map(|e| e.row_key.as_ref()).collect();
            assert_eq!(
                actual_keys, expected.row_keys,
                "MutateRows row keys mismatch"
            );
            Some(expected)
        };

        let entries: Vec<Entry> = (0..req.entries.len())
            .map(|idx| {
                let code = expected
                    .as_ref()
                    .and_then(|e| e.failures.get(&idx).copied())
                    .unwrap_or(0);
                Entry {
                    index: idx as i64,
                    status: Some(RpcStatus {
                        code,
                        message: if code != 0 {
                            "Injected failure".to_string()
                        } else {
                            String::new()
                        },
                        details: vec![],
                    }),
                }
            })
            .collect();

        // Persist successful mutations to the in-memory store so tests can
        // read them back. Any entry whose injected status was non-zero (a
        // failure) is skipped.
        for (idx, entry) in req.entries.iter().enumerate() {
            let code = expected
                .as_ref()
                .and_then(|e| e.failures.get(&idx).copied())
                .unwrap_or(0);
            if code != 0 {
                continue;
            }
            let row = state
                .rows
                .entry((table.clone(), entry.row_key.clone()))
                .or_default();
            for m in &entry.mutations {
                if let Some(mutation::Mutation::SetCell(set_cell)) = &m.mutation {
                    let key = (
                        set_cell.family_name.clone(),
                        set_cell.column_qualifier.clone(),
                    );
                    let should_write = set_cell.timestamp_micros < 0
                        || row
                            .get(&key)
                            .is_none_or(|cell| cell.timestamp_micros <= set_cell.timestamp_micros);
                    if should_write {
                        row.insert(
                            key,
                            Cell {
                                value: set_cell.value.clone(),
                                timestamp_micros: set_cell.timestamp_micros,
                            },
                        );
                    }
                }
            }
        }

        let response = MutateRowsResponse {
            entries,
            rate_limit_info: None,
        };

        // Return as a single-item stream
        let stream = tokio_stream::once(Ok(response));
        Ok(Response::new(Box::pin(stream)))
    }

    async fn read_rows(
        &self,
        request: Request<ReadRowsRequest>,
    ) -> Result<Response<Self::ReadRowsStream>, Status> {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        let req = request.into_inner();
        let state = self.state.lock().await;

        // Extract the table name suffix (after /tables/).
        let table = req
            .table_name
            .rsplit_once("/tables/")
            .map(|(_, t)| t.to_string())
            .unwrap_or_default();

        if req.reversed {
            return Err(Status::unimplemented(
                "mock ReadRows does not support reversed scans",
            ));
        }
        if req.rows_limit < 0 {
            return Err(Status::unimplemented(
                "mock ReadRows does not support negative rows_limit",
            ));
        }
        match req.filter.and_then(|f| f.filter) {
            None | Some(Filter::CellsPerColumnLimitFilter(1)) => {}
            Some(_) => {
                return Err(Status::unimplemented(
                    "mock ReadRows only supports CellsPerColumnLimitFilter(1)",
                ));
            }
        }
        let Some(row_set) = req.rows else {
            return Err(Status::unimplemented(
                "mock ReadRows requires explicit row_keys",
            ));
        };
        if !row_set.row_ranges.is_empty() {
            return Err(Status::unimplemented(
                "mock ReadRows only supports explicit row_keys, not row_ranges",
            ));
        }
        let limit = if req.rows_limit == 0 {
            row_set.row_keys.len()
        } else {
            req.rows_limit as usize
        };
        let requested_keys = row_set.row_keys.into_iter().take(limit);

        let mut chunks = Vec::new();
        for row_key in requested_keys {
            let Some(row) = state.rows.get(&(table.clone(), row_key.clone())) else {
                continue;
            };
            for ((family, qualifier), cell) in row {
                chunks.push(CellChunk {
                    row_key: row_key.clone(),
                    family_name: Some(family.clone()),
                    qualifier: Some(qualifier.to_vec()),
                    timestamp_micros: cell.timestamp_micros,
                    labels: vec![],
                    value: cell.value.clone(),
                    value_size: 0,
                    row_status: None,
                });
            }
            // Emit CommitRow on the last chunk (or a standalone marker if row is empty).
            if let Some(last) = chunks.last_mut() {
                last.row_status = Some(RowStatus::CommitRow(true));
            }
        }

        let response = ReadRowsResponse {
            chunks,
            last_scanned_row_key: Bytes::new(),
            request_stats: None,
        };
        let stream = tokio_stream::once(Ok(response));
        Ok(Response::new(Box::pin(stream)))
    }

    async fn sample_row_keys(
        &self,
        _request: Request<SampleRowKeysRequest>,
    ) -> Result<Response<Self::SampleRowKeysStream>, Status> {
        Err(Status::unimplemented("not implemented for mock"))
    }

    async fn mutate_row(
        &self,
        _request: Request<MutateRowRequest>,
    ) -> Result<Response<MutateRowResponse>, Status> {
        Err(Status::unimplemented("not implemented for mock"))
    }

    async fn check_and_mutate_row(
        &self,
        request: Request<CheckAndMutateRowRequest>,
    ) -> Result<Response<CheckAndMutateRowResponse>, Status> {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        if self
            .check_and_mutate_failures
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                if v > 0 { Some(v - 1) } else { None }
            })
            .is_ok()
        {
            return Err(Status::unavailable("injected check_and_mutate_row failure"));
        }
        let req = request.into_inner();
        let mut state = self.state.lock().await;

        let table = req
            .table_name
            .rsplit_once("/tables/")
            .map(|(_, t)| t.to_string())
            .unwrap_or_default();

        let row_key = (table.clone(), req.row_key);
        let row = state.rows.get(&row_key);

        let matched = match req.predicate_filter.and_then(|f| f.filter) {
            Some(Filter::PassAllFilter(true)) => {
                // Matches if the row exists and has any cells.
                row.is_some_and(|r| !r.is_empty())
            }
            Some(Filter::Chain(chain)) => {
                // The CAS helpers in this crate always build a Chain with at least a
                // FamilyNameRegex + ColumnQualifierRegex, optionally followed by a
                // ValueRangeFilter. Parse those components (ignoring a no-op
                // CellsPerColumnLimitFilter if present) and check whether any cell in
                // the row satisfies all of them. Anything else is unsupported.
                let mut family: Option<&str> = None;
                let mut column: Option<&Bytes> = None;
                let mut value_range: Option<&crate::bigtable::proto::bigtable::v2::ValueRange> =
                    None;
                for f in &chain.filters {
                    match &f.filter {
                        Some(Filter::FamilyNameRegexFilter(s)) => family = Some(s.as_str()),
                        Some(Filter::ColumnQualifierRegexFilter(q)) => column = Some(q),
                        Some(Filter::ValueRangeFilter(vr)) => value_range = Some(vr),
                        Some(Filter::CellsPerColumnLimitFilter(1)) => {}
                        Some(Filter::CellsPerColumnLimitFilter(_)) => {
                            return Err(Status::unimplemented(
                                "mock CAS predicate only supports CellsPerColumnLimitFilter(1)",
                            ));
                        }
                        _ => {
                            return Err(Status::unimplemented(
                                "mock only supports Chain of Family/ColumnQualifier/ValueRange",
                            ));
                        }
                    }
                }
                let Some(column) = column else {
                    return Err(Status::unimplemented(
                        "mock CAS predicate chain requires a ColumnQualifierRegex",
                    ));
                };
                // The production helpers emit `^<literal>$`; strip anchors for exact match.
                let column_literal = strip_regex_anchors(column);
                match row {
                    None => false,
                    Some(row) => row.iter().any(|((fam, col), cell)| {
                        family.is_none_or(|f| fam == f)
                            && col.as_ref() == column_literal
                            && value_range.is_none_or(|vr| value_in_range(&cell.value, vr))
                    }),
                }
            }
            None => false,
            _ => {
                return Err(Status::unimplemented(
                    "mock only supports PassAllFilter and Chain predicates",
                ));
            }
        };

        let mutations = if matched {
            req.true_mutations
        } else {
            req.false_mutations
        };

        for m in mutations {
            if let Some(mutation::Mutation::SetCell(set_cell)) = m.mutation {
                let row = state.rows.entry(row_key.clone()).or_default();
                row.insert(
                    (set_cell.family_name, set_cell.column_qualifier),
                    Cell {
                        value: set_cell.value,
                        timestamp_micros: set_cell.timestamp_micros,
                    },
                );
            }
        }

        Ok(Response::new(CheckAndMutateRowResponse {
            predicate_matched: matched,
        }))
    }

    async fn ping_and_warm(
        &self,
        _request: Request<PingAndWarmRequest>,
    ) -> Result<Response<PingAndWarmResponse>, Status> {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        if self.ping_should_fail.load(Ordering::Relaxed) {
            return Err(Status::unavailable("injected ping failure"));
        }
        Ok(Response::new(PingAndWarmResponse {}))
    }

    async fn read_modify_write_row(
        &self,
        _request: Request<ReadModifyWriteRowRequest>,
    ) -> Result<Response<ReadModifyWriteRowResponse>, Status> {
        Err(Status::unimplemented("not implemented for mock"))
    }

    async fn generate_initial_change_stream_partitions(
        &self,
        _request: Request<GenerateInitialChangeStreamPartitionsRequest>,
    ) -> Result<Response<Self::GenerateInitialChangeStreamPartitionsStream>, Status> {
        Err(Status::unimplemented("not implemented for mock"))
    }

    async fn read_change_stream(
        &self,
        _request: Request<ReadChangeStreamRequest>,
    ) -> Result<Response<Self::ReadChangeStreamStream>, Status> {
        Err(Status::unimplemented("not implemented for mock"))
    }

    async fn prepare_query(
        &self,
        _request: Request<PrepareQueryRequest>,
    ) -> Result<Response<PrepareQueryResponse>, Status> {
        Err(Status::unimplemented("not implemented for mock"))
    }

    async fn execute_query(
        &self,
        _request: Request<ExecuteQueryRequest>,
    ) -> Result<Response<Self::ExecuteQueryStream>, Status> {
        Err(Status::unimplemented("not implemented for mock"))
    }
}
