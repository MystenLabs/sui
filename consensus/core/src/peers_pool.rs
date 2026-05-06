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

/// The service types that a peer can support.
/// A peer may support multiple service types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PeerService {
    /// The validator consensus service for validator-to-validator communication
    Validator,
    /// The observer service for streaming blocks and serving observer requests
    #[allow(dead_code)]
    Observer,
}

/// Information about a registered peer
#[derive(Debug, Clone)]
struct PeerInfo {
    /// The services this peer supports
    supported_services: BTreeSet<PeerService>,
}

/// Pool of known peers (both validators and observers) that can be used for communication.
/// This pool tracks which peers are registered, what services they support, and their availability for synchronization.
///
/// The pool is used to:
/// - Register peers with their supported services when they connect
/// - Check if a peer is known and supports the required service
/// - Remove peers
/// - Get lists of known peers filtered by service support
pub(crate) struct PeersPool {
    /// Registered peers with their supported services
    registered_peers: RwLock<BTreeMap<PeerId, PeerInfo>>,
    /// Context for accessing committee information and current node type
    context: Arc<Context>,
}

impl PeersPool {
    // Initializes the peers pool with the committee as peers with the Validator service as known.
    pub(crate) fn new(context: Arc<Context>) -> Self {
        let s = Self {
            registered_peers: RwLock::new(BTreeMap::new()),
            context: context.clone(),
        };

        // Register the committee as peers with the Validator service as known.
        for (index, _) in context.committee.authorities() {
            s.register_validator(index, vec![PeerService::Validator])
                .unwrap();
        }

        // Register all the observer peers in the peers pool
        for peer in context.parameters.observer.peers.iter() {
            // If the peer's public key is in the committe, then register it as a validator.
            if let Some((index, _)) = context
                .committee
                .authorities()
                .find(|(_, authority)| authority.network_key == peer.public_key)
            {
                s.register_validator(index, vec![PeerService::Validator, PeerService::Observer])
                    .expect("Failed to register validator");
            } else {
                // Otherwise this is another Observer peer and we register it as such. This peer we know that
                // it will support the Observer service.
                s.register_observer(peer.public_key.clone());
            }
        }

        s
    }

    /// Registers a validator as known in the pool with its supported services.
    /// Note: this method will override the peer if it already exists in the pool.
    #[allow(dead_code)]
    pub(crate) fn register_validator(
        &self,
        authority_index: AuthorityIndex,
        mut supported_services: Vec<PeerService>,
    ) -> ConsensusResult<()> {
        if !self.context.committee.is_valid_index(authority_index) {
            return Err(ConsensusError::InvalidAuthorityIndex {
                index: authority_index,
                max: self.context.committee.size(),
            });
        }

        // We always ensure that whoever registers a validator supports the Validator service.
        if !supported_services.contains(&PeerService::Validator) {
            supported_services.push(PeerService::Validator);
        }
        let mut peers = self.registered_peers.write();
        peers.insert(
            PeerId::Validator(authority_index),
            PeerInfo {
                supported_services: supported_services.into_iter().collect(),
            },
        );

        Ok(())
    }

    /// Registers an observer as known in the pool.
    /// Note that we do not allow observers to register with supported services other than Observer. This is done for safety
    /// as only validators are supposed to support both the Validator and Observer services.
    #[allow(dead_code)]
    pub(crate) fn register_observer(&self, node_id: NodeId) {
        let mut peers = self.registered_peers.write();
        peers.insert(
            PeerId::Observer(Box::new(node_id)),
            PeerInfo {
                supported_services: vec![PeerService::Observer].into_iter().collect(),
            },
        );
    }

    /// Removes a peer from the pool
    #[allow(dead_code)]
    pub(crate) fn remove_peer(&self, peer: &PeerId) {
        let mut peers = self.registered_peers.write();
        peers.remove(peer);
    }

    /// Checks if a peer is known for fetching blocks/commits
    /// Takes into account:
    /// - For validators: always known unless it's our own node
    /// - For observers: must be registered in the pool
    /// - The peer must support at least one of the required services
    /// - The peer is not our own node (when a validator)
    pub(crate) fn is_peer_known(&self, peer: &PeerId) -> bool {
        self.is_peer_known_for_services(peer, &[])
    }

