// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::utils::{build_network_and_key, build_network_with_anemo_config};
use anemo::types::PeerAffinity;
use anemo::Result;
use fastcrypto::ed25519::Ed25519PublicKey;
use futures::stream::FuturesUnordered;
use std::collections::HashSet;
use sui_config::p2p::AllowlistedPeer;
use tokio::time::timeout;

#[tokio::test]
async fn get_known_peers() -> Result<()> {
    let config = P2pConfig::default();
    let (UnstartedDiscovery { state, .. }, server) = Builder::new(create_test_channel().1)
        .config(config)
        .build_internal();

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
async fn make_connection_to_seed_peer() -> Result<()> {
    let mut config = P2pConfig::default();
    let (builder, server) = Builder::new(create_test_channel().1)
        .config(config.clone())
        .build();
    let (network_1, key_1) = build_network_and_key(|router| router.add_rpc_service(server));
    let (_event_loop_1, _handle_1) = builder.build(network_1.clone(), key_1);

    config.seed_peers.push(SeedPeer {
        peer_id: None,
        address: format!("/dns/localhost/udp/{}", network_1.local_addr().port()).parse()?,
    });
    let (builder, server) = Builder::new(create_test_channel().1).config(config).build();
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
    let (builder, server) = Builder::new(create_test_channel().1)
        .config(config.clone())
        .build();
    let (network_1, key_1) = build_network_and_key(|router| router.add_rpc_service(server));
    let (_event_loop_1, _handle_1) = builder.build(network_1.clone(), key_1);

    config.seed_peers.push(SeedPeer {
        peer_id: Some(network_1.peer_id()),
        address: format!("/dns/localhost/udp/{}", network_1.local_addr().port()).parse()?,
    });
    let (builder, server) = Builder::new(create_test_channel().1).config(config).build();
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
    let (builder, server) = Builder::new(create_test_channel().1)
        .config(config.clone())
        .build();
    let (network_1, key_1) = build_network_and_key(|router| router.add_rpc_service(server));
    let (event_loop_1, _handle_1) = builder.build(network_1.clone(), key_1);

    config.seed_peers.push(SeedPeer {
        peer_id: Some(network_1.peer_id()),
        address: format!("/dns/localhost/udp/{}", network_1.local_addr().port()).parse()?,
    });
    let (builder, server) = Builder::new(create_test_channel().1)
        .config(config.clone())
        .build();
    let (network_2, key_2) = build_network_and_key(|router| router.add_rpc_service(server));
    let (mut event_loop_2, _handle_2) = builder.build(network_2.clone(), key_2);
    // Set an external_address address for node 2 so that it can share its address
    event_loop_2.config.external_address =
        Some(format!("/dns/localhost/udp/{}", network_2.local_addr().port()).parse()?);

    let (builder, server) = Builder::new(create_test_channel().1).config(config).build();
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
async fn peers_are_added_from_reconfig_channel() -> Result<()> {
    let (tx_1, rx_1) = create_test_channel();
    let config = P2pConfig::default();
    let (builder, server) = Builder::new(rx_1).config(config.clone()).build();
    let (network_1, key_1) = build_network_and_key(|router| router.add_rpc_service(server));
    let (event_loop_1, _handle_1) = builder.build(network_1.clone(), key_1);

    let (builder, server) = Builder::new(create_test_channel().1)
        .config(config.clone())
        .build();
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

    // We send peer 1 a new peer info (peer 2) in the channel.
    let peer_2_network_pubkey =
        Ed25519PublicKey(ed25519_consensus::VerificationKey::try_from(peer_id_2.0).unwrap());
    let peer2_addr: Multiaddr = format!("/dns/localhost/udp/{}", network_2.local_addr().port())
        .parse()
        .unwrap();
    tx_1.send(TrustedPeerChangeEvent {
        new_peers: vec![PeerInfo {
            peer_id: PeerId(peer_2_network_pubkey.0.to_bytes()),
            affinity: PeerAffinity::High,
            address: vec![peer2_addr.to_anemo_address().unwrap()],
        }],
    })
    .unwrap();

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
    // For allowlisted nodes, `+` means the peer is allowlisted with an address, otherwise not.
    // An allowlisted peer with address will be proactively connected in anemo network.
    //
    //
    // The topology:
    //                                      ------------  11 (private, seed: 1, allowed: 7, 8)
    //                                     /
    //                       ------ 1 (public) ------
    //                      /                        \
    //    2 (public, seed: 1, allowed: 7, 8)          3 (private, seed: 1, allowed: 4+, 5+)
    //       |                                       /             \
    //       |                 4 (private, allowed: 3+, 5, 6)     5 (private, allowed: 3, 4+)
    //       |                                        \
    //       |                                      6 (private, allowed: 4+)
    //     7 (private, allowed: 2+, 8+)
    //       |
    //       |
    //     8 (private, allowed: 7+, 9+)  p.s. 8's max connection is 0
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

    // Node 5, private, allowlisted: Node 3 and Node 4
    let (builder_5, network_5, key_5) = {
        let mut private_discovery_config = default_private_discovery_config.clone();
        private_discovery_config.allowlisted_peers = vec![
            // Intitially 5 does not know how to contact 3 or 4.
            local_allowlisted_peer(network_3.peer_id(), None),
            local_allowlisted_peer(network_4.peer_id(), Some(network_4.local_addr().port())),
        ];
        set_up_network(P2pConfig::default().set_discovery_config(private_discovery_config))
    };

    // Node 6, private, allowlisted: Node 4
    let (builder_6, network_6, key_6) = {
        let mut private_discovery_config = default_private_discovery_config.clone();
        private_discovery_config.allowlisted_peers = vec![local_allowlisted_peer(
            network_4.peer_id(),
            Some(network_4.local_addr().port()),
        )];
        set_up_network(P2pConfig::default().set_discovery_config(private_discovery_config))
    };

    // Node 3: Add Node 4 and Node 5 to allowlist
    let mut private_discovery_config = default_private_discovery_config.clone();
    private_discovery_config.allowlisted_peers = vec![
        local_allowlisted_peer(network_4.peer_id(), Some(network_4.local_addr().port())),
        local_allowlisted_peer(network_5.peer_id(), Some(network_5.local_addr().port())),
    ];
    builder_3.config.discovery = Some(private_discovery_config);

    // Node 4: Add Node 3, Node 5, and Node 6 to allowlist
    let mut private_discovery_config = default_private_discovery_config.clone();
    private_discovery_config.allowlisted_peers = vec![
        local_allowlisted_peer(network_3.peer_id(), Some(network_3.local_addr().port())),
        local_allowlisted_peer(network_5.peer_id(), None),
        local_allowlisted_peer(network_6.peer_id(), None),
    ];
    builder_4.config.discovery = Some(private_discovery_config);

    // Node 7, private, allowlisted: Node 2, Node 8
    let (mut builder_7, network_7, key_7) = set_up_network(
        P2pConfig::default().set_discovery_config(default_private_discovery_config.clone()),
    );

    // Node 9, public
    let (builder_9, network_9, key_9) = set_up_network(default_p2p_config.clone());

    // Node 8, private, allowlisted: Node 7, Node 9
    let (builder_8, network_8, key_8) = {
        let mut private_discovery_config = default_private_discovery_config.clone();
        private_discovery_config.allowlisted_peers = vec![
            local_allowlisted_peer(network_7.peer_id(), Some(network_7.local_addr().port())),
            local_allowlisted_peer(network_9.peer_id(), Some(network_9.local_addr().port())),
        ];
        let mut p2p_config = P2pConfig::default();
        let mut anemo_config = anemo::Config::default();
        anemo_config.max_concurrent_connections = Some(0);
        p2p_config.anemo_config = Some(anemo_config);
        set_up_network(p2p_config.set_discovery_config(private_discovery_config))
    };

    // Node 2, Add Node 7 and Node 8 to allowlist
    let mut discovery_config = default_discovery_config.clone();
    discovery_config.allowlisted_peers = vec![
        local_allowlisted_peer(network_7.peer_id(), None),
        local_allowlisted_peer(network_8.peer_id(), None),
    ];
    builder_2.config.discovery = Some(discovery_config);

    // Node 7: Add Node 2, and Node 8 to allowlist
    let mut private_discovery_config = default_private_discovery_config.clone();
    private_discovery_config.allowlisted_peers = vec![
        local_allowlisted_peer(network_2.peer_id(), Some(network_2.local_addr().port())),
        local_allowlisted_peer(network_8.peer_id(), Some(network_8.local_addr().port())),
    ];
    builder_7.config.discovery = Some(private_discovery_config);

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

    // Node 1 is connected to everyone. But it does not "know" private nodes.
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

    // Node 1 is connected to everyone. But it does not "know" private nodes except the allowlisted ones 7 and 8.
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

    assert_peers(
        "Node 3",
        &network_3,
        &state_3,
        HashSet::from_iter(vec![peer_id_1, peer_id_4, peer_id_5]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_4, peer_id_5, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_4, peer_id_5, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_4, peer_id_5, peer_id_9]),
    );

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

    assert_peers(
        "Node 5",
        &network_5,
        &state_5,
        HashSet::from_iter(vec![peer_id_3, peer_id_4]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_3, peer_id_4, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_3, peer_id_4, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_3, peer_id_4, peer_id_9]),
    );

    assert_peers(
        "Node 6",
        &network_6,
        &state_6,
        HashSet::from_iter(vec![peer_id_4]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_4, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_4, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_4, peer_id_9]),
    );

    // Node 11 finds Node 7 via Node 2, and invites Node 7 to connect. Node 7 says yes.
    assert_peers(
        "Node 7",
        &network_7,
        &state_7,
        HashSet::from_iter(vec![peer_id_2, peer_id_8]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_8, peer_id_9, peer_id_11]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_8, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_8, peer_id_9, peer_id_11]),
    );

    // Node 11 finds Node 8 via Node 2, and invites Node 8 to connect. Node 8 said No
    // because its `max_concurrent_connections` is 0.
    assert_peers(
        "Node 8",
        &network_8,
        &state_8,
        HashSet::from_iter(vec![peer_id_7, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_7, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_7, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_7, peer_id_9]),
    );

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

    // Node 10 does not talk to any other private nodes.
    assert_peers(
        "Node 10",
        &network_10,
        &state_10,
        HashSet::from_iter(vec![peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_9]),
    );

    // 11 allowlists 8 but 8 does not 11, so they can't connect
    // although 8 is still in 11's known peer list
    assert_peers(
        "Node 11",
        &network_11,
        &state_11,
        HashSet::from_iter(vec![peer_id_1, peer_id_7, peer_id_8]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_7, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_7, peer_id_8, peer_id_9]),
        HashSet::from_iter(vec![peer_id_1, peer_id_2, peer_id_7, peer_id_9]),
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
    let (builder, server) = Builder::new(create_test_channel().1)
        .config(p2p_config)
        .build();
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

fn create_test_channel() -> (
    watch::Sender<TrustedPeerChangeEvent>,
    watch::Receiver<TrustedPeerChangeEvent>,
) {
    let (tx, rx) = watch::channel(TrustedPeerChangeEvent { new_peers: vec![] });
    (tx, rx)
}
