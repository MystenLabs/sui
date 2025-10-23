// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, mem, str::FromStr};

use anyhow::{Context, bail};
use move_core_types::{ident_str, language_storage::StructTag, u256::U256};
use serde::Deserialize;
use serde_json::json;
use sui_indexer_alt_e2e_tests::{FullCluster, find};
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID, TypeTag,
    base_types::{ObjectDigest, ObjectID, ObjectRef, SuiAddress},
    crypto::{Signature, Signer, get_account_key_pair},
    effects::{TransactionEffects, TransactionEffectsAPI},
    object::Owner,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        Argument, Command, GasData, ObjectArg, Transaction, TransactionData, TransactionKind,
    },
};

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

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

#[tokio::test]
async fn test_objects_by_address_owner() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, akp) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Checkpoint 1: Create coins with different balances and non-coin objects
    // Create address-owned coins with specific balances (should be returned in descending order)
    let c150 = create_coin(&mut cluster, a, 150);
    let c100 = create_coin(&mut cluster, a, 100);
    let c075 = create_coin(&mut cluster, a, 75);
    let c050 = create_coin(&mut cluster, a, 50);
    let c025 = create_coin(&mut cluster, a, 25);

    // Create non-coin objects for account A
    let atu8 = create_table(&mut cluster, a, TypeTag::U8, 3);
    let abu64 = create_bag(&mut cluster, a, TypeTag::U64, 2);

    // Create objects for account B
    let c200 = create_coin(&mut cluster, b, 200);
    let btu64 = create_table(&mut cluster, b, TypeTag::U64, 2);

    cluster.create_checkpoint().await;

    // Test 1: Query all objects owned by A - coins should be in descending balance order
    let forward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "ADDRESS", "owner": a }),
        3,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let backward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "ADDRESS", "owner": a }),
        3,
        false,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    // The actual order depends on the type hierarchy and internal ordering
    // Bags come first, then coins, then tables
    let mut expected = vec![abu64, c150, c100, c075, c050, c025, atu8];
    assert_eq!(expected, forward.iter().map(|t| t.1).collect::<Vec<_>>());

    expected.reverse();
    assert_eq!(expected, backward.iter().map(|t| t.1).collect::<Vec<_>>());

    // Test 2: Query only coins owned by A with type filter
    let forward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "ADDRESS", "owner": a, "type": "0x2::coin::Coin" }),
        3,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let expected = vec![c150, c100, c075, c050, c025];
    assert_eq!(expected, forward.iter().map(|t| t.1).collect::<Vec<_>>());

    // Test 3: Query all objects owned by B
    let forward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "ADDRESS", "owner": b }),
        5,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let expected = vec![c200, btu64];
    assert_eq!(expected, forward.iter().map(|t| t.1).collect::<Vec<_>>());

    // Checkpoint 2: Transfer some objects from A to B
    let bc150 = transfer_object(&mut cluster, a, &akp, c150, b)
        .mutated()
        .into_iter()
        .find(|(obj, _)| obj.0 == c150.0)
        .map(|(obj, _)| obj)
        .unwrap();

    let bc075 = transfer_object(&mut cluster, a, &akp, c075, b)
        .mutated()
        .into_iter()
        .find(|(obj, _)| obj.0 == c075.0)
        .map(|(obj, _)| obj)
        .unwrap();

    cluster.create_checkpoint().await;

    // Test 4: Query A's objects after transfers - should have 100, 50, 25 coins
    let forward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "ADDRESS", "owner": a }),
        5,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let expected = vec![abu64, c100, c050, c025, atu8];
    assert_eq!(expected, forward.iter().map(|t| t.1).collect::<Vec<_>>());

    // Test 5: Query B's objects after transfers - should have 200, 150, 75 coins
    let forward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "ADDRESS", "owner": b, "type": "0x2::coin::Coin" }),
        5,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let expected = vec![c200, bc150, bc075];
    assert_eq!(expected, forward.iter().map(|t| t.1).collect::<Vec<_>>());

    // Checkpoint 3: Destroy coins
    destroy_coin(&mut cluster, a, &akp, c100);
    destroy_coin(&mut cluster, a, &akp, c050);

    cluster.create_checkpoint().await;

    // Test 6: Query A's objects after destroying coins - 100 and 50 coins should be deleted
    let forward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "ADDRESS", "owner": a }),
        5,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let expected = vec![abu64, c025, atu8];
    assert_eq!(expected, forward.iter().map(|t| t.1).collect::<Vec<_>>());

    // Test 7: Time-travel - Query A's objects at checkpoint 1 (before transfers and destroys)
    let forward = objects(
        &cluster,
        CHECKPOINT_QUERY,
        json!({ "ownerKind": "ADDRESS", "owner": a }),
        10,
        true,
        |v| v.pointer("/data/checkpoint/query/objects"),
    )
    .await
    .unwrap();

    let expected = vec![abu64, c150, c100, c075, c050, c025, atu8];
    assert_eq!(expected, forward.iter().map(|t| t.1).collect::<Vec<_>>());

    let all_as = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "ADDRESS", "owner": a }),
        10,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let lo = 0;
    let hi = 2;

    // Test 8: after + before
    let (page, page_info) = objects_page(
        &cluster,
        OBJECTS_QUERY,
        json!({
            "filter": { "ownerKind": "ADDRESS", "owner": a },
            "first": all_as.len(),
            "after": all_as[lo].0,
            "last": null,
            "before": all_as[hi].0,
        }),
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    assert_eq!(&page, &all_as[lo + 1..hi]);
    assert!(page_info.has_previous_page);
    assert!(page_info.has_next_page);
}

