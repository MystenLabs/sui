// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{pin::Pin, sync::Arc};

use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use tokio_stream::Iter;
use tonic::{Request, Response, Streaming};

use super::{
    ObserverNetworkService, tonic_gen::observer_consensus_service_server::ObserverConsensusService,
};
use crate::context::Context;

// Observer block streaming messages
#[derive(Clone, prost::Message)]
pub(crate) struct BlockStreamRequest {
    #[prost(oneof = "block_stream_request::Command", tags = "1, 2")]
    pub(crate) command: Option<block_stream_request::Command>,
}

pub(crate) mod block_stream_request {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub(crate) enum Command {
        #[prost(message, tag = "1")]
        Start(super::StartBlockStream),
        #[prost(message, tag = "2")]
        Stop(super::StopBlockStream),
    }
}

#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct StartBlockStream {
    #[prost(uint64, repeated, tag = "1")]
    pub(crate) highest_round_per_authority: Vec<u64>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct StopBlockStream {}

#[derive(Clone, prost::Message)]
pub(crate) struct BlockStreamResponse {
    #[prost(bytes = "bytes", tag = "1")]
    pub(crate) block: Bytes,
    #[prost(uint64, tag = "2")]
    pub(crate) highest_commit_index: u64,
}

// Observer commit streaming messages
#[derive(Clone, prost::Message)]
pub(crate) struct CommitStreamRequest {
    #[prost(oneof = "commit_stream_request::Command", tags = "1, 2")]
    pub(crate) command: Option<commit_stream_request::Command>,
}

pub(crate) mod commit_stream_request {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub(crate) enum Command {
        #[prost(message, tag = "1")]
        Start(super::StartCommitStream),
        #[prost(message, tag = "2")]
        Stop(super::StopCommitStream),
    }
}

#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct StartCommitStream {
    #[prost(uint32, tag = "1")]
    pub(crate) highest_commit_index: u32,
    #[prost(uint32, tag = "2")]
    pub(crate) batch_size: u32,
}

#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct StopCommitStream {}

#[derive(Clone, prost::Message)]
pub(crate) struct CommitStreamResponse {
    #[prost(message, repeated, tag = "1")]
    pub(crate) commits: Vec<Commit>,
    #[prost(uint32, tag = "2")]
    pub(crate) highest_commit_index: u32,
}

#[derive(Clone, prost::Message)]
pub(crate) struct Commit {
    #[prost(bytes = "bytes", tag = "1")]
    pub(crate) commit: Bytes,
    #[prost(bytes = "bytes", repeated, tag = "2")]
    pub(crate) certifier_blocks: Vec<Bytes>,
    #[prost(bytes = "bytes", repeated, tag = "3")]
    pub(crate) committed_blocks: Vec<Bytes>,
}

// Observer fetch messages
#[derive(Clone, prost::Message)]
pub(crate) struct FetchBlocksRequest {
    #[prost(bytes = "vec", repeated, tag = "1")]
    pub(crate) block_refs: Vec<Vec<u8>>,
}

#[derive(Clone, prost::Message)]
pub(crate) struct FetchBlocksResponse {
    #[prost(bytes = "bytes", repeated, tag = "1")]
    pub(crate) blocks: Vec<Bytes>,
}

#[derive(Clone, prost::Message)]
pub(crate) struct FetchCommitsRequest {
    #[prost(uint32, tag = "1")]
    pub(crate) start: u32,
    #[prost(uint32, tag = "2")]
    pub(crate) end: u32,
}

#[derive(Clone, prost::Message)]
pub(crate) struct FetchCommitsResponse {
    #[prost(bytes = "bytes", repeated, tag = "1")]
    pub(crate) commits: Vec<Bytes>,
    #[prost(bytes = "bytes", repeated, tag = "2")]
    pub(crate) certifier_blocks: Vec<Bytes>,
}

/// Proxies Observer Tonic requests to ObserverNetworkService.
/// Extracts peer NodeId from TLS certificates and delegates to the service layer.
#[allow(dead_code)]
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
        Err(tonic::Status::unimplemented(
            "stream_blocks not yet implemented for observers",
        ))
    }

    type StreamCommitsStream =
        Pin<Box<dyn Stream<Item = Result<CommitStreamResponse, tonic::Status>> + Send>>;

    async fn stream_commits(
        &self,
        _request: Request<Streaming<CommitStreamRequest>>,
    ) -> Result<Response<Self::StreamCommitsStream>, tonic::Status> {
        // TODO: Implement stream_commits for observer nodes
        Err(tonic::Status::unimplemented(
            "stream_commits not yet implemented for observers",
        ))
    }

    type FetchBlocksStream = Iter<std::vec::IntoIter<Result<FetchBlocksResponse, tonic::Status>>>;

    async fn fetch_blocks(
        &self,
        _request: Request<FetchBlocksRequest>,
    ) -> Result<Response<Self::FetchBlocksStream>, tonic::Status> {
        // TODO: Implement fetch_blocks for observer nodes
        Err(tonic::Status::unimplemented(
            "fetch_blocks not yet implemented for observers",
        ))
    }

    async fn fetch_commits(
        &self,
        _request: Request<FetchCommitsRequest>,
    ) -> Result<Response<FetchCommitsResponse>, tonic::Status> {
        // TODO: Implement fetch_commits for observer nodes
        Err(tonic::Status::unimplemented(
            "fetch_commits not yet implemented for observers",
        ))
    }
}
