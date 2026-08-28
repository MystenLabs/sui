// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use lru::LruCache;
use mysten_common::debug_fatal;
use mysten_network::Multiaddr;
use serde::{Deserialize, Serialize};
use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};
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
    /// Last update dispatched to the discovery mailbox per P2P endpoint and
    /// source, used to suppress duplicate and stale (older-versioned) updates.
    last_sent_p2p: Mutex<LastSentCache<(anemo::PeerId, AddressSource)>>,
    consensus: Mutex<ConsensusState>,
}

/// Last accepted update for an (endpoint, source) key.
struct CachedUpdate {
    /// `None` for unversioned producers (admin, chain, seed).
    version: Option<u64>,
    addresses: Vec<Multiaddr>,
}

/// Per-key cache of the last dispatched update, used to suppress duplicate
/// and stale updates.
struct LastSentCache<K> {
    entries: LruCache<K, CachedUpdate>,
}

impl<K: std::hash::Hash + Eq> LastSentCache<K> {
    // Must comfortably exceed the live working set so hot keys are never
    // evicted: at most about 1,500 entries for a 150-validator committee
    // across both endpoint kinds and all five sources.
    const SIZE_CAP: usize = 16_384;

    fn new() -> Self {
        Self {
            entries: LruCache::new(
                NonZeroUsize::new(Self::SIZE_CAP).expect("cache capacity must be non-zero"),
            ),
        }
    }

    /// Returns true if a non-empty update is a duplicate or stale and should
    /// not be dispatched.
    fn suppresses(&mut self, key: &K, version: Option<u64>, addresses: &[Multiaddr]) -> bool {
        let Some(cached) = self.entries.get_mut(key) else {
            return false;
        };
        if let (Some(version), Some(cached_version)) = (version, cached.version)
            && version <= cached_version
        {
            return true;
        }
        if cached.addresses == addresses {
            if version.is_some() {
                cached.version = version;
            }
            return true;
        }
        false
    }

