// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::time::Duration;
use test_utils::cluster::Cluster;
use types::Empty;

#[tokio::test]
async fn test_get_primary_address() {
    let mut cluster = Cluster::new(None, false);

    // start the cluster will all the possible nodes
    cluster.start(Some(2), Some(1), None).await;

    // give some time for nodes to bootstrap
    tokio::time::sleep(Duration::from_secs(2)).await;

    let committee = cluster.committee_shared.clone();
    let authority = cluster.authority(0);
    let name = authority.name.clone();

    // Test gRPC server with client call
    let mut client = authority.new_configuration_client().await;

    let request = tonic::Request::new(Empty {});

    let response = client.get_primary_address(request).await.unwrap();
    let actual_result = response.into_inner();

    assert_eq!(
        actual_result.primary_address.unwrap().address,
        committee
            .primary(&name)
            .expect("Our public key or worker id is not in the committee")
            .to_string()
    )
}