    /// Checks if a peer is known and supports at least one of the specified services.
    /// If no services are specified, just checks basic availability.
    pub(crate) fn is_peer_known_for_services(
        &self,
        peer: &PeerId,
        required_services: &[PeerService],
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
            // If no specific service required, peer is known
            if required_services.is_empty() {
                true
            } else {
                // Check if peer supports any required service
                required_services
                    .iter()
                    .any(|service| info.supported_services.contains(service))
            }
        } else {
            // Peer not registered (should not happen for validators after initialization)
            false
        }
    }

    /// Gets all known peers that are compatible with the current node.
    #[allow(dead_code)]
    pub(crate) fn get_known_peers(&self) -> Vec<PeerId> {
        let compatible_services = self.get_compatible_services();
        self.get_known_peers_for_services(&compatible_services)
    }

    /// Gets known peers that support at least one of the specified services
    ///
    /// When an empty `filter_services` is provided, all known peers are returned.
    ///
    /// Note: This method does NOT check node compatibility. If you want to ensure
    /// only compatible peers are returned, use `get_known_peers()`
    #[allow(dead_code)]
    pub(crate) fn get_known_peers_for_services(
        &self,
        filter_services: &[PeerService],
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

            // Check if peer supports any of the required services
            if filter_services.is_empty()
                || filter_services
                    .iter()
                    .any(|service| info.supported_services.contains(service))
            {
                peers.push(peer.clone());
            }
        }

        peers
    }

    /// Gets the compatible services based on the current node type
    /// - If we're a Validator node: can talk to both Validator and Observer services
    /// - If we're an Observer node: can only talk to Observer services
    #[allow(dead_code)]
    fn get_compatible_services(&self) -> Vec<PeerService> {
        if self.context.is_validator() {
            vec![PeerService::Validator, PeerService::Observer]
        } else {
            vec![PeerService::Observer]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use consensus_config::{AuthorityIndex, NetworkKeyPair};

    #[tokio::test]
    async fn test_peers_pool_basic() {
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

        // Check they're known
        assert!(pool.is_peer_known(&PeerId::Observer(Box::new(observer1.clone()))));
        assert!(pool.is_peer_known(&PeerId::Observer(Box::new(observer2.clone()))));

        // Validators should be known (except our own)
        assert!(!pool.is_peer_known(&PeerId::Validator(AuthorityIndex::new_for_test(0))));
        assert!(pool.is_peer_known(&PeerId::Validator(AuthorityIndex::new_for_test(1))));

        // Check server filtering
        let validator_peers = pool.get_known_peers_for_services(&[PeerService::Validator]);
        // Should have 3 validators (excluding our own index 0)
        assert_eq!(validator_peers.len(), 3);

        let observer_peers = pool.get_known_peers_for_services(&[PeerService::Observer]);
        // Should have 2 observers
        assert_eq!(observer_peers.len(), 2);
    }

    #[tokio::test]
    async fn test_peers_pool_remove() {
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);
        let pool = PeersPool::new(context.clone());

        let observer = NetworkKeyPair::generate(&mut rand::thread_rng()).public();
        let observer_peer = PeerId::Observer(Box::new(observer.clone()));

        // Register and verify known
        pool.register_observer(observer);
        assert!(pool.is_peer_known(&observer_peer));

        // Remove and verify not known
        pool.remove_peer(&observer_peer);
        assert!(!pool.is_peer_known(&observer_peer));
    }

    #[tokio::test]
    async fn test_get_known_peers_as_validator() {
        let (mut context, _) = Context::new_for_test(3);
        context.own_index = AuthorityIndex::new_for_test(1);
        // This is a validator node (own_index is within committee range)
        let context = Arc::new(context);
        let pool = PeersPool::new(context.clone());

        let observer = NetworkKeyPair::generate(&mut rand::thread_rng()).public();
        pool.register_observer(observer.clone());

        let peers = pool.get_known_peers();
        // As a validator, should have 2 validators (0 and 2, excluding our own 1) and 1 observer
        assert_eq!(peers.len(), 3);
        assert!(peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(0))));
        assert!(peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(2))));
        assert!(peers.contains(&PeerId::Observer(Box::new(observer))));
        assert!(!peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(1))));
    }

    #[tokio::test]
    async fn test_get_known_peers_as_observer() {
        let (mut context, _) = Context::new_for_test(3);
        // Set own_index to a value outside committee range to simulate an observer node
        context.own_index = AuthorityIndex::new_for_test(10);
        let context = Arc::new(context);
        let pool = PeersPool::new(context.clone());

        // Register an observer
        let observer1 = NetworkKeyPair::generate(&mut rand::thread_rng()).public();
        pool.register_observer(observer1.clone());

        // Register a validator with Observer server (e.g., it has an observer port open)
        pool.register_validator(AuthorityIndex::new_for_test(1), vec![PeerService::Observer])
            .unwrap();

        let peers = pool.get_known_peers();
        // As an observer, should only see peers that support Observer server
        assert_eq!(peers.len(), 2);
        assert!(peers.contains(&PeerId::Observer(Box::new(observer1))));
        assert!(peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(1))));

        // Should not see validators that only support Validator server
        assert!(!peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(2))));
    }

    #[tokio::test]
    async fn test_compatible_server_filtering() {
        let (mut context, _) = Context::new_for_test(3);
        // Set as an observer node
        context.own_index = AuthorityIndex::new_for_test(10);
        let context = Arc::new(context);
        let pool = PeersPool::new(context.clone());

        // Register a validator with both servers
        pool.register_validator(
            AuthorityIndex::new_for_test(0),
            vec![PeerService::Validator, PeerService::Observer],
        )
        .unwrap();

        // Register an observer
        let observer = NetworkKeyPair::generate(&mut rand::thread_rng()).public();
        pool.register_observer(observer.clone());

        // As an observer node, get_known_peers() should only return peers that support Observer server
        // (because observers can only communicate with Observer servers)
        let known_peers = pool.get_known_peers();
        assert_eq!(known_peers.len(), 2); // The validator with Observer server and the observer
        assert!(known_peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(0))));
        assert!(known_peers.contains(&PeerId::Observer(Box::new(observer.clone()))));

        // Direct filtering for Validator server should still return the peers that support it,
        // but this doesn't check compatibility
        let validator_peers = pool.get_known_peers_for_services(&[PeerService::Validator]);
        // Should have all validators (3 total, but our node with index 10 is not in the committee)
        // Plus the validator we just registered with both servers
        assert!(validator_peers.len() >= 3);

        // Filtering for Observer server should return peers that support it
        let observer_peers = pool.get_known_peers_for_services(&[PeerService::Observer]);
        assert_eq!(observer_peers.len(), 2); // The validator with Observer server and the observer
        assert!(observer_peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(0))));
        assert!(observer_peers.contains(&PeerId::Observer(Box::new(observer))));
    }

    #[tokio::test]
    async fn test_server_filtering() {
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
            vec![PeerService::Validator, PeerService::Observer],
        )
        .unwrap();

        // Filter for Validator server only
        let validator_peers = pool.get_known_peers_for_services(&[PeerService::Validator]);
        // Should get: validator 1 (registered with both), validators 2 and 3 (default with Validator)
        // Note: observers cannot have Validator server with the new API
        assert_eq!(validator_peers.len(), 3);

        // Filter for Observer server only
        let observer_peers = pool.get_known_peers_for_services(&[PeerService::Observer]);
        // Should get: observer1, observer2, and validator 1
        assert_eq!(observer_peers.len(), 3);
        assert!(observer_peers.contains(&PeerId::Observer(Box::new(observer1))));
        assert!(observer_peers.contains(&PeerId::Observer(Box::new(observer2))));
        assert!(observer_peers.contains(&PeerId::Validator(AuthorityIndex::new_for_test(1))));
    }
}
