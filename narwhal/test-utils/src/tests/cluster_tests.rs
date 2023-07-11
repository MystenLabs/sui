// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::cluster::Cluster;
use crate::ensure_test_environment;
use std::time::Duration;

#[tokio::test]
async fn basic_cluster_setup() {
    ensure_test_environment();
    let mut cluster = Cluster::new(None);

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
