// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::authority::authority_tests::init_state_with_objects;
use crate::consensus_client::consensus_tests::test_certificates;
use crate::consensus_client::consensus_tests::{test_gas_objects, test_shared_object};
use sui_types::serialize::{deserialize_message, SerializedMessage};
use test_utils::network::test_listener;
use tokio::sync::mpsc::channel;

#[tokio::test]
async fn listen_to_sequenced_transaction() {
    let (tx_sui_to_consensus, rx_sui_to_consensus) = channel(1);
    let (tx_consensus_to_sui, rx_consensus_to_sui) = channel(1);

    // Make a sample (serialized) consensus transaction.
    let transaction = vec![10u8, 11u8];

    // Spawn a consensus listener.
    ConsensusListener::spawn(
        /* rx_consensus_input */ rx_sui_to_consensus,
        /* rx_consensus_output */ rx_consensus_to_sui,
    );

    // Submit a sample consensus transaction.
    let (sender, receiver) = oneshot::channel();
    let input = ConsensusInput {
        serialized: transaction.clone(),
        replier: sender,
    };
    tx_sui_to_consensus.send(input).await.unwrap();

    // Notify the consensus listener that the transaction has been sequenced.
    let output = (Ok(()), transaction);
    tx_consensus_to_sui.send(output).await.unwrap();

    // Ensure the caller get notified from the consensus listener.
    assert!(receiver.await.unwrap().is_ok());
}

#[tokio::test]
async fn submit_transaction_to_consensus() {
    // TODO [issue #932]: Use a port allocator to avoid port conflicts.
    let consensus_address = "127.0.0.1:12456".parse().unwrap();
    let (tx_consensus_listener, mut rx_consensus_listener) = channel(1);

    // Initialize an authority with a (owned) gas object and a shared object; then
    // make a test certificate.
    let mut objects = test_gas_objects();
    objects.push(test_shared_object());
    let authority = init_state_with_objects(objects).await;
    let certificate = test_certificates(&authority).await.pop().unwrap();

    // Make a new consensus submitter instance.
    let submitter = ConsensusSubmitter::new(
        consensus_address,
        /* buffer_size */ 65000,
        authority.committee,
        tx_consensus_listener,
    );

    // Spawn a network listener to receive the transaction (emulating the consensus node).
    let handle = test_listener(consensus_address);

    // Notify the submitter when a consensus transaction has been sequenced.
    tokio::spawn(async move {
        let ConsensusInput { replier, .. } = rx_consensus_listener.recv().await.unwrap();
        replier.send(Ok(())).unwrap();
    });

    // Submit the transaction and ensure the submitter reports success to the caller.
    tokio::task::yield_now().await;
    let consensus_transaction = ConsensusTransaction::UserTransaction(certificate);
    let result = submitter.submit(&consensus_transaction).await;
    println!("{:?}", result);
    assert!(result.is_ok());

    // Ensure the consensus node got the transaction.
    let bytes = handle.await.unwrap();
    match deserialize_message(&bytes[..]).unwrap() {
        SerializedMessage::ConsensusTransaction(..) => (),
        _ => panic!("Unexpected protocol message"),
    }
}
