// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;
use std::{collections::BTreeMap, str::FromStr};

use anyhow::{bail, Context};
use move_core_types::{ident_str, u256::U256};
use serde::Deserialize;
use serde_json::json;
use sui_indexer_alt_e2e_tests::{find_address_owned, FullCluster};
use sui_types::{
    base_types::{ObjectDigest, ObjectID, ObjectRef, SuiAddress},
    crypto::get_account_key_pair,
    effects::TransactionEffectsAPI,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{Transaction, TransactionData},
    TypeTag, SUI_FRAMEWORK_PACKAGE_ID,
};

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    has_previous_page: bool,
    start_cursor: Option<String>,
    end_cursor: Option<String>,
}

async fn objects_page<F>(
    cluster: &FullCluster,
    query: &str,
    variables: serde_json::Value,
    projection: F,
) -> anyhow::Result<(Vec<(String, ObjectRef)>, PageInfo)>
where
    F: Fn(&serde_json::Value) -> Option<&serde_json::Value>,
{
    let client = reqwest::Client::new();
    let url = cluster.graphql_url();

    let response: serde_json::Value = client
        .post(url.as_str())
        .json(&json!({
            "query": query,
            "variables": variables
        }))
        .send()
        .await?
        .json()
        .await?;

    // Check for GraphQL errors
    if let Some(errors) = response.get("errors") {
        bail!("GraphQL errors: {errors}");
    }

    #[derive(Debug, Deserialize)]
    struct Edge {
        cursor: String,
        node: Node,
    }

    #[derive(Debug, Deserialize)]
    struct Node {
        address: String,
        version: u64,
        digest: String,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Connection {
        page_info: PageInfo,
        edges: Vec<Edge>,
    }

    let connection = projection(&response).context("Failed to find objects in response")?;
    let Connection { page_info, edges } = serde_json::from_value(connection.clone())?;

    let objects = edges
        .into_iter()
        .map(|edge| {
            let cursor = edge.cursor;
            let id = ObjectID::from_hex_literal(&edge.node.address)?;
            let version = edge.node.version.into();
            let digest = ObjectDigest::from_str(&edge.node.digest)?;

            Ok((cursor, (id, version, digest)))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok((objects, page_info))
}

/// Paginate through all results for a query
async fn objects<F>(
    cluster: &FullCluster,
    query: &str,
    filter: serde_json::Value,
    page_size: u32,
    forward: bool,
    projection: F,
) -> Result<Vec<(String, ObjectRef)>, anyhow::Error>
where
    F: Fn(&serde_json::Value) -> Option<&serde_json::Value>,
{
    let mut results = vec![];
    let mut cursor: Option<String> = None;

    loop {
        let variables = if forward {
            json!({
                "filter": filter,
                "first": page_size,
                "after": cursor,
                "last": null,
                "before": null,
            })
        } else {
            json!({
                "filter": filter,
                "first": null,
                "after": null,
                "last": page_size,
                "before": cursor,
            })
        };

        let (mut page, page_info) = objects_page(cluster, query, variables, &projection).await?;

        let continue_ = if forward {
            assert_eq!(page_info.has_previous_page, cursor.is_some());
            results.extend(page);
            cursor = page_info.end_cursor;
            page_info.has_next_page
        } else {
            assert_eq!(page_info.has_next_page, cursor.is_some());
            page.reverse();
            results.extend(page);
            cursor = page_info.start_cursor;
            page_info.has_previous_page
        };

        if !continue_ || cursor.is_none() {
            break;
        }
    }

    Ok(results)
}

#[tokio::test]
async fn test_objects_by_type_pagination() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Checkpoint 1: 6 tables, 3 bags
    let mut c1_bags = BTreeMap::new();
    let mut c1_tu8s = BTreeMap::new();
    let mut c1_tu64s = BTreeMap::new();

    for i in 0..3 {
        let bag = create_bag(&mut cluster, b, TypeTag::U8, i);
        c1_bags.insert(bag.0, bag);

        let t8 = create_table(&mut cluster, a, TypeTag::U8, i);
        c1_tu8s.insert(t8.0, t8);

        let t64 = create_table(&mut cluster, a, TypeTag::U64, i);
        c1_tu64s.insert(t64.0, t64);
    }
    cluster.create_checkpoint().await;

    // Checkpoint 2: More tables and some coins
    let mut c2_bags = c1_bags.clone();
    let mut c2_tu8s = c1_tu8s.clone();
    let c2_tu64s = c1_tu64s.clone();

    for i in 3..6 {
        let t8 = create_table(&mut cluster, b, TypeTag::U8, i);
        c2_tu8s.insert(t8.0, t8);

        let bag = create_bag(&mut cluster, a, TypeTag::U64, i);
        c2_bags.insert(bag.0, bag);
    }
    cluster.create_checkpoint().await;

    // Checkpoint 3: More tables
    let c3_bags = c2_bags.clone();
    let c3_tu8s = c2_tu8s.clone();
    let mut c3_tu64s = c2_tu64s.clone();

    for i in 6..9 {
        let t64 = create_table(&mut cluster, a, TypeTag::U64, i);
        c3_tu64s.insert(t64.0, t64);
    }
    cluster.create_checkpoint().await;

    // Wait for GraphQL to fully index checkpoint 3
    cluster
        .wait_for_graphql(3, Duration::from_secs(10))
        .await
        .unwrap();

    const OBJECTS_QUERY: &str = r#"
        query($filter: ObjectFilter!, $first: Int, $last: Int, $after: String, $before: String) {
            objects(filter: $filter, first: $first, last: $last, after: $after, before: $before) {
                pageInfo {
                    hasNextPage
                    hasPreviousPage
                    startCursor
                    endCursor
                }
                edges {
                    cursor
                    node {
                        address
                        version
                        digest
                    }
                }
            }
        }
    "#;

    // Test 1: Paginate all tables
    let forward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "type": "0x2::table::Table" }),
        5,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let backward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "type": "0x2::table::Table" }),
        5,
        false,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let mut expect: Vec<_> = c3_tu8s.values().chain(c3_tu64s.values()).collect();
    assert_eq!(expect, forward.iter().map(|t| &t.1).collect::<Vec<_>>());

    expect.reverse();
    assert_eq!(expect, backward.iter().map(|t| &t.1).collect::<Vec<_>>());

    // Test 2: Type with parameters
    let forward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "type": "0x2::table::Table<u8, u8>" }),
        5,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let backward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "type": "0x2::table::Table<u8, u8>" }),
        5,
        false,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let mut expect: Vec<_> = c3_tu8s.values().collect();
    assert_eq!(expect, forward.iter().map(|t| &t.1).collect::<Vec<_>>());

    expect.reverse();
    assert_eq!(expect, backward.iter().map(|t| &t.1).collect::<Vec<_>>());

    // Test 3: Module filter
    let forward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "type": "0x2::bag" }),
        5,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let backward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "type": "0x2::bag" }),
        5,
        false,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let mut expect: Vec<_> = c3_bags.values().collect();
    assert_eq!(expect, forward.iter().map(|t| &t.1).collect::<Vec<_>>());

    expect.reverse();
    assert_eq!(expect, backward.iter().map(|t| &t.1).collect::<Vec<_>>());

    // Fetch all the tables, with their cursors
    let all_tu8s = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({"type": "0x2::table::Table<u8, u8>"}),
        10,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let lo = 2;
    let hi = 5;

    // Test 4: last + unnecessary after
    let (page, page_info) = objects_page(
        &cluster,
        OBJECTS_QUERY,
        json!({
            "filter": { "type": "0x2::table::Table<u8, u8>" },
            "first": null,
            "after": all_tu8s[lo].0,
            "last": 3,
            "before": null,
        }),
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    assert_eq!(page, &all_tu8s[all_tu8s.len() - 3..]);
    assert!(page_info.has_previous_page);
    assert!(!page_info.has_next_page);

    // Test 5: last + after
    let (page, page_info) = objects_page(
        &cluster,
        OBJECTS_QUERY,
        json!({
            "filter": { "type": "0x2::table::Table<u8, u8>" },
            "first": null,
            "after": all_tu8s[lo].0,
            "last": all_tu8s.len(),
            "before": null,
        }),
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    assert_eq!(page, all_tu8s[lo + 1..]);
    assert!(page_info.has_previous_page);
    assert!(!page_info.has_next_page);

    // Test 6: first + unnecessary before
    let (page, page_info) = objects_page(
        &cluster,
        OBJECTS_QUERY,
        json!({
            "filter": { "type": "0x2::table::Table<u8, u8>" },
            "first": 3,
            "after": null,
            "last": null,
            "before": all_tu8s[hi].0,
        }),
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    assert_eq!(page, all_tu8s[..3]);
    assert!(!page_info.has_previous_page);
    assert!(page_info.has_next_page);

    // Test 6: first + before
    let (page, page_info) = objects_page(
        &cluster,
        OBJECTS_QUERY,
        json!({
            "filter": { "type": "0x2::table::Table<u8, u8>" },
            "first": all_tu8s.len(),
            "after": null,
            "last": null,
            "before": all_tu8s[hi].0,
        }),
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    assert_eq!(page, all_tu8s[..hi]);
    assert!(!page_info.has_previous_page);
    assert!(page_info.has_next_page);

    // Test 7: after + before
    let (page, page_info) = objects_page(
        &cluster,
        OBJECTS_QUERY,
        json!({
            "filter": { "type": "0x2::table::Table<u8, u8>" },
            "first": all_tu8s.len(),
            "after": all_tu8s[lo].0,
            "last": null,
            "before": all_tu8s[hi].0,
        }),
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    assert_eq!(page, all_tu8s[lo + 1..hi]);
    assert!(page_info.has_previous_page);
    assert!(page_info.has_next_page);

    // Test 9: Checkpoint query
    const CHECKPOINT_QUERY: &str = r#"
        query($filter: ObjectFilter!, $first: Int, $last: Int, $after: String, $before: String) {
            checkpoint(sequenceNumber: 1) {
                query {
                    objects(filter: $filter, first: $first, last: $last, after: $after, before: $before) {
                        pageInfo {
                            hasNextPage
                            hasPreviousPage
                            startCursor
                            endCursor
                        }
                        edges {
                            cursor
                            node {
                                address
                                version
                                digest
                            }
                        }
                    }
                }
            }
        }
    "#;

    let forward = objects(
        &cluster,
        CHECKPOINT_QUERY,
        json!({ "type": "0x2::table::Table" }),
        5,
        true,
        |v| v.pointer("/data/checkpoint/query/objects"),
    )
    .await
    .unwrap();

    let backward = objects(
        &cluster,
        CHECKPOINT_QUERY,
        json!({ "type": "0x2::table::Table" }),
        5,
        false,
        |v| v.pointer("/data/checkpoint/query/objects"),
    )
    .await
    .unwrap();

    let mut expect: Vec<_> = c1_tu8s.values().chain(c1_tu64s.values()).collect();
    assert_eq!(expect, forward.iter().map(|t| &t.1).collect::<Vec<_>>());

    expect.reverse();
    assert_eq!(expect, backward.iter().map(|t| &t.1).collect::<Vec<_>>());

    // Test 11: Verify no results for non-existent type
    let (no_objects, no_page) = objects_page(
        &cluster,
        OBJECTS_QUERY,
        json!({
            "filter": { "type": "0x2::table::Table<u8, u64>" },
            "first": 50,
            "after": null,
            "last": null,
            "before": null,
        }),
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    assert!(no_objects.is_empty());
    assert!(!no_page.has_next_page);
    assert!(!no_page.has_previous_page);
}

/// Run a transaction on `cluster` signed by a fresh funded account that creates a `Bag`
/// owned by `owner` with `size` many elements.
fn create_bag(cluster: &mut FullCluster, owner: SuiAddress, ty: TypeTag, size: u64) -> ObjectRef {
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
        let kv = match &ty {
            TypeTag::U8 => builder.pure(i as u8),
            TypeTag::U16 => builder.pure(i as u16),
            TypeTag::U32 => builder.pure(i as u32),
            TypeTag::U64 => builder.pure(i),
            TypeTag::U128 => builder.pure(i as u128),
            TypeTag::U256 => builder.pure(U256::from(i)),
            _ => panic!("Unsupported type for bag: {ty}"),
        }
        .expect("Failed to create pure value");

        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("bag").to_owned(),
            ident_str!("add").to_owned(),
            vec![ty.clone(), ty.clone()],
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
    find_address_owned(&fx).expect("Failed to find created bag")
}

/// Run a transaction on `cluster` signed by a fresh funded account that creates a `Table<ty, ty>`
/// owned by `owner` with `size` many elements.
fn create_table(cluster: &mut FullCluster, owner: SuiAddress, ty: TypeTag, size: u64) -> ObjectRef {
    let (sender, kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET)
        .expect("Failed to fund account");

    let mut builder = ProgrammableTransactionBuilder::new();

    let table = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("table").to_owned(),
        ident_str!("new").to_owned(),
        vec![ty.clone(), ty.clone()],
        vec![],
    );

    for i in 0..size {
        let kv = match &ty {
            TypeTag::U8 => builder.pure(i as u8),
            TypeTag::U16 => builder.pure(i as u16),
            TypeTag::U32 => builder.pure(i as u32),
            TypeTag::U64 => builder.pure(i),
            TypeTag::U128 => builder.pure(i as u128),
            TypeTag::U256 => builder.pure(U256::from(i)),
            _ => panic!("Unsupported type for table: {ty}"),
        }
        .expect("Failed to create pure value");

        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("table").to_owned(),
            ident_str!("add").to_owned(),
            vec![ty.clone(), ty.clone()],
            vec![table, kv, kv],
        );
    }

    builder.transfer_arg(owner, table);

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

    assert!(fx.status().is_ok(), "create table transaction failed");
    find_address_owned(&fx).expect("Failed to find created table")
}
