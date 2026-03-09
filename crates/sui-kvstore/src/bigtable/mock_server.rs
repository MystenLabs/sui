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

use anyhow::Result;
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
use crate::bigtable::proto::rpc::Status as RpcStatus;

/// Expected call: which row keys should arrive, and which entry indices should fail.
#[derive(Clone)]
pub struct ExpectedCall {
    /// Expected row keys in order. Panics if the actual call doesn't match.
    pub row_keys: Vec<&'static [u8]>,
    /// Map of entry index -> gRPC status code (non-zero = failure).
    pub failures: HashMap<usize, i32>,
}

/// Shared state for the mock server.
#[derive(Default)]
struct MockState {
    expectations: Vec<ExpectedCall>,
}

/// Mock BigTable server with injectable failures and call recording.
#[derive(Clone)]
pub struct MockBigtableServer {
    state: Arc<Mutex<MockState>>,
}

impl MockBigtableServer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockState::default())),
        }
    }

    /// Add an expectation for the next MutateRows call.
    /// The server will panic if the actual row keys don't match.
    pub async fn expect(&self, expected: ExpectedCall) {
        self.state.lock().await.expectations.push(expected);
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
        let mut state = self.state.lock().await;

        assert!(
            !state.expectations.is_empty(),
            "Unexpected MutateRows call with keys: {:?}",
            req.entries.iter().map(|e| &e.row_key).collect::<Vec<_>>()
        );
        let expected = state.expectations.remove(0);

        let actual_keys: Vec<&[u8]> = req.entries.iter().map(|e| e.row_key.as_ref()).collect();
        assert_eq!(
            actual_keys, expected.row_keys,
            "MutateRows row keys mismatch"
        );

        let entries: Vec<Entry> = (0..req.entries.len())
            .map(|idx| {
                let code = expected.failures.get(&idx).copied().unwrap_or(0);
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
