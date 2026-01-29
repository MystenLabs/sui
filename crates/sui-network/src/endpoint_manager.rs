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
    fn update(&self, network_pubkey: NetworkPublicKey, addresses: Vec<Multiaddr>) -> SuiResult<()>;
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
                if addresses.is_empty() {
                    warn!(?network_pubkey, "No addresses provided for consensus peer");
                    return Err(SuiErrorKind::GenericAuthorityError {
                        error: "No addresses provided for consensus peer".to_string(),
                    }
                    .into());
                }

                if let Some(updater) = self.inner.consensus_address_updater.load_full() {
                    updater
                        .update(network_pubkey.clone(), addresses)
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
