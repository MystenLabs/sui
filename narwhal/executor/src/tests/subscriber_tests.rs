// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::fixtures::{test_store, test_u64_certificates};
use primary::GetBlockResponse;
use prometheus::Registry;
use test_utils::{committee, test_channel};
use types::{
    BatchMessage, BlockError, BlockErrorKind, BlockResult, CertificateDigest, SequenceNumber,
};

/// Spawn a mock consensus core and a test subscriber.
async fn spawn_subscriber(
    rx_sequence: metered_channel::Receiver<ConsensusOutput>,
    tx_executor: metered_channel::Sender<ConsensusOutput>,
    tx_get_block_commands: metered_channel::Sender<BlockCommand>,
    restored_consensus_output: Vec<ConsensusOutput>,
) -> (
    Store<BatchDigest, Batch>,
    watch::Sender<ReconfigureNotification>,
    JoinHandle<()>,
) {
    let committee = committee(None);
    let message = ReconfigureNotification::NewEpoch(committee);
    let (tx_reconfigure, rx_reconfigure) = watch::channel(message);

    // Spawn a test subscriber.
    let store = test_store();
    let executor_metrics = ExecutorMetrics::new(&Registry::new());
    let subscriber_handle = Subscriber::spawn(
        store.clone(),
        tx_get_block_commands,
        rx_reconfigure,
        rx_sequence,
        tx_executor,
        Arc::new(executor_metrics),
        restored_consensus_output,
    );

    (store, tx_reconfigure, subscriber_handle)
}

#[tokio::test]
async fn handle_certificate_with_downloaded_batch() {
    let (tx_sequence, rx_sequence) = test_channel!(10);
    let (tx_executor, mut rx_executor) = test_channel!(10);
    let (tx_get_block_command, mut rx_get_block_command) = test_utils::test_get_block_commands!(1);

    // Spawn a subscriber.
    let (store, _tx_reconfigure, _) =
        spawn_subscriber(rx_sequence, tx_executor, tx_get_block_command, vec![]).await;

    let total_certificates = 2;
    let certificates = test_u64_certificates(
        total_certificates,
        /* batches_per_certificate */ 2,
        /* transactions_per_batch */ 2,
    );
    for (i, (certificate, _)) in certificates.clone().into_iter().enumerate() {
        let message = ConsensusOutput {
            certificate,
            consensus_index: i as SequenceNumber,
        };
        tx_sequence.send(message).await.unwrap();
    }

    for i in 0..total_certificates {
        let request = rx_get_block_command.recv().await.unwrap();

        let batches = match request {
            BlockCommand::GetBlock { id, sender } => {
                let (certificate, batches) = certificates.get(i).unwrap().to_owned();

                assert_eq!(
                    certificate.digest(),
                    id,
                    "Out of order certificate id has been received"
                );

                // Mimic the block_waiter here and respond with the payload back
                let ok = successful_block_response(id, batches.clone());

                sender.send(ok).unwrap();

                batches
            }
            _ => panic!("Unexpected command received"),
        };

        let output = rx_executor.recv().await.unwrap();
        assert_eq!(output.consensus_index, i as SequenceNumber);

        // Ensure all the batches have been written in storage
        for (batch_id, batch) in batches {
            let stored_batch = store
                .read(batch_id)
                .await
                .expect("Error while retrieving batch")
                .unwrap();
            assert_eq!(batch, stored_batch);
        }
    }
}

#[tokio::test]
async fn should_retry_when_failed_to_get_payload() {
    let (tx_sequence, rx_sequence) = test_channel!(10);
    let (tx_executor, mut rx_executor) = test_channel!(10);
    let (tx_get_block_command, mut rx_get_block_command) = test_utils::test_get_block_commands!(1);

    // Spawn a subscriber.
    let (store, _tx_reconfigure, _) =
        spawn_subscriber(rx_sequence, tx_executor, tx_get_block_command, vec![]).await;

    // Create a certificate
    let total_certificates = 1;
    let certificates = test_u64_certificates(
        total_certificates,
        /* batches_per_certificate */ 2,
        /* transactions_per_batch */ 2,
    );

    let (certificate, batches) = certificates.first().unwrap().to_owned();
    let certificate_id = certificate.digest();

    let message = ConsensusOutput {
        certificate,
        consensus_index: 500 as SequenceNumber,
    };

    // send the certificate to download payload
    tx_sequence.send(message).await.unwrap();

    // Now assume that the block_wait is responding with error for the
    // requested certificate for RETRIES -1 attempts.
    // Finally on the last one we reply with a successful result.
    const RETRIES: u32 = 3;
    for i in 0..RETRIES {
        let request = rx_get_block_command.recv().await.unwrap();

        match request {
            BlockCommand::GetBlock { id, sender } => {
                assert_eq!(certificate_id, id);

                if i < RETRIES - 1 {
                    sender
                        .send(Err(BlockError {
                            id,
                            error: BlockErrorKind::BatchTimeout,
                        }))
                        .unwrap();
                } else {
                    // Mimic the block_waiter here and respond with the payload back
                    let ok = successful_block_response(id, batches.clone());

                    sender.send(ok).unwrap();
                }
            }
            _ => panic!("Unexpected command received"),
        };
    }

    // Now the message will be delivered and should be forwarded to tx_executor
    let output = rx_executor.recv().await.unwrap();
    assert_eq!(output.consensus_index, 500 as SequenceNumber);

    // Ensure all the batches have been written in storage
    for (batch_id, batch) in batches {
        let stored_batch = store
            .read(batch_id)
            .await
            .expect("Error while retrieving batch")
            .unwrap();
        assert_eq!(batch, stored_batch);
    }
}

