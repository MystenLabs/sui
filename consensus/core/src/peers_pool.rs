// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use consensus_config::AuthorityIndex;
use parking_lot::RwLock;

use crate::{
    context::Context,
    error::{ConsensusError, ConsensusResult},
    network::{NodeId, PeerId},
};

/// The server types that a peer can support.
/// A peer may support multiple server types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PeerServer {
    /// The validator consensus server for validator-to-validator communication
    Validator,
    /// The observer server for streaming blocks and serving observer requests
    #[allow(dead_code)]
    Observer,
}

/// Information about a registered peer
#[derive(Debug, Clone)]
struct PeerInfo {
    /// The server types this peer supports
    supported_servers: BTreeSet<PeerServer>,
}

/// Pool of available peers (both validators and observers) that can be used for communication.
/// This pool tracks which peers are registered, what servers they support, and their availability for synchronization.
///
/// The pool is used to:
/// - Register peers with their supported servers when they connect
/// - Check if a peer is available and supports the required server
/// - Remove peers
/// - Get lists of available peers filtered by server support
pub(crate) struct PeersPool {
    /// Registered peers with their supported servers
    registered_peers: RwLock<BTreeMap<PeerId, PeerInfo>>,
    /// Context for accessing committee information and current node type
    context: Arc<Context>,
}

impl PeersPool {
    // Initializes the peers pool with the committee as peers with the Validator server as available.
    pub(crate) fn new(context: Arc<Context>) -> Self {
        let s = Self {
            registered_peers: RwLock::new(BTreeMap::new()),
            context: context.clone(),
        };

        // Register the committee as peers with the Validator server as available.
        for (index, _) in context.committee.authorities() {
            s.register_validator(index, vec![PeerServer::Validator])
                .unwrap();
        }

        // Register all the observer peers in the peers pool
        for peer in context.parameters.tonic.observer_peers.iter() {
            // If the peer's public key is in the committe, then register it as a validator.
            if let Some((index, _)) = context
                .committee
                .authorities()
                .find(|(_, authority)| authority.network_key == peer.public_key)
            {
                s.register_validator(index, vec![PeerServer::Validator, PeerServer::Observer])
                    .expect("Failed to register validator");
            } else {
                // Otherwise this is another Observer peer and we register it as such. This peer we know that
                // it will support the Observer server.
                s.register_observer(peer.public_key.clone());
            }
        }

        s
    }

    /// Registers a validator as available in the pool with its supported servers.
    /// Note: this method will override the peer if it already exists in the pool.
    #[allow(dead_code)]
    pub(crate) fn register_validator(
        &self,
        authority_index: AuthorityIndex,
        mut supported_servers: Vec<PeerServer>,
    ) -> ConsensusResult<()> {
        if !self.context.committee.is_valid_index(authority_index) {
            return Err(ConsensusError::InvalidAuthorityIndex {
                index: authority_index,
                max: self.context.committee.size(),
            });
        }

        // We always ensure that whoever registers a validator supports the Validator server.
        if !supported_servers.contains(&PeerServer::Validator) {
            supported_servers.push(PeerServer::Validator);
        }
        let mut peers = self.registered_peers.write();
        peers.insert(
            PeerId::Validator(authority_index),
            PeerInfo {
                supported_servers: supported_servers.into_iter().collect(),
            },
        );

        Ok(())
    }

    /// Registers an observer as available in the pool.
    /// Note that we do not allow observers to register with supported servers other than Observer. This is done for safety
    /// as only validators are supposed to support both the Validator and Observer servers.
    #[allow(dead_code)]
    pub(crate) fn register_observer(&self, node_id: NodeId) {
        let mut peers = self.registered_peers.write();
        peers.insert(
            PeerId::Observer(node_id),
            PeerInfo {
                supported_servers: vec![PeerServer::Observer].into_iter().collect(),
            },
        );
    }

    /// Removes a peer from the pool
    #[allow(dead_code)]
    pub(crate) fn remove_peer(&self, peer: &PeerId) {
        let mut peers = self.registered_peers.write();
        peers.remove(peer);
    }

    /// Checks if a peer is available for fetching blocks/commits
    /// Takes into account:
    /// - For validators: always available unless it's our own node
    /// - For observers: must be registered in the pool
    /// - The peer must support at least one of the required servers
    /// - The peer is not our own node (when a validator)
    pub(crate) fn is_peer_available(&self, peer: &PeerId) -> bool {
        self.is_peer_available_for_servers(peer, &[])
    }

