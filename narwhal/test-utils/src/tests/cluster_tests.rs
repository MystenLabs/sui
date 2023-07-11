// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::cluster::Cluster;
use crate::ensure_test_environment;
use std::time::Duration;
use types::{PublicKeyProto, RoundsRequest};

#[tokio::test]
async fn basic_cluster_setup() {
    ensure_test_environment();
    let mut cluster = Cluster::new(None, true);

    // start the cluster will all the possible nodes
    cluster.start(Some(4), Some(1), None).await;

    // give some time for nodes to bootstrap
    tokio::time::sleep(Duration::from_secs(2)).await;

    // fetch all the running authorities
    let authorities = cluster.authorities().await;

    assert_eq!(authorities.len(), 4);

    // fetch their workers transactions address
    for authority in cluster.authorities().await {
        assert_eq!(authority.worker_transaction_addresses().await.len(), 1);
    }

    // now stop all authorities
    for id in 0..4 {
        cluster.stop_node(id).await;
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    // No authority should still run
    assert!(cluster.authorities().await.is_empty());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn cluster_setup_with_consensus_disabled() {
    ensure_test_environment();
    let mut cluster = Cluster::new(None, false);

    // start the cluster will all the possible nodes
    cluster.start(Some(2), Some(1), None).await;

    // give some time for nodes to bootstrap
    tokio::time::sleep(Duration::from_secs(2)).await;

    // connect to the gRPC address and send a simple request
    let authority = cluster.authority(0);

    let mut client = authority.new_proposer_client().await;

    // send a sample rounds request
    let request = tonic::Request::new(RoundsRequest {
        public_key: Some(PublicKeyProto::from(authority.public_key.clone())),
    });
    let response = client.rounds(request).await;

    // Should get back a successful response
    let r = response.ok().unwrap().into_inner();

    assert_eq!(0, r.oldest_round);
    assert_eq!(0, r.newest_round);
}
