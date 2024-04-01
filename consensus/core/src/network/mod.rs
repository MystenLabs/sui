// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module defines the network interface, and provides network implementations for the
//! consensus protocol.
//!
//! Having an abstract network interface allows
//! - simplying the semantics of sending data and serving requests over the network
//! - hiding implementation specific types and semantics from the consensus protocol
//! - allowing easy swapping of network implementations, for better performance or testing
//!
//! When modifying the client and server interfaces, the principle is to keep the interfaces
//! low level, close to underlying implementations in semantics. For example, the client interface
//! exposes sending messages to a specific peer, instead of broadcasting to all peers. Subscribing
//! to a stream of blocks gets back the stream via response, instead of delivering the stream
//! directly to the server. This keeps the logic agnostics to the underlying network outside of
//! this module, so they can be reused easily across network implementations.

use std::{pin::Pin, sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::{AuthorityIndex, NetworkKeyPair};
use futures::Stream;

use crate::{
    block::{BlockRef, VerifiedBlock},
    context::Context,
    error::ConsensusResult,
    Round,
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

/// A stream of serialized blocks returned over the network.
pub(crate) type BlockStream = Pin<Box<dyn Stream<Item = Bytes> + Send>>;

/// Network client for communicating with peers.
///
/// NOTE: the timeout parameters help saving resources at client and potentially server.
/// But it is up to the server implementation if the timeout is honored.
/// - To bound server resources, server should implement own timeout for incoming requests.
#[async_trait]
pub(crate) trait NetworkClient: Send + Sync + 'static {
    // Whether the network client streams blocks to subscribed peers.
    const SUPPORT_STREAMING: bool;

    /// Sends a serialized SignedBlock to a peer.
    async fn send_block(
        &self,
        peer: AuthorityIndex,
        block: &VerifiedBlock,
        timeout: Duration,
    ) -> ConsensusResult<()>;

    /// Subscribes to blocks from a peer after last_received round.
    async fn subscribe_blocks(
        &self,
        peer: AuthorityIndex,
        last_received: Round,
        timeout: Duration,
    ) -> ConsensusResult<BlockStream>;

    /// Fetches serialized `SignedBlock`s from a peer.
    // TODO: add a parameter for maximum total size of blocks returned.
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
    async fn handle_subscribe_blocks(
        &self,
        peer: AuthorityIndex,
        last_received: Round,
    ) -> ConsensusResult<BlockStream>;
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
