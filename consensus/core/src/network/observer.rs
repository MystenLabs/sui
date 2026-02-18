// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{pin::Pin, sync::Arc};

use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::NetworkPublicKey;
use consensus_types::block::BlockRef;
use futures::Stream;
use tokio_stream::{Iter, iter};
use tonic::{Request, Response, Streaming};
use tracing::debug;

use super::{
    ObserverNetworkService,
    tonic_gen::observer_service_server::ObserverService,
};

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

/// Information about an observer peer connection, set in request extensions by the server.
#[derive(Clone, Debug)]
pub(crate) struct ObserverPeerInfo {
    pub(crate) public_key: NetworkPublicKey,
}

/// Proxies Observer Tonic requests to ObserverNetworkService.
/// Extracts peer NodeId from TLS certificates and delegates to the service layer.
pub(crate) struct ObserverServiceProxy<S: ObserverNetworkService> {
    service: Arc<S>,
}

impl<S: ObserverNetworkService> ObserverServiceProxy<S> {
    pub(crate) fn new(service: Arc<S>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl<S: ObserverNetworkService> ObserverService for ObserverServiceProxy<S> {
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

    type FetchBlocksStream = Iter<std::vec::IntoIter<Result<FetchBlocksResponse, tonic::Status>>>;

    async fn fetch_blocks(
        &self,
        request: Request<FetchBlocksRequest>,
    ) -> Result<Response<Self::FetchBlocksStream>, tonic::Status> {
        let Some(peer_node_id) = request
            .extensions()
            .get::<ObserverPeerInfo>()
            .map(|p| p.public_key.clone())
        else {
            return Err(tonic::Status::internal("ObserverPeerInfo not found"));
        };
        let inner = request.into_inner();
        let block_refs: Vec<BlockRef> = inner
            .block_refs
            .into_iter()
            .filter_map(|serialized| match bcs::from_bytes(&serialized) {
                Ok(r) => Some(r),
                Err(e) => {
                    debug!("Failed to deserialize block ref {:?}: {e:?}", serialized);
                    None
                }
            })
            .collect();
        let blocks = self
            .service
            .handle_fetch_blocks(peer_node_id, block_refs)
            .await
            .map_err(|e| tonic::Status::internal(format!("{e:?}")))?;
        let responses = vec![Ok(FetchBlocksResponse { blocks })].into_iter();
        Ok(Response::new(iter(responses)))
    }

    async fn fetch_commits(
        &self,
        request: Request<FetchCommitsRequest>,
    ) -> Result<Response<FetchCommitsResponse>, tonic::Status> {
        let Some(peer_node_id) = request
            .extensions()
            .get::<ObserverPeerInfo>()
            .map(|p| p.public_key.clone())
        else {
            return Err(tonic::Status::internal("ObserverPeerInfo not found"));
        };
        let request = request.into_inner();
        let (commits, certifier_blocks) = self
            .service
            .handle_fetch_commits(peer_node_id, (request.start..=request.end).into())
            .await
            .map_err(|e| tonic::Status::internal(format!("{e:?}")))?;
        let commits = commits
            .into_iter()
            .map(|c| c.serialized().clone())
            .collect();
        let certifier_blocks = certifier_blocks
            .into_iter()
            .map(|b| b.serialized().clone())
            .collect();
        Ok(Response::new(FetchCommitsResponse {
            commits,
            certifier_blocks,
        }))
    }
}
