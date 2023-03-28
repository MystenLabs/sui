// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::time::Duration;
use test_utils::cluster::Cluster;
use types::{
    Empty, MultiAddrProto, NewEpochRequest, NewNetworkInfoRequest, PublicKeyProto, ValidatorData,
};

#[tokio::test]
async fn test_new_epoch() {
    let mut cluster = Cluster::new(None, false);

    // start the cluster will all the possible nodes
    cluster.start(Some(2), Some(1), None).await;

    // give some time for nodes to bootstrap
    tokio::time::sleep(Duration::from_secs(2)).await;
    let authority = cluster.authority(0);
    let public_key = authority.public_key.clone();

    // Test gRPC server with client call
    let mut client = authority.new_configuration_client().await;

    let public_key = PublicKeyProto::from(public_key);
    let stake_weight = 1;
    let primary_address = Some(MultiAddrProto {
        address: "/ip4/127.0.0.1".to_string(),
    });

    let request = tonic::Request::new(NewEpochRequest {
        epoch_number: 0,
        validators: vec![ValidatorData {
            public_key: Some(public_key),
            stake_weight,
            primary_address,
        }],
    });

    let status = client.new_epoch(request).await.unwrap_err();

    println!("message: {:?}", status.message());

    // Not fully implemented but a 'Not Implemented!' message indicates no parsing errors.
    assert!(status.message().contains("Not Implemented!"));
}

#[tokio::test]
async fn test_new_network_info() {
    let mut cluster = Cluster::new(None, false);

    // start the cluster will all the possible nodes
    cluster.start(Some(2), Some(1), None).await;

    // give some time for nodes to bootstrap
    tokio::time::sleep(Duration::from_secs(2)).await;

    let committee = cluster.committee.clone();
    let authority = cluster.authority(0);

    // Test gRPC server with client call
    let mut client = authority.new_configuration_client().await;

    let public_keys: Vec<_> = committee.keys();

    let mut validators = Vec::new();
    for public_key in public_keys.iter() {
        let public_key_proto = PublicKeyProto::from(public_key.clone());
        let stake_weight = 1;
        let primary_address = Some(MultiAddrProto {
            address: "/ip4/127.0.0.1".to_string(),
        });

        validators.push(ValidatorData {
            public_key: Some(public_key_proto),
            stake_weight,
            primary_address,
        });
    }

    let request = tonic::Request::new(NewNetworkInfoRequest {
        epoch_number: 1,
        validators: validators.clone(),
    });

    let status = client.new_network_info(request).await.unwrap_err();

    assert!(status
        .message()
        .contains("Passed in epoch 1 does not match current epoch 0"));

    let request = tonic::Request::new(NewNetworkInfoRequest {
        epoch_number: 0,
        validators,
    });

    let response = client.new_network_info(request).await.unwrap();
    let actual_result = response.into_inner();
    assert_eq!(Empty {}, actual_result);
}

#[tokio::test]
async fn test_get_primary_address() {
    let mut cluster = Cluster::new(None, false);

    // start the cluster will all the possible nodes
    cluster.start(Some(2), Some(1), None).await;

    // give some time for nodes to bootstrap
    tokio::time::sleep(Duration::from_secs(2)).await;

    let committee = cluster.committee.clone();
    let authority = cluster.authority(0);
    let name = authority.name;

    // Test gRPC server with client call
    let mut client = authority.new_configuration_client().await;

    let request = tonic::Request::new(Empty {});

    let response = client.get_primary_address(request).await.unwrap();
    let actual_result = response.into_inner();

    assert_eq!(
        actual_result.primary_address.unwrap().address,
        committee
            .primary_by_id(&name)
            .expect("Our public key or worker id is not in the committee")
            .to_string()
    )
}
