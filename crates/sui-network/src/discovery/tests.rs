// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
    endpoint_manager::{AddressSource, EndpointId},
    utils::{build_network_and_key, build_network_with_anemo_config},
};
use anemo::Result;
use fastcrypto::ed25519::Ed25519PublicKey;
use futures::stream::FuturesUnordered;
use std::collections::HashSet;
use sui_config::p2p::{AllowlistedPeer, DiscoveryConfig, SeedPeer};
use tokio::time::timeout;

#[tokio::test]
async fn get_known_peers() -> Result<()> {
    let config = P2pConfig::default();
    let (UnstartedDiscovery { state, .. }, server, _) =
        Builder::new().config(config).build_internal();

    // Err when own_info not set
    server
        .get_known_peers_v2(Request::new(()))
        .await
        .unwrap_err();

    // Normal response with our_info
    let our_info = NodeInfo {
        peer_id: PeerId([9; 32]),
        addresses: Vec::new(),
        timestamp_ms: now_unix(),
        access_type: AccessType::Public,
    };
    state.write().unwrap().our_info = Some(SignedNodeInfo::new_from_data_and_sig(
        our_info.clone(),
        Ed25519Signature::default(),
    ));
    let response = server
        .get_known_peers_v2(Request::new(()))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.own_info.data(), &our_info);
    assert!(response.known_peers.is_empty());

    // Normal response with some known peers
    let other_peer = NodeInfo {
        peer_id: PeerId([13; 32]),
        addresses: Vec::new(),
        timestamp_ms: now_unix(),
        access_type: AccessType::Public,
    };
    state.write().unwrap().known_peers.insert(
        other_peer.peer_id,
        VerifiedSignedNodeInfo::new_unchecked(SignedNodeInfo::new_from_data_and_sig(
            other_peer.clone(),
            Ed25519Signature::default(),
        )),
    );
    let response = server
        .get_known_peers_v2(Request::new(()))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.own_info.data(), &our_info);
    assert_eq!(
        response
            .known_peers
            .into_iter()
            .map(|peer| peer.into_data())
            .collect::<Vec<_>>(),
        vec![other_peer]
    );

    Ok(())
}

