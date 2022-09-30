// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use telemetry_subscribers::TelemetryGuards;
use test_utils::cluster::Cluster;
use types::TransactionProto;

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
