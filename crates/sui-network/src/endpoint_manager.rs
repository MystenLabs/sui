// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use arc_swap::ArcSwapOption;
use mysten_network::Multiaddr;
use sui_types::crypto::NetworkPublicKey;
use sui_types::error::{SuiErrorKind, SuiResult};
use tap::TapFallible;
use tracing::warn;

use crate::discovery;

/// EndpointManager can be used to dynamically update the addresses of
/// other nodes in the network.
#[derive(Clone)]
pub struct EndpointManager {
    inner: Arc<Inner>,
}

struct Inner {
    discovery_handle: discovery::Handle,
    consensus_address_updater: ArcSwapOption<Arc<dyn ConsensusAddressUpdater>>,
}

pub trait ConsensusAddressUpdater: Send + Sync + 'static {
    fn update(
        &self,
        network_pubkey: NetworkPublicKey,
        source: AddressSource,
        addresses: Vec<Multiaddr>,
    ) -> SuiResult<()>;
}

impl EndpointManager {
    pub fn new(discovery_handle: discovery::Handle) -> Self {
        Self {
            inner: Arc::new(Inner {
                discovery_handle,
                consensus_address_updater: ArcSwapOption::empty(),
            }),
        }
    }

    pub fn set_consensus_address_updater(
        &self,
        consensus_address_updater: Arc<dyn ConsensusAddressUpdater>,
    ) {
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
                    .discovery_handle
                    .peer_address_change(peer_id, source, anemo_addresses);
            }
            EndpointId::Consensus(network_pubkey) => {
                if let Some(updater) = self.inner.consensus_address_updater.load_full() {
                    updater
                        .update(network_pubkey.clone(), source, addresses)
                        .map_err(|e| {
                            warn!(?network_pubkey, "Error updating consensus address: {e:?}");
                            e
                        })?;
                } else {
                    return Err(SuiErrorKind::GenericAuthorityError {
                        error: "Consensus address updater not configured".to_string(),
                    }
                    .into());
                }
            }
        }

        Ok(())
    }
}

pub enum EndpointId {
    P2p(anemo::PeerId),
    Consensus(NetworkPublicKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
// NOTE: AddressSources are prioritized in order of the enum variants below.
pub enum AddressSource {
    Admin,
    Config,
    Committee,
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
        fn update(
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

    // Mock discovery handle for testing
    fn create_mock_discovery_handle() -> discovery::Handle {
        use crate::utils::build_network_and_key;
        use sui_config::p2p::P2pConfig;

        let config = P2pConfig::default();
        let (unstarted, _server) = discovery::Builder::new().config(config).build();
        let (network, keypair) = build_network_and_key(|router| router);
        unstarted.start(network, keypair)
    }

    #[tokio::test]
    async fn test_update_consensus_endpoint() {
        let discovery_handle = create_mock_discovery_handle();
        let endpoint_manager = EndpointManager::new(discovery_handle);

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
    async fn test_update_consensus_endpoint_without_updater() {
        let discovery_handle = create_mock_discovery_handle();
        let endpoint_manager = EndpointManager::new(discovery_handle);

        let (_, network_key): (_, NetworkKeyPair) = get_key_pair();
        let network_pubkey = network_key.public();

        let addresses = vec!["/ip4/127.0.0.1/udp/9000".parse().unwrap()];

        let result = endpoint_manager.update_endpoint(
            EndpointId::Consensus(network_pubkey.clone()),
            AddressSource::Admin,
            addresses,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("Consensus address updater not configured")
        );
    }
}
