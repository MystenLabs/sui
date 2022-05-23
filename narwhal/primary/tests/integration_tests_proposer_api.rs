// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use config::Parameters;
use consensus::dag::Dag;
use crypto::{
    ed25519::Ed25519PublicKey,
    traits::{KeyPair, ToFromBytes},
    Hash,
};
use node::NodeStorage;
use primary::{Primary, CHANNEL_CAPACITY};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use test_utils::{committee, keys, make_optimal_certificates, temp_dir};
use tokio::sync::mpsc::channel;
use tonic::transport::Channel;
use types::{Certificate, ProposerClient, PublicKeyProto, RoundsRequest};

#[tokio::test]
async fn test_rounds_errors() {
    // GIVEN keys
    let keypair = keys(None).pop().unwrap();
    let name = keypair.public().clone();

    struct TestCase {
        public_key: Bytes,
        test_case_name: String,
        expected_error: String,
    }

    let test_cases: Vec<TestCase> = vec![
        TestCase {
            public_key: Bytes::from(name.clone().as_bytes().to_vec()),
            test_case_name: "Valid public key but no certificates available".to_string(),
            expected_error:
                "Couldn't retrieve rounds: No remaining certificates in Dag for this authority"
                    .to_string(),
        },
        TestCase {
            public_key: Bytes::from(Ed25519PublicKey::default().as_bytes().to_vec()),
            test_case_name: "Valid public key, but authority not found in committee".to_string(),
            expected_error: "Invalid public key: unknown authority".to_string(),
        },
        TestCase {
            public_key: Bytes::from(vec![0u8]),
            test_case_name: "Invalid public key provided".to_string(),
            expected_error: "Invalid public key: couldn't parse".to_string(),
        },
    ];

    let committee = committee(None);
    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };

    // AND create separate data stores
    let store_primary = NodeStorage::reopen(temp_dir());

    // Spawn the primary
    let (tx_new_certificates, rx_new_certificates) = channel(CHANNEL_CAPACITY);
    let (_tx_feedback, rx_feedback) = channel(CHANNEL_CAPACITY);

    Primary::spawn(
        name.clone(),
        keypair,
        committee.clone(),
        parameters.clone(),
        store_primary.header_store,
        store_primary.certificate_store,
        store_primary.payload_store,
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        /* external_consensus */ Some(Arc::new(Dag::new(rx_new_certificates).1)),
    );

    // AND Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // AND
    let mut client = connect_to_proposer_client(parameters.clone());

    // Run the tests
    for test in test_cases {
        println!("Test: {}", test.test_case_name);

        // WHEN we retrieve the rounds
        let request = tonic::Request::new(RoundsRequest {
            public_key: Some(PublicKeyProto {
                bytes: test.public_key,
            }),
        });
        let response = client.rounds(request).await;

        // THEN
        let err = response.err().unwrap();

        assert!(
            err.message().contains(test.expected_error.as_str()),
            "{}",
            format!("Expected error not found in response: {}", err.message())
        );
    }
}

#[tokio::test]
async fn test_rounds_return_successful_response() {
    // GIVEN keys
    let keypair = keys(None).pop().unwrap();
    let name = keypair.public().clone();

    let committee = committee(None);
    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };

    // AND create separate data stores
    let store_primary = NodeStorage::reopen(temp_dir());

    // Spawn the primary
    let (tx_new_certificates, rx_new_certificates) = channel(CHANNEL_CAPACITY);
    let (_tx_feedback, rx_feedback) = channel(CHANNEL_CAPACITY);

    // AND setup the DAG
    let dag = Arc::new(Dag::new(rx_new_certificates).1);

    Primary::spawn(
        name.clone(),
        keypair,
        committee.clone(),
        parameters.clone(),
        store_primary.header_store,
        store_primary.certificate_store,
        store_primary.payload_store,
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        /* external_consensus */ Some(dag.clone()),
    );

    // AND Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // AND create some certificates and insert to DAG
    // Make certificates for rounds 1 to 4.
    let keys: Vec<_> = keys(None)
        .into_iter()
        .map(|kp| kp.public().clone())
        .collect();
    let mut genesis_certs = Certificate::genesis(&committee);
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, _next_parents) = make_optimal_certificates(1..=4, &genesis, &keys);

    // Feed the certificates to the Dag
    while let Some(certificate) = genesis_certs.pop() {
        dag.insert(certificate).await.unwrap();
    }
    while let Some(certificate) = certificates.pop_front() {
        dag.insert(certificate).await.unwrap();
    }

    // AND
    let mut client = connect_to_proposer_client(parameters.clone());

    // WHEN we retrieve the rounds
    let request = tonic::Request::new(RoundsRequest {
        public_key: Some(PublicKeyProto::from(name)),
    });
    let response = client.rounds(request).await;

    // THEN
    let r = response.ok().unwrap().into_inner();

    assert_eq!(0, r.oldest_round);
    assert_eq!(4, r.newest_round);
}

fn connect_to_proposer_client(parameters: Parameters) -> ProposerClient<Channel> {
    let config = mysten_network::config::Config::new();
    let channel = config
        .connect_lazy(&parameters.consensus_api_grpc.socket_addr)
        .unwrap();
    ProposerClient::new(channel)
}