#[tokio::test]
async fn test_objects_by_object_owner() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, _a_kp) = get_account_key_pair();

    // Checkpoint 1: Create a table and add entries (which become dynamic field objects)
    // The table itself is owned by address A
    let table = create_table(&mut cluster, a, TypeTag::U64, 5);

    cluster.create_checkpoint().await;

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

    // Test 1: Query objects owned by the table (object ownership)
    // Tables create dynamic field objects when entries are added
    let forward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "OBJECT", "owner": table.0 }),
        3,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let backward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "OBJECT", "owner": table.0 }),
        3,
        false,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    // Tables with 5 entries should have 5 dynamic field objects
    assert_eq!(5, forward.len());
    assert_eq!(5, backward.len());

    // Verify forward and backward pagination return same objects in reverse order
    let mut backward_refs: Vec<_> = backward.iter().map(|t| t.1).collect();
    backward_refs.reverse();
    assert_eq!(
        forward.iter().map(|t| t.1).collect::<Vec<_>>(),
        backward_refs
    );

    // Test 2: Time-travel - Query table's objects at checkpoint 1 (original state)
    let checkpoint1_objects = objects(
        &cluster,
        CHECKPOINT_QUERY,
        json!({ "ownerKind": "OBJECT", "owner": table.0 }),
        10,
        true,
        |v| v.pointer("/data/checkpoint/query/objects"),
    )
    .await
    .unwrap();

    // At checkpoint 1, should have original 5 entries
    assert_eq!(5, checkpoint1_objects.len());
}

