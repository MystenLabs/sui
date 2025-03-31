// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, str::FromStr};

use move_core_types::ident_str;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use simulacrum::Simulacrum;
use sui_indexer_alt::config::IndexerConfig;
use sui_indexer_alt_e2e_tests::{find_address_owned, FullCluster};
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_jsonrpc::{
    args::SystemPackageTaskArgs,
    config::{ObjectsConfig, RpcConfig},
};
use sui_json_rpc_types::Page;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    crypto::get_account_key_pair,
    effects::TransactionEffectsAPI,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{Transaction, TransactionData},
    TypeTag, SUI_FRAMEWORK_PACKAGE_ID,
};
use tokio_util::sync::CancellationToken;

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

/// Deserialized successful JSON-RPC response for `suix_getOwnedObjects`.
#[derive(Deserialize)]
struct Response {
    result: Page<Object, String>,
}

#[derive(Deserialize)]
struct Object {
    data: Data,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    object_id: String,
    type_: String,
}

/// Running a compound filter query where the first fetch contains all the information to return a
/// response.
#[tokio::test]
async fn test_simple() {
    let mut cluster = setup_cluster(ObjectsConfig::default()).await;
    let (owner, _) = get_account_key_pair();

    let expect: BTreeSet<_> = (0..4).map(|i| create_bag(&mut cluster, owner, i)).collect();
    cluster.create_checkpoint().await;

    let Response {
        result: Page {
            data,
            has_next_page,
            ..
        },
    } = owned_objects(
        &cluster,
        owner,
        json!({
            "MatchNone": [{ "StructType": "0x2::coin::Coin" }]
        }),
        None,
        3,
    )
    .await;

    let types: BTreeSet<_> = data.iter().map(|o| o.data.type_.as_str()).collect();
    let actual = data
        .iter()
        .map(|o| ObjectID::from_str(&o.data.object_id))
        .collect::<Result<BTreeSet<_>, _>>()
        .unwrap();

    assert_eq!(actual.len(), 3);
    assert!(actual.is_subset(&expect));
    assert_eq!(types, BTreeSet::from_iter(["0x2::bag::Bag"]));
    assert!(has_next_page);
}

/// Running a compound filter query where multiple fetches are required to get enough information
/// to return a response.
#[tokio::test]
async fn test_multi_fetch() {
    let mut cluster = setup_cluster(ObjectsConfig {
        filter_scan_size: 6,
        ..Default::default()
    })
    .await;

    let (owner, _) = get_account_key_pair();
    let mut expect = BTreeSet::new();

    // Create one non-Coin objects and five Coin objects (which will be filtered out) in each
    // checkpoint.
    for i in 0..6 {
        expect.insert(create_bag(&mut cluster, owner, i));
        create_coin(&mut cluster, owner, 5 * i);
        create_coin(&mut cluster, owner, 5 * i + 1);
        create_coin(&mut cluster, owner, 5 * i + 2);
        create_coin(&mut cluster, owner, 5 * i + 3);
        create_coin(&mut cluster, owner, 5 * i + 4);
        cluster.create_checkpoint().await;
    }

    let Response {
        result: Page {
            data,
            has_next_page,
            ..
        },
    } = owned_objects(
        &cluster,
        owner,
        json!({
            "MatchNone": [{ "StructType": "0x2::coin::Coin" }]
        }),
        None,
        5,
    )
    .await;

    let types: BTreeSet<_> = data.iter().map(|o| o.data.type_.as_str()).collect();
    let actual = data
        .iter()
        .map(|o| ObjectID::from_str(&o.data.object_id))
        .collect::<Result<BTreeSet<_>, _>>()
        .unwrap();

    assert_eq!(actual.len(), 5);
    assert!(actual.is_subset(&expect));
    assert_eq!(types, BTreeSet::from_iter(["0x2::bag::Bag"]));
    assert!(has_next_page);
}

/// There are too few results in total to return a full page, so we end up fetching all the owned
/// objects to make sure.
#[tokio::test]
async fn test_too_few_results() {
    let mut cluster = setup_cluster(ObjectsConfig {
        filter_scan_size: 6,
        ..Default::default()
    })
    .await;

    let (owner, _) = get_account_key_pair();

    let expect: BTreeSet<_> = (0..4).map(|i| create_bag(&mut cluster, owner, i)).collect();

    for i in 0..8 {
        create_coin(&mut cluster, owner, i);
    }

    cluster.create_checkpoint().await;

    // Ask for too many results
    let Response {
        result: Page {
            data,
            has_next_page,
            ..
        },
    } = owned_objects(
        &cluster,
        owner,
        json!({
            "MatchNone": [{ "StructType": "0x2::coin::Coin" }]
        }),
        None,
        5,
    )
    .await;

    let types: BTreeSet<_> = data.iter().map(|o| o.data.type_.as_str()).collect();
    let actual = data
        .iter()
        .map(|o| ObjectID::from_str(&o.data.object_id))
        .collect::<Result<BTreeSet<_>, _>>()
        .unwrap();

    assert_eq!(actual, expect);
    assert_eq!(types, BTreeSet::from_iter(["0x2::bag::Bag"]));
    assert!(!has_next_page);

    // Ask for just the right number as well. This still requires fetching all the owned objects,
    // to confirm that there are no more matching results.
    let Response {
        result: Page {
            data,
            has_next_page,
            ..
        },
    } = owned_objects(
        &cluster,
        owner,
        json!({
            "MatchNone": [{ "StructType": "0x2::coin::Coin" }]
        }),
        None,
        4,
    )
    .await;

    let types: BTreeSet<_> = data.iter().map(|o| o.data.type_.as_str()).collect();
    let actual = data
        .iter()
        .map(|o| ObjectID::from_str(&o.data.object_id))
        .collect::<Result<BTreeSet<_>, _>>()
        .unwrap();

    assert_eq!(actual, expect);
    assert_eq!(types, BTreeSet::from_iter(["0x2::bag::Bag"]));
    assert!(!has_next_page);
}

