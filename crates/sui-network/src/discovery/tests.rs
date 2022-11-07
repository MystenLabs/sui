// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::utils::build_network;
use anemo::Result;
use std::collections::HashSet;

#[tokio::test]
async fn get_external_address() -> Result<()> {
    let config = P2pConfig::default();
    let (_, server) = Builder::new().config(config).build_internal();

    let address = "127.0.0.1:1337".parse::<std::net::SocketAddr>()?;
    let request = Request::new(()).with_extension(address);
    let response = server
        .get_external_address(request)
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response, address);

    Ok(())
}

#[tokio::test]
async fn get_external_address_with_real_network() -> Result<()> {
    let config = P2pConfig::default();
    let (_builder, server) = Builder::new().config(config).build();
    let network_1 = build_network(|router| router.add_rpc_service(server));

    let config = P2pConfig::default();
    let (_builder, server) = Builder::new().config(config).build();
    let network_2 = build_network(|router| router.add_rpc_service(server));

    let peer_id_2 = network_1.connect(network_2.local_addr()).await?;
    let mut client_2 = DiscoveryClient::new(network_1.peer(peer_id_2).unwrap());
    let response = client_2
        .get_external_address(())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response, network_1.local_addr());

    let mut client_1 = DiscoveryClient::new(network_2.peer(network_1.peer_id()).unwrap());
    let response = client_1
        .get_external_address(())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response, network_2.local_addr());

    Ok(())
}

#[tokio::test]
async fn get_known_peers() -> Result<()> {
    let config = P2pConfig::default();
    let (UnstartedDiscovery { state, .. }, server) = Builder::new().config(config).build_internal();

    // Err when own_info not set
    server.get_known_peers(Request::new(())).await.unwrap_err();

    // Normal response with our_info
    let our_info = NodeInfo {
        peer_id: PeerId([9; 32]),
        addresses: Vec::new(),
        external_socket_address: None,
        timestamp_ms: now_unix(),
    };
    state.write().unwrap().our_info = Some(our_info.clone());
    let response = server
        .get_known_peers(Request::new(()))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.own_info, our_info);
    assert!(response.known_peers.is_empty());

    // Normal resonse with some known peers
    let other_peer = NodeInfo {
        peer_id: PeerId([13; 32]),
        addresses: Vec::new(),
        external_socket_address: None,
        timestamp_ms: now_unix(),
    };
    state
        .write()
        .unwrap()
        .known_peers
        .insert(other_peer.peer_id, other_peer.clone());
    let response = server
        .get_known_peers(Request::new(()))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.own_info, our_info);
    assert_eq!(response.known_peers, vec![other_peer]);

    Ok(())
}

#[tokio::test]
async fn make_connection_to_seed_peer() -> Result<()> {
    let config = P2pConfig::default();
    let (builder, server) = Builder::new().config(config).build();
    let network_1 = build_network(|router| router.add_rpc_service(server));
    let (_event_loop_1, _handle_1) = builder.build(network_1.clone());

    let mut config = P2pConfig::default();
    config.seed_peers.push(SeedPeer {
        peer_id: None,
        address: format!("/dns/localhost/udp/{}", network_1.local_addr().port()).parse()?,
    });
    let (builder, server) = Builder::new().config(config).build();
    let network_2 = build_network(|router| router.add_rpc_service(server));
    let (mut event_loop_2, _handle_2) = builder.build(network_2.clone());

    let (mut subscriber_1, _) = network_1.subscribe();
    let (mut subscriber_2, _) = network_2.subscribe();

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
    let config = P2pConfig::default();
    let (builder, server) = Builder::new().config(config).build();
    let network_1 = build_network(|router| router.add_rpc_service(server));
    let (_event_loop_1, _handle_1) = builder.build(network_1.clone());

    let mut config = P2pConfig::default();
    config.seed_peers.push(SeedPeer {
        peer_id: Some(network_1.peer_id()),
        address: format!("/dns/localhost/udp/{}", network_1.local_addr().port()).parse()?,
    });
    let (builder, server) = Builder::new().config(config).build();
    let network_2 = build_network(|router| router.add_rpc_service(server));
    let (mut event_loop_2, _handle_2) = builder.build(network_2.clone());

    let (mut subscriber_1, _) = network_1.subscribe();
    let (mut subscriber_2, _) = network_2.subscribe();

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
    let config = P2pConfig::default();
    let (builder, server) = Builder::new().config(config).build();
    let network_1 = build_network(|router| router.add_rpc_service(server));
    let (event_loop_1, _handle_1) = builder.build(network_1.clone());

    let mut config = P2pConfig::default();
    config.seed_peers.push(SeedPeer {
        peer_id: Some(network_1.peer_id()),
        address: format!("/dns/localhost/udp/{}", network_1.local_addr().port()).parse()?,
    });
    let (builder, server) = Builder::new().config(config.clone()).build();
    let network_2 = build_network(|router| router.add_rpc_service(server));
    let (mut event_loop_2, _handle_2) = builder.build(network_2.clone());
    // Set an external_address address for node 2 so that it can share its address
    event_loop_2.config.external_address =
        Some(format!("/dns/localhost/udp/{}", network_2.local_addr().port()).parse()?);

    let (builder, server) = Builder::new().config(config).build();
    let network_3 = build_network(|router| router.add_rpc_service(server));
    let (event_loop_3, _handle_3) = builder.build(network_3.clone());

    let (mut subscriber_1, _) = network_1.subscribe();
    let (mut subscriber_2, _) = network_2.subscribe();
    let (mut subscriber_3, _) = network_3.subscribe();

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

fn unwrap_new_peer_event(event: PeerEvent) -> PeerId {
    match event {
        PeerEvent::NewPeer(peer_id) => peer_id,
        e => panic!("unexpected event: {e:?}"),
    }
}
