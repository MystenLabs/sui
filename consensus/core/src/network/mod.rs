// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::{AuthorityIndex, NetworkKeyPair};
use serde::{Deserialize, Serialize};

use crate::{
    block::{BlockRef, VerifiedBlock},
    context::Context,
    error::ConsensusResult,
};

// Anemo generated stubs for RPCs.
mod anemo_gen {
    include!(concat!(env!("OUT_DIR"), "/consensus.ConsensusRpc.rs"));
}

mod tonic_gen {
    include!(concat!(env!("OUT_DIR"), "/consensus.ConsensusService.rs"));
}

pub(crate) mod anemo_network;
pub(crate) mod connection_monitor;
pub(crate) mod epoch_filter;
pub(crate) mod metrics;
pub(crate) mod tonic_network;

/// Network client for communicating with peers.
///
/// NOTE: the timeout parameters help saving resources at client and potentially server.
/// But it is up to the server implementation if the timeout is honored.
/// - To bound server resources, server should implement own timeout for incoming requests.
#[async_trait]
pub(crate) trait NetworkClient: Send + Sync + 'static {
    /// Sends a serialized SignedBlock to a peer.
    async fn send_block(
        &self,
        peer: AuthorityIndex,
        block: &VerifiedBlock,
        timeout: Duration,
    ) -> ConsensusResult<()>;

    /// Fetches serialized `SignedBlock`s from a peer.
    async fn fetch_blocks(
        &self,
        peer: AuthorityIndex,
        block_refs: Vec<BlockRef>,
        timeout: Duration,
    ) -> ConsensusResult<Vec<Bytes>>;
}

/// Network service for handling requests from peers.
/// NOTE: using `async_trait` macro because `NetworkService` methods are called in the trait impl
/// of `anemo_gen::ConsensusRpc`, which itself is annotated with `async_trait`.
#[async_trait]
pub(crate) trait NetworkService: Send + Sync + 'static {
    async fn handle_send_block(&self, peer: AuthorityIndex, block: Bytes) -> ConsensusResult<()>;
    async fn handle_fetch_blocks(
        &self,
        peer: AuthorityIndex,
        block_refs: Vec<BlockRef>,
    ) -> ConsensusResult<Vec<Bytes>>;
}

/// An `AuthorityNode` holds a `NetworkManager` until shutdown.
/// Dropping `NetworkManager` will shutdown the network service.
pub(crate) trait NetworkManager<S>: Send + Sync
where
    S: NetworkService,
{
    type Client: NetworkClient;

    /// Creates a new network manager.
    fn new(context: Arc<Context>) -> Self;

    /// Returns the network client.
    fn client(&self) -> Arc<Self::Client>;

    /// Installs network service.
    async fn install_service(&mut self, network_keypair: NetworkKeyPair, service: Arc<S>);

    /// Stops the network service.
    async fn stop(&mut self);
}

/// Network message types.
#[derive(Clone, Serialize, Deserialize, prost::Message)]
pub(crate) struct SendBlockRequest {
    // Serialized SignedBlock.
    #[prost(bytes = "bytes", tag = "1")]
    block: Bytes,
}

#[derive(Clone, Serialize, Deserialize, prost::Message)]
pub(crate) struct SendBlockResponse {}

#[derive(Clone, Serialize, Deserialize, prost::Message)]
pub(crate) struct FetchBlocksRequest {
    #[prost(bytes = "vec", repeated, tag = "1")]
    block_refs: Vec<Vec<u8>>,
}

#[derive(Clone, Serialize, Deserialize, prost::Message)]
pub(crate) struct FetchBlocksResponse {
    // Serialized SignedBlock.
    #[prost(bytes = "bytes", repeated, tag = "1")]
    blocks: Vec<Bytes>,
}
