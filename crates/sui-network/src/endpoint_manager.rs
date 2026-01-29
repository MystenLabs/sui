// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anemo::types::{PeerAffinity, PeerInfo};
use arc_swap::ArcSwapOption;
use mysten_network::Multiaddr;
use sui_types::crypto::NetworkPublicKey;
use tap::TapFallible;
use tracing::warn;

use crate::discovery;

/// EndpointManager can be used to dynamically update the addresses of
/// other nodes in the network.
#[derive(Clone)]
pub struct EndpointManager {
    discovery_handle: discovery::Handle,
    consensus_address_updater: Arc<ArcSwapOption<Arc<dyn ConsensusAddressUpdater>>>,
}

pub trait ConsensusAddressUpdater: Send + Sync + 'static {
    fn update(&self, network_pubkey: NetworkPublicKey, addresses: Vec<Multiaddr>);
}

impl EndpointManager {
    pub fn new(discovery_handle: discovery::Handle) -> Self {
        Self {
            discovery_handle,
            consensus_address_updater: Arc::new(ArcSwapOption::empty()),
        }
    }

    pub fn set_consensus_address_updater(
        &self,
        consensus_address_updater: Arc<dyn ConsensusAddressUpdater>,
    ) {
        self.consensus_address_updater
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
    ) {
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
                self.discovery_handle
                    .peer_address_change(peer_id, source, anemo_addresses);
            }
            EndpointId::Consensus(network_pubkey) => {
                if addresses.is_empty() {
                    warn!(?network_pubkey, "No addresses provided for consensus peer");
                    return;
                }

                if let Some(updater) = self.consensus_address_updater.load_full() {
                    updater.update(network_pubkey, addresses);
                } else {
                    warn!(
                        ?network_pubkey,
                        "Consensus address updater not configured, ignoring update"
                    );
                }
            }
        }
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
