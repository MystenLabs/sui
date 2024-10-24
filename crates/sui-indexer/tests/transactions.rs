// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, sync::Arc, time::Duration};

use jsonrpsee::http_client::HttpClientBuilder;
use simulacrum::Simulacrum;
use sui_config::local_ip_utils::new_local_tcp_socket_for_testing_string;
use sui_indexer::{
    indexer_reader::IndexerReader,
    test_utils::{set_up, start_indexer_jsonrpc_for_testing, wait_for_checkpoint},
};
use sui_json_rpc_api::{IndexerApiClient, ReadApiClient};
use sui_json_rpc_types::{SuiTransactionBlockResponseQuery, TransactionFilter};
use sui_test_transaction_builder::publish_package;
use sui_types::{
    base_types::SuiAddress, effects::TransactionEffectsAPI, message_envelope::Message,
    transaction::CallArg,
};
use tempfile::tempdir;
use test_cluster::TestClusterBuilder;
use tokio::time::sleep;

#[tokio::test]
async fn test_multi_get_transaction_blocks() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new()
        .with_indexer_backed_rpc()
        .build()
        .await;

    // publish package
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    let move_package = publish_package(&cluster.wallet, path).await.0;

    // execute a transaction
    let function = "emit_3";
    let arguments = vec![CallArg::Pure(bcs::to_bytes(&5u64).unwrap())];
    let transaction = cluster
        .test_transaction_builder()
        .await
        .move_call(move_package, "events_queries", function, arguments)
        .build();
    let signed_transaction = cluster.wallet.sign_transaction(&transaction);
    let first_tx = cluster.execute_transaction(signed_transaction).await;

    // another transaction
    let function = "emit_3";
    let arguments = vec![CallArg::Pure(bcs::to_bytes(&5u64).unwrap())];
    let transaction = cluster
        .test_transaction_builder()
        .await
        .move_call(move_package, "events_queries", function, arguments)
        .build();
    let signed_transaction = cluster.wallet.sign_transaction(&transaction);
    let second_tx = cluster.execute_transaction(signed_transaction).await;

    let http_client = cluster.rpc_client();

    // individual queries
    let fetched_first_tx = http_client
        .get_transaction_block(first_tx.digest, None)
        .await?;
    assert_eq!(first_tx.digest, fetched_first_tx.digest);
    let fetched_second_tx = http_client
        .get_transaction_block(second_tx.digest, None)
        .await?;
    assert_eq!(second_tx.digest, fetched_second_tx.digest);

    // multi-get query
    let fetched_txs = http_client
        .multi_get_transaction_blocks(vec![first_tx.digest, second_tx.digest], None)
        .await?;

    // First verify we got exactly 2 transactions
    assert_eq!(fetched_txs.len(), 2);

    // Then verify that both original digests are present exactly once
    let fetched_digests: Vec<_> = fetched_txs.iter().map(|tx| tx.digest).collect();

    // Verify first_tx.digest is present exactly once
    assert!(fetched_digests.contains(&first_tx.digest));
    assert_eq!(
        fetched_digests
            .iter()
            .filter(|&&d| d == first_tx.digest)
            .count(),
        1
    );

    // Verify second_tx.digest is present exactly once
    assert!(fetched_digests.contains(&second_tx.digest));
    assert_eq!(
        fetched_digests
            .iter()
            .filter(|&&d| d == second_tx.digest)
            .count(),
        1
    );

    Ok(())
}

