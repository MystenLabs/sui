// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{pin::Pin, sync::Arc};

use async_trait::async_trait;
use futures::Stream;
use tokio_stream::Iter;
use tonic::{Request, Response, Streaming};

use super::{
    ObserverNetworkService,
    tonic_gen::observer_consensus_service_server::ObserverConsensusService,
    tonic_network::{
        BlockStreamRequest, BlockStreamResponse, FetchBlocksRequest, FetchBlocksResponse,
        FetchCommitsRequest, FetchCommitsResponse,
    },
};
use crate::context::Context;

/// Proxies Observer Tonic requests to ObserverNetworkService.
/// Extracts peer NodeId from TLS certificates and delegates to the service layer.
pub(crate) struct ObserverServiceProxy<S: ObserverNetworkService> {
    context: Arc<Context>,
    service: Arc<S>,
}

impl<S: ObserverNetworkService> ObserverServiceProxy<S> {
    pub(crate) fn new(context: Arc<Context>, service: Arc<S>) -> Self {
        Self { context, service }
    }
}

#[async_trait]
impl<S: ObserverNetworkService> ObserverConsensusService for ObserverServiceProxy<S> {
    type StreamBlocksStream =
        Pin<Box<dyn Stream<Item = Result<BlockStreamResponse, tonic::Status>> + Send>>;

    async fn stream_blocks(
        &self,
        _request: Request<Streaming<BlockStreamRequest>>,
    ) -> Result<Response<Self::StreamBlocksStream>, tonic::Status> {
        // TODO: Implement stream_blocks for observer nodes
        // 1. Extract peer public key from TLS certificate
        // 2. Handle bidirectional streaming with flow control (Start/Stop commands)
        // 3. Delegate to ObserverNetworkService::handle_stream_blocks
        // 4. Map blocks to BlockStreamResponse with highest_commit_index
        Err(tonic::Status::unimplemented(
            "stream_blocks not yet implemented for observers",
        ))
    }

    type FetchBlocksStream = Iter<std::vec::IntoIter<Result<FetchBlocksResponse, tonic::Status>>>;

    async fn fetch_blocks(
        &self,
        _request: Request<FetchBlocksRequest>,
    ) -> Result<Response<Self::FetchBlocksStream>, tonic::Status> {
        // TODO: Implement fetch_blocks for observer nodes
        // 1. Extract peer public key from TLS certificate
        // 2. Deserialize block_refs from request
        // 3. Delegate to ObserverNetworkService::handle_fetch_blocks
        // 4. Chunk blocks and return as streaming response
        Err(tonic::Status::unimplemented(
            "fetch_blocks not yet implemented for observers",
        ))
    }

    async fn fetch_commits(
        &self,
        _request: Request<FetchCommitsRequest>,
    ) -> Result<Response<FetchCommitsResponse>, tonic::Status> {
        // TODO: Implement fetch_commits for observer nodes
        // 1. Extract peer public key from TLS certificate
        // 2. Extract commit range from request
        // 3. Delegate to ObserverNetworkService::handle_fetch_commits
        // 4. Serialize commits and certifier_blocks
        Err(tonic::Status::unimplemented(
            "fetch_commits not yet implemented for observers",
        ))
    }
}