    /// Checks if a peer is available and supports at least one of the specified servers.
    /// If no servers are specified, just checks basic availability.
    pub(crate) fn is_peer_available_for_servers(
        &self,
        peer: &PeerId,
        required_servers: &[PeerServer],
    ) -> bool {
        // Don't fetch from ourselves
        if let PeerId::Validator(index) = peer
            && *index == self.context.own_index
        {
            return false;
        }

        // Check if peer is registered
        let peers = self.registered_peers.read();
        if let Some(info) = peers.get(peer) {
            // If no specific server required, peer is available
            if required_servers.is_empty() {
                true
            } else {
                // Check if peer supports any required server
                required_servers
                    .iter()
                    .any(|server| info.supported_servers.contains(server))
            }
        } else {
            // Peer not registered (should not happen for validators after initialization)
            false
        }
    }

    /// Gets all available peers based that are compatible with the current node.
    #[allow(dead_code)]
    pub(crate) fn get_available_peers(&self) -> Vec<PeerId> {
        let compatible_servers = self.get_compatible_servers();
        self.get_available_peers_for_servers(&compatible_servers)
    }

    /// Gets available peers that support at least one of the specified servers
    ///
    /// When an empty `filter_servers` is provided, all available peers are returned.
    ///
    /// Note: This method does NOT check node compatibility. If you want to ensure
    /// only compatible peers are returned, use `get_available_peers()`
    #[allow(dead_code)]
    pub(crate) fn get_available_peers_for_servers(
        &self,
        filter_servers: &[PeerServer],
    ) -> Vec<PeerId> {
        let mut peers = Vec::new();
        let registered = self.registered_peers.read();

        for (peer, info) in registered.iter() {
            // Skip our own validator index
            if let PeerId::Validator(index) = peer
                && *index == self.context.own_index
            {
                continue;
            }

            // Check if peer supports any of the required servers
            if filter_servers.is_empty()
                || filter_servers
                    .iter()
                    .any(|server| info.supported_servers.contains(server))
            {
                peers.push(peer.clone());
            }
        }

        peers
    }

