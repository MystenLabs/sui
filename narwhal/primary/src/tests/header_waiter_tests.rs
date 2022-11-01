// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    common::{create_db_stores, worker_listener},
    header_waiter::{HeaderWaiter, WaiterMessage},
    metrics::PrimaryMetrics,
};

use fastcrypto::{hash::Hash, traits::KeyPair};
use network::P2pNetwork;
use prometheus::Registry;
use std::{sync::Arc, time::Duration};
use test_utils::{fixture_payload, CommitteeFixture};
use tokio::{sync::watch, time::timeout};
use types::{BatchDigest, ReconfigureNotification, Round};

#[tokio::test]
async fn successfully_synchronize_batches() {
    // GIVEN
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let name = primary.public_key();
    let (_, certificate_store, payload_store) = create_db_stores();
    let gc_depth: Round = 1;
    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_synchronizer, rx_synchronizer) = test_utils::test_channel!(10);
    let (tx_core, mut rx_core) = test_utils::test_channel!(10);
    let (tx_primary_messages, _rx_primary_messages) = test_utils::test_channel!(1000);
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();

    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(primary.network_keypair().copy().private().0.to_bytes())
        .start(anemo::Router::new())
        .unwrap();

    let _header_waiter_handle = HeaderWaiter::spawn(
        name.clone(),
        committee.clone(),
        worker_cache.clone(),
        certificate_store,
        payload_store.clone(),
        rx_consensus_round_updates,
        gc_depth,
        rx_reconfigure,
        rx_synchronizer,
        tx_core,
        tx_primary_messages,
        metrics,
        P2pNetwork::new(network.clone()),
    );

    // AND spin up a worker node that primary owns
    let worker_id = 0;
    let worker = primary.worker(worker_id);
    let worker_keypair = worker.keypair();
    let worker_name = worker_keypair.public().to_owned();
    let worker_address = &worker.info().worker_address;

    let handle = worker_listener(1, worker_address.clone(), worker_keypair);
    let address = network::multiaddr_to_address(worker_address).unwrap();
    let peer_id = anemo::PeerId(worker_name.0.to_bytes());
    network
        .connect_with_peer_id(address, peer_id)
        .await
        .unwrap();

    // AND a header
    let header = author
        .header_builder(&committee)
        .payload(fixture_payload(2))
        .build(author.keypair())
        .unwrap();
    let missing_digests = vec![BatchDigest::default()];
    let missing_digests_map = missing_digests
        .clone()
        .into_iter()
        .map(|d| (d, worker_id))
        .collect();

    // AND send a message to synchronizer batches
    tx_synchronizer
        .send(WaiterMessage::SyncBatches(
            missing_digests_map,
            header.clone(),
        ))
        .await
        .unwrap();

    // THEN
    if let Ok(Ok((_primary_msg, mut sync_msg))) =
        timeout(Duration::from_millis(4_000), handle).await
    {
        let msg = sync_msg.remove(0);
        assert_eq!(
            missing_digests, msg.digests,
            "Expected missing digests don't match"
        );

        // now simulate the write of the batch to the payload store
        payload_store
            .sync_write_all(missing_digests.into_iter().map(|e| ((e, worker_id), 1)))
            .await
            .unwrap();

        // now get the output as expected
        let header_result = rx_core.recv().await.unwrap();
        assert_eq!(header.digest(), header_result.digest());
    } else {
        panic!("Messages not received by worker");
    }
}
