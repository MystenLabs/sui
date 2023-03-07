// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use config::{Epoch, Parameters};
use consensus::dag::Dag;
use crypto::PublicKey;
use fastcrypto::{
    hash::Hash,
    traits::{KeyPair as _, ToFromBytes},
};
use narwhal_primary as primary;
use narwhal_primary::NUM_SHUTDOWN_RECEIVERS;
use network::client::NetworkClient;
use primary::{NetworkModel, Primary, CHANNEL_CAPACITY};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use storage::NodeStorage;
use test_utils::{
    make_optimal_certificates, make_optimal_signed_certificates, temp_dir, CommitteeFixture,
};
use tokio::sync::watch;
use tonic::transport::Channel;
use types::{
    Certificate, CertificateDigest, NodeReadCausalRequest, PreSubscribedBroadcastSender,
    ProposerClient, PublicKeyProto, RoundsRequest,
};

#[tokio::test]
async fn test_rounds_errors() {
    // GIVEN keys
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();

    let author = fixture.authorities().last().unwrap();
    let keypair = author.keypair().copy();
    let network_keypair = author.network_keypair().copy();
    let authority_id = author.id();
    let name = keypair.public().clone();
    let client = NetworkClient::new_from_keypair(&author.network_keypair());

    let other_keypair = KeyPair::generate(&mut thread_rng());

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
            public_key: Bytes::from(other_keypair.public().as_bytes().to_vec()),
            test_case_name: "Valid public key, but authority not found in committee".to_string(),
            expected_error: "Invalid public key: unknown authority".to_string(),
        },
        TestCase {
            public_key: Bytes::from(vec![0u8]),
            test_case_name: "Invalid public key provided".to_string(),
            expected_error: "Invalid public key: couldn't parse".to_string(),
        },
    ];

    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };

    // AND create separate data stores
    let store_primary = NodeStorage::reopen(temp_dir(), None);

    // Spawn the primary
    let (tx_new_certificates, rx_new_certificates) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_feedback, rx_feedback) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // AND create a committee passed exclusively to the DAG that does not include the name public key
    // In this way, the genesis certificate is not run for that authority and is absent when we try to fetch it
    let mut builder = CommitteeBuilder::new(Epoch::default());

    for authority in fixture.authorities().map(|a| a.authority()) {
        if authority.id() != authority_id {
            builder = builder.add_authority(
                authority.protocol_key().clone(),
                authority.stake(),
                authority.primary_address(),
                authority.network_key(),
            );
        }
    }

    let no_name_committee = builder.build();

    Primary::spawn(
        author.authority().clone(),
        keypair.copy(),
        network_keypair,
        committee.clone(),
        worker_cache,
        parameters.clone(),
        client,
        store_primary.header_store,
        store_primary.certificate_store,
        store_primary.proposer_store,
        store_primary.payload_store,
        store_primary.vote_digest_store,
        tx_new_certificates,
        rx_feedback,
        rx_consensus_round_updates,
        /* external_consensus */
        Some(Arc::new(
            Dag::new(
                &no_name_committee,
                rx_new_certificates,
                tx_shutdown.subscribe(),
            )
            .1,
        )),
        NetworkModel::Asynchronous,
        &mut tx_shutdown,
        tx_feedback,
        None,
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
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();

    let author = fixture.authorities().last().unwrap();
    let keypair = author.keypair().copy();
    let public_key = author.keypair().public().clone();
    let client = NetworkClient::new_from_keypair(&author.network_keypair());

    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };

    // AND create separate data stores
    let store_primary = NodeStorage::reopen(temp_dir(), None);

    // Spawn the primary
    let (tx_new_certificates, rx_new_certificates) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_feedback, rx_feedback) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // AND setup the DAG
    let dag = Arc::new(Dag::new(&committee, rx_new_certificates, tx_shutdown.subscribe()).1);

    Primary::spawn(
        author.authority().clone(),
        keypair.copy(),
        author.network_keypair().copy(),
        committee.clone(),
        worker_cache,
        parameters.clone(),
        client,
        store_primary.header_store,
        store_primary.certificate_store,
        store_primary.proposer_store,
        store_primary.payload_store,
        store_primary.vote_digest_store,
        tx_new_certificates,
        rx_feedback,
        rx_consensus_round_updates,
        /* external_consensus */ Some(dag.clone()),
        NetworkModel::Asynchronous,
        &mut tx_shutdown,
        tx_feedback,
        None,
    );

    // AND Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // AND create some certificates and insert to DAG
    // Make certificates for rounds 1 to 4.
    let mut genesis_certs = Certificate::genesis(&committee);
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, _next_parents) = make_optimal_certificates(
        &committee,
        1..=4,
        &genesis,
        &committee
            .authorities()
            .map(|authority| authority.id())
            .collect::<Vec<AuthorityIdentifier>>(),
    );

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
        public_key: Some(PublicKeyProto::from(public_key)),
    });
    let response = client.rounds(request).await;

    // THEN
    let r = response.ok().unwrap().into_inner();

    assert_eq!(1, r.oldest_round); // genesis compressed
    assert_eq!(4, r.newest_round);
}