/// There are no matching results (again, we fetch all the owned objects to make sure).
#[tokio::test]
async fn test_no_results() {
    let mut cluster = setup_cluster(ObjectsConfig::default()).await;
    let (owner, _) = get_account_key_pair();

    for i in 0..10 {
        create_coin(&mut cluster, owner, i);
    }

    cluster.create_checkpoint().await;

    let Response {
        result: Page {
            data,
            has_next_page,
            ..
        },
    } = owned_objects(
        &cluster,
        owner,
        json!({
            "MatchNone": [{ "StructType": "0x2::coin::Coin" }]
        }),
        None,
        5,
    )
    .await;

    let actual = data
        .iter()
        .map(|o| ObjectID::from_str(&o.data.object_id))
        .collect::<Result<BTreeSet<_>, _>>()
        .unwrap();

    assert!(actual.is_empty());
    assert!(!has_next_page);
}

/// The next cursor for the overall query may point in the middle of one of the extra batches that
/// gets fetched. Make sure that in that case, we correctly discard intermediate results and fix up
/// the next cursor.
#[tokio::test]
async fn test_next_cursor() {
    let mut cluster = setup_cluster(ObjectsConfig {
        filter_scan_size: 5,
        ..Default::default()
    })
    .await;

    let (owner, _) = get_account_key_pair();

    let mut expect = BTreeSet::new();

    // Create one Coin object (which will be filtered out) and 5 non-Coin objects in each
    // checkpoint, for a total of 30 result rows.
    for i in 0..6 {
        create_coin(&mut cluster, owner, i);
        expect.insert(create_bag(&mut cluster, owner, 5 * i));
        expect.insert(create_bag(&mut cluster, owner, 5 * i + 1));
        expect.insert(create_bag(&mut cluster, owner, 5 * i + 2));
        expect.insert(create_bag(&mut cluster, owner, 5 * i + 3));
        expect.insert(create_bag(&mut cluster, owner, 5 * i + 4));
        cluster.create_checkpoint().await;
    }

    // Fetch pages 6 results at a time. The presence of the Coin object would mean that we always
    // need to issue one extra fetch to get enough results, and the cursor would point somewhere in
    // the middle of that fetch.
    let mut actual = BTreeSet::new();
    let mut cursor = None;

    loop {
        let Response { result } = owned_objects(
            &cluster,
            owner,
            json!({
                "MatchNone": [{ "StructType": "0x2::coin::Coin" }]
            }),
            cursor,
            6,
        )
        .await;

        actual.extend(
            result
                .data
                .into_iter()
                .map(|o| ObjectID::from_str(&o.data.object_id).unwrap()),
        );

        cursor = result.next_cursor;
        if !result.has_next_page || cursor.is_none() {
            break;
        }
    }

    assert_eq!(actual, expect);
}

/// Create a new full cluster with the given objects config.
async fn setup_cluster(config: ObjectsConfig) -> FullCluster {
    FullCluster::new_with_configs(
        Simulacrum::new(),
        IndexerArgs::default(),
        SystemPackageTaskArgs::default(),
        IndexerConfig::example(),
        RpcConfig {
            objects: config,
            ..RpcConfig::default()
        },
        &prometheus::Registry::new(),
        CancellationToken::new(),
    )
    .await
    .expect("Failed to set-up cluster")
}

/// Run a transaction on `cluster` signed by a fresh funded account that sends a coin with value
/// `amount` to `owner`.
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

    find_address_owned(&fx)
        .expect("Failed to find created coin")
        .0
}

/// Run a transaction on `cluster` signed by a fresh funded account that creates a `Bag<u64, u64>`
/// owned by `owner` with `size` many elements. The purpose of this is to create an object that
/// isn't a coin.
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

    find_address_owned(&fx)
        .expect("Failed to find created coin")
        .0
}

/// Make a call to `suix_getOwnedObjects` to the RPC server running on `cluster`.
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
        .post(cluster.rpc_url())
        .json(&query)
        .send()
        .await
        .expect("Request to JSON-RPC server failed")
        .json()
        .await
        .expect("Failed to parse JSON-RPC response")
}
