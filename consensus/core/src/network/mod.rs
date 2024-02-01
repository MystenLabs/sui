// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use std::sync::Arc;

use bytes::Bytes;
use consensus_config::AuthorityIndex;

use crate::{block::BlockRef, error::ConsensusResult};

/// An `AuthorityNode` holds a `NetworkManager` until shutdown.
pub(crate) trait NetworkManager<C, S>
where
    C: NetworkClient,
    S: NetworkService,
{
    /// Returns the network client.
    fn client(&self) -> Arc<C>;

    /// Installs network service.
    fn install_service(&self, service: Box<S>);
}

/// Network client for communicating with peers.
#[async_trait]
pub(crate) trait NetworkClient: Send + Sync {
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
#[async_trait]
pub(crate) trait NetworkService: Send + Sync {
    async fn handle_send_block(&self, peer: AuthorityIndex, block: Bytes) -> ConsensusResult<()>;
    async fn handle_fetch_blocks(
        &self,
        peer: AuthorityIndex,
        block_refs: Vec<BlockRef>,
    ) -> ConsensusResult<Vec<Bytes>>;
}
