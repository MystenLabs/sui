// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{pin::Pin, sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::NetworkKeyPair;
use consensus_types::block::BlockRef;
use futures::{Stream, StreamExt as _};
use tokio_stream::Iter;
use tonic::{Request, Response, Streaming};

use super::{
    NodeId, ObserverBlockStream, ObserverNetworkClient, ObserverNetworkService,
    tonic_gen::observer_service_server::ObserverService,
};
use crate::{
    commit::CommitRange,
    context::Context,
    error::{ConsensusError, ConsensusResult},
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

/// Tonic-based implementation of ObserverNetworkClient to talk to a peer's observer server.
#[allow(dead_code)]
pub(crate) struct TonicObserverClient {
    context: Arc<Context>,
    network_keypair: NetworkKeyPair,
}

impl TonicObserverClient {
    #[allow(dead_code)]
    pub(crate) fn new(context: Arc<Context>, network_keypair: NetworkKeyPair) -> Self {
        Self {
            context,
            network_keypair,
        }
    }
}

#[async_trait]
impl ObserverNetworkClient for TonicObserverClient {
    async fn stream_blocks(
        &self,
        _peer: NodeId,
        _request_stream: super::BlockRequestStream,
        _timeout: Duration,
    ) -> ConsensusResult<ObserverBlockStream> {
        // TODO: Implement bidirectional block streaming for observers
        // This requires resolving async-trait lifetime issues with streaming parameters
        Err(ConsensusError::NetworkRequest(
            "stream_blocks not yet implemented".to_string(),
        ))
    }

    async fn fetch_blocks(
        &self,
        _peer: NodeId,
        _block_refs: Vec<BlockRef>,
        _timeout: Duration,
    ) -> ConsensusResult<Vec<Bytes>> {
        // TODO: Implement block fetching for observers
        Err(ConsensusError::NetworkRequest(
            "fetch_blocks not yet implemented".to_string(),
        ))
    }

    async fn fetch_commits(
        &self,
        _peer: NodeId,
        _commit_range: CommitRange,
        _timeout: Duration,
    ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
        // TODO: Implement commit fetching for observers
        Err(ConsensusError::NetworkRequest(
            "fetch_commits not yet implemented".to_string(),
        ))
    }
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
impl<S: ObserverNetworkService> ObserverService for ObserverServiceProxy<S> {
    type StreamBlocksStream =
        Pin<Box<dyn Stream<Item = Result<BlockStreamResponse, tonic::Status>> + Send>>;

    /// Handles block streaming requests from observers.
    ///
    /// # Authentication
    /// This method requires TLS client certificate authentication. The observer's
    /// public key must be present in the request extensions as `ObserverPeerInfo`.
    /// If authentication fails, returns `Status::Unauthenticated`.
    ///
    /// # Arguments
    /// * `request` - The streaming request containing observer commands
    ///
    /// # Returns
    /// A stream of blocks matching the observer's request, or an error if:
    /// - The observer is not authenticated (missing peer info)
    /// - The underlying service returns an error
    async fn stream_blocks(
        &self,
        request: Request<Streaming<BlockStreamRequest>>,
    ) -> Result<Response<Self::StreamBlocksStream>, tonic::Status> {
        let peer_id = request
            .extensions()
            .get::<crate::network::tonic_network::ObserverPeerInfo>()
            .map(|info| info.public_key.clone())
            .ok_or_else(|| {
                tonic::Status::unauthenticated(
                    "Observer peer info not found in request. TLS authentication required.",
                )
            })?;

        let request_stream = Box::pin(
            request
                .into_inner()
                .filter_map(|result| async move { result.ok() }),
        );

        let block_stream = self
            .service
            .handle_stream_blocks(peer_id, request_stream)
            .await
            .map_err(|e| tonic::Status::internal(format!("{e:?}")))?;

        let response_stream = block_stream.map(|item| {
            Ok(BlockStreamResponse {
                block: item.block,
                highest_commit_index: item.highest_commit_index,
            })
        });

        Ok(Response::new(Box::pin(response_stream)))
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytes::Bytes;
    use consensus_types::block::Round;
    use futures::{StreamExt as _, stream};
    use parking_lot::Mutex;

    use super::{BlockStreamRequest, StartBlockStream, block_stream_request::Command};
    use crate::{
        context::Context,
        network::{ExtendedSerializedBlock, ObserverNetworkService, test_network::TestService},
    };

    fn block_for_round(round: Round) -> ExtendedSerializedBlock {
        ExtendedSerializedBlock {
            block: Bytes::from(vec![round as u8; 16]),
            excluded_ancestors: vec![],
        }
    }

    #[tokio::test]
    async fn observer_stream_blocks_from_start() {
        let (_context, keys) = Context::new_for_test(4);

        let service = Arc::new(Mutex::new(TestService::new()));
        {
            let mut s = service.lock();
            let own_blocks = (0..=100u8)
                .map(|i| block_for_round(i as Round))
                .collect::<Vec<_>>();
            s.add_own_blocks(own_blocks);
            s.set_highest_commit_index(42);
        }

        let observer_peer_id = keys[0].0.public().clone();

        let start_request = BlockStreamRequest {
            command: Some(Command::Start(StartBlockStream {
                highest_round_per_authority: vec![0u64, 0, 0, 0],
            })),
        };
        let request_stream = Box::pin(stream::once(async move { start_request }));

        let block_stream = service
            .handle_stream_blocks(observer_peer_id.clone(), request_stream)
            .await
            .unwrap();

        let blocks: Vec<_> = block_stream.collect().await;

        assert_eq!(blocks.len(), 100);
        assert_eq!(blocks[0].block, Bytes::from(vec![1u8; 16]));
        assert_eq!(blocks[0].highest_commit_index, 42);
        assert_eq!(blocks[99].block, Bytes::from(vec![100u8; 16]));
        assert_eq!(blocks[99].highest_commit_index, 42);

        for block_item in &blocks {
            assert_eq!(block_item.highest_commit_index, 42);
        }

        assert_eq!(service.lock().handle_stream_blocks.len(), 1);
        assert_eq!(service.lock().handle_stream_blocks[0], observer_peer_id);

        let commands = service.lock().stream_commands_received.lock().clone();
        assert_eq!(commands.len(), 1);
        assert!(matches!(commands[0], Command::Start(_)));
    }

    #[tokio::test]
    async fn observer_stream_blocks_filtered_by_round() {
        let (_context, keys) = Context::new_for_test(4);

        let service = Arc::new(Mutex::new(TestService::new()));
        {
            let mut s = service.lock();
            let own_blocks = (0..=100u8)
                .map(|i| block_for_round(i as Round))
                .collect::<Vec<_>>();
            s.add_own_blocks(own_blocks);
            s.set_highest_commit_index(50);
        }

        let observer_peer_id = keys[0].0.public().clone();

        let highest_round_per_authority = vec![50u64, 50, 50, 50];
        let start_request = BlockStreamRequest {
            command: Some(Command::Start(StartBlockStream {
                highest_round_per_authority,
            })),
        };
        let request_stream = Box::pin(stream::once(async move { start_request }));

        let block_stream = service
            .handle_stream_blocks(observer_peer_id, request_stream)
            .await
            .unwrap();

        let blocks: Vec<_> = block_stream.collect().await;

        assert_eq!(blocks.len(), 50);
        assert_eq!(blocks[0].block, Bytes::from(vec![51u8; 16]));
        assert_eq!(blocks[0].highest_commit_index, 50);
        assert_eq!(blocks[49].block, Bytes::from(vec![100u8; 16]));
        assert_eq!(blocks[49].highest_commit_index, 50);
    }
}