    /// Gets the compatible servers based on the current node type
    /// - If we're a Validator node: can talk to both Validator and Observer servers
    /// - If we're an Observer node: can only talk to Observer servers
    #[allow(dead_code)]
    fn get_compatible_servers(&self) -> Vec<PeerServer> {
        if self.context.is_validator() {
            // Validators can talk to both Validator and Observer servers
            vec![PeerServer::Validator, PeerServer::Observer]
        } else {
            // Observers can only talk to Observer servers
            vec![PeerServer::Observer]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use consensus_config::{AuthorityIndex, NetworkKeyPair};

    #[test]
    fn test_peers_pool_basic() {
        let (mut context, _) = Context::new_for_test(4);
        context.own_index = AuthorityIndex::new_for_test(0);
        // This is a validator node (own_index is within committee range)
        let context = Arc::new(context);
        let pool = PeersPool::new(context.clone());

        // Register some observers
        let observer1 = NetworkKeyPair::generate(&mut rand::thread_rng()).public();
        let observer2 = NetworkKeyPair::generate(&mut rand::thread_rng()).public();
        pool.register_observer(observer1.clone());
        pool.register_observer(observer2.clone());

        // Check they're available
        assert!(pool.is_peer_available(&PeerId::Observer(observer1.clone())));
        assert!(pool.is_peer_available(&PeerId::Observer(observer2.clone())));

        // Validators should be available (except our own)
        assert!(!pool.is_peer_available(&PeerId::Validator(AuthorityIndex::new_for_test(0))));
        assert!(pool.is_peer_available(&PeerId::Validator(AuthorityIndex::new_for_test(1))));

        // Check server filtering
        let validator_peers = pool.get_available_peers_for_servers(&[PeerServer::Validator]);
        // Should have 3 validators (excluding our own index 0)
        assert_eq!(validator_peers.len(), 3);

        let observer_peers = pool.get_available_peers_for_servers(&[PeerServer::Observer]);
        // Should have 2 observers
        assert_eq!(observer_peers.len(), 2);
    }

    #[test]
    fn test_peers_pool_remove() {
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);
        let pool = PeersPool::new(context.clone());

        let observer = NetworkKeyPair::generate(&mut rand::thread_rng()).public();
        let observer_peer = PeerId::Observer(observer.clone());

        // Register and verify available
        pool.register_observer(observer);
        assert!(pool.is_peer_available(&observer_peer));

        // Remove and verify not available
        pool.remove_peer(&observer_peer);
        assert!(!pool.is_peer_available(&observer_peer));
    }

    #[test]
    fn test_get_available_peers_as_validator() {
        let (mut context, _) = Context::new_for_test(3);
        context.own_index = AuthorityIndex::new_for_test(1);
        // This is a validator node (own_index is within committee range)
        let context = Arc::new(context);
        let pool = PeersPool::new(context.clone());

        let observer = NetworkKeyPair::generate(&mut rand::thread_rng()).public();
        pool.register_observer(observer.clone());

        let peers = pool.get_available_peers();
        // As a validator, should have 2 validators (0 and 2, excluding our own 1) and 1 observer
        assert_eq!(peers.len(), 3);
        assert!(peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(0))));
        assert!(peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(2))));
        assert!(peers.contains(&PeerId::Observer(observer)));
        assert!(!peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(1))));
    }

    #[test]
    fn test_get_available_peers_as_observer() {
        let (mut context, _) = Context::new_for_test(3);
        // Set own_index to a value outside committee range to simulate an observer node
        context.own_index = AuthorityIndex::new_for_test(10);
        let context = Arc::new(context);
        let pool = PeersPool::new(context.clone());

        // Register an observer
        let observer1 = NetworkKeyPair::generate(&mut rand::thread_rng()).public();
        pool.register_observer(observer1.clone());

        // Register a validator with Observer server (e.g., it has an observer port open)
        pool.register_validator(AuthorityIndex::new_for_test(1), vec![PeerServer::Observer])
            .unwrap();

        let peers = pool.get_available_peers();
        // As an observer, should only see peers that support Observer server
        assert_eq!(peers.len(), 2);
        assert!(peers.contains(&PeerId::Observer(observer1)));
        assert!(peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(1))));

        // Should not see validators that only support Validator server
        assert!(!peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(2))));
    }

    #[test]
    fn test_compatible_server_filtering() {
        let (mut context, _) = Context::new_for_test(3);
        // Set as an observer node
        context.own_index = AuthorityIndex::new_for_test(10);
        let context = Arc::new(context);
        let pool = PeersPool::new(context.clone());

        // Register a validator with both servers
        pool.register_validator(
            AuthorityIndex::new_for_test(0),
            vec![PeerServer::Validator, PeerServer::Observer],
        )
        .unwrap();

        // Register an observer
        let observer = NetworkKeyPair::generate(&mut rand::thread_rng()).public();
        pool.register_observer(observer.clone());

        // As an observer node, get_available_peers() should only return peers that support Observer server
        // (because observers can only communicate with Observer servers)
        let available_peers = pool.get_available_peers();
        assert_eq!(available_peers.len(), 2); // The validator with Observer server and the observer
        assert!(available_peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(0))));
        assert!(available_peers.contains(&PeerId::Observer(observer.clone())));

        // Direct filtering for Validator server should still return the peers that support it,
        // but this doesn't check compatibility
        let validator_peers = pool.get_available_peers_for_servers(&[PeerServer::Validator]);
        // Should have all validators (3 total, but our node with index 10 is not in the committee)
        // Plus the validator we just registered with both servers
        assert!(validator_peers.len() >= 3);

        // Filtering for Observer server should return peers that support it
        let observer_peers = pool.get_available_peers_for_servers(&[PeerServer::Observer]);
        assert_eq!(observer_peers.len(), 2); // The validator with Observer server and the observer
        assert!(observer_peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(0))));
        assert!(observer_peers.contains(&PeerId::Observer(observer)));
    }

    #[test]
    fn test_server_filtering() {
        let (mut context, _) = Context::new_for_test(4);
        context.own_index = AuthorityIndex::new_for_test(0);
        // This is a validator node (own_index is within committee range)
        let context = Arc::new(context);
        let pool = PeersPool::new(context.clone());

        // Register observers
        let observer1 = NetworkKeyPair::generate(&mut rand::thread_rng()).public();
        let observer2 = NetworkKeyPair::generate(&mut rand::thread_rng()).public();
        pool.register_observer(observer1.clone());
        // Note: register_observer only sets Observer server, cannot add Validator server to observers
        pool.register_observer(observer2.clone());

        // Register a validator with both servers
        pool.register_validator(
            AuthorityIndex::new_for_test(1),
            vec![PeerServer::Validator, PeerServer::Observer],
        )
        .unwrap();

        // Filter for Validator server only
        let validator_peers = pool.get_available_peers_for_servers(&[PeerServer::Validator]);
        // Should get: validator 1 (registered with both), validators 2 and 3 (default with Validator)
        // Note: observers cannot have Validator server with the new API
        assert_eq!(validator_peers.len(), 3);

        // Filter for Observer server only
        let observer_peers = pool.get_available_peers_for_servers(&[PeerServer::Observer]);
        // Should get: observer1, observer2, and validator 1
        assert_eq!(observer_peers.len(), 3);
        assert!(observer_peers.contains(&PeerId::Observer(observer1)));
        assert!(observer_peers.contains(&PeerId::Observer(observer2)));
        assert!(observer_peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(1))));
    }
}