#[tokio::test]
async fn test_query_transaction_blocks_by_checkpoint() -> Result<(), anyhow::Error> {
    let tempdir = tempdir().unwrap();
    let mut sim = Simulacrum::new();
    let data_ingestion_path = tempdir.path().to_path_buf();
    sim.set_data_ingestion_path(data_ingestion_path.clone());
    sim.create_checkpoint();
    // Create 3 transactions in checkpoint 2
    let mut ascending_tx_digests = vec![];
    for _ in 0..3 {
        let transfer_recipient = SuiAddress::random_for_testing_only();
        let (transaction, _) = sim.transfer_txn(transfer_recipient);
        let (effects, err) = sim.execute_transaction(transaction).unwrap();
        let tx_digest = effects.transaction_digest().clone();
        ascending_tx_digests.push(tx_digest);
        assert!(err.is_none());
    }
    sim.create_checkpoint();
    // Create 1 transaction in checkpoint 3
    let transfer_recipient = SuiAddress::random_for_testing_only();
    let (transaction, _) = sim.transfer_txn(transfer_recipient);
    let (effects, err) = sim.execute_transaction(transaction).unwrap();
    let tx_digest = effects.transaction_digest().clone();
    ascending_tx_digests.push(tx_digest);
    assert!(err.is_none());
    sim.create_checkpoint();
    let indexer_jsonrpc_address = new_local_tcp_socket_for_testing_string();
    // Spoofed fullnode_url, we don't actually need it for this test
    let fullnode_url = "http://localhost:8080";
    let (_, pg_store, _, database) = set_up(Arc::new(sim), data_ingestion_path).await;
    wait_for_checkpoint(&pg_store, 3).await.unwrap();
    let _ = start_indexer_jsonrpc_for_testing(
        database.database().url().as_str().to_owned(),
        fullnode_url.to_owned(),
        indexer_jsonrpc_address.clone(),
        None,
    )
    .await;
    let jsonrpc_url = format!("http://{}", indexer_jsonrpc_address);
    let rpc_client = HttpClientBuilder::default().build(&jsonrpc_url).unwrap();
    // Wait for the rpc client to be ready
    while rpc_client.get_chain_identifier().await.is_err() {
        sleep(Duration::from_millis(100)).await;
    }

    // test by querying with limit 1, making sure pagination is correct forwards
    let filter = TransactionFilter::Checkpoint(2);
    let query = SuiTransactionBlockResponseQuery::new(Some(filter.clone()), None);
    let cursor = None;
    let limit = Some(1);
    let descending_order = None;
    let ascending_first_page = rpc_client
        .query_transaction_blocks(query.clone(), cursor, limit, descending_order)
        .await?;

    assert_eq!(ascending_first_page.data.len(), 1);
    assert_eq!(ascending_first_page.data[0].checkpoint, Some(2));
    assert_eq!(ascending_first_page.data[0].digest, ascending_tx_digests[0]);
    assert_eq!(ascending_first_page.has_next_page, true);

    let ascending_second_page = rpc_client
        .query_transaction_blocks(
            query.clone(),
            ascending_first_page.next_cursor,
            limit,
            descending_order,
        )
        .await?;

    assert_eq!(ascending_second_page.data.len(), 1);
    assert_eq!(ascending_second_page.data[0].checkpoint, Some(2));
    assert_eq!(
        ascending_second_page.data[0].digest,
        ascending_tx_digests[1]
    );
    assert_eq!(ascending_second_page.has_next_page, true);

    let ascending_third_page = rpc_client
        .query_transaction_blocks(
            query.clone(),
            ascending_second_page.next_cursor,
            limit,
            descending_order,
        )
        .await?;

    assert_eq!(ascending_third_page.data.len(), 1);
    assert_eq!(ascending_third_page.data[0].checkpoint, Some(2));
    assert_eq!(ascending_third_page.data[0].digest, ascending_tx_digests[2]);
    assert_eq!(ascending_third_page.has_next_page, false);

    // and backwards
    let filter = TransactionFilter::Checkpoint(2);
    let query = SuiTransactionBlockResponseQuery::new(Some(filter.clone()), None);
    let cursor = None;
    let limit = Some(1);
    let descending_order = Some(true);
    let descending_first_page = rpc_client
        .query_transaction_blocks(query.clone(), cursor, limit, descending_order)
        .await?;

    assert_eq!(descending_first_page.data.len(), 1);
    assert_eq!(descending_first_page.data[0].checkpoint, Some(2));
    assert_eq!(
        descending_first_page.data[0].digest,
        ascending_tx_digests[2]
    );
    assert_eq!(descending_first_page.has_next_page, true);

    let descending_second_page = rpc_client
        .query_transaction_blocks(
            query.clone(),
            descending_first_page.next_cursor,
            limit,
            descending_order,
        )
        .await?;

    assert_eq!(descending_second_page.data.len(), 1);
    assert_eq!(descending_second_page.data[0].checkpoint, Some(2));
    assert_eq!(
        descending_second_page.data[0].digest,
        ascending_tx_digests[1]
    );
    assert_eq!(descending_second_page.has_next_page, true);

    let descending_third_page = rpc_client
        .query_transaction_blocks(
            query.clone(),
            descending_second_page.next_cursor,
            limit,
            descending_order,
        )
        .await?;

    assert_eq!(descending_third_page.data.len(), 1);
    assert_eq!(descending_third_page.data[0].checkpoint, Some(2));
    assert_eq!(
        descending_third_page.data[0].digest,
        ascending_tx_digests[0]
    );
    assert_eq!(descending_third_page.has_next_page, false);

    Ok(())
}
