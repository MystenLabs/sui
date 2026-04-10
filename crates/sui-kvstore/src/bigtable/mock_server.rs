// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Mock BigTable server for testing partial write retry logic.
//!
//! This module provides a mock implementation of the BigTable gRPC service
//! that can inject failures and record calls for verification.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use anyhow::Result;
use bytes::Bytes;
use tokio::sync::Mutex;
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

/// In-memory cell: (family, column_qualifier) -> value.
type Row = HashMap<(String, Bytes), Bytes>;

/// Shared state for the mock server.
#[derive(Default)]
struct MockState {
    expectations: Vec<ExpectedCall>,
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
    /// Artificial delay added before each `mutate_rows` returns (ms). Tests
    /// use this to widen the window between commit and durable write so
    /// race-sensitive behaviour becomes observable.
    pub mutate_delay_ms: Arc<AtomicUsize>,
}

impl MockBigtableServer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockState::default())),
            ping_should_fail: Arc::new(AtomicBool::new(false)),
            request_count: Arc::new(AtomicUsize::new(0)),
            mutate_delay_ms: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn set_mutate_delay(&self, ms: usize) {
        self.mutate_delay_ms.store(ms, Ordering::Relaxed);
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
            .cloned()
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
        let delay_ms = self.mutate_delay_ms.load(Ordering::Relaxed);
        if delay_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms as u64)).await;
        }
        let req = request.into_inner();
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
                    row.insert(
                        (
                            set_cell.family_name.clone(),
                            set_cell.column_qualifier.clone(),
                        ),
                        set_cell.value.clone(),
                    );
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

        let requested_keys: Vec<Bytes> = req.rows.map(|rs| rs.row_keys).unwrap_or_default();

        let mut chunks = Vec::new();
        for row_key in &requested_keys {
            let Some(row) = state.rows.get(&(table.clone(), row_key.clone())) else {
                continue;
            };
            for ((family, qualifier), value) in row {
                chunks.push(CellChunk {
                    row_key: row_key.clone(),
                    family_name: Some(family.clone()),
                    qualifier: Some(qualifier.to_vec()),
                    timestamp_micros: 0,
                    labels: vec![],
                    value: value.clone(),
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
                // We only support the specific pattern used by CAS:
                // Chain(CellsPerColumnLimitFilter(1), ValueRangeFilter(exact)).
                // Just evaluate the ValueRangeFilter — the limit filter is a no-op
                // in a single-version mock.
                let value_range = chain.filters.iter().find_map(|f| {
                    if let Some(Filter::ValueRangeFilter(vr)) = &f.filter {
                        Some(vr)
                    } else {
                        None
                    }
                });
                match (row, value_range) {
                    (Some(row), Some(vr)) => {
                        let expected = match (&vr.start_value, &vr.end_value) {
                            (
                                Some(StartValue::StartValueClosed(s)),
                                Some(EndValue::EndValueClosed(e)),
                            ) if s == e => s,
                            _ => {
                                return Err(Status::unimplemented(
                                    "mock only supports exact value range",
                                ));
                            }
                        };
                        row.values().any(|v| v == expected)
                    }
                    _ => false,
                }
            }
            None => false,
            _ => {
                return Err(Status::unimplemented(
                    "mock only supports PassAllFilter and Chain(CellsPerColumnLimit, ValueRange)",
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
                    set_cell.value,
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