#[tokio::test]
async fn test_shared_and_immutable_objects() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Checkpoint 1: Create regular owned tables for comparison, and shared tables
    let atu8 = create_table(&mut cluster, a, TypeTag::U8, 1);
    let btu64 = create_table(&mut cluster, b, TypeTag::U64, 2);

    // Create shared tables
    let mut stu8_0 = create_shared_table(&mut cluster, TypeTag::U8, 3);
    let stu64 = create_shared_table(&mut cluster, TypeTag::U64, 4);
    let mut stu8_1 = create_shared_table(&mut cluster, TypeTag::U8, 5);

    if stu8_0.0 > stu8_1.0 {
        mem::swap(&mut stu8_0, &mut stu8_1);
    }

    cluster.create_checkpoint().await;

    // Checkpoint 2: Create immutable tables
    let mut itu8_0 = create_immutable_table(&mut cluster, TypeTag::U8, 6);
    let itu64 = create_immutable_table(&mut cluster, TypeTag::U64, 7);
    let mut itu8_1 = create_immutable_table(&mut cluster, TypeTag::U8, 8);

    if itu8_0.0 > itu8_1.0 {
        mem::swap(&mut itu8_0, &mut itu8_1);
    }

    cluster.create_checkpoint().await;

    // Test 1: Query all shared tables (unqualified type)
    let forward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "SHARED", "type": "0x2::table::Table" }),
        2,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let backward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "SHARED", "type": "0x2::table::Table" }),
        2,
        false,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let mut expected = vec![stu8_0, stu8_1, stu64];
    assert_eq!(expected, forward.iter().map(|t| t.1).collect::<Vec<_>>());

    expected.reverse();
    assert_eq!(expected, backward.iter().map(|t| t.1).collect::<Vec<_>>());

    // Test 2: Query shared tables with specific type parameters
    let forward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "SHARED", "type": "0x2::table::Table<u8, u8>" }),
        5,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let expected = vec![stu8_0, stu8_1];
    assert_eq!(expected, forward.iter().map(|t| t.1).collect::<Vec<_>>());

    // Test 3: Query all immutable tables (unqualified type)
    let forward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "IMMUTABLE", "type": "0x2::table::Table" }),
        2,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let backward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "IMMUTABLE", "type": "0x2::table::Table" }),
        2,
        false,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let mut expected = vec![itu8_0, itu8_1, itu64];
    assert_eq!(expected, forward.iter().map(|t| t.1).collect::<Vec<_>>());

    expected.reverse();
    assert_eq!(expected, backward.iter().map(|t| t.1).collect::<Vec<_>>());

    // Test 4: Query immutable tables with specific type parameters
    let forward = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "IMMUTABLE", "type": "0x2::table::Table<u64, u64>" }),
        5,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let expected = vec![itu64];
    assert_eq!(expected, forward.iter().map(|t| t.1).collect::<Vec<_>>());

    // Test 5: Verify owned tables aren't included in shared/immutable queries
    let tables = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "type": "0x2::table::Table" }),
        20,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    // Should have 2 owned + 3 shared + 3 immutable = 8 total tables
    assert_eq!(8, tables.len());

    // Verify owned tables are in the result
    assert!(tables.iter().any(|(_, obj)| obj == &atu8));
    assert!(tables.iter().any(|(_, obj)| obj == &btu64));

    // Test 6: Time-travel - Query shared objects at checkpoint 1 (before immutable objects)
    let forward = objects(
        &cluster,
        CHECKPOINT_QUERY,
        json!({ "ownerKind": "SHARED", "type": "0x2::table::Table" }),
        10,
        true,
        |v| v.pointer("/data/checkpoint/query/objects"),
    )
    .await
    .unwrap();

    // At checkpoint 1, should only have shared tables
    let expected = vec![stu8_0, stu8_1, stu64];
    assert_eq!(expected, forward.iter().map(|t| t.1).collect::<Vec<_>>());

    // Test 7: Query immutable objects at checkpoint 1 (should be empty)
    let forward = objects(
        &cluster,
        CHECKPOINT_QUERY,
        json!({ "ownerKind": "IMMUTABLE", "type": "0x2::table::Table" }),
        10,
        true,
        |v| v.pointer("/data/checkpoint/query/objects"),
    )
    .await
    .unwrap();

    // At checkpoint 1, no immutable objects exist yet
    assert!(forward.is_empty());

    let all_shared = objects(
        &cluster,
        OBJECTS_QUERY,
        json!({ "ownerKind": "SHARED", "type": "0x2::table::Table" }),
        10,
        true,
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    let lo = 0;
    let hi = 2;

    // Test 8: first + before
    let (page, page_info) = objects_page(
        &cluster,
        OBJECTS_QUERY,
        json!({
            "filter": { "ownerKind": "SHARED", "type": "0x2::table::Table" },
            "first": all_shared.len(),
            "after": all_shared[lo].0,
            "last": null,
            "before": all_shared[hi].0,
        }),
        |v| v.pointer("/data/objects"),
    )
    .await
    .unwrap();

    assert_eq!(&page, &all_shared[lo + 1..hi]);
    assert!(page_info.has_previous_page);
    assert!(page_info.has_next_page);
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
    find::address_owned(&fx).expect("Failed to find created bag")
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
    find::address_owned(&fx).expect("Failed to find created table")
}