    fn remove(&mut self, key: &K) {
        self.entries.pop(key);
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    fn record(&mut self, key: K, version: Option<u64>, addresses: Vec<Multiaddr>) {
        self.entries.put(key, CachedUpdate { version, addresses });
    }
}

struct ConsensusState {
    updater: Option<Arc<dyn ConsensusAddressUpdater>>,
    pending_updates: Vec<(NetworkPublicKey, AddressSource, Option<u64>, Vec<Multiaddr>)>,
    last_sent: LastSentCache<(NetworkPublicKey, AddressSource)>,
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
                last_sent_p2p: Mutex::new(LastSentCache::new()),
                consensus: Mutex::new(ConsensusState {
                    updater: None,
                    pending_updates: Vec::new(),
                    last_sent: LastSentCache::new(),
                }),
            }),
        }
    }

    pub fn set_consensus_address_updater(
        &self,
        consensus_address_updater: Arc<dyn ConsensusAddressUpdater>,
    ) {
        // Holding the guard through replay and publication prevents an
        // in-flight update from recording delivery to the old updater.
        let mut consensus = self.inner.consensus.lock().unwrap();
        consensus.last_sent.clear();
        let pending = std::mem::take(&mut consensus.pending_updates);

        for (pubkey, source, version, addrs) in pending {
            if let Err(e) = deliver_consensus_update(
                &mut consensus.last_sent,
                consensus_address_updater.as_ref(),
                pubkey.clone(),
                source,
                version,
                addrs,
            ) {
                warn!(
                    ?pubkey,
                    "Error replaying buffered consensus address update: {e:?}"
                );
            }
        }

        consensus.updater = Some(consensus_address_updater);
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
        self.update_endpoint_impl(endpoint, source, None, addresses)
    }

    /// Like [`Self::update_endpoint`], but silently drops the update if
    /// `version` is not newer than the last accepted version for this
    /// (endpoint, source).
    ///
    /// Non-empty updates must carry a version iff
    /// [`AddressSource::is_versioned`]; clears
    /// go through [`Self::update_endpoint`] regardless of source.
    pub fn update_endpoint_versioned(
        &self,
        endpoint: EndpointId,
        source: AddressSource,
        version: u64,
        addresses: Vec<Multiaddr>,
    ) -> SuiResult<()> {
        self.update_endpoint_impl(endpoint, source, Some(version), addresses)
    }

    fn update_endpoint_impl(
        &self,
        endpoint: EndpointId,
        source: AddressSource,
        version: Option<u64>,
        addresses: Vec<Multiaddr>,
    ) -> SuiResult<()> {
        if !addresses.is_empty() && version.is_some() != source.is_versioned() {
            debug_fatal!(
                "{:?} update must {}carry a version",
                source,
                if source.is_versioned() { "" } else { "not " }
            );
        }

        match endpoint {
            EndpointId::P2p(peer_id) => {
                let key = (peer_id, source);
                let mut last_sent = self.inner.last_sent_p2p.lock().unwrap();
                if addresses.is_empty() {
                    // Always dispatch clears and forget the key: the event
                    // loop's Chain fallback can re-install Discovery
                    // addresses from known_peers_v2 behind this cache's back,
                    // so "already cleared" is never proof they're still gone.
                    last_sent.remove(&key);
                } else if last_sent.suppresses(&key, version, &addresses) {
                    return Ok(());
                }

                let anemo_addresses: Vec<_> = addresses
                    .iter()
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

                if !addresses.is_empty() {
                    last_sent.record(key, version, addresses);
                }
            }
            EndpointId::Consensus(network_pubkey) => {
                let consensus = &mut *self.inner.consensus.lock().unwrap();
                if let Some(updater) = &consensus.updater {
                    deliver_consensus_update(
                        &mut consensus.last_sent,
                        updater.as_ref(),
                        network_pubkey.clone(),
                        source,
                        version,
                        addresses,
                    )
                    .map_err(|e| {
                        warn!(?network_pubkey, "Error updating consensus address: {e:?}");
                        e
                    })?;
                } else {
                    info!(
                        ?network_pubkey,
                        "Buffering consensus address update (updater not yet set)"
                    );
                    consensus
                        .pending_updates
                        .push((network_pubkey, source, version, addresses));
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

fn deliver_consensus_update(
    last_sent: &mut LastSentCache<(NetworkPublicKey, AddressSource)>,
    updater: &dyn ConsensusAddressUpdater,
    network_pubkey: NetworkPublicKey,
    source: AddressSource,
    version: Option<u64>,
    addresses: Vec<Multiaddr>,
) -> SuiResult<()> {
    let key = (network_pubkey.clone(), source);
    if !addresses.is_empty() && last_sent.suppresses(&key, version, &addresses) {
        return Ok(());
    }

    // Evict before attempting delivery: an updater may mutate its durable
    // override before returning an error, so retaining the old cached value
    // could suppress the update needed to repair that partial failure.
    last_sent.remove(&key);
    updater.update_address(network_pubkey, source, addresses.clone())?;

    // Clears are always delivered and never cached because discovery has a
    // direct Chain-fallback insertion path that bypasses this manager.
    if !addresses.is_empty() {
        last_sent.record(key, version, addresses);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EndpointId {
    P2p(anemo::PeerId),
    Consensus(NetworkPublicKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
// NOTE: AddressSources are prioritized in order of the enum variants below.
pub enum AddressSource {
    Admin = 1,     // override from admin server
    Config = 2,    // override from config file
    Discovery = 3, // address received from P2P peers via Discovery protocol
    Seed = 4,      // locally-configured seed address
    Chain = 5,     // public on-chain address
}

impl AddressSource {
    /// Reserved metric value for the "no override" state.
    pub const DEFAULT_ADDRESS_SOURCE_CODE: i64 = 0;

    /// Whether non-empty updates from this source carry a monotonic version
    /// (see [`EndpointManager::update_endpoint_versioned`]).
    pub const fn is_versioned(self) -> bool {
        match self {
            AddressSource::Discovery => true,
            AddressSource::Admin
            | AddressSource::Config
            | AddressSource::Seed
            | AddressSource::Chain => false,
        }
    }

    /// Used as the value of the active-address-source metrics.
    pub const fn metric_code(self) -> i64 {
        self as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fastcrypto::traits::KeyPair;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc as std_mpsc,
    };
    use sui_types::crypto::{NetworkKeyPair, get_key_pair};
    use sui_types::error::SuiErrorKind;

    type UpdateEntry = (NetworkPublicKey, Vec<Multiaddr>);
    // Mock consensus address updater for testing. Records every delivered
    // update; fails deliveries while the shared `fail` flag is set.
    struct MockConsensusAddressUpdater {
        updates: Arc<Mutex<Vec<UpdateEntry>>>,
        fail: Arc<AtomicBool>,
    }

    impl MockConsensusAddressUpdater {
        fn new() -> (Self, Arc<Mutex<Vec<UpdateEntry>>>) {
            let (updater, updates, _) = Self::new_failable();
            (updater, updates)
        }

        fn new_failable() -> (Self, Arc<Mutex<Vec<UpdateEntry>>>, Arc<AtomicBool>) {
            let updates = Arc::new(Mutex::new(Vec::new()));
            let fail = Arc::new(AtomicBool::new(false));
            let updater = Self {
                updates: updates.clone(),
                fail: fail.clone(),
            };
            (updater, updates, fail)
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
            if self.fail.load(Ordering::SeqCst) {
                Err(SuiErrorKind::GenericAuthorityError {
                    error: "mock failure".to_owned(),
                }
                .into())
            } else {
                Ok(())
            }
        }
    }

    fn create_mock_endpoint_manager() -> EndpointManager {
        use sui_config::p2p::P2pConfig;

        let config = P2pConfig::default();
        let (_unstarted, _server, endpoint_manager) =
            discovery::Builder::new().config(config).build();
        endpoint_manager
    }

    fn create_mock_endpoint_manager_with_mailbox(
        capacity: usize,
    ) -> (
        EndpointManager,
        tokio::sync::mpsc::Receiver<discovery::DiscoveryMessage>,
    ) {
        let (sender, receiver) = tokio::sync::mpsc::channel(capacity);
        (
            EndpointManager::new(discovery::Sender::new(sender)),
            receiver,
        )
    }

    /// Sends a P2P address update that must succeed.
    fn send_p2p_update(
        endpoint_manager: &EndpointManager,
        peer_id: anemo::PeerId,
        source: AddressSource,
        addresses: Vec<Multiaddr>,
    ) {
        endpoint_manager
            .update_endpoint(EndpointId::P2p(peer_id), source, addresses)
            .unwrap();
    }

    /// Sends a versioned Discovery-sourced P2P update that must succeed.
    fn send_p2p_versioned(
        endpoint_manager: &EndpointManager,
        peer_id: anemo::PeerId,
        version: u64,
        addresses: Vec<Multiaddr>,
    ) {
        endpoint_manager
            .update_endpoint_versioned(
                EndpointId::P2p(peer_id),
                AddressSource::Discovery,
                version,
                addresses,
            )
            .unwrap();
    }

    /// Sends a versioned Discovery-sourced consensus address update that must
    /// succeed. Empty `addresses` are sent as an unversioned clear.
    fn send_consensus_discovery_update(
        endpoint_manager: &EndpointManager,
        network_pubkey: &NetworkPublicKey,
        version: u64,
        addresses: Vec<Multiaddr>,
    ) {
        let endpoint = EndpointId::Consensus(network_pubkey.clone());
        if addresses.is_empty() {
            endpoint_manager
                .update_endpoint(endpoint, AddressSource::Discovery, addresses)
                .unwrap();
        } else {
            endpoint_manager
                .update_endpoint_versioned(endpoint, AddressSource::Discovery, version, addresses)
                .unwrap();
        }
    }

    struct BlockingConsensusAddressUpdater {
        updates: Arc<Mutex<Vec<UpdateEntry>>>,
        entered: Mutex<Option<std_mpsc::Sender<()>>>,
        release: Mutex<std_mpsc::Receiver<()>>,
    }

    impl ConsensusAddressUpdater for BlockingConsensusAddressUpdater {
        fn update_address(
            &self,
            network_pubkey: NetworkPublicKey,
            _source: AddressSource,
            addresses: Vec<Multiaddr>,
        ) -> SuiResult<()> {
            self.updates
                .lock()
                .unwrap()
                .push((network_pubkey, addresses));
            if let Some(entered) = self.entered.lock().unwrap().take() {
                entered.send(()).unwrap();
            }
            self.release.lock().unwrap().recv().unwrap();
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_p2p_updates_are_deduplicated_per_source_and_clears_are_not() {
        let (endpoint_manager, mailbox) = create_mock_endpoint_manager_with_mailbox(16);
        let peer_id = anemo::PeerId([7; 32]);
        let other_peer_id = anemo::PeerId([8; 32]);
        let address_a = vec!["/ip4/127.0.0.1/udp/9000".parse().unwrap()];
        let address_b = vec!["/ip4/127.0.0.1/udp/9001".parse().unwrap()];

        send_p2p_versioned(&endpoint_manager, peer_id, 1, address_a.clone());
        send_p2p_versioned(&endpoint_manager, peer_id, 2, address_a.clone());
        assert_eq!(mailbox.len(), 1);

        send_p2p_versioned(&endpoint_manager, peer_id, 3, address_b);
        send_p2p_versioned(&endpoint_manager, peer_id, 4, address_a.clone());
        endpoint_manager
            .update_endpoint(EndpointId::P2p(peer_id), AddressSource::Chain, address_a)
            .unwrap();
        assert_eq!(mailbox.len(), 4);

        endpoint_manager
            .update_endpoint(EndpointId::P2p(peer_id), AddressSource::Discovery, vec![])
            .unwrap();
        endpoint_manager
            .update_endpoint(EndpointId::P2p(peer_id), AddressSource::Discovery, vec![])
            .unwrap();
        endpoint_manager
            .update_endpoint(EndpointId::P2p(other_peer_id), AddressSource::Admin, vec![])
            .unwrap();
        assert_eq!(mailbox.len(), 7);
    }

    #[tokio::test]
    async fn test_p2p_last_sent_cache_evicts_lru_at_cap() {
        let (endpoint_manager, mailbox) = create_mock_endpoint_manager_with_mailbox(2);
        let filler_key = |i: u64| {
            let mut bytes = [0; 32];
            bytes[..8].copy_from_slice(&i.to_le_bytes());
            (anemo::PeerId(bytes), AddressSource::Discovery)
        };
        let size_cap = LastSentCache::<(anemo::PeerId, AddressSource)>::SIZE_CAP;
        {
            let mut last_sent = endpoint_manager.inner.last_sent_p2p.lock().unwrap();
            for i in 0..size_cap {
                // Only entry count matters here, so empty filler addresses
                // keep this test cheap.
                last_sent.entries.put(
                    filler_key(i as u64),
                    CachedUpdate {
                        version: None,
                        addresses: Vec::new(),
                    },
                );
            }
        }

        // Recording a new key at capacity evicts the LRU entry; the new entry
        // survives, so the identical follow-up is still deduplicated.
        let peer_id = anemo::PeerId([255; 32]);
        let addresses = vec!["/ip4/127.0.0.1/udp/9001".parse().unwrap()];
        send_p2p_versioned(&endpoint_manager, peer_id, 1, addresses.clone());
        send_p2p_versioned(&endpoint_manager, peer_id, 2, addresses);

        assert_eq!(mailbox.len(), 1);
        let last_sent = endpoint_manager.inner.last_sent_p2p.lock().unwrap();
        assert_eq!(last_sent.entries.len(), size_cap);
        // The oldest filler key was the LRU entry and is gone.
        assert!(!last_sent.entries.contains(&filler_key(0)));
        assert!(last_sent.entries.contains(&filler_key(1)));
    }

    #[tokio::test]
    async fn test_p2p_versioned_updates_drop_stale() {
        let (endpoint_manager, mailbox) = create_mock_endpoint_manager_with_mailbox(8);
        let peer_id = anemo::PeerId([7; 32]);

        send_p2p_versioned(
            &endpoint_manager,
            peer_id,
            2,
            vec!["/ip4/127.0.0.1/udp/9002".parse().unwrap()],
        );
        assert_eq!(mailbox.len(), 1);

        // Older version with different content: dropped at the sink.
        send_p2p_versioned(
            &endpoint_manager,
            peer_id,
            1,
            vec!["/ip4/127.0.0.1/udp/9001".parse().unwrap()],
        );
        assert_eq!(mailbox.len(), 1);

        // Newer version dispatches.
        send_p2p_versioned(
            &endpoint_manager,
            peer_id,
            3,
            vec!["/ip4/127.0.0.1/udp/9003".parse().unwrap()],
        );
        assert_eq!(mailbox.len(), 2);
    }

    #[tokio::test]
    async fn test_p2p_value_dedup_bumps_version() {
        let (endpoint_manager, mailbox) = create_mock_endpoint_manager_with_mailbox(8);
        let peer_id = anemo::PeerId([8; 32]);
        let addrs_x: Vec<Multiaddr> = vec!["/ip4/127.0.0.1/udp/9010".parse().unwrap()];
        let addrs_y: Vec<Multiaddr> = vec!["/ip4/127.0.0.1/udp/9011".parse().unwrap()];

        // X at v2 dispatches; X again at v4 is value-deduped but must raise
        // the stored version so the out-of-order Y at v3 is recognized as
        // stale — the sink then holds the true latest content (X).
        send_p2p_versioned(&endpoint_manager, peer_id, 2, addrs_x.clone());
        send_p2p_versioned(&endpoint_manager, peer_id, 4, addrs_x);
        send_p2p_versioned(&endpoint_manager, peer_id, 3, addrs_y);
        assert_eq!(mailbox.len(), 1);
    }

    #[tokio::test]
    async fn test_p2p_clear_resets_version_tracking() {
        let (endpoint_manager, mailbox) = create_mock_endpoint_manager_with_mailbox(8);
        let peer_id = anemo::PeerId([9; 32]);

        send_p2p_versioned(
            &endpoint_manager,
            peer_id,
            5,
            vec!["/ip4/127.0.0.1/udp/9020".parse().unwrap()],
        );
        // Clears always dispatch and remove the entry, version included...
        send_p2p_update(&endpoint_manager, peer_id, AddressSource::Discovery, vec![]);
        // ...so a later low-versioned update applies again (fail-open).
        send_p2p_versioned(
            &endpoint_manager,
            peer_id,
            1,
            vec!["/ip4/127.0.0.1/udp/9021".parse().unwrap()],
        );
        assert_eq!(mailbox.len(), 3);
    }

    #[tokio::test]
    async fn test_consensus_versioned_updates_drop_stale() {
        let endpoint_manager = create_mock_endpoint_manager();
        let (mock_updater, updates) = MockConsensusAddressUpdater::new();
        endpoint_manager.set_consensus_address_updater(Arc::new(mock_updater));

        let (_, network_key): (_, NetworkKeyPair) = get_key_pair();
        let network_pubkey = network_key.public();

        endpoint_manager
            .update_endpoint_versioned(
                EndpointId::Consensus(network_pubkey.clone()),
                AddressSource::Discovery,
                2,
                vec!["/ip4/127.0.0.1/udp/9030".parse().unwrap()],
            )
            .unwrap();
        assert_eq!(updates.lock().unwrap().len(), 1);

        endpoint_manager
            .update_endpoint_versioned(
                EndpointId::Consensus(network_pubkey.clone()),
                AddressSource::Discovery,
                1,
                vec!["/ip4/127.0.0.1/udp/9031".parse().unwrap()],
            )
            .unwrap();
        assert_eq!(updates.lock().unwrap().len(), 1);

        endpoint_manager
            .update_endpoint_versioned(
                EndpointId::Consensus(network_pubkey.clone()),
                AddressSource::Discovery,
                3,
                vec!["/ip4/127.0.0.1/udp/9032".parse().unwrap()],
            )
            .unwrap();
        assert_eq!(updates.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_consensus_buffered_replay_respects_versions() {
        let endpoint_manager = create_mock_endpoint_manager();
        let (_, network_key): (_, NetworkKeyPair) = get_key_pair();
        let network_pubkey = network_key.public();
        let addrs_newer: Vec<Multiaddr> = vec!["/ip4/127.0.0.1/udp/9040".parse().unwrap()];

        // Buffer a newer and then a stale update before any updater exists.
        endpoint_manager
            .update_endpoint_versioned(
                EndpointId::Consensus(network_pubkey.clone()),
                AddressSource::Discovery,
                2,
                addrs_newer.clone(),
            )
            .unwrap();
        endpoint_manager
            .update_endpoint_versioned(
                EndpointId::Consensus(network_pubkey.clone()),
                AddressSource::Discovery,
                1,
                vec!["/ip4/127.0.0.1/udp/9041".parse().unwrap()],
            )
            .unwrap();

        // Replay runs each buffered update through the versioned check, so the
        // stale one is dropped rather than delivered after the newer one.
        let (mock_updater, updates) = MockConsensusAddressUpdater::new();
        endpoint_manager.set_consensus_address_updater(Arc::new(mock_updater));

        let recorded = updates.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].1, addrs_newer);
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
    async fn test_consensus_updates_are_deduplicated_and_changes_dispatch() {
        let endpoint_manager = create_mock_endpoint_manager();
        let (mock_updater, updates) = MockConsensusAddressUpdater::new();
        endpoint_manager.set_consensus_address_updater(Arc::new(mock_updater));

        let (_, network_key): (_, NetworkKeyPair) = get_key_pair();
        let network_pubkey = network_key.public();
        let address_a = vec!["/ip4/127.0.0.1/udp/9000".parse().unwrap()];
        let address_b = vec!["/ip4/127.0.0.1/udp/9001".parse().unwrap()];

        for version in 1..=3 {
            send_consensus_discovery_update(
                &endpoint_manager,
                network_pubkey,
                version,
                address_a.clone(),
            );
        }
        send_consensus_discovery_update(&endpoint_manager, network_pubkey, 4, address_b);
        for _ in 0..2 {
            send_consensus_discovery_update(&endpoint_manager, network_pubkey, 0, vec![]);
        }

        assert_eq!(updates.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn test_consensus_cache_is_invalidated_when_updater_is_replaced() {
        let endpoint_manager = create_mock_endpoint_manager();
        let (first_updater, first_updates) = MockConsensusAddressUpdater::new();
        endpoint_manager.set_consensus_address_updater(Arc::new(first_updater));

        let (_, network_key): (_, NetworkKeyPair) = get_key_pair();
        let network_pubkey = network_key.public();
        let addresses = vec!["/ip4/127.0.0.1/udp/9000".parse().unwrap()];
        send_consensus_discovery_update(&endpoint_manager, network_pubkey, 1, addresses.clone());

        let (replacement, replacement_updates) = MockConsensusAddressUpdater::new();
        endpoint_manager.set_consensus_address_updater(Arc::new(replacement));
        send_consensus_discovery_update(&endpoint_manager, network_pubkey, 1, addresses);

        assert_eq!(first_updates.lock().unwrap().len(), 1);
        assert_eq!(replacement_updates.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_update_consensus_endpoint_without_updater_buffers() {
        let endpoint_manager = create_mock_endpoint_manager();

        let (_, network_key): (_, NetworkKeyPair) = get_key_pair();
        let network_pubkey = network_key.public();

        let addresses = vec!["/ip4/127.0.0.1/udp/9000".parse().unwrap()];

        // Should succeed (buffered) even without an updater set.
        let result = endpoint_manager.update_endpoint_versioned(
            EndpointId::Consensus(network_pubkey.clone()),
            AddressSource::Discovery,
            1,
            addresses.clone(),
        );
        assert!(result.is_ok());

        // Now set the updater and verify the buffered update was replayed.
        let (mock_updater, updates) = MockConsensusAddressUpdater::new();
        endpoint_manager.set_consensus_address_updater(Arc::new(mock_updater));

        send_consensus_discovery_update(&endpoint_manager, network_pubkey, 2, addresses.clone());

        let recorded_updates = updates.lock().unwrap();
        assert_eq!(recorded_updates.len(), 1);
        assert_eq!(recorded_updates[0].0, network_pubkey.clone());
        assert_eq!(recorded_updates[0].1, addresses);
    }

    #[tokio::test]
    async fn test_consensus_buffer_replay_deduplicates_identical_updates() {
        let endpoint_manager = create_mock_endpoint_manager();
        let (_, network_key): (_, NetworkKeyPair) = get_key_pair();
        let network_pubkey = network_key.public();
        let addresses = vec!["/ip4/127.0.0.1/udp/9000".parse().unwrap()];

        for version in 1..=3 {
            send_consensus_discovery_update(
                &endpoint_manager,
                network_pubkey,
                version,
                addresses.clone(),
            );
        }

        let (updater, updates) = MockConsensusAddressUpdater::new();
        endpoint_manager.set_consensus_address_updater(Arc::new(updater));
        assert_eq!(updates.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_consensus_failure_does_not_leave_a_false_cache_hit() {
        let endpoint_manager = create_mock_endpoint_manager();
        let (mock_updater, updates, fail) = MockConsensusAddressUpdater::new_failable();
        endpoint_manager.set_consensus_address_updater(Arc::new(mock_updater));

        let (_, network_key): (_, NetworkKeyPair) = get_key_pair();
        let network_pubkey = network_key.public();
        let address_a = vec!["/ip4/127.0.0.1/udp/9000".parse().unwrap()];
        let address_b = vec!["/ip4/127.0.0.1/udp/9001".parse().unwrap()];

        send_consensus_discovery_update(&endpoint_manager, network_pubkey, 1, address_a.clone());
        fail.store(true, Ordering::SeqCst);
        assert!(
            endpoint_manager
                .update_endpoint_versioned(
                    EndpointId::Consensus(network_pubkey.clone()),
                    AddressSource::Discovery,
                    2,
                    address_b,
                )
                .is_err()
        );
        fail.store(false, Ordering::SeqCst);
        send_consensus_discovery_update(&endpoint_manager, network_pubkey, 3, address_a);

        assert_eq!(updates.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_consensus_failed_first_delivery_is_retried() {
        let endpoint_manager = create_mock_endpoint_manager();
        let (mock_updater, updates, fail) = MockConsensusAddressUpdater::new_failable();
        fail.store(true, Ordering::SeqCst);
        endpoint_manager.set_consensus_address_updater(Arc::new(mock_updater));

        let (_, network_key): (_, NetworkKeyPair) = get_key_pair();
        let network_pubkey = network_key.public();
        let addresses = vec!["/ip4/127.0.0.1/udp/9000".parse().unwrap()];
        for _ in 0..2 {
            assert!(
                endpoint_manager
                    .update_endpoint_versioned(
                        EndpointId::Consensus(network_pubkey.clone()),
                        AddressSource::Discovery,
                        1,
                        addresses.clone(),
                    )
                    .is_err()
            );
        }

        assert_eq!(updates.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_consensus_updater_replacement_cannot_interleave_with_delivery() {
        let endpoint_manager = create_mock_endpoint_manager();
        let old_updates = Arc::new(Mutex::new(Vec::new()));
        let (entered_tx, entered_rx) = std_mpsc::channel();
        let (release_tx, release_rx) = std_mpsc::channel();
        endpoint_manager.set_consensus_address_updater(Arc::new(BlockingConsensusAddressUpdater {
            updates: old_updates.clone(),
            entered: Mutex::new(Some(entered_tx)),
            release: Mutex::new(release_rx),
        }));

        let (_, network_key): (_, NetworkKeyPair) = get_key_pair();
        let network_pubkey = network_key.public();
        let addresses = vec!["/ip4/127.0.0.1/udp/9000".parse().unwrap()];
        let update_manager = endpoint_manager.clone();
        let update_pubkey = network_pubkey.clone();
        let update_addresses = addresses.clone();
        let update_thread = std::thread::spawn(move || {
            update_manager
                .update_endpoint_versioned(
                    EndpointId::Consensus(update_pubkey),
                    AddressSource::Discovery,
                    1,
                    update_addresses,
                )
                .unwrap();
        });
        entered_rx.recv().unwrap();

        let (replacement, replacement_updates) = MockConsensusAddressUpdater::new();
        let setter_manager = endpoint_manager.clone();
        let (setter_done_tx, setter_done_rx) = std_mpsc::channel();
        let setter_thread = std::thread::spawn(move || {
            setter_manager.set_consensus_address_updater(Arc::new(replacement));
            setter_done_tx.send(()).unwrap();
        });
        assert!(matches!(
            setter_done_rx.recv_timeout(std::time::Duration::from_millis(50)),
            Err(std_mpsc::RecvTimeoutError::Timeout)
        ));

        release_tx.send(()).unwrap();
        update_thread.join().unwrap();
        setter_thread.join().unwrap();
        send_consensus_discovery_update(&endpoint_manager, network_pubkey, 1, addresses);

        assert_eq!(old_updates.lock().unwrap().len(), 1);
        assert_eq!(replacement_updates.lock().unwrap().len(), 1);
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
                .update_endpoint_versioned(
                    EndpointId::Consensus(network_key.public().clone()),
                    AddressSource::Discovery,
                    1,
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
                em.update_endpoint_versioned(
                    EndpointId::Consensus(pubkey),
                    AddressSource::Discovery,
                    1,
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
