// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    execution_state::{TestState, KILLER_TRANSACTION, MALFORMED_TRANSACTION},
    fixtures::{test_batch, test_certificate, test_store, test_u64_certificates},
};
use crypto::ed25519::Ed25519PublicKey;
use std::sync::Arc;
use test_utils::committee;
use tokio::sync::mpsc::channel;
use types::Certificate;

#[tokio::test]
async fn execute_transactions() {
    let (tx_executor, rx_executor) = channel(10);
    let (tx_output, mut rx_output) = channel(10);

    let committee = committee(None);
    let message = ReconfigureNotification::NewCommittee(committee);
    let (_tx_reconfigure, rx_reconfigure) = watch::channel(message);

    // Spawn the executor.
    let store = test_store();
    let execution_state = Arc::new(TestState::default());
    Core::<TestState, Ed25519PublicKey>::spawn(
        store.clone(),
        execution_state.clone(),
        rx_reconfigure,
        /* rx_subscriber */ rx_executor,
        tx_output,
    );

    // Feed certificates to the mock sequencer and add the transaction data to storage (as if
    // the batch loader downloaded them).
    let certificates = test_u64_certificates(
        /* certificates */ 2, /* batches_per_certificate */ 2,
        /* transactions_per_batch */ 2,
    );
    for (certificate, batches) in certificates {
        for (digest, batch) in batches {
            store.write(digest, batch).await;
        }
        let message = ConsensusOutput {
            certificate,
            consensus_index: SequenceNumber::default(),
        };
        tx_executor.send(message).await.unwrap();
    }

    // Ensure the execution state is updated accordingly.
    let _ = rx_output.recv().await;
    let expected = ExecutionIndices {
        next_certificate_index: 2,
        next_batch_index: 0,
        next_transaction_index: 0,
    };
    assert_eq!(execution_state.get_execution_indices().await, expected);
}

#[tokio::test]
async fn execute_empty_certificate() {
    let (tx_executor, rx_executor) = channel(10);
    let (tx_output, mut rx_output) = channel(10);

    let committee = committee(None);
    let message = ReconfigureNotification::NewCommittee(committee);
    let (_tx_reconfigure, rx_reconfigure) = watch::channel(message);

    // Spawn the executor.
    let store = test_store();
    let execution_state = Arc::new(TestState::default());
    Core::<TestState, Ed25519PublicKey>::spawn(
        store.clone(),
        execution_state.clone(),
        rx_reconfigure,
        /* rx_subscriber */ rx_executor,
        tx_output,
    );

    // Feed empty certificates to the executor.
    let empty_certificates = 2;
    for _ in 0..empty_certificates {
        let message = ConsensusOutput {
            certificate: Certificate::default(),
            consensus_index: SequenceNumber::default(),
        };
        tx_executor.send(message).await.unwrap();
    }

    // Then feed one non-empty certificate.
    let certificates = test_u64_certificates(
        /* certificates */ 1, /* batches_per_certificate */ 2,
        /* transactions_per_batch */ 2,
    );
    for (certificate, batches) in certificates {
        for (digest, batch) in batches {
            store.write(digest, batch).await;
        }
        let message = ConsensusOutput {
            certificate,
            consensus_index: SequenceNumber::default(),
        };
        tx_executor.send(message).await.unwrap();
    }

    // Ensure the certificate index is updated.
    let _ = rx_output.recv().await;
    let expected = ExecutionIndices {
        next_certificate_index: 3,
        next_batch_index: 0,
        next_transaction_index: 0,
    };
    assert_eq!(execution_state.get_execution_indices().await, expected);
}

#[tokio::test]
async fn execute_malformed_transactions() {
    let (tx_executor, rx_executor) = channel(10);
    let (tx_output, mut rx_output) = channel(10);

    let committee = committee(None);
    let message = ReconfigureNotification::NewCommittee(committee);
    let (_tx_reconfigure, rx_reconfigure) = watch::channel(message);

    // Spawn the executor.
    let store = test_store();
    let execution_state = Arc::new(TestState::default());
    Core::<TestState, Ed25519PublicKey>::spawn(
        store.clone(),
        execution_state.clone(),
        rx_reconfigure,
        /* rx_subscriber */ rx_executor,
        tx_output,
    );

    // Feed a malformed transaction to the mock sequencer
    let tx0 = MALFORMED_TRANSACTION;
    let tx1 = 10;
    let (digest, batch) = test_batch(vec![tx0, tx1]);

    store.write(digest, batch).await;

    let payload = [(digest, 0)].iter().cloned().collect();
    let certificate = test_certificate(payload);

    let message = ConsensusOutput {
        certificate,
        consensus_index: SequenceNumber::default(),
    };
    tx_executor.send(message).await.unwrap();

    // Feed two certificates with good transactions to the executor.
    let certificates = test_u64_certificates(
        /* certificates */ 2, /* batches_per_certificate */ 2,
        /* transactions_per_batch */ 2,
    );
    for (certificate, batches) in certificates {
        for (digest, batch) in batches {
            store.write(digest, batch).await;
        }
        let message = ConsensusOutput {
            certificate,
            consensus_index: SequenceNumber::default(),
        };
        tx_executor.send(message).await.unwrap();
    }

    // Ensure the execution state is updated accordingly.
    let _ = rx_output.recv().await;
    let expected = ExecutionIndices {
        next_certificate_index: 3,
        next_batch_index: 0,
        next_transaction_index: 0,
    };
    assert_eq!(execution_state.get_execution_indices().await, expected);
}

