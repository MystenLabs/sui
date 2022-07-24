// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use test_utils::{committee, transaction};
use tokio::sync::mpsc::channel;

#[tokio::test]
async fn make_batch() {
    let committee = committee(None).clone();
    let (_tx_reconfiguration, rx_reconfiguration) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (tx_transaction, rx_transaction) = channel(1);
    let (tx_message, mut rx_message) = channel(1);

    // Spawn a `BatchMaker` instance.
    BatchMaker::spawn(
        committee,
        /* max_batch_size */ 200,
        /* max_batch_delay */
        Duration::from_millis(1_000_000), // Ensure the timer is not triggered.
        rx_reconfiguration,
        rx_transaction,
        tx_message,
    );

    // Send enough transactions to seal a batch.
    let tx = transaction();
    tx_transaction.send(tx.clone()).await.unwrap();
    tx_transaction.send(tx.clone()).await.unwrap();

    // Ensure the batch is as expected.
    let expected_batch = Batch(vec![tx.clone(), tx.clone()]);
    let batch = rx_message.recv().await.unwrap();
    assert_eq!(batch, expected_batch);
}

#[tokio::test]
async fn batch_timeout() {
    let committee = committee(None).clone();
    let (_tx_reconfiguration, rx_reconfiguration) =
        watch::channel(ReconfigureNotification::NewCommittee(committee.clone()));
    let (tx_transaction, rx_transaction) = channel(1);
    let (tx_message, mut rx_message) = channel(1);

    // Spawn a `BatchMaker` instance.
    BatchMaker::spawn(
        committee,
        /* max_batch_size */ 200,
        /* max_batch_delay */
        Duration::from_millis(50), // Ensure the timer is triggered.
        rx_reconfiguration,
        rx_transaction,
        tx_message,
    );

    // Do not send enough transactions to seal a batch.
    let tx = transaction();
    tx_transaction.send(tx.clone()).await.unwrap();

    // Ensure the batch is as expected.
    let expected_batch = Batch(vec![tx]);
    let batch = rx_message.recv().await.unwrap();
    assert_eq!(batch, expected_batch);
}