/// Run a sponsored transaction to destroy a coin by merging it with gas.
/// A fresh funded account sponsors the transaction while the owner signs it.
fn destroy_coin(
    cluster: &mut FullCluster,
    owner: SuiAddress,
    owner_kp: &dyn Signer<Signature>,
    coin: ObjectRef,
) -> TransactionEffects {
    let (sponsor, sponsor_kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET)
        .expect("Failed to fund sponsor account");

    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder
        .obj(ObjectArg::ImmOrOwnedObject(coin))
        .expect("Failed to add coin object");
    builder.command(Command::MergeCoins(Argument::GasCoin, vec![coin_arg]));

    let gas_data = GasData {
        payment: vec![gas],
        owner: sponsor,
        price: cluster.reference_gas_price(),
        budget: DEFAULT_GAS_BUDGET,
    };

    let data = TransactionData::new_with_gas_data(
        TransactionKind::ProgrammableTransaction(builder.finish()),
        owner, // sender is the owner of the objects
        gas_data,
    );

    // Sign with both sponsor (for gas) and owner (for the coin)
    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(
            data,
            vec![&sponsor_kp, owner_kp],
        ))
        .expect("Failed to execute transaction");

    assert!(fx.status().is_ok(), "destroy coin transaction failed");
    fx
}

/// Run a transaction on `cluster` signed by a fresh funded account that creates a shared `Table<ty, ty>`
/// with `size` many elements.
fn create_shared_table(cluster: &mut FullCluster, ty: TypeTag, size: u64) -> ObjectRef {
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

    // Share the table
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("transfer").to_owned(),
        ident_str!("public_share_object").to_owned(),
        vec![
            StructTag {
                address: SUI_FRAMEWORK_PACKAGE_ID.into(),
                module: ident_str!("table").to_owned(),
                name: ident_str!("Table").to_owned(),
                type_params: vec![ty.clone(), ty],
            }
            .into(),
        ],
        vec![table],
    );

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

    assert!(
        fx.status().is_ok(),
        "create shared table transaction failed"
    );
    fx.created()
        .into_iter()
        .find_map(|((obj_id, version, digest), owner)| match owner {
            Owner::Shared { .. } => Some((obj_id, version, digest)),
            _ => None,
        })
        .expect("Failed to find shared table")
}

/// Run a transaction on `cluster` signed by a fresh funded account that creates an immutable `Table<ty, ty>`
/// with `size` many elements.
fn create_immutable_table(cluster: &mut FullCluster, ty: TypeTag, size: u64) -> ObjectRef {
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

    // Freeze the table to make it immutable
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("transfer").to_owned(),
        ident_str!("public_freeze_object").to_owned(),
        vec![
            StructTag {
                address: SUI_FRAMEWORK_PACKAGE_ID.into(),
                module: ident_str!("table").to_owned(),
                name: ident_str!("Table").to_owned(),
                type_params: vec![ty.clone(), ty],
            }
            .into(),
        ],
        vec![table],
    );

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

    assert!(
        fx.status().is_ok(),
        "create immutable table transaction failed"
    );
    fx.created()
        .into_iter()
        .find_map(|((obj_id, version, digest), owner)| match owner {
            Owner::Immutable => Some((obj_id, version, digest)),
            _ => None,
        })
        .expect("Failed to find immutable table")
}

/// Run a sponsored transaction to transfer an object between addresses.
/// A fresh funded account sponsors the transaction while the owner signs it.
fn transfer_object(
    cluster: &mut FullCluster,
    owner: SuiAddress,
    owner_kp: &dyn Signer<Signature>,
    object: ObjectRef,
    recipient: SuiAddress,
) -> TransactionEffects {
    let (sponsor, sponsor_kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET)
        .expect("Failed to fund sponsor account");

    let mut builder = ProgrammableTransactionBuilder::new();
    let obj_arg = builder
        .obj(ObjectArg::ImmOrOwnedObject(object))
        .expect("Failed to add object");
    builder.transfer_arg(recipient, obj_arg);

    let gas_data = GasData {
        payment: vec![gas],
        owner: sponsor,
        price: cluster.reference_gas_price(),
        budget: DEFAULT_GAS_BUDGET,
    };

    let data = TransactionData::new_with_gas_data(
        TransactionKind::ProgrammableTransaction(builder.finish()),
        owner,
        gas_data,
    );

    // Sign with both sponsor (for gas) and owner (for the object)
    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(
            data,
            vec![&sponsor_kp, owner_kp],
        ))
        .expect("Failed to execute transaction");

    assert!(fx.status().is_ok(), "transfer object transaction failed");
    fx
}