#[tokio::test]
async fn internal_error_execution() {
    let (tx_executor, rx_executor) = channel(10);
    let (tx_output, mut rx_output) = channel(10);

    let committee = committee(None);
    let message = ReconfigureNotification::NewCommittee(committee);
    let (_tx_reconfigure, rx_reconfigure) = watch::channel(message);

    // Spawn the executor.
    let store = test_store();
    let execution_state = Arc::new(TestState::default());
    Core::<TestState, Ed25519PublicKey>::spawn(
        store.clone(),
        execution_state.clone(),
        rx_reconfigure,
        /* rx_subscriber */ rx_executor,
        tx_output,
    );

    // Feed a 'killer' transaction to the executor. This is a special test transaction that
    // crashes the test executor engine.
    let tx00 = 10;
    let tx01 = 11;
    let tx10 = 12;
    let tx11 = KILLER_TRANSACTION;

    let (digest_0, batch_0) = test_batch(vec![tx00, tx01]);
    let (digest_1, batch_1) = test_batch(vec![tx10, tx11]);

    store.write(digest_0, batch_0).await;
    store.write(digest_1, batch_1).await;

    let payload = [(digest_0, 0), (digest_1, 1)].iter().cloned().collect();
    let certificate = test_certificate(payload);

    let message = ConsensusOutput {
        certificate,
        consensus_index: SequenceNumber::default(),
    };
    tx_executor.send(message).await.unwrap();

    // Ensure the execution state does not change.
    let _ = rx_output.recv().await;
    let expected = ExecutionIndices {
        next_certificate_index: 0,
        next_batch_index: 0,
        next_transaction_index: 1,
    };
    assert_eq!(execution_state.get_execution_indices().await, expected);
}

#[tokio::test]
async fn crash_recovery() {
    let (tx_executor, rx_executor) = channel(10);
    let (tx_output, mut rx_output) = channel(10);

    let committee = committee(None);
    let reconfigure_notification = ReconfigureNotification::NewCommittee(committee);
    let (_tx_reconfigure, rx_reconfigure) = watch::channel(reconfigure_notification.clone());

    // Spawn the executor.
    let store = test_store();
    let execution_state = Arc::new(TestState::default());
    Core::<TestState, Ed25519PublicKey>::spawn(
        store.clone(),
        execution_state.clone(),
        rx_reconfigure,
        /* rx_subscriber */ rx_executor,
        tx_output,
    );

    // Feed two certificates with good transactions to the executor.
    let certificates = test_u64_certificates(
        /* certificates */ 2, /* batches_per_certificate */ 2,
        /* transactions_per_batch */ 2,
    );
    for (certificate, batches) in certificates {
        for (digest, batch) in batches {
            store.write(digest, batch).await;
        }
        let message = ConsensusOutput {
            certificate,
            consensus_index: SequenceNumber::default(),
        };
        tx_executor.send(message).await.unwrap();
    }

    // Feed a 'killer' transaction to the executor. This is a special test transaction that
    // crashes the test executor engine.
    let tx0 = 10;
    let tx1 = KILLER_TRANSACTION;
    let (digest, batch) = test_batch(vec![tx0, tx1]);

    store.write(digest, batch).await;

    let payload = [(digest, 0)].iter().cloned().collect();
    let certificate = test_certificate(payload);

    let message = ConsensusOutput {
        certificate,
        consensus_index: SequenceNumber::default(),
    };
    tx_executor.send(message).await.unwrap();

    // Ensure the execution state is as expected.
    let _ = rx_output.recv().await;
    let expected = ExecutionIndices {
        next_certificate_index: 2,
        next_batch_index: 0,
        next_transaction_index: 1,
    };
    assert_eq!(execution_state.get_execution_indices().await, expected);

    // Reboot the executor.
    let (tx_executor, rx_executor) = channel(10);
    let (tx_output, mut rx_output) = channel(10);
    let (_tx_reconfigure, rx_reconfigure) = watch::channel(reconfigure_notification);

    Core::<TestState, Ed25519PublicKey>::spawn(
        store.clone(),
        execution_state.clone(),
        rx_reconfigure,
        /* rx_subscriber */ rx_executor,
        tx_output,
    );

    // Feed two certificates with good transactions to the executor.
    let certificates = test_u64_certificates(
        /* certificates */ 2, /* batches_per_certificate */ 2,
        /* transactions_per_batch */ 2,
    );
    for (certificate, batches) in certificates {
        for (digest, batch) in batches {
            store.write(digest, batch).await;
        }
        let message = ConsensusOutput {
            certificate,
            consensus_index: SequenceNumber::default(),
        };
        tx_executor.send(message).await.unwrap();
    }

    // Ensure the execution state is as expected.
    let _ = rx_output.recv().await;
    let expected = ExecutionIndices {
        next_certificate_index: 4,
        next_batch_index: 0,
        next_transaction_index: 0,
    };
    assert_eq!(execution_state.get_execution_indices().await, expected);
}
