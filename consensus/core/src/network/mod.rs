// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::{AuthorityIndex, NetworkKeyPair};
use serde::{Deserialize, Serialize};

use crate::{block::BlockRef, context::Context, error::ConsensusResult};

// Anemo generated stubs for RPCs.
mod anemo_gen {
    include!(concat!(env!("OUT_DIR"), "/consensus.ConsensusRpc.rs"));
}

pub(crate) mod anemo_network;

/// Network client for communicating with peers.
#[async_trait]
pub(crate) trait NetworkClient: Send + Sync + 'static {
    /// Sends a serialized SignedBlock to a peer.
    async fn send_block(&self, peer: AuthorityIndex, block: &Bytes) -> ConsensusResult<()>;

    /// Fetches serialized `SignedBlock`s from a peer.
    async fn fetch_blocks(
        &self,
        peer: AuthorityIndex,
        block_refs: Vec<BlockRef>,
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
    fn install_service(&self, network_keypair: NetworkKeyPair, service: Arc<S>);

    /// Stops the network service.
    async fn stop(&self);
}

/// Network message types.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SendBlockRequest {
    // Serialized SignedBlock.
    block: Bytes,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SendBlockResponse {}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct FetchBlocksRequest {
    block_refs: Vec<BlockRef>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct FetchBlocksResponse {
    // Serialized SignedBlock.
    blocks: Vec<Bytes>,
}