#[tokio::test]
async fn get_known_peers_v3() -> Result<()> {
    let config = P2pConfig::default();
    let (UnstartedDiscovery { state, .. }, server, _) =
        Builder::new().config(config).build_internal();

    let our_peer_id = PeerId([9; 32]);
    let mut our_addresses = BTreeMap::new();
    our_addresses.insert(EndpointId::P2p(our_peer_id), vec![]);

    let our_info_v2 = VersionedNodeInfo::V2(NodeInfoV2 {
        addresses: our_addresses.clone(),
        timestamp_ms: now_unix(),
        access_type: AccessType::Public,
    });

    // Err when own_info_v2 not set
    let request_info = VersionedNodeInfo::V2(NodeInfoV2 {
        addresses: BTreeMap::new(),
        timestamp_ms: now_unix(),
        access_type: AccessType::Public,
    });
    server
        .get_known_peers_v3(Request::new(GetKnownPeersRequestV3 {
            own_info: SignedVersionedNodeInfo::new_from_data_and_sig(
                request_info,
                Ed25519Signature::default(),
            ),
        }))
        .await
        .unwrap_err();

    // Normal response with our_info_v2
    state.write().unwrap().our_info_v2 = Some(SignedVersionedNodeInfo::new_from_data_and_sig(
        our_info_v2.clone(),
        Ed25519Signature::default(),
    ));

    let request_info = VersionedNodeInfo::V2(NodeInfoV2 {
        addresses: BTreeMap::new(),
        timestamp_ms: now_unix(),
        access_type: AccessType::Public,
    });
    let response = server
        .get_known_peers_v3(Request::new(GetKnownPeersRequestV3 {
            own_info: SignedVersionedNodeInfo::new_from_data_and_sig(
                request_info,
                Ed25519Signature::default(),
            ),
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.own_info.data(), &our_info_v2);
    assert!(response.known_peers.is_empty());

    // Normal response with some known peers
    let other_peer_id = PeerId([13; 32]);
    let mut other_addresses = BTreeMap::new();
    other_addresses.insert(EndpointId::P2p(other_peer_id), vec![]);
    let other_peer = VersionedNodeInfo::V2(NodeInfoV2 {
        addresses: other_addresses,
        timestamp_ms: now_unix(),
        access_type: AccessType::Public,
    });
    state.write().unwrap().known_peers_v2.insert(
        other_peer_id,
        VerifiedSignedVersionedNodeInfo::new_unchecked(
            SignedVersionedNodeInfo::new_from_data_and_sig(
                other_peer.clone(),
                Ed25519Signature::default(),
            ),
        ),
    );

    let request_info = VersionedNodeInfo::V2(NodeInfoV2 {
        addresses: BTreeMap::new(),
        timestamp_ms: now_unix(),
        access_type: AccessType::Public,
    });
    let response = server
        .get_known_peers_v3(Request::new(GetKnownPeersRequestV3 {
            own_info: SignedVersionedNodeInfo::new_from_data_and_sig(
                request_info,
                Ed25519Signature::default(),
            ),
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.own_info.data(), &our_info_v2);
    assert_eq!(
        response
            .known_peers
            .into_iter()
            .map(|peer| peer.into_data())
            .collect::<Vec<_>>(),
        vec![other_peer]
    );

    Ok(())
}

#[tokio::test]
async fn trusted_peers_shared_only_with_configured_peers() {
    // Set up a config with a configured peer (via seed_peers).
    let configured_peer_id = PeerId([42; 32]);
    let non_configured_peer_id = PeerId([99; 32]);

    let mut config = P2pConfig::default();
    config.seed_peers.push(SeedPeer {
        peer_id: Some(configured_peer_id),
        address: "/dns/localhost/udp/8080".parse().unwrap(),
    });

    let (builder, server, _) = Builder::new().config(config).build_internal();
    let (network, keypair) = crate::utils::build_network_and_key(|router| router);
    let _ = builder.build(network, keypair);

    let our_info = NodeInfo {
        peer_id: PeerId([9; 32]),
        addresses: Vec::new(),
        timestamp_ms: now_unix(),
        access_type: AccessType::Public,
    };
    server.state.write().unwrap().our_info = Some(SignedNodeInfo::new_from_data_and_sig(
        our_info,
        Ed25519Signature::default(),
    ));

    // Add a Trusted peer and a Public peer to known_peers.
    let trusted_peer = NodeInfo {
        peer_id: PeerId([10; 32]),
        addresses: Vec::new(),
        timestamp_ms: now_unix(),
        access_type: AccessType::Trusted,
    };
    let public_peer = NodeInfo {
        peer_id: PeerId([11; 32]),
        addresses: Vec::new(),
        timestamp_ms: now_unix(),
        access_type: AccessType::Public,
    };

    {
        let mut state = server.state.write().unwrap();
        state.known_peers.insert(
            trusted_peer.peer_id,
            VerifiedSignedNodeInfo::new_unchecked(SignedNodeInfo::new_from_data_and_sig(
                trusted_peer.clone(),
                Ed25519Signature::default(),
            )),
        );
        state.known_peers.insert(
            public_peer.peer_id,
            VerifiedSignedNodeInfo::new_unchecked(SignedNodeInfo::new_from_data_and_sig(
                public_peer.clone(),
                Ed25519Signature::default(),
            )),
        );
    }

    // Request from a configured peer - should see both Public and Trusted peers.
    let request_from_configured = Request::new(()).with_extension(configured_peer_id);
    let response = server
        .get_known_peers_v2(request_from_configured)
        .await
        .unwrap()
        .into_inner();
    let returned_peer_ids: HashSet<_> = response.known_peers.iter().map(|p| p.peer_id).collect();
    assert!(
        returned_peer_ids.contains(&trusted_peer.peer_id),
        "Configured peer should see Trusted peers"
    );
    assert!(
        returned_peer_ids.contains(&public_peer.peer_id),
        "Configured peer should see Public peers"
    );

    // Request from a non-configured peer - should see only Public peer, not Trusted.
    let request_from_non_configured = Request::new(()).with_extension(non_configured_peer_id);
    let response = server
        .get_known_peers_v2(request_from_non_configured)
        .await
        .unwrap()
        .into_inner();
    let returned_peer_ids: HashSet<_> = response.known_peers.iter().map(|p| p.peer_id).collect();
    assert!(
        !returned_peer_ids.contains(&trusted_peer.peer_id),
        "Non-configured peer should NOT see Trusted peers"
    );
    assert!(
        returned_peer_ids.contains(&public_peer.peer_id),
        "Non-configured peer should see Public peers"
    );

    // Request with no peer_id - should see only Public peer, not Trusted.
    let request_anonymous = Request::new(());
    let response = server
        .get_known_peers_v2(request_anonymous)
        .await
        .unwrap()
        .into_inner();
    let returned_peer_ids: HashSet<_> = response.known_peers.iter().map(|p| p.peer_id).collect();
    assert!(
        !returned_peer_ids.contains(&trusted_peer.peer_id),
        "Anonymous request should NOT see Trusted peers"
    );
    assert!(
        returned_peer_ids.contains(&public_peer.peer_id),
        "Anonymous request should see Public peers"
    );
}

#[tokio::test]
async fn trusted_peers_shared_only_with_configured_peers_v3() {
    // Set up a config with a configured peer (via seed_peers).
    let configured_peer_id = PeerId([42; 32]);
    let non_configured_peer_id = PeerId([99; 32]);

    let mut config = P2pConfig::default();
    config.seed_peers.push(SeedPeer {
        peer_id: Some(configured_peer_id),
        address: "/dns/localhost/udp/8080".parse().unwrap(),
    });

    let (builder, server, _) = Builder::new().config(config).build_internal();
    let (network, keypair) = crate::utils::build_network_and_key(|router| router);
    let _ = builder.build(network, keypair);

    let our_peer_id = PeerId([9; 32]);
    let mut our_addresses = BTreeMap::new();
    our_addresses.insert(EndpointId::P2p(our_peer_id), vec![]);
    let our_info_v2 = VersionedNodeInfo::V2(NodeInfoV2 {
        addresses: our_addresses,
        timestamp_ms: now_unix(),
        access_type: AccessType::Public,
    });
    server.state.write().unwrap().our_info_v2 = Some(
        SignedVersionedNodeInfo::new_from_data_and_sig(our_info_v2, Ed25519Signature::default()),
    );

    // Add a Trusted peer and a Public peer to known_peers_v2.
    let trusted_peer_id = PeerId([10; 32]);
    let mut trusted_addresses = BTreeMap::new();
    trusted_addresses.insert(EndpointId::P2p(trusted_peer_id), vec![]);
    let trusted_peer = VersionedNodeInfo::V2(NodeInfoV2 {
        addresses: trusted_addresses,
        timestamp_ms: now_unix(),
        access_type: AccessType::Trusted,
    });

    let public_peer_id = PeerId([11; 32]);
    let mut public_addresses = BTreeMap::new();
    public_addresses.insert(EndpointId::P2p(public_peer_id), vec![]);
    let public_peer = VersionedNodeInfo::V2(NodeInfoV2 {
        addresses: public_addresses,
        timestamp_ms: now_unix(),
        access_type: AccessType::Public,
    });

    {
        let mut state = server.state.write().unwrap();
        state.known_peers_v2.insert(
            trusted_peer_id,
            VerifiedSignedVersionedNodeInfo::new_unchecked(
                SignedVersionedNodeInfo::new_from_data_and_sig(
                    trusted_peer.clone(),
                    Ed25519Signature::default(),
                ),
            ),
        );
        state.known_peers_v2.insert(
            public_peer_id,
            VerifiedSignedVersionedNodeInfo::new_unchecked(
                SignedVersionedNodeInfo::new_from_data_and_sig(
                    public_peer.clone(),
                    Ed25519Signature::default(),
                ),
            ),
        );
    }

    let make_request = || {
        let request_info = VersionedNodeInfo::V2(NodeInfoV2 {
            addresses: BTreeMap::new(),
            timestamp_ms: now_unix(),
            access_type: AccessType::Public,
        });
        GetKnownPeersRequestV3 {
            own_info: SignedVersionedNodeInfo::new_from_data_and_sig(
                request_info,
                Ed25519Signature::default(),
            ),
        }
    };

    // Request from a configured peer - should see both Public and Trusted peers.
    let request_from_configured = Request::new(make_request()).with_extension(configured_peer_id);
    let response = server
        .get_known_peers_v3(request_from_configured)
        .await
        .unwrap()
        .into_inner();
    let returned_peer_ids: HashSet<_> = response
        .known_peers
        .iter()
        .filter_map(|p| p.peer_id())
        .collect();
    assert!(
        returned_peer_ids.contains(&trusted_peer_id),
        "Configured peer should see Trusted peers"
    );
    assert!(
        returned_peer_ids.contains(&public_peer_id),
        "Configured peer should see Public peers"
    );

    // Request from a non-configured peer - should see only Public peer, not Trusted.
    let request_from_non_configured =
        Request::new(make_request()).with_extension(non_configured_peer_id);
    let response = server
        .get_known_peers_v3(request_from_non_configured)
        .await
        .unwrap()
        .into_inner();
    let returned_peer_ids: HashSet<_> = response
        .known_peers
        .iter()
        .filter_map(|p| p.peer_id())
        .collect();
    assert!(
        !returned_peer_ids.contains(&trusted_peer_id),
        "Non-configured peer should NOT see Trusted peers"
    );
    assert!(
        returned_peer_ids.contains(&public_peer_id),
        "Non-configured peer should see Public peers"
    );

    // Request with no peer_id - should see only Public peer, not Trusted.
    let request_anonymous = Request::new(make_request());
    let response = server
        .get_known_peers_v3(request_anonymous)
        .await
        .unwrap()
        .into_inner();
    let returned_peer_ids: HashSet<_> = response
        .known_peers
        .iter()
        .filter_map(|p| p.peer_id())
        .collect();
    assert!(
        !returned_peer_ids.contains(&trusted_peer_id),
        "Anonymous request should NOT see Trusted peers"
    );
    assert!(
        returned_peer_ids.contains(&public_peer_id),
        "Anonymous request should see Public peers"
    );
}

#[tokio::test]
async fn make_connection_to_seed_peer() -> Result<()> {
    let mut config = P2pConfig::default();
    let (builder, server, _em) = Builder::new().config(config.clone()).build();
    let (network_1, key_1) = build_network_and_key(|router| router.add_rpc_service(server));
    let (_event_loop_1, _handle_1) = builder.build(network_1.clone(), key_1);

    config.seed_peers.push(SeedPeer {
        peer_id: None,
        address: format!("/dns/localhost/udp/{}", network_1.local_addr().port()).parse()?,
    });
    let (builder, server, _em) = Builder::new().config(config).build();
    let (network_2, key_2) = build_network_and_key(|router| router.add_rpc_service(server));
    let (mut event_loop_2, _handle_2) = builder.build(network_2.clone(), key_2);

    let (mut subscriber_1, _) = network_1.subscribe()?;
    let (mut subscriber_2, _) = network_2.subscribe()?;

    event_loop_2.handle_tick(std::time::Instant::now(), now_unix());

    assert_eq!(
        subscriber_2.recv().await?,
        PeerEvent::NewPeer(network_1.peer_id())
    );
    assert_eq!(
        subscriber_1.recv().await?,
        PeerEvent::NewPeer(network_2.peer_id())
    );

    Ok(())
}

#[tokio::test]
async fn make_connection_to_seed_peer_with_peer_id() -> Result<()> {
    let mut config = P2pConfig::default();
    let (builder, server, _em) = Builder::new().config(config.clone()).build();
    let (network_1, key_1) = build_network_and_key(|router| router.add_rpc_service(server));
    let (_event_loop_1, _handle_1) = builder.build(network_1.clone(), key_1);

    config.seed_peers.push(SeedPeer {
        peer_id: Some(network_1.peer_id()),
        address: format!("/dns/localhost/udp/{}", network_1.local_addr().port()).parse()?,
    });
    let (builder, server, _em) = Builder::new().config(config).build();
    let (network_2, key_2) = build_network_and_key(|router| router.add_rpc_service(server));
    let (mut event_loop_2, _handle_2) = builder.build(network_2.clone(), key_2);

    let (mut subscriber_1, _) = network_1.subscribe()?;
    let (mut subscriber_2, _) = network_2.subscribe()?;

    event_loop_2.handle_tick(std::time::Instant::now(), now_unix());

    assert_eq!(
        subscriber_2.recv().await?,
        PeerEvent::NewPeer(network_1.peer_id())
    );
    assert_eq!(
        subscriber_1.recv().await?,
        PeerEvent::NewPeer(network_2.peer_id())
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn three_nodes_can_connect_via_discovery() -> Result<()> {
    // Setup the peer that will be the seed for the other two
    let mut config = P2pConfig::default();
    let (builder, server, _em) = Builder::new().config(config.clone()).build();
    let (network_1, key_1) = build_network_and_key(|router| router.add_rpc_service(server));
    let (event_loop_1, _handle_1) = builder.build(network_1.clone(), key_1);

    config.seed_peers.push(SeedPeer {
        peer_id: Some(network_1.peer_id()),
        address: format!("/dns/localhost/udp/{}", network_1.local_addr().port()).parse()?,
    });
    let (builder, server, _em) = Builder::new().config(config.clone()).build();
    let (network_2, key_2) = build_network_and_key(|router| router.add_rpc_service(server));
    let (mut event_loop_2, _handle_2) = builder.build(network_2.clone(), key_2);
    // Set an external_address address for node 2 so that it can share its address
    event_loop_2.config.external_address =
        Some(format!("/dns/localhost/udp/{}", network_2.local_addr().port()).parse()?);

    let (builder, server, _em) = Builder::new().config(config).build();
    let (network_3, key_3) = build_network_and_key(|router| router.add_rpc_service(server));
    let (event_loop_3, _handle_3) = builder.build(network_3.clone(), key_3);

    let (mut subscriber_1, _) = network_1.subscribe()?;
    let (mut subscriber_2, _) = network_2.subscribe()?;
    let (mut subscriber_3, _) = network_3.subscribe()?;

    // Start all the event loops
    tokio::spawn(event_loop_1.start());
    tokio::spawn(event_loop_2.start());
    tokio::spawn(event_loop_3.start());

    let peer_id_1 = network_1.peer_id();
    let peer_id_2 = network_2.peer_id();
    let peer_id_3 = network_3.peer_id();

    // Get two events from node and make sure they're all connected
    let peers_1 = [subscriber_1.recv().await?, subscriber_1.recv().await?]
        .into_iter()
        .map(unwrap_new_peer_event)
        .collect::<HashSet<_>>();
    assert!(peers_1.contains(&peer_id_2));
    assert!(peers_1.contains(&peer_id_3));

    let peers_2 = [subscriber_2.recv().await?, subscriber_2.recv().await?]
        .into_iter()
        .map(unwrap_new_peer_event)
        .collect::<HashSet<_>>();
    assert!(peers_2.contains(&peer_id_1));
    assert!(peers_2.contains(&peer_id_3));

    let peers_3 = [subscriber_3.recv().await?, subscriber_3.recv().await?]
        .into_iter()
        .map(unwrap_new_peer_event)
        .collect::<HashSet<_>>();
    assert!(peers_3.contains(&peer_id_1));
    assert!(peers_3.contains(&peer_id_2));

    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn peers_are_added_from_endpoint_manager() -> Result<()> {
    let config = P2pConfig::default();
    let (builder, server, endpoint_manager_1) = Builder::new().config(config.clone()).build();
    let (network_1, key_1) = build_network_and_key(|router| router.add_rpc_service(server));
    let (event_loop_1, _handle_1) = builder.build(network_1.clone(), key_1);

    let (builder, server, _em) = Builder::new().config(config.clone()).build();
    let (network_2, key_2) = build_network_and_key(|router| router.add_rpc_service(server));
    let (event_loop_2, _handle_2) = builder.build(network_2.clone(), key_2);

    let (mut subscriber_1, _) = network_1.subscribe()?;
    let (mut subscriber_2, _) = network_2.subscribe()?;

    // Start all the event loops
    tokio::spawn(event_loop_1.start());
    tokio::spawn(event_loop_2.start());

    let peer_id_1 = network_1.peer_id();
    let peer_id_2 = network_2.peer_id();

    // At this moment peer 1 and peer 2 are not connected.
    let mut futures = FuturesUnordered::new();
    futures.push(timeout(Duration::from_secs(2), subscriber_1.recv()));
    futures.push(timeout(Duration::from_secs(2), subscriber_2.recv()));
    while let Some(result) = futures.next().await {
        let _elapse = result.unwrap_err();
    }

    let (mut subscriber_1, _) = network_1.subscribe()?;
    let (mut subscriber_2, _) = network_2.subscribe()?;

    // We send peer 1 a new peer info (peer 2) via endpoint manager.
    let peer_2_network_pubkey =
        Ed25519PublicKey(ed25519_consensus::VerificationKey::try_from(peer_id_2.0).unwrap());
    let peer2_addr: Multiaddr = format!("/dns/localhost/udp/{}", network_2.local_addr().port())
        .parse()
        .unwrap();
    let _ = endpoint_manager_1.update_endpoint(
        EndpointId::P2p(PeerId(peer_2_network_pubkey.0.to_bytes())),
        AddressSource::Chain,
        vec![peer2_addr],
    );

    // Now peer 1 and peer 2 are connected.
    let new_peer_for_1 = unwrap_new_peer_event(subscriber_1.recv().await.unwrap());
    assert_eq!(new_peer_for_1, peer_id_2);
    let new_peer_for_2 = unwrap_new_peer_event(subscriber_2.recv().await.unwrap());
    assert_eq!(new_peer_for_2, peer_id_1);

    Ok(())
}

#[tokio::test]
async fn test_access_types() {
    // This test case constructs a mesh graph of 11 nodes, with the following topology.
    // Only seed peers are proactively connected. Allowlisted peers allow inbound connections
    // but do NOT proactively connect outbound.
    //
    //
    // The topology:
    //                                      ------------  11 (private, seed: 1, allowed: 7, 8)
    //                                     /
    //                       ------ 1 (public) ------
    //                      /                        \
    //    2 (public, seed: 1, allowed: 7, 8)          3 (private, seed: 1, 4, 5)
    //       |                                       /             \
    //       |                 4 (private, seed: 3, allowed: 5, 6)  5 (private, seed: 4, allowed: 3)
    //       |                                        \
    //       |                                      6 (private, seed: 4)
    //     7 (private, seed: 2, 8)
    //       |
    //       |
    //     8 (private, seed: 7, 9)  p.s. 8's max connection is 0
    //       |
    //       |
    //     9 (public)
    //       |
    //       |
    //    10 (private, seed: 9)

    telemetry_subscribers::init_for_testing();

    let default_discovery_config = DiscoveryConfig {
        target_concurrent_connections: Some(100),
        interval_period_ms: Some(1000),
        ..Default::default()
    };
    let default_p2p_config = P2pConfig {
        discovery: Some(default_discovery_config.clone()),
        ..Default::default()
    };
    let default_private_discovery_config = DiscoveryConfig {
        target_concurrent_connections: Some(100),
        interval_period_ms: Some(1000),
        access_type: Some(AccessType::Private),
        ..Default::default()
    };

    // None 1, public
    let (builder_1, network_1, key_1) = set_up_network(default_p2p_config.clone());

    let mut config = default_p2p_config.clone();
    config.seed_peers.push(SeedPeer {
        peer_id: Some(network_1.peer_id()),
        address: format!("/dns/localhost/udp/{}", network_1.local_addr().port())
            .parse()
            .unwrap(),
    });

    // Node 2, public, seed: Node 1, allowlist: Node 7, Node 8
    let (mut builder_2, network_2, key_2) = set_up_network(config.clone());

    // Node 3, private, seed: Node 1
    let (mut builder_3, network_3, key_3) = set_up_network(config.clone());

    // Node 4, private, allowlist: Node 3, 5, and 6
    let (mut builder_4, network_4, key_4) = set_up_network(P2pConfig::default());

    // Node 5, private, seed: 4, allowed: 3
    let (builder_5, network_5, key_5) = {
        let mut private_discovery_config = default_private_discovery_config.clone();
        private_discovery_config.allowlisted_peers =
            vec![local_allowlisted_peer(network_3.peer_id(), None)];
        let mut p2p_config = P2pConfig::default().set_discovery_config(private_discovery_config);
        p2p_config.seed_peers.push(SeedPeer {
            peer_id: Some(network_4.peer_id()),
            address: format!("/dns/localhost/udp/{}", network_4.local_addr().port())
                .parse()
                .unwrap(),
        });
        set_up_network(p2p_config)
    };

    // Node 6, private, seed: 4
    let (builder_6, network_6, key_6) = {
        let mut p2p_config =
            P2pConfig::default().set_discovery_config(default_private_discovery_config.clone());
        p2p_config.seed_peers.push(SeedPeer {
            peer_id: Some(network_4.peer_id()),
            address: format!("/dns/localhost/udp/{}", network_4.local_addr().port())
                .parse()
                .unwrap(),
        });
        set_up_network(p2p_config)
    };

    // Node 3: Add Node 4 and Node 5 as seeds
    builder_3.config.discovery = Some(default_private_discovery_config.clone());
    builder_3.config.seed_peers.push(SeedPeer {
        peer_id: Some(network_4.peer_id()),
        address: format!("/dns/localhost/udp/{}", network_4.local_addr().port())
            .parse()
            .unwrap(),
    });
    builder_3.config.seed_peers.push(SeedPeer {
        peer_id: Some(network_5.peer_id()),
        address: format!("/dns/localhost/udp/{}", network_5.local_addr().port())
            .parse()
            .unwrap(),
    });

    // Node 4: Add Node 3 as seed, Node 5 and Node 6 to allowlist
    let mut private_discovery_config = default_private_discovery_config.clone();
    private_discovery_config.allowlisted_peers = vec![
        local_allowlisted_peer(network_5.peer_id(), None),
        local_allowlisted_peer(network_6.peer_id(), None),
    ];
    builder_4.config.discovery = Some(private_discovery_config);
    builder_4.config.seed_peers.push(SeedPeer {
        peer_id: Some(network_3.peer_id()),
        address: format!("/dns/localhost/udp/{}", network_3.local_addr().port())
            .parse()
            .unwrap(),
    });

    // Node 7, private, seed: 2, 8
    let (mut builder_7, network_7, key_7) = set_up_network(
        P2pConfig::default().set_discovery_config(default_private_discovery_config.clone()),
    );

    // Node 9, public
    let (builder_9, network_9, key_9) = set_up_network(default_p2p_config.clone());

    // Node 8, private, seed: 7, 9.  p.s. 8's max connection is 0
    let (builder_8, network_8, key_8) = {
        let mut p2p_config = P2pConfig::default();
        let mut anemo_config = anemo::Config::default();
        anemo_config.max_concurrent_connections = Some(0);
        p2p_config.anemo_config = Some(anemo_config);
        p2p_config.discovery = Some(default_private_discovery_config.clone());
        p2p_config.seed_peers.push(SeedPeer {
            peer_id: Some(network_7.peer_id()),
            address: format!("/dns/localhost/udp/{}", network_7.local_addr().port())
                .parse()
                .unwrap(),
        });
        p2p_config.seed_peers.push(SeedPeer {
            peer_id: Some(network_9.peer_id()),
            address: format!("/dns/localhost/udp/{}", network_9.local_addr().port())
                .parse()
                .unwrap(),
        });
        set_up_network(p2p_config)
    };

    // Node 2, Add Node 7 and Node 8 to allowlist
    let mut discovery_config = default_discovery_config.clone();
    discovery_config.allowlisted_peers = vec![
        local_allowlisted_peer(network_7.peer_id(), None),
        local_allowlisted_peer(network_8.peer_id(), None),
    ];
    builder_2.config.discovery = Some(discovery_config);

    // Node 7: Add Node 2 and Node 8 as seeds
    builder_7.config.discovery = Some(default_private_discovery_config.clone());
    builder_7.config.seed_peers.push(SeedPeer {
        peer_id: Some(network_2.peer_id()),
        address: format!("/dns/localhost/udp/{}", network_2.local_addr().port())
            .parse()
            .unwrap(),
    });
    builder_7.config.seed_peers.push(SeedPeer {
        peer_id: Some(network_8.peer_id()),
        address: format!("/dns/localhost/udp/{}", network_8.local_addr().port())
            .parse()
            .unwrap(),
    });

    // Node 10, private, seed: 9
    let (builder_10, network_10, key_10) = {
        let mut p2p_config = default_p2p_config.clone();
        p2p_config.seed_peers.push(SeedPeer {
            peer_id: Some(network_9.peer_id()),
            address: format!("/dns/localhost/udp/{}", network_9.local_addr().port())
                .parse()
                .unwrap(),
        });
        p2p_config.discovery = Some(default_private_discovery_config.clone());
        set_up_network(p2p_config.clone())
    };

    // Node 11, private, seed: 1, allow: 7, 8
    let (builder_11, network_11, key_11) = {
        let mut p2p_config = default_p2p_config.clone();
        p2p_config.seed_peers.push(SeedPeer {
            peer_id: Some(network_1.peer_id()),
            address: format!("/dns/localhost/udp/{}", network_1.local_addr().port())
                .parse()
                .unwrap(),
        });
        let mut private_discovery_config = default_private_discovery_config.clone();
        private_discovery_config.allowlisted_peers = vec![
            local_allowlisted_peer(network_8.peer_id(), None),
            local_allowlisted_peer(network_7.peer_id(), None),
        ];
        p2p_config.discovery = Some(private_discovery_config);
        set_up_network(p2p_config)
    };

    let (event_loop_1, _handle_1, state_1) = start_network(builder_1, network_1.clone(), key_1);
    let (event_loop_2, _handle_2, state_2) = start_network(builder_2, network_2.clone(), key_2);
    let (event_loop_3, _handle_3, state_3) = start_network(builder_3, network_3.clone(), key_3);
    let (event_loop_4, _handle_4, state_4) = start_network(builder_4, network_4.clone(), key_4);
    let (event_loop_5, _handle_5, state_5) = start_network(builder_5, network_5.clone(), key_5);
    let (event_loop_6, _handle_6, state_6) = start_network(builder_6, network_6.clone(), key_6);
    let (event_loop_7, _handle_7, state_7) = start_network(builder_7, network_7.clone(), key_7);
    let (event_loop_8, _handle_8, state_8) = start_network(builder_8, network_8.clone(), key_8);
    let (event_loop_9, _handle_9, state_9) = start_network(builder_9, network_9.clone(), key_9);
    let (event_loop_10, _handle_10, state_10) =
        start_network(builder_10, network_10.clone(), key_10);
    let (event_loop_11, _handle_11, state_11) =
        start_network(builder_11, network_11.clone(), key_11);

    // Start all the event loops
    tokio::spawn(event_loop_1.start());
    tokio::spawn(event_loop_2.start());
    tokio::spawn(event_loop_3.start());
    tokio::spawn(event_loop_4.start());
    tokio::spawn(event_loop_5.start());
    tokio::spawn(event_loop_6.start());
    tokio::spawn(event_loop_7.start());
    tokio::spawn(event_loop_8.start());
    tokio::spawn(event_loop_9.start());
    tokio::spawn(event_loop_10.start());
    tokio::spawn(event_loop_11.start());

    let peer_id_1 = network_1.peer_id();
    let peer_id_2 = network_2.peer_id();
    let peer_id_3 = network_3.peer_id();
    let peer_id_4 = network_4.peer_id();
    let peer_id_5 = network_5.peer_id();
    let peer_id_6 = network_6.peer_id();
    let peer_id_7 = network_7.peer_id();
    let peer_id_8 = network_8.peer_id();
    let peer_id_9 = network_9.peer_id();
    let peer_id_10 = network_10.peer_id();
    let peer_id_11 = network_11.peer_id();

    info!("peer_id_1: {:?}", peer_id_1);
    info!("peer_id_2: {:?}", peer_id_2);
    info!("peer_id_3: {:?}", peer_id_3);
    info!("peer_id_4: {:?}", peer_id_4);
    info!("peer_id_5: {:?}", peer_id_5);
    info!("peer_id_6: {:?}", peer_id_6);
    info!("peer_id_7: {:?}", peer_id_7);
    info!("peer_id_8: {:?}", peer_id_8);
    info!("peer_id_9: {:?}", peer_id_9);
    info!("peer_id_10: {:?}", peer_id_10);
    info!("peer_id_11: {:?}", peer_id_11);

    // Let them fully connect
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Node 1 is connected to everyone. But it only "knows" public nodes (2, 9).
    assert_peers(
        "Node 1",
        &network_1,
        &state_1,
        HashSet::from_iter(vec![]),
        HashSet::from_iter(vec![
            peer_id_2, peer_id_3, peer_id_4, peer_id_5, peer_id_6, peer_id_7, peer_id_8, peer_id_9,
            peer_id_10, peer_id_11,
        ]),
        HashSet::from_iter(vec![peer_id_2, peer_id_9]),
        HashSet::from_iter(vec![
            peer_id_2, peer_id_3, peer_id_4, peer_id_5, peer_id_6, peer_id_7, peer_id_8, peer_id_9,
            peer_id_10, peer_id_11,
        ]),
    );

    // Node 2 is connected to everyone. But it does not "know" private nodes except the allowlisted ones 7 and 8.
    assert_peers(
        "Node 2",
        &network_2,
        &state_2,
        HashSet::from_iter(vec![peer_id_1, peer_id_7, peer_id_8]),
        HashSet::from_iter(vec![
            peer_id_1, peer_id_3, peer_id_4, peer_id_5, peer_id_6, peer_id_7, peer_id_8, peer_id_9,
            peer_id_10, peer_id_11,
        ]),
        HashSet::from_iter(vec![peer_id_1, peer_id_7, peer_id_8, peer_id_9]),
        HashSet::from_iter(vec![
            peer_id_1, peer_id_3, peer_id_4, peer_id_5, peer_id_6, peer_id_7, peer_id_8, peer_id_9,
            peer_id_10, peer_id_11,
        ]),
    );

    // Node 3 connects to seeds 1, 4, 5 and discovers more via gossip.
    assert_peers(
        "Node 3",
        &network_3,
        &state_3,
        HashSet::from_iter(vec![peer_id_1, peer_id_4, peer_id_5]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_4, peer_id_5, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_4, peer_id_5, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_4, peer_id_5, peer_id_9]),
    );

    // Node 4 connects to seed 3 and discovers more via gossip.
    assert_peers(
        "Node 4",
        &network_4,
        &state_4,
        HashSet::from_iter(vec![peer_id_3, peer_id_5, peer_id_6]),
        HashSet::from_iter(vec![
            peer_id_1, peer_id_2, peer_id_3, peer_id_5, peer_id_6, peer_id_9,
        ]),
        HashSet::from_iter(vec![
            peer_id_1, peer_id_2, peer_id_3, peer_id_5, peer_id_6, peer_id_9,
        ]),
        HashSet::from_iter(vec![
            peer_id_1, peer_id_2, peer_id_3, peer_id_5, peer_id_6, peer_id_9,
        ]),
    );

    // Node 5 connects to seed 4 and discovers more via gossip.
    assert_peers(
        "Node 5",
        &network_5,
        &state_5,
        HashSet::from_iter(vec![peer_id_3, peer_id_4]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_3, peer_id_4, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_3, peer_id_4, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_3, peer_id_4, peer_id_9]),
    );

    // Node 6 connects to seed 4 and discovers more via gossip.
    assert_peers(
        "Node 6",
        &network_6,
        &state_6,
        HashSet::from_iter(vec![peer_id_4]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_4, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_4, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_4, peer_id_9]),
    );

    // Node 7 connects to seeds 2 and 8 and discovers more via gossip.
    // Node 7 is private so its info is NOT shared via gossip - Node 11 can't discover it.
    assert_peers(
        "Node 7",
        &network_7,
        &state_7,
        HashSet::from_iter(vec![peer_id_2, peer_id_8]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_8, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_8, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_8, peer_id_9]),
    );

    // Node 8 has seeds 7 and 9, but max_concurrent_connections is 0 so it can't accept more connections.
    // Node 8 is private so its info is NOT shared via gossip.
    assert_peers(
        "Node 8",
        &network_8,
        &state_8,
        HashSet::from_iter(vec![peer_id_7, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_7, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_7, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_7, peer_id_9]),
    );

    // Node 9 (public, no seeds) is connected by many nodes that discover it.
    assert_peers(
        "Node 9",
        &network_9,
        &state_9,
        HashSet::from_iter(vec![]),
        HashSet::from_iter(vec![
            peer_id_1, peer_id_2, peer_id_3, peer_id_4, peer_id_5, peer_id_6, peer_id_7, peer_id_8,
            peer_id_10, peer_id_11,
        ]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2]),
        HashSet::from_iter(vec![
            peer_id_1, peer_id_2, peer_id_3, peer_id_4, peer_id_5, peer_id_6, peer_id_7, peer_id_8,
            peer_id_10, peer_id_11,
        ]),
    );

    // Node 10 connects to Node 9 (seed) and discovers more.
    assert_peers(
        "Node 10",
        &network_10,
        &state_10,
        HashSet::from_iter(vec![peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_9]),
    );

    // Node 11 connects to seed 1 and discovers more.
    // Node 11 allowlists 7 and 8 (no addresses), but they're private so their info isn't shared via gossip.
    // Node 11 can't discover them.
    assert_peers(
        "Node 11",
        &network_11,
        &state_11,
        HashSet::from_iter(vec![peer_id_1, peer_id_7, peer_id_8]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_9]),
    );
}

fn assert_peers(
    self_name: &str,
    network: &Network,
    state: &Arc<RwLock<State>>,
    expected_network_known_peers: HashSet<PeerId>,
    expected_network_connected_peers: HashSet<PeerId>,
    expected_discovery_known_peers: HashSet<PeerId>,
    expected_discovery_connected_peers: HashSet<PeerId>,
) {
    let actual = network
        .known_peers()
        .get_all()
        .iter()
        .map(|pi| pi.peer_id)
        .collect::<HashSet<_>>();
    assert_eq!(
        actual, expected_network_known_peers,
        "{} network known peers mismatch. Expected: {:#?}, actual: {:#?}",
        self_name, expected_network_known_peers, actual,
    );
    let actual = network.peers().iter().copied().collect::<HashSet<_>>();
    assert_eq!(
        actual, expected_network_connected_peers,
        "{} network connected peers mismatch. Expected: {:#?}, actual: {:#?}",
        self_name, expected_network_connected_peers, actual,
    );
    let actual = state
        .read()
        .unwrap()
        .known_peers
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    assert_eq!(
        actual, expected_discovery_known_peers,
        "{} discovery known peers mismatch. Expected: {:#?}, actual: {:#?}",
        self_name, expected_discovery_known_peers, actual,
    );

    let actual = state
        .read()
        .unwrap()
        .connected_peers
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    assert_eq!(
        actual, expected_discovery_connected_peers,
        "{} discovery connected peers mismatch. Expected: {:#?}, actual: {:#?}",
        self_name, expected_discovery_connected_peers, actual,
    );
}

fn unwrap_new_peer_event(event: PeerEvent) -> PeerId {
    match event {
        PeerEvent::NewPeer(peer_id) => peer_id,
        e => panic!("unexpected event: {e:?}"),
    }
}

fn local_allowlisted_peer(peer_id: PeerId, port: Option<u16>) -> AllowlistedPeer {
    AllowlistedPeer {
        peer_id,
        address: port.map(|port| format!("/dns/localhost/udp/{}", port).parse().unwrap()),
    }
}

fn set_up_network(p2p_config: P2pConfig) -> (UnstartedDiscovery, Network, NetworkKeyPair) {
    let anemo_config = p2p_config.anemo_config.clone().unwrap_or_default();
    let (builder, server, _em) = Builder::new().config(p2p_config).build();
    let (network, keypair) =
        build_network_with_anemo_config(|router| router.add_rpc_service(server), anemo_config);
    (builder, network, keypair)
}

fn start_network(
    builder: UnstartedDiscovery,
    network: Network,
    keypair: NetworkKeyPair,
) -> (DiscoveryEventLoop, Handle, Arc<RwLock<State>>) {
    let (mut event_loop, handle) = builder.build(network.clone(), keypair);
    event_loop.config.external_address = Some(
        format!("/dns/localhost/udp/{}", network.local_addr().port())
            .parse()
            .unwrap(),
    );
    let state = event_loop.state.clone();
    (event_loop, handle, state)
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_address_source_priority() -> Result<()> {
    let config = P2pConfig::default();
    let (builder, server, endpoint_manager_1) = Builder::new().config(config.clone()).build();
    let (network_1, key_1) = build_network_and_key(|router| router.add_rpc_service(server));
    let (event_loop_1, _handle_1) = builder.build(network_1.clone(), key_1);

    let (builder, server, _em) = Builder::new().config(config.clone()).build();
    let (network_2, key_2) = build_network_and_key(|router| router.add_rpc_service(server));
    let (event_loop_2, _handle_2) = builder.build(network_2.clone(), key_2);

    let state_1 = event_loop_1.state.clone();

    tokio::spawn(event_loop_1.start());
    tokio::spawn(event_loop_2.start());

    let peer_id_2 = network_2.peer_id();
    let peer_2_network_pubkey =
        Ed25519PublicKey(ed25519_consensus::VerificationKey::try_from(peer_id_2.0).unwrap());

    let chain_addr: Multiaddr = "/dns/chain.example.com/udp/8080".parse().unwrap();
    let admin_addr: Multiaddr = "/dns/admin.example.com/udp/9090".parse().unwrap();

    // First, set Chain source address
    endpoint_manager_1
        .update_endpoint(
            EndpointId::P2p(PeerId(peer_2_network_pubkey.0.to_bytes())),
            AddressSource::Chain,
            vec![chain_addr.clone()],
        )
        .unwrap();

    // Allow discovery to process the message
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify Chain address is used
    let known_peer = network_1.known_peers().get(&peer_id_2);
    assert!(known_peer.is_some());
    let addrs = known_peer.unwrap().address;
    assert_eq!(addrs.len(), 1);
    assert!(addrs[0].to_string().contains("chain"));

    // Now set Admin source address (should take priority)
    endpoint_manager_1
        .update_endpoint(
            EndpointId::P2p(PeerId(peer_2_network_pubkey.0.to_bytes())),
            AddressSource::Admin,
            vec![admin_addr.clone()],
        )
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify Admin address now takes priority
    let known_peer = network_1.known_peers().get(&peer_id_2);
    assert!(known_peer.is_some());
    let addrs = known_peer.unwrap().address;
    assert_eq!(addrs.len(), 1);
    assert!(addrs[0].to_string().contains("admin"));

    // Both sources should be stored
    let state = state_1.read().unwrap();
    let sources = state.peer_addresses.get(&peer_id_2).unwrap();
    assert_eq!(sources.len(), 2);
    assert!(sources.contains_key(&AddressSource::Admin));
    assert!(sources.contains_key(&AddressSource::Chain));

    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_address_source_clear() -> Result<()> {
    let config = P2pConfig::default();
    let (builder, server, endpoint_manager_1) = Builder::new().config(config.clone()).build();
    let (network_1, key_1) = build_network_and_key(|router| router.add_rpc_service(server));
    let (event_loop_1, _handle_1) = builder.build(network_1.clone(), key_1);

    let (builder, server, _em) = Builder::new().config(config.clone()).build();
    let (network_2, key_2) = build_network_and_key(|router| router.add_rpc_service(server));
    let (event_loop_2, _handle_2) = builder.build(network_2.clone(), key_2);

    let state_1 = event_loop_1.state.clone();

    tokio::spawn(event_loop_1.start());
    tokio::spawn(event_loop_2.start());

    let peer_id_2 = network_2.peer_id();
    let peer_2_network_pubkey =
        Ed25519PublicKey(ed25519_consensus::VerificationKey::try_from(peer_id_2.0).unwrap());

    let chain_addr: Multiaddr = "/dns/chain.example.com/udp/8080".parse().unwrap();
    let admin_addr: Multiaddr = "/dns/admin.example.com/udp/9090".parse().unwrap();

    // Set both sources
    endpoint_manager_1
        .update_endpoint(
            EndpointId::P2p(PeerId(peer_2_network_pubkey.0.to_bytes())),
            AddressSource::Chain,
            vec![chain_addr.clone()],
        )
        .unwrap();
    endpoint_manager_1
        .update_endpoint(
            EndpointId::P2p(PeerId(peer_2_network_pubkey.0.to_bytes())),
            AddressSource::Admin,
            vec![admin_addr.clone()],
        )
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify Admin address is used
    let known_peer = network_1.known_peers().get(&peer_id_2);
    assert!(known_peer.is_some());
    let addrs = known_peer.unwrap().address;
    assert!(addrs[0].to_string().contains("admin"));

    // Clear Admin source by sending empty addresses
    endpoint_manager_1
        .update_endpoint(
            EndpointId::P2p(PeerId(peer_2_network_pubkey.0.to_bytes())),
            AddressSource::Admin,
            vec![],
        )
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify it falls back to Chain address
    let known_peer = network_1.known_peers().get(&peer_id_2);
    assert!(known_peer.is_some());
    let addrs = known_peer.unwrap().address;
    assert_eq!(addrs.len(), 1);
    assert!(addrs[0].to_string().contains("chain"));

    // Only Chain source should remain
    let state = state_1.read().unwrap();
    let sources = state.peer_addresses.get(&peer_id_2).unwrap();
    assert_eq!(sources.len(), 1);
    assert!(sources.contains_key(&AddressSource::Chain));
    assert!(!sources.contains_key(&AddressSource::Admin));

    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_address_source_clear_all() -> Result<()> {
    let config = P2pConfig::default();
    let (builder, server, endpoint_manager_1) = Builder::new().config(config.clone()).build();
    let (network_1, key_1) = build_network_and_key(|router| router.add_rpc_service(server));
    let (event_loop_1, _handle_1) = builder.build(network_1.clone(), key_1);

    let (builder, server, _em) = Builder::new().config(config.clone()).build();
    let (network_2, key_2) = build_network_and_key(|router| router.add_rpc_service(server));
    let (event_loop_2, _handle_2) = builder.build(network_2.clone(), key_2);

    tokio::spawn(event_loop_1.start());
    tokio::spawn(event_loop_2.start());

    let peer_id_2 = network_2.peer_id();
    let peer_2_network_pubkey =
        Ed25519PublicKey(ed25519_consensus::VerificationKey::try_from(peer_id_2.0).unwrap());

    let chain_addr: Multiaddr = "/dns/chain.example.com/udp/8080".parse().unwrap();

    // Set Chain source
    endpoint_manager_1
        .update_endpoint(
            EndpointId::P2p(PeerId(peer_2_network_pubkey.0.to_bytes())),
            AddressSource::Chain,
            vec![chain_addr.clone()],
        )
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify peer is known
    assert!(network_1.known_peers().get(&peer_id_2).is_some());

    // Clear Chain source
    endpoint_manager_1
        .update_endpoint(
            EndpointId::P2p(PeerId(peer_2_network_pubkey.0.to_bytes())),
            AddressSource::Chain,
            vec![],
        )
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify peer has empty addresses (clearing all sources)
    let known_peer = network_1.known_peers().get(&peer_id_2);
    assert!(known_peer.is_some());
    assert!(known_peer.unwrap().address.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_save_and_load_stored_peers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("peer_cache.yaml");

    use fastcrypto::traits::KeyPair as _;
    let keypair = sui_types::crypto::NetworkKeyPair::generate(&mut rand::thread_rng());
    let peer_id = PeerId(*fastcrypto::traits::KeyPair::public(&keypair).0.as_bytes());
    let mut addresses = BTreeMap::new();
    addresses.insert(
        EndpointId::P2p(peer_id),
        vec!["/ip4/127.0.0.1/udp/8080".parse().unwrap()],
    );
    let info = VersionedNodeInfo::V2(NodeInfoV2 {
        addresses,
        timestamp_ms: now_unix(),
        access_type: sui_config::p2p::AccessType::Public,
    });
    let signed = info.sign(&keypair);

    save_stored_peers(&path, std::slice::from_ref(&signed));
    assert!(path.exists());

    let loaded = load_stored_peers(&path);
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].data(), signed.data());
}

#[tokio::test]
async fn test_load_stored_peers_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nonexistent.yaml");
    let loaded = load_stored_peers(&path);
    assert!(loaded.is_empty());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_discovery_address_cleared_on_expiry() -> Result<()> {
    // Set up two peers. Peer 2 is a seed peer of peer 1's network.
    let (network_2, _key_2) = build_network_and_key(|router| router);
    let peer_id_2 = network_2.peer_id();

    let port = network_2.local_addr().port();
    let seed_multiaddr: Multiaddr = format!("/ip4/127.0.0.1/udp/{port}").parse().unwrap();
    let seed_addr: anemo::types::Address = seed_multiaddr.to_anemo_address().unwrap();
    let config = P2pConfig {
        seed_peers: vec![SeedPeer {
            peer_id: Some(peer_id_2),
            address: seed_multiaddr,
        }],
        ..Default::default()
    };

    let (builder, server, _em) = Builder::new().config(config).build();
    let (network_1, key_1) = build_network_and_key(|router| router.add_rpc_service(server));
    let (mut event_loop, _handle) = builder.build(network_1.clone(), key_1);

    // Simulate startup: configure preferred peers (registers Seed addresses)
    event_loop.construct_our_info();
    event_loop.configure_preferred_peers();

    // Verify Seed source is registered
    {
        let state = event_loop.state.read().unwrap();
        let sources = state.peer_addresses.get(&peer_id_2).unwrap();
        assert!(sources.contains_key(&AddressSource::Seed));
    }

    // Simulate receiving Discovery addresses for peer 2 (P2P)
    let discovery_multiaddr: Multiaddr = "/ip4/10.0.0.1/udp/9000".parse().unwrap();
    let discovery_addr = discovery_multiaddr.to_anemo_address().unwrap();
    assert_ne!(
        seed_addr, discovery_addr,
        "test requires distinct addresses"
    );
    event_loop.handle_peer_address_change(
        peer_id_2,
        AddressSource::Discovery,
        vec![discovery_addr.clone()],
    );

    // Also simulate receiving a Consensus Discovery address for peer 2.
    // This mirrors what update_known_peers_versioned does for V2 node info.
    let consensus_addr: Multiaddr = "/ip4/10.0.0.1/udp/9001".parse().unwrap();
    let peer_2_network_pubkey =
        NetworkPublicKey::from_bytes(&peer_id_2.0).expect("PeerId is a valid public key");
    event_loop
        .endpoint_manager
        .update_endpoint(
            EndpointId::Consensus(peer_2_network_pubkey.clone()),
            AddressSource::Discovery,
            vec![consensus_addr],
        )
        .unwrap();

    // Verify Discovery takes priority over Seed
    {
        let state = event_loop.state.read().unwrap();
        let sources = state.peer_addresses.get(&peer_id_2).unwrap();
        assert!(sources.contains_key(&AddressSource::Discovery));
        assert!(sources.contains_key(&AddressSource::Seed));
        let (top_source, _) = sources.first_key_value().unwrap();
        assert_eq!(*top_source, AddressSource::Discovery);
    }
    let known = network_1.known_peers().get(&peer_id_2).unwrap();
    assert_eq!(known.address, vec![discovery_addr.clone()]);

    // Insert an expired entry in known_peers_v2 for peer 2
    {
        use fastcrypto::traits::KeyPair as _;
        let keypair = sui_types::crypto::NetworkKeyPair::generate(&mut rand::thread_rng());
        let old_timestamp = now_unix() - ONE_DAY_MILLISECONDS - 1000;
        let mut addresses = BTreeMap::new();
        addresses.insert(
            EndpointId::P2p(peer_id_2),
            vec!["/ip4/10.0.0.1/udp/9000".parse().unwrap()],
        );
        let info = VersionedNodeInfo::V2(NodeInfoV2 {
            addresses,
            timestamp_ms: old_timestamp,
            access_type: AccessType::Public,
        });
        let signed = info.sign(&keypair);
        let verified = VerifiedSignedVersionedNodeInfo::new_unchecked(signed);
        event_loop
            .state
            .write()
            .unwrap()
            .known_peers_v2
            .insert(peer_id_2, verified);
    }

    // Run handle_tick — the expired entry should be culled and Discovery addresses cleared.
    // clear_source sends the P2P clear through the mailbox, so drain pending messages.
    event_loop.handle_tick(std::time::Instant::now(), now_unix());
    while let Ok(msg) = event_loop.mailbox.try_recv() {
        event_loop.handle_message(msg);
    }

    // Verify P2P Discovery source was cleared, only Seed remains
    {
        let state = event_loop.state.read().unwrap();
        let sources = state.peer_addresses.get(&peer_id_2).unwrap();
        assert!(
            !sources.contains_key(&AddressSource::Discovery),
            "Discovery source should be cleared after expiry"
        );
        assert!(
            sources.contains_key(&AddressSource::Seed),
            "Seed source should remain"
        );
    }

    // Verify network now uses Seed addresses (fallback)
    let known = network_1.known_peers().get(&peer_id_2).unwrap();
    assert_eq!(known.address, vec![seed_addr]);

    // Verify Consensus Discovery address was also cleared.
    // The endpoint_manager buffers consensus updates when no updater is set.
    // Set a mock updater to drain the buffer and inspect the updates.
    use crate::endpoint_manager::ConsensusAddressUpdater;
    use sui_types::error::SuiResult;

    struct RecordingUpdater(
        std::sync::Mutex<Vec<(NetworkPublicKey, AddressSource, Vec<Multiaddr>)>>,
    );
    impl ConsensusAddressUpdater for RecordingUpdater {
        fn update_address(
            &self,
            pubkey: NetworkPublicKey,
            source: AddressSource,
            addrs: Vec<Multiaddr>,
        ) -> SuiResult<()> {
            self.0.lock().unwrap().push((pubkey, source, addrs));
            Ok(())
        }
    }

    let updater = Arc::new(RecordingUpdater(std::sync::Mutex::new(Vec::new())));
    event_loop
        .endpoint_manager
        .set_consensus_address_updater(updater.clone());

    let updates = updater.0.lock().unwrap();
    let clear = updates.iter().find(|(pubkey, source, addrs)| {
        *pubkey == peer_2_network_pubkey && *source == AddressSource::Discovery && addrs.is_empty()
    });
    assert!(
        clear.is_some(),
        "Expected a Consensus Discovery clear for the expired peer"
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_seed_fallback_on_discovery_clear() -> Result<()> {
    let config = P2pConfig::default();
    let (builder, server, endpoint_manager) = Builder::new().config(config.clone()).build();
    let (network_1, key_1) = build_network_and_key(|router| router.add_rpc_service(server));
    let (event_loop, _handle) = builder.build(network_1.clone(), key_1);

    let state = event_loop.state.clone();

    tokio::spawn(event_loop.start());

    let (network_2, _key_2) = build_network_and_key(|router| router);
    let peer_id_2 = network_2.peer_id();
    let peer_2_network_pubkey =
        Ed25519PublicKey(ed25519_consensus::VerificationKey::try_from(peer_id_2.0).unwrap());

    let seed_addr: Multiaddr = "/dns/seed.example.com/udp/8080".parse().unwrap();
    let discovery_addr: Multiaddr = "/dns/discovery.example.com/udp/9090".parse().unwrap();

    // Set Seed source
    endpoint_manager
        .update_endpoint(
            EndpointId::P2p(PeerId(peer_2_network_pubkey.0.to_bytes())),
            AddressSource::Seed,
            vec![seed_addr.clone()],
        )
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify Seed address is used
    let known = network_1.known_peers().get(&peer_id_2);
    assert!(known.is_some());
    assert!(known.unwrap().address[0].to_string().contains("seed"));

    // Set Discovery source (higher priority)
    endpoint_manager
        .update_endpoint(
            EndpointId::P2p(PeerId(peer_2_network_pubkey.0.to_bytes())),
            AddressSource::Discovery,
            vec![discovery_addr.clone()],
        )
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify Discovery takes priority
    let known = network_1.known_peers().get(&peer_id_2).unwrap();
    assert!(known.address[0].to_string().contains("discovery"));

    // Clear Discovery source
    endpoint_manager
        .update_endpoint(
            EndpointId::P2p(PeerId(peer_2_network_pubkey.0.to_bytes())),
            AddressSource::Discovery,
            vec![],
        )
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify fallback to Seed
    let known = network_1.known_peers().get(&peer_id_2).unwrap();
    assert_eq!(known.address.len(), 1);
    assert!(known.address[0].to_string().contains("seed"));

    // Only Seed source should remain
    let s = state.read().unwrap();
    let sources = s.peer_addresses.get(&peer_id_2).unwrap();
    assert_eq!(sources.len(), 1);
    assert!(sources.contains_key(&AddressSource::Seed));

    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_discovery_only_peer_not_in_network_known_peers() -> Result<()> {
    // A peer learned purely through discovery gossip (no Seed/Chain/Config/Admin source)
    // should appear in state.known_peers_v2 but NOT in network.known_peers().
    let config = P2pConfig::default();
    let (builder, server, _em) = Builder::new().config(config).build();
    let (network, key) = build_network_and_key(|router| router.add_rpc_service(server));
    let (mut event_loop, _handle) = builder.build(network.clone(), key);

    event_loop.construct_our_info();
    event_loop.configure_preferred_peers();

    // Simulate receiving a gossiped peer via update_known_peers_versioned (V3 path).
    use fastcrypto::traits::KeyPair as _;
    let gossip_keypair = sui_types::crypto::NetworkKeyPair::generate(&mut rand::thread_rng());
    let gossip_peer_id = PeerId(
        *fastcrypto::traits::KeyPair::public(&gossip_keypair)
            .0
            .as_bytes(),
    );
    let mut addresses = BTreeMap::new();
    addresses.insert(
        EndpointId::P2p(gossip_peer_id),
        vec!["/ip4/10.0.0.1/udp/8080".parse().unwrap()],
    );
    let gossiped_info = VersionedNodeInfo::V2(NodeInfoV2 {
        addresses,
        timestamp_ms: now_unix(),
        access_type: AccessType::Public,
    })
    .sign(&gossip_keypair);

    update_known_peers_versioned(
        event_loop.state.clone(),
        event_loop.metrics.clone(),
        vec![gossiped_info],
        event_loop.configured_peers.clone(),
        &event_loop.endpoint_manager,
    );

    // Drain mailbox in case update_known_peers_versioned sent any messages.
    while let Ok(msg) = event_loop.mailbox.try_recv() {
        event_loop.handle_message(msg);
    }

    // Peer should be in discovery state...
    {
        let state = event_loop.state.read().unwrap();
        assert!(
            state.known_peers_v2.contains_key(&gossip_peer_id),
            "Gossiped peer should be in state.known_peers_v2"
        );
    }

    // ...but NOT in network.known_peers (no address source was registered)
    assert!(
        network.known_peers().get(&gossip_peer_id).is_none(),
        "Discovery-only peer should not appear in network.known_peers()"
    );
    assert!(
        event_loop
            .state
            .read()
            .unwrap()
            .peer_addresses
            .get(&gossip_peer_id)
            .is_none(),
        "Discovery-only peer should not have any entry in peer_addresses"
    );

    // Run a tick — discovery should try to dial the peer via try_to_connect_to_peer
    // but still not add it to network.known_peers().
    event_loop.handle_tick(std::time::Instant::now(), now_unix());

    assert!(
        network.known_peers().get(&gossip_peer_id).is_none(),
        "Discovery-only peer should not appear in network.known_peers() after tick"
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_configured_peer_discovery_address_in_network_known_peers() -> Result<()> {
    // When a configured (seed) peer's cached node info is loaded on startup,
    // the Discovery P2P addresses should appear in network.known_peers(),
    // overriding the lower-priority Seed address.
    use fastcrypto::traits::KeyPair as _;

    let dir = tempfile::tempdir().unwrap();
    let store_path = dir.path().join("peer_cache.yaml");

    // Generate a keypair for the seed peer.
    let seed_keypair = sui_types::crypto::NetworkKeyPair::generate(&mut rand::thread_rng());
    let seed_peer_id = PeerId(
        *fastcrypto::traits::KeyPair::public(&seed_keypair)
            .0
            .as_bytes(),
    );

    // Save a cached node info entry with a Discovery-sourced P2P address.
    let discovery_multiaddr: Multiaddr = "/ip4/10.0.0.1/udp/9000".parse().unwrap();
    let mut addresses = BTreeMap::new();
    addresses.insert(
        EndpointId::P2p(seed_peer_id),
        vec![discovery_multiaddr.clone()],
    );
    let cached_info = VersionedNodeInfo::V2(NodeInfoV2 {
        addresses,
        timestamp_ms: now_unix(),
        access_type: AccessType::Public,
    })
    .sign(&seed_keypair);
    save_stored_peers(&store_path, std::slice::from_ref(&cached_info));

    // Build a node with this peer as a seed and the store path configured.
    let seed_multiaddr: Multiaddr = "/ip4/192.168.1.1/udp/8080".parse().unwrap();
    let config = P2pConfig {
        seed_peers: vec![SeedPeer {
            peer_id: Some(seed_peer_id),
            address: seed_multiaddr.clone(),
        }],
        discovery: Some(DiscoveryConfig {
            peer_addr_store_path: Some(store_path),
            ..Default::default()
        }),
        ..Default::default()
    };

    let (builder, server, _em) = Builder::new().config(config).build();
    let (network, key) = build_network_and_key(|router| router.add_rpc_service(server));
    let (mut event_loop, _handle) = builder.build(network.clone(), key);

    // Run the startup sequence.
    event_loop.construct_our_info();
    event_loop.configure_preferred_peers();
    event_loop.load_stored_peers_on_startup();

    // Verify the Discovery address (higher priority) is used in network.known_peers,
    // not the Seed address.
    let known = network.known_peers().get(&seed_peer_id);
    assert!(known.is_some(), "Configured peer should be in known_peers");
    let discovery_anemo_addr = discovery_multiaddr.to_anemo_address().unwrap();
    assert_eq!(
        known.unwrap().address,
        vec![discovery_anemo_addr],
        "network.known_peers should use the Discovery address (higher priority than Seed)"
    );

    // Both sources should be tracked.
    let state = event_loop.state.read().unwrap();
    let sources = state.peer_addresses.get(&seed_peer_id).unwrap();
    assert!(sources.contains_key(&AddressSource::Discovery));
    assert!(sources.contains_key(&AddressSource::Seed));

    Ok(())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_runtime_gossip_updates_configured_peer_address() -> Result<()> {
    // Scenario: a configured peer starts with a Chain address, then during runtime
    // we receive a different P2P address via Discovery gossip. The Discovery address
    // should override the Chain address in network.known_peers().
    use fastcrypto::traits::KeyPair as _;

    // Generate a keypair for the remote peer (a validator we'll configure).
    let remote_keypair = sui_types::crypto::NetworkKeyPair::generate(&mut rand::thread_rng());
    let remote_peer_id = PeerId(
        *fastcrypto::traits::KeyPair::public(&remote_keypair)
            .0
            .as_bytes(),
    );

    // Configure the remote peer as a seed so it's in configured_peers.
    let seed_multiaddr: Multiaddr = "/ip4/192.168.1.1/udp/8080".parse().unwrap();
    let config = P2pConfig {
        seed_peers: vec![SeedPeer {
            peer_id: Some(remote_peer_id),
            address: seed_multiaddr.clone(),
        }],
        ..Default::default()
    };

    let (builder, server, endpoint_manager) = Builder::new().config(config).build();
    let (network, key) = build_network_and_key(|router| router.add_rpc_service(server));
    let (mut event_loop, _handle) = builder.build(network.clone(), key);

    // Run the startup sequence (no stored peers).
    event_loop.construct_our_info();
    event_loop.configure_preferred_peers();

    // Verify Seed address is active.
    let seed_anemo_addr = seed_multiaddr.to_anemo_address().unwrap();
    let known = network.known_peers().get(&remote_peer_id).unwrap();
    assert_eq!(known.address, vec![seed_anemo_addr.clone()]);

    // Also set a Chain address (simulating what sui-node does at epoch start).
    let chain_multiaddr: Multiaddr = "/ip4/172.16.0.1/udp/8080".parse().unwrap();
    endpoint_manager
        .update_endpoint(
            EndpointId::P2p(remote_peer_id),
            AddressSource::Chain,
            vec![chain_multiaddr.clone()],
        )
        .unwrap();

    // Process the mailbox message from update_endpoint.
    while let Ok(msg) = event_loop.mailbox.try_recv() {
        event_loop.handle_message(msg);
    }

    // Seed has higher priority than Chain, so Seed address should still be active.
    let known = network.known_peers().get(&remote_peer_id).unwrap();
    assert_eq!(known.address, vec![seed_anemo_addr]);

    // Now simulate runtime gossip: we receive a V2 node info for the remote peer
    // with a different P2P address.
    let discovery_multiaddr: Multiaddr = "/ip4/10.0.0.1/udp/9000".parse().unwrap();
    let mut addresses = BTreeMap::new();
    addresses.insert(
        EndpointId::P2p(remote_peer_id),
        vec![discovery_multiaddr.clone()],
    );
    let gossiped_info = VersionedNodeInfo::V2(NodeInfoV2 {
        addresses,
        timestamp_ms: now_unix(),
        access_type: AccessType::Public,
    })
    .sign(&remote_keypair);

    update_known_peers_versioned(
        event_loop.state.clone(),
        event_loop.metrics.clone(),
        vec![gossiped_info],
        event_loop.configured_peers.clone(),
        &event_loop.endpoint_manager,
    );

    // Process any mailbox messages generated by update_known_peers_versioned.
    while let Ok(msg) = event_loop.mailbox.try_recv() {
        event_loop.handle_message(msg);
    }

    // The Discovery address should now be active (higher priority than Seed and Chain).
    let discovery_anemo_addr = discovery_multiaddr.to_anemo_address().unwrap();
    let known = network.known_peers().get(&remote_peer_id).unwrap();
    assert_eq!(
        known.address,
        vec![discovery_anemo_addr],
        "network.known_peers should use the Discovery address from runtime gossip"
    );

    // All three sources should be tracked.
    let state = event_loop.state.read().unwrap();
    let sources = state.peer_addresses.get(&remote_peer_id).unwrap();
    assert!(
        sources.contains_key(&AddressSource::Discovery),
        "Discovery source should be registered from runtime gossip"
    );
    assert!(sources.contains_key(&AddressSource::Seed));
    assert!(sources.contains_key(&AddressSource::Chain));

    Ok(())
}

#[tokio::test]
async fn peer_failure_report_adds_cooldown() -> Result<()> {
    let config = P2pConfig {
        discovery: Some(DiscoveryConfig {
            min_peers_for_disconnect: Some(0),
            ..Default::default()
        }),
        ..Default::default()
    };
    let (builder, _server, _em) = Builder::new().config(config).build();
    let (network, keypair) = build_network_and_key(|router| router);
    let (mut event_loop, _handle) = builder.build(network.clone(), keypair);

    let peer_id = PeerId([42; 32]);

    assert!(!event_loop.peer_cooldowns.contains_key(&peer_id));

    event_loop.handle_peer_failure_report(peer_id);

    assert!(event_loop.peer_cooldowns.contains_key(&peer_id));
    Ok(())
}

#[tokio::test]
async fn cooldown_peers_deprioritized_in_handle_tick() -> Result<()> {
    let mut config = P2pConfig::default();
    let (builder, server, _em) = Builder::new().config(config.clone()).build();
    let (network_1, key_1) = build_network_and_key(|router| router.add_rpc_service(server));
    let (_event_loop_1, _handle_1) = builder.build(network_1.clone(), key_1);

    config.seed_peers.push(SeedPeer {
        peer_id: Some(network_1.peer_id()),
        address: format!("/dns/localhost/udp/{}", network_1.local_addr().port()).parse()?,
    });
    let (builder, server, _em) = Builder::new().config(config).build();
    let (network_2, key_2) = build_network_and_key(|router| router.add_rpc_service(server));
    let (mut event_loop_2, _handle_2) = builder.build(network_2.clone(), key_2);

    // Put network_1's peer on cooldown
    event_loop_2
        .peer_cooldowns
        .insert(network_1.peer_id(), std::time::Instant::now());

    // Add network_1 as a known peer so it's eligible for dialing
    let peer_info = NodeInfo {
        peer_id: network_1.peer_id(),
        addresses: vec![format!("/dns/localhost/udp/{}", network_1.local_addr().port()).parse()?],
        timestamp_ms: now_unix(),
        access_type: AccessType::Public,
    };
    event_loop_2.state.write().unwrap().known_peers.insert(
        network_1.peer_id(),
        VerifiedSignedNodeInfo::new_unchecked(SignedNodeInfo::new_from_data_and_sig(
            peer_info,
            Ed25519Signature::default(),
        )),
    );

    // Since the peer is on cooldown and it's the only peer, it should still be dialed
    // (cooldown peers are deprioritized, not blocked)
    event_loop_2.handle_tick(std::time::Instant::now(), now_unix());

    assert!(
        event_loop_2
            .pending_dials
            .contains_key(&network_1.peer_id()),
        "cooldown peer should still be dialed when no preferred peers exist"
    );

    Ok(())
}

#[tokio::test]
async fn expired_cooldown_moves_peer_to_preferred() -> Result<()> {
    let config = P2pConfig {
        discovery: Some(DiscoveryConfig {
            peer_failure_cooldown_ms: Some(1),
            ..Default::default()
        }),
        ..Default::default()
    };
    let (builder, _server, _em) = Builder::new().config(config).build();
    let (network, keypair) = build_network_and_key(|router| router);
    let (mut event_loop, _handle) = builder.build(network, keypair);

    let peer_id = PeerId([42; 32]);
    event_loop.peer_cooldowns.insert(
        peer_id,
        std::time::Instant::now() - Duration::from_millis(10),
    );

    // After a tick, the expired cooldown should be cleaned up
    event_loop.handle_tick(std::time::Instant::now(), now_unix());

    assert!(
        !event_loop.peer_cooldowns.contains_key(&peer_id),
        "expired cooldown should be removed"
    );

    Ok(())
}

#[tokio::test]
async fn configured_peer_exempt_from_failure_report() -> Result<()> {
    let peer_id = PeerId([42; 32]);
    let config = P2pConfig {
        seed_peers: vec![SeedPeer {
            peer_id: Some(peer_id),
            address: "/dns/localhost/udp/8080".parse()?,
        }],
        ..Default::default()
    };
    let (builder, _server, _em) = Builder::new().config(config).build();
    let (network, keypair) = build_network_and_key(|router| router);
    let (mut event_loop, _handle) = builder.build(network, keypair);

    event_loop.handle_peer_failure_report(peer_id);

    assert!(
        !event_loop.peer_cooldowns.contains_key(&peer_id),
        "configured peer should not be placed on cooldown"
    );
    Ok(())
}
