// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use consensus::bullshark::Bullshark;
use consensus::metrics::ConsensusMetrics;
use consensus::Consensus;
use fastcrypto::hash::Hash;
use narwhal_executor::MockExecutionState;
use narwhal_executor::{get_restored_consensus_output, ExecutionIndices};
use prometheus::Registry;
use std::collections::BTreeSet;
use std::sync::Arc;
use storage::NodeStorage;
use telemetry_subscribers::TelemetryGuards;
use test_utils::{cluster::Cluster, temp_dir, CommitteeFixture};
use tokio::sync::watch;

use types::{Certificate, ReconfigureNotification, TransactionProto};

#[tokio::test]
async fn test_recovery() {
    // Create storage
    let storage = NodeStorage::reopen(temp_dir());

    let consensus_store = storage.consensus_store;
    let certificate_store = storage.certificate_store;

    // Setup consensus
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();

    // Make certificates for rounds 1 and 2.
    let keys: Vec<_> = fixture.authorities().map(|a| a.public_key()).collect();
    let genesis = Certificate::genesis(&committee)
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, next_parents) =
        test_utils::make_optimal_certificates(&committee, 1..=2, &genesis, &keys);

    // Make two certificate (f+1) with round 3 to trigger the commits.
    let (_, certificate) =
        test_utils::mock_certificate(&committee, keys[0].clone(), 3, next_parents.clone());
    certificates.push_back(certificate);
    let (_, certificate) =
        test_utils::mock_certificate(&committee, keys[1].clone(), 3, next_parents);
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = test_utils::test_channel!(1);
    let (tx_primary, mut rx_primary) = test_utils::test_channel!(1);
    let (tx_output, mut rx_output) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, _rx_consensus_round_updates) = watch::channel(0);

    let initial_committee = ReconfigureNotification::NewEpoch(committee.clone());
    let (_tx_reconfigure, rx_reconfigure) = watch::channel(initial_committee);

    let gc_depth = 50;
    let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let bullshark = Bullshark::new(
        committee.clone(),
        consensus_store.clone(),
        gc_depth,
        metrics.clone(),
    );

    let _consensus_handle = Consensus::spawn(
        committee,
        consensus_store.clone(),
        certificate_store.clone(),
        rx_reconfigure,
        rx_waiter,
        tx_primary,
        tx_consensus_round_updates,
        tx_output,
        bullshark,
        metrics,
        gc_depth,
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
    let mut consensus_index_counter = 0;
    let num_of_committed_certificates = 5;

    let committed_sub_dag = rx_output.recv().await.unwrap();
    let leader_round = committed_sub_dag.leader.round();
    let mut sequence = committed_sub_dag.certificates.into_iter();
    for i in 1..=num_of_committed_certificates {
        let output = sequence.next().unwrap();
        assert_eq!(output.consensus_index, consensus_index_counter);

        if i < 5 {
            assert_eq!(output.certificate.round(), 1);
        } else {
            assert_eq!(output.certificate.round(), 2);
        }

        consensus_index_counter += 1;
    }

    // Now assume that we want to recover from a crash. We are testing all the recovery cases
    // from having executed no certificates at all (or certificate with index = 0), up to
    // have executed the last committed certificate
    for last_executed_certificate_index in 0..consensus_index_counter {
        let mut execution_state = MockExecutionState::new();
        execution_state
            .expect_load_execution_indices()
            .times(1)
            .returning(move || ExecutionIndices {
                next_certificate_index: last_executed_certificate_index,
                next_batch_index: 0,
                next_transaction_index: 0,
                last_committed_round: leader_round,
            });

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
        let tr = bincode::serialize(&tx).unwrap();
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
        let output_transaction = bincode::deserialize::<String>(&result).unwrap();

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

    telemetry_subscribers::TelemetryConfig::new("narwhal")
        // load env variables
        .with_env()
        // load special log filter
        .with_log_level(&log_filter)
        .init()
        .0
}
