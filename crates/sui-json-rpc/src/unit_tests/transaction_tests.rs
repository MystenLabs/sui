// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_config::SUI_KEYSTORE_FILENAME;
use sui_json_rpc_types::SuiTransactionBlockResponseQuery;
use sui_json_rpc_types::TransactionFilter;
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiObjectResponseQuery, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions, TransactionBlockBytes,
};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_macros::sim_test;
use sui_types::messages::{ExecuteTransactionRequestType, SenderSignedData};
use sui_types::utils::to_sender_signed_transaction;
use test_utils::network::TestClusterBuilder;

use crate::api::{IndexerApiClient, TransactionBuilderClient, WriteApiClient};

#[sim_test]
async fn test_get_transaction_block() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client
        .get_owned_objects(
            *address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await?
        .data;
    let gas_id = objects.last().unwrap().object().unwrap().object_id;

    // Make some transactions
    let mut tx_responses: Vec<SuiTransactionBlockResponse> = Vec::new();
    for obj in &objects[..objects.len() - 1] {
        let oref = obj.object().unwrap();
        let transaction_bytes: TransactionBlockBytes = http_client
            .transfer_object(
                *address,
                oref.object_id,
                Some(gas_id),
                100_000.into(),
                *address,
            )
            .await?;
        let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
        let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
        let tx =
            to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);

        let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

        let response = http_client
            .execute_transaction_block(
                tx_bytes,
                signatures,
                Some(SuiTransactionBlockResponseOptions::new()),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await?;

        tx_responses.push(response);
    }

    // TODO(chris): re-enable after rewriting get_transactions_in_range_deprecated with query_transactions
    // test get_transaction_batch
    // let batch_responses: Vec<SuiTransactionBlockResponse> = http_client
    //     .multi_get_transaction_blocks(tx, Some(SuiTransactionBlockResponseOptions::new()))
    //     .await?;

    // assert_eq!(5, batch_responses.len());

    // for r in batch_responses.iter().skip(1) {
    //     assert!(tx_responses
    //         .iter()
    //         .any(|resp| matches!(resp, SuiTransactionBlockResponse {digest, ..} if *digest == r.digest)))
    // }

    // // test get_transaction
    // for tx_digest in tx {
    //     let response: SuiTransactionBlockResponse = http_client
    //         .get_transaction_block(
    //             tx_digest,
    //             Some(SuiTransactionBlockResponseOptions::new().with_raw_input()),
    //         )
    //         .await?;
    //     assert!(tx_responses.iter().any(
    //         |resp| matches!(resp, SuiTransactionBlockResponse {digest, ..} if *digest == response.digest)
    //     ));
    //     let sender_signed_data: SenderSignedData =
    //         bcs::from_bytes(&response.raw_transaction).unwrap();
    //     assert_eq!(sender_signed_data.digest(), tx_digest);
    // }

    Ok(())
}

#[sim_test]
async fn test_get_raw_transaction() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await?;
    let http_client = cluster.rpc_client();
    let address = cluster.accounts.first().unwrap();

    let objects = http_client
        .get_owned_objects(
            *address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new(),
            )),
            None,
            None,
        )
        .await?
        .data;
    let object_to_transfer = objects.first().unwrap().object().unwrap().object_id;

    // Make a transfer transactions
    let transaction_bytes: TransactionBlockBytes = http_client
        .transfer_object(*address, object_to_transfer, None, 10_000.into(), *address)
        .await?;
    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
    let tx = to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
    let original_sender_signed_data = tx.data().clone();

    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    let response = http_client
        .execute_transaction_block(
            tx_bytes,
            signatures,
            Some(SuiTransactionBlockResponseOptions::new().with_raw_input()),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    let decode_sender_signed_data: SenderSignedData =
        bcs::from_bytes(&response.raw_transaction).unwrap();
    // verify that the raw transaction data returned by the response is the same
    // as the original transaction data
    assert_eq!(decode_sender_signed_data, original_sender_signed_data);

    Ok(())
}

