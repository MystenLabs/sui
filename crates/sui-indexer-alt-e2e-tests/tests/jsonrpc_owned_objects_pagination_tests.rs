// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::str::FromStr;

use move_core_types::ident_str;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use simulacrum::Simulacrum;
use sui_indexer_alt_jsonrpc::config::ObjectsConfig;
use sui_indexer_alt_jsonrpc::config::RpcConfig as JsonRpcConfig;
use sui_json_rpc_types::Page;
use sui_json_rpc_types::SuiObjectResponse;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::TypeTag;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::get_account_key_pair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;

use sui_indexer_alt_e2e_tests::FullCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_e2e_tests::find;

const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

#[derive(Deserialize)]
struct Response {
    result: Page<SuiObjectResponse, String>,
}

/// Paginate through all owned objects (no filter) using cursors from responses.
#[tokio::test]
async fn test_owned_objects_pagination_limit_and_cursor() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (owner, _) = get_account_key_pair();

    for i in 0..5 {
        create_bag(&mut cluster, owner, i);
    }

    cluster.create_checkpoint().await;

    let Response {
        result: Page {
            data, next_cursor, ..
        },
    } = owned_objects(&cluster, owner, json!(null), None, 2).await;

    assert_eq!(data.len(), 2);

    let Response {
        result: Page {
            data,
            has_next_page,
            ..
        },
    } = owned_objects(&cluster, owner, json!(null), next_cursor, 3).await;

    assert_eq!(data.len(), 3);
    assert!(!has_next_page);
}

/// Simple test combining filtering by type params with cursors.
#[tokio::test]
async fn test_owned_objects_pagination_by_type_params() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (owner, _) = get_account_key_pair();

    for i in 0..5 {
        create_bag(&mut cluster, owner, i);
        create_coin(&mut cluster, owner, i);
        create_coin(&mut cluster, owner, i + 1);
    }

    cluster.create_checkpoint().await;

    let Response {
        result: Page {
            data, next_cursor, ..
        },
    } = owned_objects(
        &cluster,
        owner,
        json!({"StructType": "0x2::coin::Coin<0x2::sui::SUI>"}),
        None,
        2,
    )
    .await;

    assert_eq!(data.len(), 2);

    let Response {
        result: Page {
            data,
            has_next_page,
            ..
        },
    } = owned_objects(
        &cluster,
        owner,
        json!({"StructType": "0x2::coin::Coin<0x2::sui::SUI>"}),
        next_cursor,
        8,
    )
    .await;

    assert_eq!(data.len(), 8);
    assert!(!has_next_page);
}

// /// Paginate through owned objects with a type filter using cursors from responses.
// #[tokio::test]
// async fn test_owned_objects_pagination_with_type_filter() {
//     let mut cluster = setup_cluster(ObjectsConfig::default()).await;
//     let (owner, _) = get_account_key_pair();

//     let mut expect_bags = BTreeSet::new();
//     for i in 0..6 {
//         expect_bags.insert(create_bag(&mut cluster, owner, i));
//         create_coin(&mut cluster, owner, i);
//     }
//     cluster.create_checkpoint().await;

//     let mut actual = BTreeSet::new();
//     let mut cursor = None;

//     loop {
//         let Response { result } = owned_objects(
//             &cluster,
//             owner,
//             json!({ "StructType": "0x2::bag::Bag" }),
//             cursor,
//             2,
//         )
//         .await;

//         let types: BTreeSet<_> = result.data.iter().map(|o| o.data.type_.as_str()).collect();
//         if !result.data.is_empty() {
//             assert_eq!(types, BTreeSet::from_iter(["0x2::bag::Bag"]));
//         }

//         actual.extend(result.data.into_iter().map(|o| o.data.unwrap().object_id));

//         cursor = result.next_cursor;
//         if !result.has_next_page || cursor.is_none() {
//             break;
//         }
//     }

//     assert_eq!(actual, expect_bags);
// }

// Paginate through owned objects with an exclusion filter using cursors from responses.
// #[tokio::test]
// async fn test_owned_objects_pagination_with_exclusion() {
//     let mut cluster = setup_cluster(ObjectsConfig::default()).await;
//     let (owner, _) = get_account_key_pair();

//     let mut expect_bags = BTreeSet::new();
//     for i in 0..6 {
//         expect_bags.insert(create_bag(&mut cluster, owner, i));
//         create_coin(&mut cluster, owner, i);
//     }
//     cluster.create_checkpoint().await;

//     let mut actual = BTreeSet::new();
//     let mut cursor = None;

//     loop {
//         let Response { result } = owned_objects(
//             &cluster,
//             owner,
//             json!({
//                 "MatchNone": [{ "StructType": "0x2::coin::Coin" }]
//             }),
//             cursor,
//             2,
//         )
//         .await;

//         let types: BTreeSet<_> = result
//             .data
//             .iter()
//             .map(|o| o.data.unwrap().type_.unwrap()
//             .collect();
//         if !result.data.is_empty() {
//             assert_eq!(types, BTreeSet::from_iter(["0x2::bag::Bag"]));
//         }

//         actual.extend(result.data.into_iter().map(|o| o.data.unwrap().object_id));

//         cursor = result.next_cursor;
//         if !result.has_next_page || cursor.is_none() {
//             break;
//         }
//     }

//     assert_eq!(actual, expect_bags);
// }

fn create_coin(cluster: &mut FullCluster, owner: SuiAddress, amount: u64) -> ObjectID {
    let (sender, kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET + amount)
        .expect("Failed to fund account");

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(owner, Some(amount));

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .expect("Failed to execute transaction");

    assert!(fx.status().is_ok(), "create coin transaction failed");

    find::address_owned(&fx)
        .expect("Failed to find created coin")
        .0
}

fn create_bag(cluster: &mut FullCluster, owner: SuiAddress, size: u64) -> ObjectID {
    let (sender, kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET)
        .expect("Failed to fund account");

    let mut builder = ProgrammableTransactionBuilder::new();

    let bag = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("bag").to_owned(),
        ident_str!("new").to_owned(),
        vec![],
        vec![],
    );

    for i in 0..size {
        let kv = builder.pure(i).expect("Failed to create pure value");
        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("bag").to_owned(),
            ident_str!("add").to_owned(),
            vec![TypeTag::U64, TypeTag::U64],
            vec![bag, kv, kv],
        );
    }

    builder.transfer_arg(owner, bag);

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .expect("Failed to execute transaction");

    assert!(fx.status().is_ok(), "create bag transaction failed");

    find::address_owned(&fx)
        .expect("Failed to find created bag")
        .0
}

async fn owned_objects(
    cluster: &FullCluster,
    owner: SuiAddress,
    filter: Value,
    cursor: Option<String>,
    limit: usize,
) -> Response {
    let query = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "suix_getOwnedObjects",
        "params": [
            owner.to_string(),
            {
                "filter": filter,
                "options": { "showType": true },
            },
            cursor,
            limit,
        ],
    });

    Client::new()
        .post(cluster.jsonrpc_url())
        .json(&query)
        .send()
        .await
        .expect("Request to JSON-RPC server failed")
        .json()
        .await
        .expect("Failed to parse JSON-RPC response")
}
