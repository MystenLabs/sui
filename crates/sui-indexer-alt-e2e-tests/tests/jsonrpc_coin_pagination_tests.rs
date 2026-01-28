// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;
use serde_json::json;
use sui_json_rpc_types::Coin;
use sui_json_rpc_types::Page;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::get_account_key_pair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::GAS;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::GasData;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionKind;

use sui_indexer_alt_e2e_tests::FullCluster;
use sui_indexer_alt_e2e_tests::find;

const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

/// Deserialized successful JSON-RPC response for `suix_getCoins`.
#[derive(Deserialize)]
struct Response {
    result: Page<Coin, String>,
}

#[tokio::test]
async fn test_coins_in_desc_balance_order() {
    // SUI coin is available from genesis, no need to publish
    let mut cluster = FullCluster::new().await.unwrap();
    let with_prefix = true;
    let gas_type = GAS::type_().to_canonical_string(with_prefix);
    let (a, _) = get_account_key_pair();

    create_coin(&mut cluster, a, 1);
    create_coin(&mut cluster, a, 2);
    create_coin(&mut cluster, a, 3);

    cluster.create_checkpoint().await;

    let Response {
        result: Page {
            data,
            has_next_page,
            ..
        },
    } = get_coins(&cluster, a, &gas_type, None, 10).await;

    // Check coins returned in descending order by balance
    let balances: Vec<u64> = data.iter().map(|coin| coin.balance).collect();
    assert_eq!(balances, vec![3, 2, 1]);
    assert!(!has_next_page);
}

/// Test to demonstrate inconsistency for suix_getCoins. Given a cursor pointing to the 100_000
/// coin, the next query using the cursor will return the coin at its latest state instead of the
/// same 100_000 coin balance.
#[tokio::test]
async fn test_coins_pagination_and_inconsistency() {
    // SUI coin is available from genesis, no need to publish
    let mut cluster = FullCluster::new().await.unwrap();
    let with_prefix = true;
    let gas_type = GAS::type_().to_canonical_string(with_prefix);
    let (a, akp) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    create_coin(&mut cluster, a, 300_000);
    create_coin(&mut cluster, a, 200_000);
    create_coin(&mut cluster, a, 100_000);
    create_coin(&mut cluster, a, 10_000);

    cluster.create_checkpoint().await;

    // Retrieve the object ref of the coin with 100_000
    let Response {
        result: Page { data, .. },
    } = get_coins(&cluster, a, &gas_type, None, 10).await;
    let to_mutate = data
        .iter()
        .find(|coin| coin.balance == 100_000)
        .expect("Failed to find coin to mutate")
        .object_ref();

    // Paginate again from beginning, with limit so 100_000 coin is not included in the first page
    let Response {
        result: Page {
            has_next_page,
            next_cursor,
            ..
        },
    } = get_coins(&cluster, a, &gas_type, None, 2).await;
    assert!(has_next_page);

    // Test paginating works
    let Response {
        result: Page {
            data,
            has_next_page,
            ..
        },
    } = get_coins(&cluster, a, &gas_type, next_cursor.clone(), 2).await;

    let balances: Vec<u64> = data.iter().map(|coin| coin.balance).collect();
    assert_eq!(balances, vec![100_000, 10_000]);
    assert!(!has_next_page);

    // Split off some of A's gas and transfer it to B using a sponsored transaction
    let (sponsor, sponsor_kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET)
        .expect("Failed to fund sponsor account");

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.split_coin(b, to_mutate, vec![40_000]);

    let gas_data = GasData {
        payment: vec![gas],
        owner: sponsor,
        price: cluster.reference_gas_price(),
        budget: DEFAULT_GAS_BUDGET,
    };

    let data = TransactionData::new_with_gas_data(
        TransactionKind::ProgrammableTransaction(builder.finish()),
        a,
        gas_data,
    );

    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(
            data,
            vec![&akp, &sponsor_kp],
        ))
        .expect("Failed to execute split transaction");

    assert!(fx.status().is_ok(), "split coin transaction failed");

    cluster.create_checkpoint().await;

    // Paginate again using the previous cursor, should return the coin with value 100_000 - 40_000
    // = 60_000
    let Response {
        result: Page {
            data,
            has_next_page,
            ..
        },
    } = get_coins(&cluster, a, &gas_type, next_cursor, 2).await;
    let balances: Vec<u64> = data.iter().map(|coin| coin.balance).collect();
    assert_eq!(balances, vec![60_000, 10_000]);
    assert!(!has_next_page);
}

#[tokio::test]
async fn test_coins_pagination_and_creation() {
    let mut cluster = FullCluster::new().await.unwrap();
    let with_prefix = true;
    let gas_type = GAS::type_().to_canonical_string(with_prefix);
    let (a, _) = get_account_key_pair();

    create_coin(&mut cluster, a, 300_000);
    create_coin(&mut cluster, a, 100_000);

    cluster.create_checkpoint().await;

    let Response {
        result:
            Page {
                data,
                has_next_page,
                next_cursor,
            },
    } = get_coins(&cluster, a, &gas_type, None, 1).await;
    let balances: Vec<u64> = data.iter().map(|coin| coin.balance).collect();

    assert!(has_next_page);
    assert_eq!(balances, vec![300_000]);

    create_coin(&mut cluster, a, 200_000);
    cluster.create_checkpoint().await;

    let Response {
        result: Page {
            data,
            has_next_page,
            ..
        },
    } = get_coins(&cluster, a, &gas_type, next_cursor, 2).await;
    let balances: Vec<u64> = data.iter().map(|coin| coin.balance).collect();
    assert_eq!(balances, vec![200_000, 100_000]);
    assert!(!has_next_page);
}

/// Run a transaction on `cluster` signed by a fresh funded account that sends a coin with value
/// `amount` to `owner`.
fn create_coin(cluster: &mut FullCluster, owner: SuiAddress, amount: u64) -> ObjectRef {
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
    find::address_owned(&fx).expect("Failed to find created coin")
}

async fn get_coins(
    cluster: &FullCluster,
    owner: SuiAddress,
    coin_type: &str,
    cursor: Option<String>,
    limit: usize,
) -> Response {
    let query = json!({
        "jsonrpc": "2.0",
        "method": "suix_getCoins",
        "params": [
            owner.to_string(),
            coin_type,
            cursor,
            limit,
        ],
        "id": 1
    });

    reqwest::Client::new()
        .post(cluster.jsonrpc_url().as_str())
        .json(&query)
        .send()
        .await
        .expect("Request to JSON-RPC server failed")
        .json()
        .await
        .expect("Failed to parse JSON-RPC response")
}