#[tokio::test]
async fn subscriber_should_crash_when_irrecoverable_error() {
    let (tx_sequence, rx_sequence) = test_channel!(10);
    let (tx_executor, _rx_executor) = test_channel!(10);
    let (tx_get_block_command, mut rx_get_block_command) = test_utils::test_get_block_commands!(1);

    // Spawn a subscriber.
    let (_store, _tx_reconfigure, handle) =
        spawn_subscriber(rx_sequence, tx_executor, tx_get_block_command, vec![]).await;

    // Create a certificate
    let total_certificates = 1;
    let certificates = test_u64_certificates(
        total_certificates,
        /* batches_per_certificate */ 2,
        /* transactions_per_batch */ 2,
    );

    let (certificate, _batches) = certificates.first().unwrap().to_owned();

    let message = ConsensusOutput {
        certificate,
        consensus_index: 500 as SequenceNumber,
    };

    // now close the tx_get_block_command in order to inject an artificial
    // error and make any retries stop and propagate the error
    rx_get_block_command.close();

    // send the certificate to download payload
    // We expect this to make the subscriber crash
    tx_sequence.send(message).await.unwrap();

    let err = handle
        .await
        .expect_err("Expected an error, instead a successful response returned");
    assert!(err.is_panic());
}

#[tokio::test]
async fn test_subscriber_with_restored_consensus_output() {
    let (_tx_sequence, rx_sequence) = test_channel!(10);
    let (tx_executor, mut rx_executor) = test_channel!(10);
    let (tx_get_block_command, mut rx_get_block_command) = test_utils::test_get_block_commands!(1);

    // Create restored consensus output
    let total_certificates = 2;
    let certificates = test_u64_certificates(
        total_certificates,
        /* batches_per_certificate */ 2,
        /* transactions_per_batch */ 2,
    );
    let restored_consensus = certificates
        .clone()
        .into_iter()
        .enumerate()
        .map(|(i, (certificate, _))| ConsensusOutput {
            certificate,
            consensus_index: i as SequenceNumber,
        })
        .collect();

    // Spawn a subscriber.
    let (_store, _tx_reconfigure, _handle) = spawn_subscriber(
        rx_sequence,
        tx_executor,
        tx_get_block_command,
        restored_consensus,
    )
    .await;

    for i in 0..total_certificates {
        let request = rx_get_block_command.recv().await.unwrap();

        let _batches = match request {
            BlockCommand::GetBlock { id, sender } => {
                let (_certificate, batches) = certificates.get(i).unwrap().to_owned();

                // Mimic the block_waiter here and respond with the payload back
                let ok = successful_block_response(id, batches.clone());

                sender.send(ok).unwrap();

                batches
            }
            _ => panic!("Unexpected command received"),
        };

        // Ensure restored messages are delivered.
        let output = rx_executor.recv().await.unwrap();
        assert_eq!(output.consensus_index, i as SequenceNumber);
    }
}

// Helper method to create a successful (OK) get_block response.
fn successful_block_response(
    id: CertificateDigest,
    batches: Vec<(BatchDigest, Batch)>,
) -> BlockResult<GetBlockResponse> {
    // Mimic the block_waiter here and respond with the payload back
    let batch_messages = batches
        .iter()
        .map(|(batch_id, batch)| BatchMessage {
            id: *batch_id,
            transactions: batch.clone(),
        })
        .collect();

    Ok(GetBlockResponse {
        id,
        batches: batch_messages,
    })
}
