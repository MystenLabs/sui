// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::Parameters;
use consensus::dag::Dag;
use crypto::traits::KeyPair;
use node::NodeStorage;
use primary::{NetworkModel, Primary, CHANNEL_CAPACITY};
use std::{sync::Arc, time::Duration};
use test_utils::{committee, keys, temp_dir};
use tokio::sync::mpsc::channel;
use tonic::transport::Channel;
use types::{
    ConfigurationClient, MultiAddrProto, NewEpochRequest, NewNetworkInfoRequest, PublicKeyProto,
    ValidatorData,
};

#[tokio::test]
async fn test_new_epoch() {
    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let keypair = keys(None).pop().unwrap();
    let name = keypair.public().clone();
    let signer = keypair;
    let committee = committee(None);

    // Make the data store.
    let store = NodeStorage::reopen(temp_dir());

    let (tx_new_certificates, rx_new_certificates) = channel(CHANNEL_CAPACITY);
    let (tx_feedback, rx_feedback) = channel(CHANNEL_CAPACITY);

    Primary::spawn(
        name.clone(),
        signer,
        committee.clone(),
        parameters.clone(),
        store.header_store.clone(),
        store.certificate_store.clone(),
        store.payload_store.clone(),
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        /* dag */ Some(Arc::new(Dag::new(&committee, rx_new_certificates).1)),
        NetworkModel::Asynchronous,
        tx_feedback,
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Test gRPC server with client call
    let mut client = connect_to_configuration_client(parameters.clone());

    let public_key = PublicKeyProto::from(name);
    let stake_weight = 1;
    let address = MultiAddrProto {
        address: "/ip4/127.0.0.1".to_string(),
    };

    let request = tonic::Request::new(NewEpochRequest {
        epoch_number: 0,
        validators: vec![ValidatorData {
            public_key: Some(public_key),
            stake_weight,
            address: Some(address),
        }],
    });

    let status = client.new_epoch(request).await.unwrap_err();

    println!("message: {:?}", status.message());

    // Not fully implemented but a 'Not Implemented!' message indicates no parsing errors.
    assert!(status.message().contains("Not Implemented!"));
}

#[tokio::test]
async fn test_new_network_info() {
    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let keypair = keys(None).pop().unwrap();
    let name = keypair.public().clone();
    let signer = keypair;
    let committee = committee(None);

    // Make the data store.
    let store = NodeStorage::reopen(temp_dir());

    let (tx_new_certificates, rx_new_certificates) = channel(CHANNEL_CAPACITY);
    let (tx_feedback, rx_feedback) = channel(CHANNEL_CAPACITY);

    Primary::spawn(
        name.clone(),
        signer,
        committee.clone(),
        parameters.clone(),
        store.header_store.clone(),
        store.certificate_store.clone(),
        store.payload_store.clone(),
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        /* dag */ Some(Arc::new(Dag::new(&committee, rx_new_certificates).1)),
        NetworkModel::Asynchronous,
        /* tx_committed_certificates */ tx_feedback,
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Test gRPC server with client call
    let mut client = connect_to_configuration_client(parameters.clone());

    let public_key = PublicKeyProto::from(name);
    let stake_weight = 1;
    let address = MultiAddrProto {
        address: "/ip4/127.0.0.1".to_string(),
    };

    let request = tonic::Request::new(NewNetworkInfoRequest {
        epoch_number: 0,
        validators: vec![ValidatorData {
            public_key: Some(public_key),
            stake_weight,
            address: Some(address),
        }],
    });

    let status = client.new_network_info(request).await.unwrap_err();

    println!("message: {:?}", status.message());

    // Not fully implemented but a 'Not Implemented!' message indicates no parsing errors.
    assert!(status.message().contains("Not Implemented!"));
}

fn connect_to_configuration_client(parameters: Parameters) -> ConfigurationClient<Channel> {
    let config = mysten_network::config::Config::new();
    let channel = config
        .connect_lazy(&parameters.consensus_api_grpc.socket_addr)
        .unwrap();
    ConfigurationClient::new(channel)
}
