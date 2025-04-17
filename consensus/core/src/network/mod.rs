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
    block::{BlockRef, ExtendedBlock, VerifiedBlock},
    commit::{CommitRange, TrustedCommit},
    context::Context,
    error::ConsensusResult,
    Round,
};

// Anemo generated RPC stubs.
mod anemo_gen {
    include!(concat!(env!("OUT_DIR"), "/consensus.ConsensusRpc.rs"));
}

// Tonic generated RPC stubs.
mod tonic_gen {
    include!(concat!(env!("OUT_DIR"), "/consensus.ConsensusService.rs"));
}

pub mod connection_monitor;

pub(crate) mod anemo_network;
pub(crate) mod epoch_filter;
pub(crate) mod metrics;
mod metrics_layer;
#[cfg(all(test, not(msim)))]
mod network_tests;
#[cfg(test)]
pub(crate) mod test_network;
#[cfg(not(msim))]
pub(crate) mod tonic_network;
#[cfg(msim)]
pub mod tonic_network;
mod tonic_tls;

/// A stream of serialized filtered blocks returned over the network.
pub(crate) type BlockStream = Pin<Box<dyn Stream<Item = ExtendedSerializedBlock> + Send>>;

/// Network client for communicating with peers.
///
/// NOTE: the timeout parameters help saving resources at client and potentially server.
/// But it is up to the server implementation if the timeout is honored.
/// - To bound server resources, server should implement own timeout for incoming requests.
#[async_trait]
pub(crate) trait NetworkClient: Send + Sync + Sized + 'static {
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

    // TODO: add a parameter for maximum total size of blocks returned.
    /// Fetches serialized `SignedBlock`s from a peer. It also might return additional ancestor blocks
    /// of the requested blocks according to the provided `highest_accepted_rounds`. The `highest_accepted_rounds`
    /// length should be equal to the committee size. If `highest_accepted_rounds` is empty then it will
    /// be simply ignored.
    async fn fetch_blocks(
        &self,
        peer: AuthorityIndex,
        block_refs: Vec<BlockRef>,
        highest_accepted_rounds: Vec<Round>,
        timeout: Duration,
    ) -> ConsensusResult<Vec<Bytes>>;

    /// Fetches serialized commits in the commit range from a peer.
    /// Returns a tuple of both the serialized commits, and serialized blocks that contain
    /// votes certifying the last commit.
    async fn fetch_commits(
        &self,
        peer: AuthorityIndex,
        commit_range: CommitRange,
        timeout: Duration,
    ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)>;

    /// Fetches the latest block from `peer` for the requested `authorities`. The latest blocks
    /// are returned in the serialised format of `SignedBlocks`. The method can return multiple
    /// blocks per peer as its possible to have equivocations.
    async fn fetch_latest_blocks(
        &self,
        peer: AuthorityIndex,
        authorities: Vec<AuthorityIndex>,
        timeout: Duration,
    ) -> ConsensusResult<Vec<Bytes>>;

    /// Gets the latest received & accepted rounds of all authorities from the peer.
    async fn get_latest_rounds(
        &self,
        peer: AuthorityIndex,
        timeout: Duration,
    ) -> ConsensusResult<(Vec<Round>, Vec<Round>)>;
}

/// Network service for handling requests from peers.
/// NOTE: using `async_trait` macro because `NetworkService` methods are called in the trait impl
/// of `anemo_gen::ConsensusRpc`, which itself is annotated with `async_trait`.
#[async_trait]
pub(crate) trait NetworkService: Send + Sync + 'static {
    /// Handles the block sent from the peer via either unicast RPC or subscription stream.
    /// Peer value can be trusted to be a valid authority index.
    /// But serialized_block must be verified before its contents are trusted.
    /// Excluded ancestors are also included as part of an effort to further propagate
    /// blocks to peers despite the current exclusion.
    async fn handle_send_block(
        &self,
        peer: AuthorityIndex,
        block: ExtendedSerializedBlock,
    ) -> ConsensusResult<()>;

    /// Handles the subscription request from the peer.
    /// A stream of newly proposed blocks is returned to the peer.
    /// The stream continues until the end of epoch, peer unsubscribes, or a network error / crash
    /// occurs.
    async fn handle_subscribe_blocks(
        &self,
        peer: AuthorityIndex,
        last_received: Round,
    ) -> ConsensusResult<BlockStream>;

    /// Handles the request to fetch blocks by references from the peer.
    async fn handle_fetch_blocks(
        &self,
        peer: AuthorityIndex,
        block_refs: Vec<BlockRef>,
        highest_accepted_rounds: Vec<Round>,
    ) -> ConsensusResult<Vec<Bytes>>;

    /// Handles the request to fetch commits by index range from the peer.
    async fn handle_fetch_commits(
        &self,
        peer: AuthorityIndex,
        commit_range: CommitRange,
    ) -> ConsensusResult<(Vec<TrustedCommit>, Vec<VerifiedBlock>)>;

    /// Handles the request to fetch the latest block for the provided `authorities`.
    async fn handle_fetch_latest_blocks(
        &self,
        peer: AuthorityIndex,
        authorities: Vec<AuthorityIndex>,
    ) -> ConsensusResult<Vec<Bytes>>;

    /// Handles the request to get the latest received & accepted rounds of all authorities.
    async fn handle_get_latest_rounds(
        &self,
        peer: AuthorityIndex,
    ) -> ConsensusResult<(Vec<Round>, Vec<Round>)>;
}

/// An `AuthorityNode` holds a `NetworkManager` until shutdown.
/// Dropping `NetworkManager` will shutdown the network service.
pub(crate) trait NetworkManager<S>: Send + Sync
where
    S: NetworkService,
{
    type Client: NetworkClient;

    /// Creates a new network manager.
    fn new(context: Arc<Context>, network_keypair: NetworkKeyPair) -> Self;

    /// Returns the network client.
    fn client(&self) -> Arc<Self::Client>;

    /// Installs network service.
    async fn install_service(&mut self, service: Arc<S>);

    /// Stops the network service.
    async fn stop(&mut self);
}

/// Serialized block with extended information from the proposing authority.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct ExtendedSerializedBlock {
    pub(crate) block: Bytes,
    // Serialized BlockRefs that are excluded from the blocks ancestors.
    pub(crate) excluded_ancestors: Vec<Vec<u8>>,
}

impl From<ExtendedBlock> for ExtendedSerializedBlock {
    fn from(extended_block: ExtendedBlock) -> Self {
        Self {
            block: extended_block.block.serialized().clone(),
            excluded_ancestors: extended_block
                .excluded_ancestors
                .iter()
                .filter_map(|r| match bcs::to_bytes(r) {
                    Ok(serialized) => Some(serialized),
                    Err(e) => {
                        tracing::debug!("Failed to serialize block ref {:?}: {e:?}", r);
                        None
                    }
                })
                .collect(),
        }
    }
}
