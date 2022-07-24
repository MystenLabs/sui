use arc_swap::ArcSwap;
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::Parameters;
use consensus::{dag::Dag, metrics::ConsensusMetrics};
use crypto::traits::KeyPair;
use node::NodeStorage;
use primary::{NetworkModel, Primary, CHANNEL_CAPACITY};
use prometheus::Registry;
use std::{sync::Arc, time::Duration};
use test_utils::{committee, keys, temp_dir};
use tokio::sync::{mpsc::channel, watch};
use tonic::transport::Channel;
use types::{
    ConfigurationClient, Empty, MultiAddrProto, NewEpochRequest, NewNetworkInfoRequest,
    PrimaryAddressesProto, PublicKeyProto, ReconfigureNotification, ValidatorData,
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
    let initial_committee = ReconfigureNotification::NewCommittee(committee.clone());
    let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);
    let consensus_metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

    Primary::spawn(
        name.clone(),
        signer,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        parameters.clone(),
        store.header_store.clone(),
        store.certificate_store.clone(),
        store.payload_store.clone(),
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        /* dag */
        Some(Arc::new(
            Dag::new(&committee, rx_new_certificates, consensus_metrics).1,
        )),
        NetworkModel::Asynchronous,
        tx_reconfigure,
        tx_feedback,
        &Registry::new(),
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Test gRPC server with client call
    let mut client = connect_to_configuration_client(parameters.clone());

    let public_key = PublicKeyProto::from(name);
    let stake_weight = 1;
    let primary_to_primary = Some(MultiAddrProto {
        address: "/ip4/127.0.0.1".to_string(),
    });
    let worker_to_primary = Some(MultiAddrProto {
        address: "/ip4/127.0.0.1".to_string(),
    });

    let request = tonic::Request::new(NewEpochRequest {
        epoch_number: 0,
        validators: vec![ValidatorData {
            public_key: Some(public_key),
            stake_weight,
            primary_addresses: Some(PrimaryAddressesProto {
                primary_to_primary,
                worker_to_primary,
            }),
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
    let initial_committee = ReconfigureNotification::NewCommittee(committee.clone());
    let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);
    let consensus_metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

    Primary::spawn(
        name.clone(),
        signer,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        parameters.clone(),
        store.header_store.clone(),
        store.certificate_store.clone(),
        store.payload_store.clone(),
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        /* dag */
        Some(Arc::new(
            Dag::new(&committee, rx_new_certificates, consensus_metrics).1,
        )),
        NetworkModel::Asynchronous,
        tx_reconfigure,
        /* tx_committed_certificates */ tx_feedback,
        &Registry::new(),
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Test gRPC server with client call
    let mut client = connect_to_configuration_client(parameters.clone());

    let public_keys: Vec<_> = committee.authorities.keys().cloned().collect();

    let mut validators = Vec::new();
    for public_key in public_keys.iter() {
        let public_key_proto = PublicKeyProto::from(public_key.clone());
        let stake_weight = 1;
        let primary_to_primary = Some(MultiAddrProto {
            address: "/ip4/127.0.0.1".to_string(),
        });
        let worker_to_primary = Some(MultiAddrProto {
            address: "/ip4/127.0.0.1".to_string(),
        });

        validators.push(ValidatorData {
            public_key: Some(public_key_proto),
            stake_weight,
            primary_addresses: Some(PrimaryAddressesProto {
                primary_to_primary,
                worker_to_primary,
            }),
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

fn connect_to_configuration_client(parameters: Parameters) -> ConfigurationClient<Channel> {
    let config = mysten_network::config::Config::new();
    let channel = config
        .connect_lazy(&parameters.consensus_api_grpc.socket_addr)
        .unwrap();
    ConfigurationClient::new(channel)
}
