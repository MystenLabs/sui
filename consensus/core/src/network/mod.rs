// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use bytes::Bytes;
use consensus_config::AuthorityIndex;

use crate::block::BlockRef;

/// An AuthorityNode will keep a NetworkManager until shutdown.
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
pub(crate) trait NetworkClient: Send + Sync {
    /// Sends a block to all connected peers.
    async fn send_block(&self, target: AuthorityIndex, block: Bytes);

    /// Fetches blocks from a peer.
    async fn fetch_blocks(&self, target: AuthorityIndex, block_refs: Vec<BlockRef>);
}

/// Network service for handling requests from peers.
pub(crate) trait NetworkService: Send + Sync {
    async fn handle_send_block(&self, source: AuthorityIndex, block: Bytes);
    async fn handle_fetch_blocks(&self, source: AuthorityIndex, block_refs: Vec<BlockRef>);
}
