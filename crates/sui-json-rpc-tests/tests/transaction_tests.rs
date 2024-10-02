// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(not(msim))]
use std::str::FromStr;

use move_core_types::identifier::Identifier;
use sui_json::{call_args, type_args};
use sui_json_rpc_types::SuiTransactionBlockResponseQuery;
use sui_json_rpc_types::TransactionFilter;
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiObjectResponseQuery, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions, TransactionBlockBytes,
};
use sui_macros::sim_test;
use sui_types::base_types::ObjectID;
use sui_types::gas_coin::GAS;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::transaction::Command;
use sui_types::transaction::SenderSignedData;
use sui_types::transaction::TransactionData;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use test_cluster::TestClusterBuilder;

use sui_json_rpc_api::{IndexerApiClient, TransactionBuilderClient, WriteApiClient};

#[sim_test]
async fn test_get_transaction_block() -> Result<(), anyhow::Error> {
    let cluster = TestClusterBuilder::new().build().await;
    let http_client = cluster.rpc_client();
    let address = cluster.get_address_0();

    let objects = http_client
        .get_owned_objects(
            address,
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
                address,
                oref.object_id,
                Some(gas_id),
                1_000_000.into(),
                address,
            )
            .await?;
        let tx = cluster
            .wallet
            .sign_transaction(&transaction_bytes.to_data()?);

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
    let cluster = TestClusterBuilder::new().build().await;
    let http_client = cluster.rpc_client();
    let address = cluster.get_address_0();

    let objects = http_client
        .get_owned_objects(
            address,
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
        .transfer_object(address, object_to_transfer, None, 1_000_000.into(), address)
        .await?;
    let tx = cluster
        .wallet
        .sign_transaction(&transaction_bytes.to_data()?);
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
    let cluster = TestClusterBuilder::new().build().await;

    let context = &cluster.wallet;

    let mut tx_responses: Vec<SuiTransactionBlockResponse> = Vec::new();

    let client = context.get_client().await.unwrap();

    for address in cluster.get_addresses() {
        let objects = client
            .read_api()
            .get_owned_objects(
                address,
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
                .transfer_object(address, oref.object_id, Some(gas_id), 1_000_000, address)
                .await?;
            let tx = cluster.wallet.sign_transaction(&data);

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
    let query = SuiTransactionBlockResponseQuery {
        options: Some(SuiTransactionBlockResponseOptions {
            show_input: true,
            show_effects: true,
            show_events: true,
            ..Default::default()
        }),
        ..Default::default()
    };

    let tx = client
        .read_api()
        .query_transaction_blocks(query, None, Some(3), true)
        .await
        .unwrap();
    assert_eq!(3, tx.data.len());
    assert!(tx.data[0].transaction.is_some());
    assert!(tx.data[0].effects.is_some());
    assert!(tx.data[0].events.is_some());
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
    assert!(second_page.data.len() > 5);

    let mut all_txs = first_page.data.clone();
    all_txs.extend(second_page.data);

    // test get 10 transactions paged
    let latest = client
        .read_api()
        .query_transaction_blocks(
            SuiTransactionBlockResponseQuery::default(),
            None,
            Some(10),
            false,
        )
        .await
        .unwrap();
    assert_eq!(10, latest.data.len());
    assert_eq!(Some(all_txs[9].digest), latest.next_cursor);
    assert_eq!(all_txs[0..10], latest.data);
    assert!(latest.has_next_page);

    // test get from address txs in ascending order
    let address_txs_asc = client
        .read_api()
        .query_transaction_blocks(
            SuiTransactionBlockResponseQuery::new_with_filter(TransactionFilter::FromAddress(
                cluster.get_address_0(),
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
                cluster.get_address_0(),
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
        assert_eq!(tx_resp.digest, response.digest);
    }

    Ok(())
}

#[sim_test]
async fn test_query_transaction_blocks() -> Result<(), anyhow::Error> {
    let mut cluster = TestClusterBuilder::new().build().await;
    let context = &cluster.wallet;
    let client = context.get_client().await.unwrap();

    let address = cluster.get_address_0();
    let objects = client
        .read_api()
        .get_owned_objects(
            address,
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

    // make 2 move calls of same package & module, but different functions
    let package_id = ObjectID::new(SUI_FRAMEWORK_ADDRESS.into_bytes());
    let coin = objects.first().unwrap();
    let coin_2 = &objects[1];
    let signer = cluster.wallet.active_address().unwrap();

    let tx_builder = client.transaction_builder().clone();
    let mut pt_builer = ProgrammableTransactionBuilder::new();
    let gas = objects.last().unwrap().object().unwrap().object_ref();

    let module = Identifier::from_str("pay")?;
    let function_1 = Identifier::from_str("split")?;
    let function_2 = Identifier::from_str("divide_and_keep")?;

    let sui_type_args = type_args![GAS::type_tag()]?;
    let type_args = sui_type_args
        .into_iter()
        .map(|ty| ty.try_into())
        .collect::<Result<Vec<_>, _>>()?;

    let sui_call_args_1 = call_args!(coin.data.clone().unwrap().object_id, 10)?;
    let call_args_1 = tx_builder
        .resolve_and_checks_json_args(
            &mut pt_builer,
            package_id,
            &module,
            &function_1,
            &type_args,
            sui_call_args_1,
        )
        .await?;
    let cmd_1 = Command::move_call(
        package_id,
        module.clone(),
        function_1,
        type_args.clone(),
        call_args_1.clone(),
    );

    let sui_call_args_2 = call_args!(coin_2.data.clone().unwrap().object_id, 10)?;
    let call_args_2 = tx_builder
        .resolve_and_checks_json_args(
            &mut pt_builer,
            package_id,
            &module,
            &function_2,
            &type_args,
            sui_call_args_2,
        )
        .await?;
    let cmd_2 = Command::move_call(package_id, module, function_2, type_args, call_args_2);
    pt_builer.command(cmd_1);
    pt_builer.command(cmd_2);
    let pt = pt_builer.finish();

    let tx_data = TransactionData::new_programmable(signer, vec![gas], pt, 10_000_000, 1000);
    let signed_data = cluster.wallet.sign_transaction(&tx_data);
    let _response = client
        .quorum_driver_api()
        .execute_transaction_block(
            signed_data,
            SuiTransactionBlockResponseOptions::new(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .unwrap();
    // match with None function, the DB should have 2 records, but both points to the same tx
    let filter = TransactionFilter::MoveFunction {
        package: package_id,
        module: Some("pay".to_string()),
        function: None,
    };
    let move_call_query = SuiTransactionBlockResponseQuery::new_with_filter(filter);
    let tx = client
        .read_api()
        .query_transaction_blocks(move_call_query, None, Some(20), true)
        .await
        .unwrap();
    // verify that only 1 tx is returned and no SuiRpcInputError::ContainsDuplicates error
    assert_eq!(1, tx.data.len());
    Ok(())
}
