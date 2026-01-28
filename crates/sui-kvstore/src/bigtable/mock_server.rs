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
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use bytes::Bytes;
use tokio::sync::Mutex;
use tokio_stream::Stream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use crate::bigtable::proto::bigtable::v2::{
    CheckAndMutateRowRequest, CheckAndMutateRowResponse, ExecuteQueryRequest, ExecuteQueryResponse,
    GenerateInitialChangeStreamPartitionsRequest, GenerateInitialChangeStreamPartitionsResponse,
    MutateRowRequest, MutateRowResponse, MutateRowsRequest, MutateRowsResponse, PingAndWarmRequest,
    PingAndWarmResponse, PrepareQueryRequest, PrepareQueryResponse, ReadChangeStreamRequest,
    ReadChangeStreamResponse, ReadModifyWriteRowRequest, ReadModifyWriteRowResponse,
    ReadRowsRequest, ReadRowsResponse, SampleRowKeysRequest, SampleRowKeysResponse,
    bigtable_server::Bigtable, bigtable_server::BigtableServer, mutate_rows_response::Entry,
};
use crate::bigtable::proto::rpc::Status as RpcStatus;

/// Configuration for which entries should fail in a MutateRows call.
#[derive(Default, Clone)]
pub struct FailureConfig {
    /// Map of entry index -> gRPC status code (non-zero = failure).
    pub entry_failures: HashMap<usize, i32>,
}

/// Records what was sent in each MutateRows call.
#[derive(Clone, Debug)]
pub struct RecordedCall {
    pub row_keys: Vec<Bytes>,
}

/// Shared state for the mock server.
#[derive(Clone)]
struct MockState {
    expectations: Arc<Mutex<Vec<FailureConfig>>>,
    recorded_calls: Arc<Mutex<Vec<RecordedCall>>>,
    call_count: Arc<AtomicUsize>,
}

/// Mock BigTable server with injectable failures and call recording.
#[derive(Clone)]
pub struct MockBigtableServer {
    state: MockState,
}

impl MockBigtableServer {
    pub fn new() -> Self {
        Self {
            state: MockState {
                expectations: Arc::new(Mutex::new(vec![])),
                recorded_calls: Arc::new(Mutex::new(vec![])),
                call_count: Arc::new(AtomicUsize::new(0)),
            },
        }
    }

    /// Add an expectation for the next MutateRows call.
    pub async fn expect(&self, config: FailureConfig) {
        self.state.expectations.lock().await.push(config);
    }

    /// Get recorded calls for verification.
    pub async fn calls(&self) -> Vec<RecordedCall> {
        self.state.recorded_calls.lock().await.clone()
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

/// Stream type for MutateRows responses.
type MutateRowsStream = Pin<Box<dyn Stream<Item = Result<MutateRowsResponse, Status>> + Send>>;

/// Stream type for other streaming methods (not implemented).
type UnimplementedStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

#[tonic::async_trait]
impl Bigtable for MockBigtableServer {
    type ReadRowsStream = UnimplementedStream<ReadRowsResponse>;
    type SampleRowKeysStream = UnimplementedStream<SampleRowKeysResponse>;
    type MutateRowsStream = MutateRowsStream;
    type GenerateInitialChangeStreamPartitionsStream =
        UnimplementedStream<GenerateInitialChangeStreamPartitionsResponse>;
    type ReadChangeStreamStream = UnimplementedStream<ReadChangeStreamResponse>;
    type ExecuteQueryStream = UnimplementedStream<ExecuteQueryResponse>;

    async fn mutate_rows(
        &self,
        request: Request<MutateRowsRequest>,
    ) -> Result<Response<Self::MutateRowsStream>, Status> {
        let req = request.into_inner();
        self.state.call_count.fetch_add(1, Ordering::SeqCst);

        // Record the call
        {
            let mut calls = self.state.recorded_calls.lock().await;
            calls.push(RecordedCall {
                row_keys: req.entries.iter().map(|e| e.row_key.clone()).collect(),
            });
        }

        // Get expectation (default: all succeed)
        let expectation = {
            let mut expectations = self.state.expectations.lock().await;
            if expectations.is_empty() {
                FailureConfig::default()
            } else {
                expectations.remove(0)
            }
        };

        // Build response entries
        let entries: Vec<Entry> = (0..req.entries.len())
            .map(|idx| {
                let code = expectation.entry_failures.get(&idx).copied().unwrap_or(0);
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
        _request: Request<ReadRowsRequest>,
    ) -> Result<Response<Self::ReadRowsStream>, Status> {
        Err(Status::unimplemented("not implemented for mock"))
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
        _request: Request<CheckAndMutateRowRequest>,
    ) -> Result<Response<CheckAndMutateRowResponse>, Status> {
        Err(Status::unimplemented("not implemented for mock"))
    }

    async fn ping_and_warm(
        &self,
        _request: Request<PingAndWarmRequest>,
    ) -> Result<Response<PingAndWarmResponse>, Status> {
        Err(Status::unimplemented("not implemented for mock"))
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