#[tokio::test]
async fn test_node_read_causal_signed_certificates() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();

    let authority_1 = fixture.authorities().next().unwrap();
    let authority_2 = fixture.authorities().nth(1).unwrap();

    // Make the data store.
    let primary_store_1 = NodeStorage::reopen(temp_dir(), None);
    let primary_store_2: NodeStorage = NodeStorage::reopen(temp_dir(), None);

    let client_1 = NetworkClient::new_from_keypair(&authority_1.network_keypair());
    let client_2 = NetworkClient::new_from_keypair(&authority_2.network_keypair());

    let mut collection_ids: Vec<CertificateDigest> = Vec::new();

    // Make the Dag
    let (tx_new_certificates, rx_new_certificates) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let dag = Arc::new(Dag::new(&committee, rx_new_certificates, tx_shutdown.subscribe()).1);

    // No need to populate genesis in the Dag
    let genesis_certs = Certificate::genesis(&committee);

    // Write genesis certs to primary 1 & 2
    primary_store_1
        .certificate_store
        .write_all(genesis_certs.clone())
        .unwrap();
    primary_store_2
        .certificate_store
        .write_all(genesis_certs.clone())
        .unwrap();

    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let keys = fixture
        .authorities()
        .map(|a| (a.id(), a.keypair().copy()))
        .collect::<Vec<_>>();
    let (certificates, _next_parents) =
        make_optimal_signed_certificates(1..=4, &genesis, &committee, &keys);

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
        .write_all(certificates.clone().into_iter().skip(1))
        .unwrap();

    // Write all certificates to Primary 2, so Primary 1 has a place to retrieve
    // missing certificate from.
    primary_store_2
        .certificate_store
        .write_all(certificates.clone())
        .unwrap();

    let (tx_feedback, rx_feedback) =
        test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    let primary_1_parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let keypair_1 = authority_1.keypair().copy();
    let public_key_1 = authority_1.public_key();

    // Spawn Primary 1 that we will be interacting with.
    Primary::spawn(
        authority_1.authority().clone(),
        keypair_1.copy(),
        authority_1.network_keypair().copy(),
        committee.clone(),
        worker_cache.clone(),
        primary_1_parameters.clone(),
        client_1,
        primary_store_1.header_store.clone(),
        primary_store_1.certificate_store.clone(),
        primary_store_1.proposer_store.clone(),
        primary_store_1.payload_store.clone(),
        primary_store_1.vote_digest_store.clone(),
        tx_new_certificates,
        rx_feedback,
        rx_consensus_round_updates,
        /* dag */ Some(dag.clone()),
        NetworkModel::Asynchronous,
        &mut tx_shutdown,
        tx_feedback,
        None,
    );

    let (tx_new_certificates_2, rx_new_certificates_2) =
        test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
    let (tx_feedback_2, rx_feedback_2) = test_utils::test_channel!(CHANNEL_CAPACITY);
    let (_tx_consensus_round_updates_2, rx_consensus_round_updates_2) =
        watch::channel(ConsensusRound::default());

    let mut tx_shutdown_2 = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let primary_2_parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };
    let keypair_2 = authority_2.keypair().copy();
    let name_2 = keypair_2.public().clone();

    // Spawn Primary 2
    Primary::spawn(
        authority_2.authority().clone(),
        keypair_2.copy(),
        authority_2.network_keypair().copy(),
        committee.clone(),
        worker_cache.clone(),
        primary_2_parameters.clone(),
        client_2,
        primary_store_2.header_store,
        primary_store_2.certificate_store,
        primary_store_2.proposer_store,
        primary_store_2.payload_store,
        primary_store_2.vote_digest_store,
        tx_new_certificates_2,
        rx_feedback_2,
        rx_consensus_round_updates_2,
        /* external_consensus */
        Some(Arc::new(
            Dag::new(&committee, rx_new_certificates_2, tx_shutdown.subscribe()).1,
        )),
        NetworkModel::Asynchronous,
        &mut tx_shutdown_2,
        tx_feedback_2,
        None,
    );

    // Wait for tasks to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Test gRPC server with client call
    let mut client = connect_to_proposer_client(primary_1_parameters.clone());

    // Test node read causal for existing round in Primary 1
    // Genesis aka round 0 so we expect BFT 1 + 0 * 4 vertices
    let request = tonic::Request::new(NodeReadCausalRequest {
        public_key: Some(PublicKeyProto::from(public_key_1.clone())),
        round: 0,
    });

    let response = client.node_read_causal(request).await.unwrap();
    assert_eq!(1, response.into_inner().collection_ids.len());

    // Test node read causal for existing round in Primary 1
    // Round 1 so we expect BFT 1 + 0 * 4 vertices (genesis round elided)
    let request = tonic::Request::new(NodeReadCausalRequest {
        public_key: Some(PublicKeyProto::from(public_key_1.clone())),
        round: 1,
    });

    let response = client.node_read_causal(request).await.unwrap();
    assert_eq!(1, response.into_inner().collection_ids.len());

    // Test node read causal for round 4 (we ack all of the prior round),
    // we expect BFT 1 + 3 * 4 vertices (genesis round elided)
    let request = tonic::Request::new(NodeReadCausalRequest {
        public_key: Some(PublicKeyProto::from(public_key_1.clone())),
        round: 4,
    });

    let response = client.node_read_causal(request).await.unwrap();
    assert_eq!(13, response.into_inner().collection_ids.len());

    // Test node read causal for removed round
    let request = tonic::Request::new(NodeReadCausalRequest {
        public_key: Some(PublicKeyProto::from(public_key_1.clone())),
        round: 0,
    });

    let status = client.node_read_causal(request).await.unwrap_err();
    assert!(status.message().contains(
        "Couldn't read causal for provided key & round: Dag invariant violation The vertex known by this digest was dropped"
    ));

    // Test node read causal for round 4 (we ack all of the prior round),
    // we expect BFT 1 + 3 * 4 vertices with round 0 removed. (genesis round elided)
    let request = tonic::Request::new(NodeReadCausalRequest {
        public_key: Some(PublicKeyProto::from(public_key_1.clone())),
        round: 4,
    });

    let response = client.node_read_causal(request).await.unwrap();
    assert_eq!(13, response.into_inner().collection_ids.len());

    // Test node read causal for round 5 which does not exist.
    let request = tonic::Request::new(NodeReadCausalRequest {
        public_key: Some(PublicKeyProto::from(public_key_1.clone())),
        round: 5,
    });

    let status = client.node_read_causal(request).await.unwrap_err();
    assert!(status.message().contains(
        "Couldn't read causal for provided key & round: No known certificates for this authority"
    ));

    // Test node read causal for key that is not an authority of the mempool.
    let unknown_keypair = test_utils::random_key();
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
