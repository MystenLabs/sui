// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module defines the network interface, and provides network implementations for the
//! consensus protocol.
//!
//! Having an abstract network interface allows
//! - simplifying the semantics of sending data and serving requests over the network
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
use consensus_config::{AuthorityIndex, NetworkKeyPair, NetworkPublicKey};
use consensus_types::block::{BlockRef, Round};
use futures::Stream;
use mysten_network::Multiaddr;

use crate::{
    block::{ExtendedBlock, VerifiedBlock},
    commit::{CommitRange, TrustedCommit},
    context::Context,
    error::ConsensusResult,
};

/// Identifies an observer node by its network public key.
pub(crate) type NodeId = NetworkPublicKey;

// Tonic generated RPC stubs.
mod tonic_gen {
    include!(concat!(env!("OUT_DIR"), "/consensus.ConsensusService.rs"));
    include!(concat!(env!("OUT_DIR"), "/consensus.ObserverService.rs"));
}

pub(crate) mod metrics;
mod metrics_layer;
#[cfg(all(test, not(msim)))]
mod network_tests;
#[cfg(not(msim))]
pub(crate) mod observer;
#[cfg(msim)]
pub mod observer;
#[cfg(test)]
pub(crate) mod test_network;
#[cfg(not(msim))]
pub(crate) mod tonic_network;
#[cfg(msim)]
pub mod tonic_network;
mod tonic_tls;

/// A stream of serialized filtered blocks returned over the network.
pub(crate) type BlockStream = Pin<Box<dyn Stream<Item = ExtendedSerializedBlock> + Send>>;

/// Validator network client for communicating with validator peers.
///
/// NOTE: the timeout parameters help saving resources at client and potentially server.
/// But it is up to the server implementation if the timeout is honored.
/// - To bound server resources, server should implement own timeout for incoming requests.
#[async_trait]
pub(crate) trait ValidatorNetworkClient: Send + Sync + Sized + 'static {
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
        breadth_first: bool,
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

    /// Sends a serialized SignedBlock to a peer.
    #[cfg(test)]
    async fn send_block(
        &self,
        peer: AuthorityIndex,
        block: &VerifiedBlock,
        timeout: Duration,
    ) -> ConsensusResult<()>;
}

/// Validator network service for handling requests from validator peers.
#[async_trait]
pub(crate) trait ValidatorNetworkService: Send + Sync + 'static {
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
        breadth_first: bool,
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

/// Observer network service for handling requests from observer nodes.
/// Unlike ValidatorNetworkService which uses AuthorityIndex, this uses NodeId (NetworkPublicKey)
/// to identify peers since observers are not part of the committee.
#[async_trait]
pub(crate) trait ObserverNetworkService: Send + Sync + 'static {
    /// Handles the request to fetch blocks by references from an observer peer.
    async fn handle_fetch_blocks(
        &self,
        peer: NodeId,
        block_refs: Vec<BlockRef>,
    ) -> ConsensusResult<Vec<Bytes>>;

    /// Handles the request to fetch commits by index range from an observer peer.
    async fn handle_fetch_commits(
        &self,
        peer: NodeId,
        commit_range: CommitRange,
    ) -> ConsensusResult<(Vec<TrustedCommit>, Vec<VerifiedBlock>)>;
}

/// An `AuthorityNode` holds a `NetworkManager` until shutdown.
/// Dropping `NetworkManager` will shutdown the network service.
pub(crate) trait NetworkManager: Send + Sync {
    type ValidatorClient: ValidatorNetworkClient;

    /// Creates a new network manager.
    fn new(context: Arc<Context>, network_keypair: NetworkKeyPair) -> Self;

    /// Returns the validator network client.
    fn validator_client(&self) -> Arc<Self::ValidatorClient>;

    /// Starts the validator network server with the provided service.
    async fn start_validator_server<V>(&mut self, service: Arc<V>)
    where
        V: ValidatorNetworkService;

    /// Starts the observer network server with the provided service.
    async fn start_observer_server<O>(&mut self, service: Arc<O>)
    where
        O: ObserverNetworkService;

    /// Stops the network service.
    async fn stop(&mut self);

    /// Updates the network address for a peer identified by their authority index.
    /// If address is None, the override is cleared and the committee address will be used.
    fn update_peer_address(&self, peer: AuthorityIndex, address: Option<Multiaddr>);
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
