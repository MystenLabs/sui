// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{tusk::consensus_tests::*, Consensus};
use bytes::BytesMut;
use crypto::{ed25519::Ed25519PublicKey, traits::KeyPair, Hash};
use std::collections::{BTreeSet, VecDeque};

/// Make enough certificates to commit a leader.
pub fn commit_certificates() -> VecDeque<Certificate<Ed25519PublicKey>> {
    // Make certificates for rounds 1 to 4.
    let keys: Vec<_> = keys().into_iter().map(|kp| kp.public().clone()).collect();
    let genesis = Certificate::genesis(&mock_committee(&keys[..]))
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, next_parents) = make_optimal_certificates(1, 4, &genesis, &keys);

    // Make one certificate with round 5 to trigger the commits.
    let (_, certificate) = mock_certificate(keys[0].clone(), 5, next_parents);
    certificates.push_back(certificate);
    certificates
}

/// Spawn the consensus core and the subscriber handler. Also add to storage enough certificates to
/// commit a leader (as if they were added by the Primary).
pub async fn spawn_validator(address: SocketAddr) -> Sender<Certificate<Ed25519PublicKey>> {
    // Make enough certificates to commit a leader.
    let certificates = commit_certificates();

    // Make the committee.
    let keys: Vec<_> = keys().into_iter().map(|kp| kp.public().clone()).collect();
    let committee = mock_committee(&keys[..]);

    // Create the storages.
    let consensus_store_path = temp_testdir::TempDir::default();
    let consensus_store = make_consensus_store(&consensus_store_path);
    let certificate_store_path = temp_testdir::TempDir::default();
    let certificate_store = make_certificate_store(&certificate_store_path);

    // Persist the certificates to storage (they may be require by the synchronizer).
    let to_store = certificates.into_iter().map(|x| (x.digest(), x));
    certificate_store.write_all(to_store).await.unwrap();

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = channel(1);
    let (tx_primary, mut rx_primary) = channel(1);
    let (tx_output, rx_output) = channel(1);
    Consensus::spawn(
        committee,
        consensus_store.clone(),
        /* gc_depth */ 50,
        rx_waiter,
        tx_primary,
        tx_output,
    );
    tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

    // Spawn the subscriber handler.
    SubscriberHandler::spawn(
        address,
        consensus_store,
        certificate_store,
        /* rx_sequence */ rx_output,
        /* max_subscribers */ 1,
    );

    // Return the channel to input certificates to consensus.
    tx_waiter
}

/// Facility to read consensus outputs out of a stream and return them in the right order.
pub async fn order_stream<Stream>(
    reader: &mut Stream,
    last_known_client_index: u64,
    last_known_server_index: u64,
) -> Vec<ConsensusOutput<Ed25519PublicKey>>
where
    Stream: StreamExt<Item = Result<BytesMut, std::io::Error>> + Unpin,
{
    let mut next_ordinary_sequence = last_known_server_index + 1;
    let mut next_catchup_sequence = last_known_client_index + 1;
    let mut buffer = Vec::new();
    let mut sequence = Vec::new();
    loop {
        let bytes = reader.next().await.unwrap().unwrap();
        let output: ConsensusOutput<Ed25519PublicKey> = bincode::deserialize(&bytes).unwrap();
        let consensus_index = output.consensus_index;

        if consensus_index == next_ordinary_sequence {
            buffer.push(output);
            next_ordinary_sequence += 1;
        } else if consensus_index == next_catchup_sequence {
            sequence.push(output);
            next_catchup_sequence += 1;
        } else {
            panic!("Unexpected consensus index");
        }

        if consensus_index == last_known_server_index {
            break;
        }
    }

    sequence.extend(buffer);
    sequence
}

#[tokio::test]
async fn subscribe() {
    // Make enough certificates to commit a leader.
    let mut certificates = commit_certificates();

    // Spawn the consensus and subscriber handler.
    let address = "127.0.0.1:12000".parse::<SocketAddr>().unwrap();
    let tx_consensus_input = spawn_validator(address).await;

    // Spawn a subscriber.
    tokio::task::yield_now().await;
    let socket = TcpStream::connect(address).await.unwrap();
    let (_writer, mut reader) = Framed::new(socket, LengthDelimitedCodec::new()).split();

    // Feed all certificates to the consensus. Only the last certificate should trigger commits,
    // so the task should not block.
    tokio::task::yield_now().await;
    while let Some(certificate) = certificates.pop_front() {
        tx_consensus_input.send(certificate).await.unwrap();
    }

    // Ensure the first 4 ordered certificates are from round 1 (they are the parents of the committed
    // leader); then the leader's certificate should be committed.
    for i in 1..=4 {
        let bytes = reader.next().await.unwrap().unwrap();
        let output: ConsensusOutput<Ed25519PublicKey> = bincode::deserialize(&bytes).unwrap();
        assert_eq!(output.consensus_index, i);
    }
    let bytes = reader.next().await.unwrap().unwrap();
    let output: ConsensusOutput<Ed25519PublicKey> = bincode::deserialize(&bytes).unwrap();
    assert_eq!(output.consensus_index, 5);
}

#[tokio::test]
async fn subscribe_sync() {
    // Make enough certificates to commit a leader.
    let mut certificates = commit_certificates();

    // Spawn the consensus and subscriber handler.
    let address = "127.0.0.1:12001".parse::<SocketAddr>().unwrap();
    let tx_consensus_input = spawn_validator(address).await;

    // Spawn a subscriber.
    tokio::task::yield_now().await;
    let socket = TcpStream::connect(address).await.unwrap();
    let (mut writer, mut reader) = Framed::new(socket, LengthDelimitedCodec::new()).split();

    // Feed all certificates to the consensus. Only the last certificate should trigger commits,
    // so the task should not block.
    tokio::task::yield_now().await;
    while let Some(certificate) = certificates.pop_front() {
        tx_consensus_input.send(certificate).await.unwrap();
    }

    // Read first 4 certificates. Then pretend we crashed after reading the first certificate and
    // try to sync to get up to speed.
    for i in 1..=4 {
        let bytes = reader.next().await.unwrap().unwrap();
        let output: ConsensusOutput<Ed25519PublicKey> = bincode::deserialize(&bytes).unwrap();
        assert_eq!(output.consensus_index, i);
    }

    let last_known_client_index = 1;
    let last_known_server_index = 4;

    let message = ConsensusSyncRequest {
        missing: (last_known_client_index + 1..=last_known_server_index),
    };
    let serialized = bincode::serialize(&message).unwrap();
    writer.send(Bytes::from(serialized)).await.unwrap();

    // Check that we got the complete sequence of certificates in the right order.
    let ok = order_stream(
        &mut reader,
        last_known_client_index,
        last_known_server_index,
    )
    .await
    .into_iter()
    .enumerate()
    .all(|(i, output)| output.consensus_index == last_known_client_index + 1 + i as u64);
    assert!(ok);
}
