// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use sui_adapter::genesis;
use sui_types::{base_types::SequenceNumber, crypto::get_key_pair, object::Object};

use super::*;
use crate::{
    authority_aggregator::authority_aggregator_tests::*,
    authority_client::{AuthorityClient, NetworkAuthorityClient},
};

#[tokio::test(flavor = "current_thread", start_paused = true)]
pub async fn test_gossip_plain() {
    let (addr1, key1) = get_key_pair();
    let gas_object1 = Object::with_owner_for_testing(addr1);
    let gas_object2 = Object::with_owner_for_testing(addr1);
    let genesis_objects =
        authority_genesis_objects(4, vec![gas_object1.clone(), gas_object2.clone()]);

    let (aggregator, states) = init_local_authorities(genesis_objects).await;
    let clients = aggregator.authority_clients.clone();

    let authority_clients: Vec<_> = aggregator.authority_clients.values().collect();
    let framework_obj_ref = genesis::get_framework_object_ref();

    // Start batch processes, and active processes.
    for state in states {
        let inner_state = state.clone();
        let _batch_handle = tokio::task::spawn(async move {
            inner_state
                .run_batch_service(5, Duration::from_millis(50))
                .await
        });
        let inner_state = state.clone();
        let inner_clients: BTreeMap<_, AuthorityClient> = clients
            .iter()
            .map(|(k, c)| {
                let client: AuthorityClient = Arc::new(c.clone());
                (*k, client)
            })
            .collect();
        //let inner_clients: BTreeMap<_, AuthorityClient> = clients.clone();

        let _active_handle = tokio::task::spawn(async move {
            let active_state = ActiveAuthority::new(inner_state, inner_clients).unwrap();
            active_state.spawn_all_active_processes().await
        });
    }

    println!("All authorities started");
    // Let the helper tasks start
    tokio::time::sleep(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;

    println!("Do transactions");
    // Make a schedule of transactions
    let gas_ref_1 = get_latest_ref(authority_clients[0], gas_object1.id()).await;
    let create1 =
        crate_object_move_transaction(addr1, &key1, addr1, 100, framework_obj_ref, gas_ref_1);

    do_transaction(authority_clients[0], &create1).await;
    do_transaction(authority_clients[1], &create1).await;
    do_transaction(authority_clients[2], &create1).await;

    // Get a cert
    let cert1 = extract_cert(&authority_clients, &aggregator.committee, create1.digest()).await;

    println!("Submit cert");
    // Submit the cert to 1 authority.
    let _new_ref_1 = do_cert(authority_clients[0], &cert1).await.created[0].0;

    println!("Wait for cert");
    tokio::time::sleep(Duration::from_secs(10)).await;
    let gas_ref_1 = get_latest_ref(authority_clients[3], gas_object1.id()).await;
    println!("Ref: {:?}", gas_ref_1);

    println!("Check object");
    assert_eq!(gas_ref_1.1, SequenceNumber::from(1));
}

#[tokio::test]
pub async fn test_gossip_no_network() {
    info!("Start running test");

    let (addr1, _) = get_key_pair();
    let gas_object1 = Object::with_owner_for_testing(addr1);
    let gas_object2 = Object::with_owner_for_testing(addr1);
    let genesis_objects =
        authority_genesis_objects(4, vec![gas_object1.clone(), gas_object2.clone()]);

    let (_aggregator, states) = init_local_authorities(genesis_objects).await;

    // Connect to non-existing peer
    let aggregator = AuthorityAggregator::new(
        _aggregator.committee.clone(),
        _aggregator
            .authority_clients
            .iter()
            .map(|(name, _)| {
                let net: AuthorityClient = Arc::new(
                    NetworkAuthorityClient::connect_lazy(
                        &"/ip4/127.0.0.1/tcp/332/http".parse().unwrap(),
                    )
                    .unwrap(),
                );
                (*name, net)
            })
            .collect(),
    );

    let clients = aggregator.authority_clients.clone();

    // Start batch processes, and active processes.
    if let Some(state) = states.into_iter().next() {
        let inner_state = state;
        let inner_clients: BTreeMap<_, AuthorityClient> = clients
            .iter()
            .map(|(k, c)| {
                let client: AuthorityClient = Arc::new(c.clone());
                (*k, client)
            })
            .collect();

        let _active_handle = tokio::task::spawn(async move {
            let active_state = ActiveAuthority::new(inner_state, inner_clients).unwrap();
            active_state.spawn_all_active_processes().await
        });
    }

    // Let the helper tasks start
    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_secs(10)).await;
}
