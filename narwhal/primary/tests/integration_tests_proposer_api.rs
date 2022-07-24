// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use arc_swap::ArcSwap;
use bytes::Bytes;
use config::{Epoch, Parameters};
use consensus::{dag::Dag, metrics::ConsensusMetrics};
use crypto::{
    ed25519::Ed25519PublicKey,
    traits::{KeyPair, ToFromBytes},
    Hash,
};
use node::NodeStorage;
use primary::{NetworkModel, Primary, CHANNEL_CAPACITY};
use prometheus::Registry;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use test_utils::{
    committee, committee_from_keys, keys, make_optimal_certificates,
    make_optimal_signed_certificates, temp_dir,
};
use tokio::sync::{mpsc::channel, watch};
use tonic::transport::Channel;
use types::{
    Certificate, CertificateDigest, NodeReadCausalRequest, ProposerClient, PublicKeyProto,
    ReconfigureNotification, RoundsRequest,
};

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
    let (tx_feedback, rx_feedback) = channel(CHANNEL_CAPACITY);
    let initial_committee = ReconfigureNotification::NewCommittee(committee.clone());
    let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

    // AND create a committee passed exclusively to the DAG that does not include the name public key
    // In this way, the genesis certificate is not run for that authority and is absent when we try to fetch it
    let no_name_committee = config::Committee {
        epoch: Epoch::default(),
        authorities: committee
            .authorities
            .iter()
            .filter_map(|(pk, a)| (*pk != name).then_some((pk.clone(), a.clone())))
            .collect::<BTreeMap<_, _>>(),
    };

    let consensus_metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

    Primary::spawn(
        name.clone(),
        keypair,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        parameters.clone(),
        store_primary.header_store,
        store_primary.certificate_store,
        store_primary.payload_store,
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        /* external_consensus */
        Some(Arc::new(
            Dag::new(&no_name_committee, rx_new_certificates, consensus_metrics).1,
        )),
        NetworkModel::Asynchronous,
        tx_reconfigure,
        tx_feedback,
        &Registry::new(),
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
    let (tx_feedback, rx_feedback) = channel(CHANNEL_CAPACITY);
    let initial_committee = ReconfigureNotification::NewCommittee(committee.clone());
    let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

    // AND setup the DAG
    let consensus_metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let dag = Arc::new(Dag::new(&committee, rx_new_certificates, consensus_metrics).1);

    Primary::spawn(
        name.clone(),
        keypair,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        parameters.clone(),
        store_primary.header_store,
        store_primary.certificate_store,
        store_primary.payload_store,
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        /* external_consensus */ Some(dag.clone()),
        NetworkModel::Asynchronous,
        tx_reconfigure,
        tx_feedback,
        &Registry::new(),
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

    assert_eq!(1, r.oldest_round); // genesis compressed
    assert_eq!(4, r.newest_round);
}

#[tokio::test]
async fn test_node_read_causal_signed_certificates() {
    let mut k = keys(None);

    let committee = committee_from_keys(&k);

    // Make the data store.
    let primary_store_1 = NodeStorage::reopen(temp_dir());
    let primary_store_2: NodeStorage<Ed25519PublicKey> = NodeStorage::reopen(temp_dir());

    let mut collection_ids: Vec<CertificateDigest> = Vec::new();

    // Make the Dag
    let consensus_metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let (tx_new_certificates, rx_new_certificates) = channel(CHANNEL_CAPACITY);
    let dag = Arc::new(Dag::new(&committee, rx_new_certificates, consensus_metrics).1);

    // No need to populate genesis in the Dag
    let genesis_certs = Certificate::genesis(&committee);

    // Write genesis certs to primary 1 & 2
    primary_store_1
        .certificate_store
        .write_all(genesis_certs.clone().into_iter().map(|c| (c.digest(), c)))
        .await
        .unwrap();
    primary_store_2
        .certificate_store
        .write_all(genesis_certs.clone().into_iter().map(|c| (c.digest(), c)))
        .await
        .unwrap();

    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let (certificates, _next_parents) = make_optimal_signed_certificates(1..=4, &genesis, &k);

    collection_ids.extend(
        certificates
            .iter()
            .map(|c| c.digest())
            .collect::<Vec<CertificateDigest>>(),
    );

    // Feed the certificates to the Dag
    for certificate in certificates.clone() {
        dag.insert(certificate).await.unwrap();
    }

    // Write the certificates to Primary 1 but intentionally miss one certificate.
    primary_store_1
        .certificate_store
        .write_all(
            certificates
                .clone()
                .into_iter()
                .skip(1)
                .map(|c| (c.digest(), c)),
        )
        .await
        .unwrap();

    // Write all certificates to Primary 2, so Primary 1 has a place to retrieve
    // missing certificate from.
    primary_store_2
        .certificate_store
        .write_all(certificates.clone().into_iter().map(|c| (c.digest(), c)))
        .await
        .unwrap();

    let (tx_feedback, rx_feedback) = channel(CHANNEL_CAPACITY);
    let initial_committee = ReconfigureNotification::NewCommittee(committee.clone());
    let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

    let primary_1_parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let keypair_1 = k.pop().unwrap();
    let name_1 = keypair_1.public().clone();

    // Spawn Primary 1 that we will be interacting with.
    Primary::spawn(
        name_1.clone(),
        keypair_1,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        primary_1_parameters.clone(),
        primary_store_1.header_store.clone(),
        primary_store_1.certificate_store.clone(),
        primary_store_1.payload_store.clone(),
        /* tx_consensus */ tx_new_certificates,
        /* rx_consensus */ rx_feedback,
        /* dag */ Some(dag.clone()),
        NetworkModel::Asynchronous,
        tx_reconfigure,
        tx_feedback,
        &Registry::new(),
    );

    let (tx_new_certificates_2, rx_new_certificates_2) = channel(CHANNEL_CAPACITY);
    let (tx_feedback_2, rx_feedback_2) = channel(CHANNEL_CAPACITY);

    let initial_committee = ReconfigureNotification::NewCommittee(committee.clone());
    let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

    let primary_2_parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let keypair_2 = k.pop().unwrap();
    let name_2 = keypair_2.public().clone();
    let consensus_metrics_2 = Arc::new(ConsensusMetrics::new(&Registry::new()));

    // Spawn Primary 2
    Primary::spawn(
        name_2.clone(),
        keypair_2,
        Arc::new(ArcSwap::from_pointee(committee.clone())),
        primary_2_parameters.clone(),
        primary_store_2.header_store,
        primary_store_2.certificate_store,
        primary_store_2.payload_store,
        /* tx_consensus */ tx_new_certificates_2,
        /* rx_consensus */ rx_feedback_2,
        /* external_consensus */
        Some(Arc::new(
            Dag::new(&committee, rx_new_certificates_2, consensus_metrics_2).1,
        )),
        NetworkModel::Asynchronous,
        tx_reconfigure,
        tx_feedback_2,
        &Registry::new(),
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Test gRPC server with client call
    let mut client = connect_to_proposer_client(primary_1_parameters.clone());

    // Test node read causal for existing round in Primary 1
    // Genesis aka round 0 so we expect BFT 1 + 0 * 4 vertices
    let request = tonic::Request::new(NodeReadCausalRequest {
        public_key: Some(PublicKeyProto::from(name_1.clone())),
        round: 0,
    });

    let response = client.node_read_causal(request).await.unwrap();
    assert_eq!(1, response.into_inner().collection_ids.len());

    // Test node read causal for existing round in Primary 1
    // Round 1 so we expect BFT 1 + 0 * 4 vertices (genesis round elided)
    let request = tonic::Request::new(NodeReadCausalRequest {
        public_key: Some(PublicKeyProto::from(name_1.clone())),
        round: 1,
    });

    let response = client.node_read_causal(request).await.unwrap();
    assert_eq!(1, response.into_inner().collection_ids.len());

    // Test node read causal for round 4 (we ack all of the prior round),
    // we expect BFT 1 + 3 * 4 vertices (genesis round elided)
    let request = tonic::Request::new(NodeReadCausalRequest {
        public_key: Some(PublicKeyProto::from(name_1.clone())),
        round: 4,
    });

    let response = client.node_read_causal(request).await.unwrap();
    assert_eq!(13, response.into_inner().collection_ids.len());

    // Test node read causal for removed round
    let request = tonic::Request::new(NodeReadCausalRequest {
        public_key: Some(PublicKeyProto::from(name_1.clone())),
        round: 0,
    });

    let status = client.node_read_causal(request).await.unwrap_err();
    assert!(status.message().contains(
        "Couldn't read causal for provided key & round: Dag invariant violation The vertex known by this digest was dropped"
    ));

    // Test node read causal for round 4 (we ack all of the prior round),
    // we expect BFT 1 + 3 * 4 vertices with round 0 removed. (genesis round elided)
    let request = tonic::Request::new(NodeReadCausalRequest {
        public_key: Some(PublicKeyProto::from(name_1.clone())),
        round: 4,
    });

    let response = client.node_read_causal(request).await.unwrap();
    assert_eq!(13, response.into_inner().collection_ids.len());

    // Test node read causal for round 5 which does not exist.
    let request = tonic::Request::new(NodeReadCausalRequest {
        public_key: Some(PublicKeyProto::from(name_1.clone())),
        round: 5,
    });

    let status = client.node_read_causal(request).await.unwrap_err();
    assert!(status.message().contains(
        "Couldn't read causal for provided key & round: No known certificates for this authority"
    ));

    // Test node read causal for key that is not an authority of the mempool.
    let unknown_keypair = keys(1).pop().unwrap();
    let unknown_name = unknown_keypair.public().clone();

    let request = tonic::Request::new(NodeReadCausalRequest {
        public_key: Some(PublicKeyProto::from(unknown_name.clone())),
        round: 4,
    });

    let status = client.node_read_causal(request).await.unwrap_err();
    assert!(status
        .message()
        .contains("Invalid public key: unknown authority"));
}

fn connect_to_proposer_client(parameters: Parameters) -> ProposerClient<Channel> {
    let config = mysten_network::config::Config::new();
    let channel = config
        .connect_lazy(&parameters.consensus_api_grpc.socket_addr)
        .unwrap();
    ProposerClient::new(channel)
}
