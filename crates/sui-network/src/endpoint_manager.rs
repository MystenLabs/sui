// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};

use arc_swap::ArcSwapOption;
use mysten_network::Multiaddr;
use serde::{Deserialize, Serialize};
use sui_types::crypto::{NetworkPublicKey, ToFromBytes};
use sui_types::error::SuiResult;
use tap::TapFallible;
use tracing::{info, warn};

use crate::discovery;

/// EndpointManager can be used to dynamically update the addresses of
/// other nodes in the network.
#[derive(Clone)]
pub struct EndpointManager {
    inner: Arc<Inner>,
}

struct Inner {
    discovery_sender: discovery::Sender,
    consensus_address_updater: ArcSwapOption<Arc<dyn ConsensusAddressUpdater>>,
    pending_consensus_updates: Mutex<Vec<(NetworkPublicKey, AddressSource, Vec<Multiaddr>)>>,
}

pub trait ConsensusAddressUpdater: Send + Sync + 'static {
    fn update_address(
        &self,
        network_pubkey: NetworkPublicKey,
        source: AddressSource,
        addresses: Vec<Multiaddr>,
    ) -> SuiResult<()>;
}

impl EndpointManager {
    pub fn new(discovery_sender: discovery::Sender) -> Self {
        Self {
            inner: Arc::new(Inner {
                discovery_sender,
                consensus_address_updater: ArcSwapOption::empty(),
                pending_consensus_updates: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn set_consensus_address_updater(
        &self,
        consensus_address_updater: Arc<dyn ConsensusAddressUpdater>,
    ) {
        let mut pending = self.inner.pending_consensus_updates.lock().unwrap();

        for (pubkey, source, addrs) in pending.drain(..) {
            if let Err(e) = consensus_address_updater.update_address(pubkey.clone(), source, addrs)
            {
                warn!(
                    ?pubkey,
                    "Error replaying buffered consensus address update: {e:?}"
                );
            }
        }

        self.inner
            .consensus_address_updater
            .store(Some(Arc::new(consensus_address_updater)));
    }

    /// Updates the address(es) for the given endpoint from the specified source.
    ///
    /// Multiple sources can provide addresses for the same peer. The highest-priority
    /// source's addresses are used. Empty `addresses` clears a source.
    pub fn update_endpoint(
        &self,
        endpoint: EndpointId,
        source: AddressSource,
        addresses: Vec<Multiaddr>,
    ) -> SuiResult<()> {
        match endpoint {
            EndpointId::P2p(peer_id) => {
                let anemo_addresses: Vec<_> = addresses
                    .into_iter()
                    .filter_map(|addr| {
                        addr.to_anemo_address()
                            .tap_err(|_| {
                                warn!(
                                    ?addr,
                                    "Skipping peer address: can't convert to anemo address"
                                )
                            })
                            .ok()
                    })
                    .collect();

                self.inner
                    .discovery_sender
                    .peer_address_change(peer_id, source, anemo_addresses);
            }
            EndpointId::Consensus(network_pubkey) => {
                // Lock first, then check updater — this must be atomic with
                // set_consensus_address_updater's drain-then-store sequence
                // to avoid a race where an update is buffered after the drain
                // but before the updater becomes visible.
                let mut pending = self.inner.pending_consensus_updates.lock().unwrap();
                if let Some(updater) = self.inner.consensus_address_updater.load_full() {
                    drop(pending);
                    updater
                        .update_address(network_pubkey.clone(), source, addresses)
                        .map_err(|e| {
                            warn!(?network_pubkey, "Error updating consensus address: {e:?}");
                            e
                        })?;
                } else {
                    info!(
                        ?network_pubkey,
                        "Buffering consensus address update (updater not yet set)"
                    );
                    pending.push((network_pubkey, source, addresses));
                }
            }
        }

        Ok(())
    }

    /// Clears the given address source for a peer across all endpoint types.
    pub fn clear_source(&self, peer_id: anemo::PeerId, source: AddressSource) {
        let _ = self.update_endpoint(EndpointId::P2p(peer_id), source, vec![]);
        if let Ok(network_pubkey) = NetworkPublicKey::from_bytes(&peer_id.0) {
            let _ = self.update_endpoint(EndpointId::Consensus(network_pubkey), source, vec![]);
        }

        // If adding a new EndpointId, make sure it's covered in this function.
        // (Unused fn below only serves to cause a build failure here if
        // a new variant is added without updating.)
        fn _assert_all_variants_handled(id: &EndpointId) {
            match id {
                EndpointId::P2p(_) | EndpointId::Consensus(_) => {}
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EndpointId {
    P2p(anemo::PeerId),
    Consensus(NetworkPublicKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
// NOTE: AddressSources are prioritized in order of the enum variants below.
pub enum AddressSource {
    Admin,     // override from admin server
    Config,    // override from config file
    Discovery, // address received from P2P peers via Discovery protocol
    Seed,      // locally-configured seed address
    Chain,     // public on-chain address
}

#[cfg(test)]
mod tests {
    use super::*;
    use fastcrypto::traits::KeyPair;
    use std::sync::{Arc, Mutex};
    use sui_types::crypto::{NetworkKeyPair, get_key_pair};

    type UpdateEntry = (NetworkPublicKey, Vec<Multiaddr>);
    // Mock consensus address updater for testing
    struct MockConsensusAddressUpdater {
        updates: Arc<Mutex<Vec<UpdateEntry>>>,
    }

    impl MockConsensusAddressUpdater {
        fn new() -> (Self, Arc<Mutex<Vec<UpdateEntry>>>) {
            let updates = Arc::new(Mutex::new(Vec::new()));
            let updater = Self {
                updates: updates.clone(),
            };
            (updater, updates)
        }
    }

    impl ConsensusAddressUpdater for MockConsensusAddressUpdater {
        fn update_address(
            &self,
            network_pubkey: NetworkPublicKey,
            _source: AddressSource,
            addresses: Vec<Multiaddr>,
        ) -> SuiResult<()> {
            self.updates
                .lock()
                .unwrap()
                .push((network_pubkey.clone(), addresses));
            Ok(())
        }
    }

    fn create_mock_endpoint_manager() -> EndpointManager {
        use sui_config::p2p::P2pConfig;

        let config = P2pConfig::default();
        let (_unstarted, _server, endpoint_manager) =
            discovery::Builder::new().config(config).build();
        endpoint_manager
    }

    #[tokio::test]
    async fn test_update_consensus_endpoint() {
        let endpoint_manager = create_mock_endpoint_manager();

        let (mock_updater, updates) = MockConsensusAddressUpdater::new();
        endpoint_manager.set_consensus_address_updater(Arc::new(mock_updater));

        let (_, network_key): (_, NetworkKeyPair) = get_key_pair();
        let network_pubkey = network_key.public();

        let addresses = vec![
            "/ip4/127.0.0.1/udp/9000".parse().unwrap(),
            "/ip4/127.0.0.1/udp/9001".parse().unwrap(),
        ];

        let result = endpoint_manager.update_endpoint(
            EndpointId::Consensus(network_pubkey.clone()),
            AddressSource::Admin,
            addresses.clone(),
        );

        assert!(result.is_ok());

        let recorded_updates = updates.lock().unwrap();
        assert_eq!(recorded_updates.len(), 1);
        assert_eq!(recorded_updates[0].0, network_pubkey.clone());
        assert_eq!(recorded_updates[0].1, addresses);
    }

    #[tokio::test]
    async fn test_update_consensus_endpoint_without_updater_buffers() {
        let endpoint_manager = create_mock_endpoint_manager();

        let (_, network_key): (_, NetworkKeyPair) = get_key_pair();
        let network_pubkey = network_key.public();

        let addresses = vec!["/ip4/127.0.0.1/udp/9000".parse().unwrap()];

        // Should succeed (buffered) even without an updater set.
        let result = endpoint_manager.update_endpoint(
            EndpointId::Consensus(network_pubkey.clone()),
            AddressSource::Discovery,
            addresses.clone(),
        );
        assert!(result.is_ok());

        // Now set the updater and verify the buffered update was replayed.
        let (mock_updater, updates) = MockConsensusAddressUpdater::new();
        endpoint_manager.set_consensus_address_updater(Arc::new(mock_updater));

        let recorded_updates = updates.lock().unwrap();
        assert_eq!(recorded_updates.len(), 1);
        assert_eq!(recorded_updates[0].0, network_pubkey.clone());
        assert_eq!(recorded_updates[0].1, addresses);
    }

    #[tokio::test]
    async fn test_concurrent_update_endpoint_and_set_updater_no_lost_updates() {
        use std::sync::Barrier;

        let endpoint_manager = create_mock_endpoint_manager();

        let num_buffered = 5;
        let num_concurrent = 20;

        // Buffer some updates before the updater is set.
        for _ in 0..num_buffered {
            let (_, network_key): (_, NetworkKeyPair) = get_key_pair();
            endpoint_manager
                .update_endpoint(
                    EndpointId::Consensus(network_key.public().clone()),
                    AddressSource::Discovery,
                    vec!["/ip4/127.0.0.1/udp/9000".parse().unwrap()],
                )
                .unwrap();
        }

        // Use a barrier so all concurrent threads start at the same time.
        let barrier = Arc::new(Barrier::new(num_concurrent + 1));
        let mut handles = Vec::new();

        // Spawn threads that call update_endpoint concurrently.
        for i in 0..num_concurrent {
            let em = endpoint_manager.clone();
            let b = barrier.clone();
            let (_, network_key): (_, NetworkKeyPair) = get_key_pair();
            let pubkey = network_key.public().clone();
            handles.push(std::thread::spawn(move || {
                b.wait();
                // Small stagger so some threads race with set_consensus_address_updater.
                if i % 2 == 0 {
                    std::thread::yield_now();
                }
                em.update_endpoint(
                    EndpointId::Consensus(pubkey),
                    AddressSource::Discovery,
                    vec!["/ip4/127.0.0.1/udp/9000".parse().unwrap()],
                )
                .unwrap();
            }));
        }

        // Set the updater concurrently with the update_endpoint calls.
        let (mock_updater, updates) = MockConsensusAddressUpdater::new();
        let em = endpoint_manager.clone();
        let b = barrier.clone();
        let setter_handle = std::thread::spawn(move || {
            b.wait();
            em.set_consensus_address_updater(Arc::new(mock_updater));
        });

        for h in handles {
            h.join().unwrap();
        }
        setter_handle.join().unwrap();

        let recorded = updates.lock().unwrap();
        assert_eq!(
            recorded.len(),
            num_buffered + num_concurrent,
            "expected {} updates but got {} — some were lost",
            num_buffered + num_concurrent,
            recorded.len(),
        );
    }
}