#[sim_test]
async fn test_get_fullnode_transaction() -> Result<(), anyhow::Error> {
    let mut cluster = TestClusterBuilder::new().build().await.unwrap();

    let context = &mut cluster.wallet;

    let keystore_path = cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
    let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    let mut tx_responses: Vec<SuiTransactionBlockResponse> = Vec::new();

    let client = context.get_client().await.unwrap();

    for address in cluster.accounts.iter() {
        let objects = client
            .read_api()
            .get_owned_objects(
                *address,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::new()
                        .with_type()
                        .with_owner()
                        .with_previous_transaction(),
                )),
                None,
                None,
            )
            .await?
            .data;
        let gas_id = objects.last().unwrap().object().unwrap().object_id;

        // Make some transactions
        for obj in &objects[..objects.len() - 1] {
            let oref = obj.object().unwrap();
            let data = client
                .transaction_builder()
                .transfer_object(*address, oref.object_id, Some(gas_id), 100_000, *address)
                .await?;
            let tx = to_sender_signed_transaction(data, keystore.get_key(address).unwrap());

            let response = client
                .quorum_driver_api()
                .execute_transaction_block(
                    tx,
                    SuiTransactionBlockResponseOptions::new(),
                    Some(ExecuteTransactionRequestType::WaitForLocalExecution),
                )
                .await
                .unwrap();

            tx_responses.push(response);
        }
    }

    // test get_recent_transactions with smaller range
    let tx = client
        .read_api()
        .query_transaction_blocks(
            SuiTransactionBlockResponseQuery::default(),
            None,
            Some(3),
            true,
        )
        .await
        .unwrap();
    assert_eq!(3, tx.data.len());
    assert!(tx.has_next_page);

    // test get all transactions paged
    let first_page = client
        .read_api()
        .query_transaction_blocks(
            SuiTransactionBlockResponseQuery::default(),
            None,
            Some(5),
            false,
        )
        .await
        .unwrap();
    assert_eq!(5, first_page.data.len());
    assert!(first_page.has_next_page);

    let second_page = client
        .read_api()
        .query_transaction_blocks(
            SuiTransactionBlockResponseQuery::default(),
            first_page.next_cursor,
            None,
            false,
        )
        .await
        .unwrap();
    assert_eq!(16, second_page.data.len());
    assert!(!second_page.has_next_page);

    let mut all_txs_rev = first_page.data.clone();
    all_txs_rev.extend(second_page.data);
    all_txs_rev.reverse();

    // test get 10 latest transactions paged
    let latest = client
        .read_api()
        .query_transaction_blocks(
            SuiTransactionBlockResponseQuery::default(),
            None,
            Some(10),
            true,
        )
        .await
        .unwrap();
    assert_eq!(10, latest.data.len());
    assert_eq!(Some(all_txs_rev[9].digest), latest.next_cursor);
    assert_eq!(all_txs_rev[0..10], latest.data);
    assert!(latest.has_next_page);

    // test get from address txs in ascending order
    let address_txs_asc = client
        .read_api()
        .query_transaction_blocks(
            SuiTransactionBlockResponseQuery::new_with_filter(TransactionFilter::FromAddress(
                cluster.accounts[0],
            )),
            None,
            None,
            false,
        )
        .await
        .unwrap();
    assert_eq!(4, address_txs_asc.data.len());

    // test get from address txs in descending order
    let address_txs_desc = client
        .read_api()
        .query_transaction_blocks(
            SuiTransactionBlockResponseQuery::new_with_filter(TransactionFilter::FromAddress(
                cluster.accounts[0],
            )),
            None,
            None,
            true,
        )
        .await
        .unwrap();
    assert_eq!(4, address_txs_desc.data.len());

    // test get from address txs in both ordering are the same.
    let mut data_asc = address_txs_asc.data;
    data_asc.reverse();
    assert_eq!(data_asc, address_txs_desc.data);

    // test get_recent_transactions
    let tx = client
        .read_api()
        .query_transaction_blocks(
            SuiTransactionBlockResponseQuery::default(),
            None,
            Some(20),
            true,
        )
        .await
        .unwrap();
    assert_eq!(20, tx.data.len());

    // test get_transaction
    for tx_resp in tx.data {
        let response: SuiTransactionBlockResponse = client
            .read_api()
            .get_transaction_with_options(tx_resp.digest, SuiTransactionBlockResponseOptions::new())
            .await
            .unwrap();
        assert!(tx_responses
            .iter()
            .any(|resp| resp.digest == response.digest))
    }

    Ok(())
}
