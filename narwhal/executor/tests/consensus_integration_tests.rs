// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use consensus::bullshark::Bullshark;
use consensus::Consensus;
use fastcrypto::hash::Hash;
use narwhal_executor::get_restored_consensus_output;
use narwhal_executor::MockExecutionState;
use primary::NUM_SHUTDOWN_RECEIVERS;
use std::collections::BTreeSet;
use storage::NodeStorage;
use telemetry_subscribers::TelemetryGuards;
use test_utils::{cluster::Cluster, temp_dir, CommitteeFixture};
use tokio::sync::watch;

use types::{Certificate, PreSubscribedBroadcastSender, Round, TransactionProto};

#[tokio::test]
async fn test_recovery() {
    // Create storage
    let storage = NodeStorage::reopen(temp_dir(), None);

    let consensus_store = storage.consensus_store;
    let certificate_store = storage.certificate_store;

    // Setup consensus
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();

    // Make certificates for rounds 1 and 2.
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, next_parents) =
        test_utils::make_optimal_certificates(&committee, 1..=2, &genesis, &ids);

    // Make two certificate (f+1) with round 3 to trigger the commits.
    let (_, certificate) =
        test_utils::mock_certificate(&committee, ids[0], 3, next_parents.clone());
    certificates.push_back(certificate);
    let (_, certificate) = test_utils::mock_certificate(&committee, ids[1], 3, next_parents);
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) =
        watch::channel(ConsensusRound::default());

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let gc_depth = 50;
    let bullshark = Bullshark::new(committee.clone(), consensus_store.clone(), gc_depth);

    let _consensus_handle = Consensus::spawn(
        committee,
        GC_DEPTH,
        consensus_store.clone(),
        certificate_store.clone(),
        tx_shutdown.subscribe(),
        rx_waiter,
        tx_primary,
        tx_consensus_round_updates,
        tx_output,
        bullshark,
    );
    tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

    // Feed all certificates to the consensus. Only the last certificate should trigger
    // commits, so the task should not block.
    while let Some(certificate) = certificates.pop_front() {
        // we store the certificates so we can enable the recovery
        // mechanism later.
        certificate_store.write(certificate.clone()).unwrap();
        tx_waiter.send(certificate).await.unwrap();
    }

    // Ensure the first 4 ordered certificates are from round 1 (they are the parents of the committed
    // leader); then the leader's certificate should be committed.
    let consensus_index_counter = 4;
    let num_of_committed_certificates = 5;

    let committed_sub_dag = rx_output.recv().await.unwrap();
    let mut sequence = committed_sub_dag.certificates.into_iter();
    for i in 1..=num_of_committed_certificates {
        let output = sequence.next().unwrap();

        if i < 5 {
            assert_eq!(output.round(), 1);
        } else {
            assert_eq!(output.round(), 2);
        }
    }

    // Now assume that we want to recover from a crash. We are testing all the recovery cases
    // from having executed no certificates at all (or certificate with index = 0), up to
    // have executed the last committed certificate
    for last_executed_certificate_index in 0..consensus_index_counter {
        let mut execution_state = MockExecutionState::new();
        execution_state
            .expect_last_executed_sub_dag_index()
            .times(1)
            .returning(|| 1);

        let consensus_output = get_restored_consensus_output(
            consensus_store.clone(),
            certificate_store.clone(),
            &execution_state,
        )
        .await
        .unwrap();

        // we expect to have recovered all the certificates from the last commit. The Sui executor engine
        // will not execute twice the same certificate.
        assert_eq!(consensus_output.len(), 1);
        assert!(
            consensus_output[0].len()
                >= (num_of_committed_certificates - last_executed_certificate_index) as usize
        );
    }
}

#[tokio::test]
async fn test_internal_consensus_output() {
    // Enabled debug tracing so we can easily observe the
    // nodes logs.
    let _guard = setup_tracing();

    let mut cluster = Cluster::new(None, true);

    // start the cluster
    cluster.start(Some(4), Some(1), None).await;

    // get a client to send transactions
    let worker_id = 0;

    let authority = cluster.authority(0);
    let mut client = authority.new_transactions_client(&worker_id).await;

    // Subscribe to the transaction confirmation channel
    let mut receiver = authority
        .primary()
        .await
        .tx_transaction_confirmation
        .subscribe();

    // Create arbitrary transactions
    let mut transactions = Vec::new();

    const NUM_OF_TRANSACTIONS: u32 = 10;
    for i in 0..NUM_OF_TRANSACTIONS {
        let tx = string_transaction(i);

        // serialise and send
        let tr = bcs::to_bytes(&tx).unwrap();
        let txn = TransactionProto {
            transaction: Bytes::from(tr),
        };
        client.submit_transaction(txn).await.unwrap();

        transactions.push(tx);
    }

    // wait for transactions to complete
    loop {
        let result = receiver.recv().await.unwrap();

        // deserialise transaction
        let output_transaction = bcs::from_bytes::<String>(&result).unwrap();

        // we always remove the first transaction and check with the one
        // sequenced. We want the transactions to be sequenced in the
        // same order as we post them.
        let expected_transaction = transactions.remove(0);

        assert_eq!(
            expected_transaction, output_transaction,
            "Expected to have received transaction with same id. Ordering is important"
        );

        if transactions.is_empty() {
            break;
        }
    }
}

fn string_transaction(id: u32) -> String {
    format!("test transaction:{id}")
}

fn setup_tracing() -> TelemetryGuards {
    // Setup tracing
    let tracing_level = "debug";
    let network_tracing_level = "info";

    let log_filter = format!("{tracing_level},h2={network_tracing_level},tower={network_tracing_level},hyper={network_tracing_level},tonic::transport={network_tracing_level}");

    telemetry_subscribers::TelemetryConfig::new()
        // load env variables
        .with_env()
        // load special log filter
        .with_log_level(&log_filter)
        .init()
        .0
}
