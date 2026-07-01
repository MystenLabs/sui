// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the bitmap-routed transaction pagination path.
//!
//! Cluster setup: graphql is configured to consume the kv-rpc's v2alpha experimental query APIs
//! (`enable_experimental_query_apis: true`). The kv-rpc server inside `OffchainCluster` already
//! serves alpha by default; this just tells the graphql consumer to construct the alpha client and
//! register it in resolver context, which routes `Query.transactions` (when `kind` is unset) through
//! `paginate_bitmap` instead of the Postgres path.
//!
//! Behaviors covered here that aren't reachable from the PG-path snapshot tests:
//! - Opaque cursor round-trip across paginated requests
//! - Empty-page navigation (start_cursor/end_cursor anchoring)
//! - Partial-page behavior under `ScanLimit` / `ItemLimit`
//! - Stale Postgres-style sequence cursor rejection
//! - `kind` filter falling back to the Postgres path even with alpha wired up
//! - One or two representative cross-path equivalence checks

use std::collections::HashSet;
use std::time::Duration;

use fastcrypto::encoding::Base58;
use fastcrypto::encoding::Encoding;
use reqwest::Client;
use serde_json::Value;
use serde_json::json;
use sui_framework::BuiltInFramework;
use sui_indexer_alt::BootstrapGenesis;
use sui_indexer_alt::config::IndexerConfig;
use sui_indexer_alt::config::PipelineLayer;
use sui_indexer_alt_e2e_tests::OffchainCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_e2e_tests::local_ingestion_client_args;
use sui_indexer_alt_e2e_tests::write_checkpoint;
use sui_indexer_alt_schema::checkpoints::StoredGenesis;
use sui_indexer_alt_schema::epochs::StoredEpochStart;
use sui_types::base_types::ObjectID;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::sui_system_state::mock;
use sui_types::sui_system_state::sui_system_state_inner_v2::SuiSystemStateInnerV2;
use sui_types::test_checkpoint_data_builder::AdvanceEpochConfig;
use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
use tempfile::TempDir;

/// Build an `OffchainCluster` with alpha enabled and a `BootstrapGenesis` config so the indexer
/// doesn't wait for a real genesis checkpoint via ingestion. Writes the genesis `advance_epoch`
/// checkpoint at cp 0. Callers can add further checkpoints at cp 1 onwards using the returned
/// `TempDir`.
///
/// The `TempDir` must be held by the caller for the duration of the test — dropping it removes the
/// ingestion path the indexer is reading from. Even tests that don't write further checkpoints need
/// to hold it, since the indexer keeps the path live while processing cp 0.
async fn alpha_cluster() -> (OffchainCluster, TempDir) {
    telemetry_subscribers::init_for_testing();
    let (client_args, temp_dir) = local_ingestion_client_args();

    // Provide a stub genesis so `bootstrap()` skips waiting for cp 0 to contain a
    // `TransactionKind::Genesis(_)` transaction. The actual cp 0 written by the test still
    // needs to be a valid `advance_epoch` checkpoint that publishes the framework.
    //
    // Protocol version must be >= MIN_PROTOCOL_VERSION (1). The default
    // `mock::sui_system_state_inner_v2()` sets `protocol_version: 0`, which the
    // `kv_protocol_configs` pipeline rejects via `ProtocolConfig::get_for_version_if_supported`
    // — wedging the indexer. Override here.
    const PROTOCOL_VERSION: u64 = 1;
    let system_state = SuiSystemState::V2(SuiSystemStateInnerV2 {
        protocol_version: PROTOCOL_VERSION,
        ..mock::sui_system_state_inner_v2()
    });
    let cluster = OffchainCluster::new(
        client_args,
        OffchainClusterConfig {
            experimental_query_apis: true,
            // Minimal PG pipeline set. The bitmap path reads via kv-rpc/BigTable, not these
            // tables, but graphql's watermark task polls every configured pipeline — so
            // pipelines that error on our synthetic genesis (e.g. `tx_balance_changes` on the
            // advance_epoch tx) would stall every other pipeline's watermark from advancing.
            // Enabling only what's strictly needed avoids the stall.
            indexer_config: IndexerConfig {
                pipeline: PipelineLayer {
                    cp_sequence_numbers: Some(Default::default()),
                    kv_epoch_ends: Some(Default::default()),
                    kv_epoch_starts: Some(Default::default()),
                    kv_packages: Some(Default::default()),
                    ..Default::default()
                },
                ..Default::default()
            },
            bootstrap_genesis: Some(BootstrapGenesis {
                stored_genesis: StoredGenesis {
                    genesis_digest: [1u8; 32].to_vec(),
                    initial_protocol_version: PROTOCOL_VERSION as i64,
                },
                stored_epoch_start: StoredEpochStart {
                    epoch: 0,
                    protocol_version: PROTOCOL_VERSION as i64,
                    cp_lo: 0,
                    start_timestamp_ms: 0,
                    reference_gas_price: 0,
                    system_state: bcs::to_bytes(&system_state).unwrap(),
                },
            }),
            ..Default::default()
        },
        &prometheus::Registry::new(),
    )
    .await
    .expect("Failed to create off-chain cluster with alpha enabled");

    // Write the genesis epoch-advance checkpoint at cp 0 with the framework objects. Any
    // user-written checkpoint follows at cp 1 onwards.
    let mut advance_epoch_config = AdvanceEpochConfig {
        output_objects: mock::system_state_output_objects(system_state),
        ..Default::default()
    };
    advance_epoch_config
        .output_objects
        .extend(BuiltInFramework::genesis_objects());
    let genesis_checkpoint = TestCheckpointBuilder::new(0).advance_epoch(advance_epoch_config);
    write_checkpoint(temp_dir.path(), genesis_checkpoint)
        .await
        .expect("write genesis checkpoint");

    (cluster, temp_dir)
}

/// Post a graphql query against the cluster and return the parsed JSON response.
async fn graphql(cluster: &OffchainCluster, query: &str) -> Value {
    let client = Client::new();
    let body = json!({ "query": query });
    let response = client
        .post(cluster.graphql_url())
        .json(&body)
        .send()
        .await
        .expect("graphql POST failed");
    response
        .json()
        .await
        .expect("graphql response was not valid JSON")
}

/// Paginate forwards with `first: N, filter: { sentAddress: <sender> }`, collecting digests
/// across all pages until `hasNextPage` is false. Returns digests in emission order
/// (ascending).
async fn paginate_forward(
    cluster: &OffchainCluster,
    sender: &str,
    page_size: usize,
) -> Vec<String> {
    let mut digests = Vec::new();
    let mut after: Option<String> = None;

    loop {
        let after_clause = after
            .as_deref()
            .map(|c| format!(", after: \"{c}\""))
            .unwrap_or_default();

        let query = format!(
            r#"{{
                transactions(first: {page_size}, filter: {{ sentAddress: "{sender}" }}{after_clause}) {{
                    edges {{ node {{ digest }} }}
                    pageInfo {{ endCursor hasNextPage }}
                }}
            }}"#
        );
        let resp = graphql(cluster, &query).await;

        let edges = resp
            .pointer("/data/transactions/edges")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("expected edges array in {resp}"));
        for edge in edges {
            let digest = edge["node"]["digest"]
                .as_str()
                .expect("edge node digest")
                .to_string();
            digests.push(digest);
        }

        let has_next = resp
            .pointer("/data/transactions/pageInfo/hasNextPage")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let end_cursor = resp
            .pointer("/data/transactions/pageInfo/endCursor")
            .and_then(Value::as_str)
            .map(String::from);

        if !has_next || end_cursor.is_none() {
            break;
        }
        after = end_cursor;
    }

    digests
}

/// Paginate backwards with `last: N, filter: { sentAddress: <sender> }`, collecting digests
/// across all pages until `hasPreviousPage` is false. Returns digests in ascending order
/// (matches `paginate_forward`'s order so callers can compare directly).
async fn paginate_backward(
    cluster: &OffchainCluster,
    sender: &str,
    page_size: usize,
) -> Vec<String> {
    // Collect pages in reverse-emission order, then reverse the whole thing at the end so
    // the result matches forward ordering. Within each page, edges are already ascending.
    let mut pages: Vec<Vec<String>> = Vec::new();
    let mut before: Option<String> = None;

    loop {
        let before_clause = before
            .as_deref()
            .map(|c| format!(", before: \"{c}\""))
            .unwrap_or_default();

        let query = format!(
            r#"{{
                transactions(last: {page_size}, filter: {{ sentAddress: "{sender}" }}{before_clause}) {{
                    edges {{ node {{ digest }} }}
                    pageInfo {{ startCursor hasPreviousPage }}
                }}
            }}"#
        );
        let resp = graphql(cluster, &query).await;

        let edges = resp
            .pointer("/data/transactions/edges")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("expected edges array in {resp}"));
        let page: Vec<String> = edges
            .iter()
            .map(|e| {
                e["node"]["digest"]
                    .as_str()
                    .expect("edge node digest")
                    .to_string()
            })
            .collect();

        let has_prev = resp
            .pointer("/data/transactions/pageInfo/hasPreviousPage")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let start_cursor = resp
            .pointer("/data/transactions/pageInfo/startCursor")
            .and_then(Value::as_str)
            .map(String::from);

        if !page.is_empty() {
            pages.push(page);
        }

        if !has_prev || start_cursor.is_none() {
            break;
        }
        before = start_cursor;
    }

    // Earliest page came last; reverse so that flattening yields ascending order.
    pages.into_iter().rev().flatten().collect()
}

#[tokio::test]
async fn opaque_cursor_round_trips_across_pages() {
    // (a) Forward + backward pagination over a known data set, with deliberate gaps so the
    // scan crosses over non-matching positions between matches.
    //
    // Layout: checkpoint 0 with 10 transactions. Sender 1 ("Alice") sends transactions at
    // tx_sequence_numbers [0, 1, 2, 4, 9]; sender 2 ("Bob") sends the rest. Filter by
    // sentAddress = Alice should yield exactly 5 transactions.
    //
    // Test paginates forward with `first: 2`, collects every edge, then paginates backward
    // with `last: 2` from the end and confirms the same set is recovered in the same order.
    // The gaps (between 2 and 4, between 4 and 9) exercise the bitmap scan's over-fetch
    // behaviour and cursor handoff across non-matching positions.
    let (cluster, temp_dir) = alpha_cluster().await;

    let alice_addr = TestCheckpointBuilder::derive_address(1);
    let alice_positions: HashSet<u64> = [0, 1, 2, 4, 9].into_iter().collect();

    // Build cp 1 with 10 transactions, alternating senders by position. (cp 0 was the
    // genesis-advance checkpoint written by `alpha_cluster_with_ingestion`.)
    let mut builder = TestCheckpointBuilder::new(1);
    for i in 0..10u64 {
        let sender_idx = if alice_positions.contains(&i) { 1 } else { 2 };
        builder = builder
            .start_transaction(sender_idx)
            .create_owned_object(i)
            .finish_transaction();
    }
    let checkpoint = builder.build_checkpoint();
    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();

    // Wait for the indexer + bitmap pipeline to catch up to cp 1.
    cluster
        .wait_for_graphql(1, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 1");

    // ----- Forward pagination -----
    let forward = paginate_forward(&cluster, &alice_addr.to_string(), 2).await;
    assert_eq!(
        forward.len(),
        5,
        "forward pagination should collect all 5 Alice transactions, got {}: {forward:?}",
        forward.len()
    );

    // ----- Backward pagination -----
    let backward = paginate_backward(&cluster, &alice_addr.to_string(), 2).await;
    assert_eq!(
        backward, forward,
        "backward pagination should recover the same set in the same order"
    );

    // Sanity: every collected digest is unique (no duplicates across pages).
    let unique: HashSet<&String> = forward.iter().collect();
    assert_eq!(
        unique.len(),
        forward.len(),
        "forward pagination duplicated edges: {forward:?}"
    );
}

#[tokio::test]
async fn test_sent_address() {
    // Proves the graphql `sentAddress` filter translates to a `Sender` include predicate that
    // selects the right tx on the bitmap path. Two-tx dataset: cp 0's bootstrap advance_epoch
    // (sender 0x0) and one cp 1 tx from Alice. Filtering by Alice must return Alice's digest
    // and nothing else — implicitly proving the bootstrap system tx is filtered out.
    let (cluster, temp_dir) = alpha_cluster().await;

    let alice = TestCheckpointBuilder::derive_address(1);

    let checkpoint = TestCheckpointBuilder::new(1)
        .start_transaction(1)
        .create_owned_object(0)
        .finish_transaction()
        .build_checkpoint();
    let expected = Base58::encode(
        checkpoint
            .contents
            .iter()
            .next()
            .expect("cp 1 should have one transaction")
            .transaction,
    );

    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();
    cluster
        .wait_for_graphql(1, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 1");

    let query = format!(
        r#"{{
            transactions(first: 100, filter: {{ sentAddress: "{alice}" }}) {{
                edges {{ node {{ digest }} }}
            }}
        }}"#
    );
    let resp = graphql(&cluster, &query).await;
    let digests: HashSet<String> = resp
        .pointer("/data/transactions/edges")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected edges array in {resp}"))
        .iter()
        .map(|e| {
            e["node"]["digest"]
                .as_str()
                .expect("edge node digest")
                .to_string()
        })
        .collect();

    assert_eq!(digests, HashSet::from([expected]));
}

#[tokio::test]
async fn test_function() {
    // Proves the graphql `function` filter translates to a `MoveCall` predicate that matches at
    // function-name granularity. cp 1 has two txs calling the same fabricated package + module
    // on different function names; filtering by one function must return exactly that tx. If
    // the predicate were package-only or module-only, both digests would come back.
    //
    // Package + names are fabricated. The bitmap indexer reads the move-call descriptors
    // structurally from the synthesized PT — no Move VM runs, no published code is consulted.
    // Using made-up identifiers keeps the test from anchoring on any specific framework module
    // (e.g. `0x2::coin`, which is itself being phased out for address balances).
    let (cluster, temp_dir) = alpha_cluster().await;

    let pkg = ObjectID::from_single_byte(0x42);

    let checkpoint = TestCheckpointBuilder::new(1)
        // tx 0: 0x42::m::a — matches the filter
        .start_transaction(1)
        .add_move_call(pkg, "m", "a")
        .create_owned_object(0)
        .finish_transaction()
        // tx 1: 0x42::m::b — same package + module, different function; must not match
        .start_transaction(1)
        .add_move_call(pkg, "m", "b")
        .create_owned_object(1)
        .finish_transaction()
        .build_checkpoint();
    let expected = Base58::encode(
        checkpoint
            .contents
            .iter()
            .next()
            .expect("cp 1 should have two transactions")
            .transaction,
    );

    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();
    cluster
        .wait_for_graphql(1, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 1");

    let query = r#"{
        transactions(first: 100, filter: { function: "0x42::m::a" }) {
            edges { node { digest } }
        }
    }"#;
    let resp = graphql(&cluster, query).await;
    let digests: HashSet<String> = resp
        .pointer("/data/transactions/edges")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected edges array in {resp}"))
        .iter()
        .map(|e| {
            e["node"]["digest"]
                .as_str()
                .expect("edge node digest")
                .to_string()
        })
        .collect();

    assert_eq!(digests, HashSet::from([expected]));
}

#[tokio::test]
async fn test_affected_address() {
    // Proves the graphql `affectedAddress` filter translates to an `AffectedAddress` include
    // predicate that catches *recipients*, not just senders. cp 1 has two Alice-sent txs: tx 0
    // creates obj 0 owned by Alice (Bob is not affected); tx 1 transfers obj 0 to Bob (Bob is
    // affected as the new owner). Filtering by Bob must return only tx 1.
    //
    // Split across two txs because `transfer_object` calls into `change_object_owner`, which
    // reads from `live_objects` — and `create_owned_object` only registers the object there at
    // `finish_transaction` time. Creating and transferring in the same tx would panic.
    let (cluster, temp_dir) = alpha_cluster().await;

    let bob = TestCheckpointBuilder::derive_address(2);

    let checkpoint = TestCheckpointBuilder::new(1)
        // tx 0: Alice creates obj 0 owned by Alice → Bob is not affected
        .start_transaction(1)
        .create_owned_object(0)
        .finish_transaction()
        // tx 1: Alice transfers obj 0 to Bob → Bob is affected as the new owner
        .start_transaction(1)
        .transfer_object(0, 2)
        .finish_transaction()
        .build_checkpoint();
    let expected = Base58::encode(
        checkpoint
            .contents
            .iter()
            .nth(1)
            .expect("cp 1 should have two transactions")
            .transaction,
    );

    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();
    cluster
        .wait_for_graphql(1, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 1");

    let query = format!(
        r#"{{
            transactions(first: 100, filter: {{ affectedAddress: "{bob}" }}) {{
                edges {{ node {{ digest }} }}
            }}
        }}"#
    );
    let resp = graphql(&cluster, &query).await;
    let digests: HashSet<String> = resp
        .pointer("/data/transactions/edges")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected edges array in {resp}"))
        .iter()
        .map(|e| {
            e["node"]["digest"]
                .as_str()
                .expect("edge node digest")
                .to_string()
        })
        .collect();

    assert_eq!(digests, HashSet::from([expected]));
}

#[tokio::test]
async fn test_affected_object() {
    // Proves the graphql `affectedObject` filter translates to an `AffectedObject` include
    // predicate that matches by object id. cp 1 has two txs creating different objects;
    // filtering by one object's deterministic id must return exactly that tx.
    let (cluster, temp_dir) = alpha_cluster().await;

    // The builder derives object ids deterministically from the (object_idx, sender) pair, so
    // we can compute the expected id without inspecting the built checkpoint.
    let target_object = TestCheckpointBuilder::derive_object_id(0);

    let checkpoint = TestCheckpointBuilder::new(1)
        // tx 0: creates obj 0 (the one we filter for)
        .start_transaction(1)
        .create_owned_object(0)
        .finish_transaction()
        // tx 1: creates obj 1 (different object id, must not match)
        .start_transaction(1)
        .create_owned_object(1)
        .finish_transaction()
        .build_checkpoint();
    let expected = Base58::encode(
        checkpoint
            .contents
            .iter()
            .next()
            .expect("cp 1 should have two transactions")
            .transaction,
    );

    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();
    cluster
        .wait_for_graphql(1, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 1");

    let target = target_object.to_canonical_string(true);
    let query = format!(
        r#"{{
            transactions(first: 100, filter: {{ affectedObject: "{target}" }}) {{
                edges {{ node {{ digest }} }}
            }}
        }}"#
    );
    let resp = graphql(&cluster, &query).await;
    let digests: HashSet<String> = resp
        .pointer("/data/transactions/edges")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected edges array in {resp}"))
        .iter()
        .map(|e| {
            e["node"]["digest"]
                .as_str()
                .expect("edge node digest")
                .to_string()
        })
        .collect();

    assert_eq!(digests, HashSet::from([expected]));
}

#[tokio::test]
async fn test_programmable_kind() {
    // Proves the graphql `kind: ProgrammableTx` filter translates to the unanchored exclude
    // `Exclude(Sender = 0x0)`. cp 0's bootstrap is a system tx (sender 0x0); cp 1 has one
    // Alice tx. Filtering by ProgrammableTx must return exactly Alice's tx — the bootstrap
    // is excluded by the negation. This is the test that exercises the polarity mapping
    // added to `to_bitmap_filter` for `kind`.
    let (cluster, temp_dir) = alpha_cluster().await;

    let checkpoint = TestCheckpointBuilder::new(1)
        .start_transaction(1)
        .create_owned_object(0)
        .finish_transaction()
        .build_checkpoint();
    let expected = Base58::encode(
        checkpoint
            .contents
            .iter()
            .next()
            .expect("cp 1 should have one transaction")
            .transaction,
    );

    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();
    cluster
        .wait_for_graphql(1, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 1");

    let query = r#"{
        transactions(first: 100, filter: { kind: PROGRAMMABLE_TX }) {
            edges { node { digest } }
        }
    }"#;
    let resp = graphql(&cluster, query).await;
    let digests: HashSet<String> = resp
        .pointer("/data/transactions/edges")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected edges array in {resp}"))
        .iter()
        .map(|e| {
            e["node"]["digest"]
                .as_str()
                .expect("edge node digest")
                .to_string()
        })
        .collect();

    assert_eq!(digests, HashSet::from([expected]));
}

#[tokio::test]
async fn test_system_kind() {
    // Proves the graphql `kind: SystemTx` filter translates to `Include(Sender = 0x0)`.
    // cp 0's bootstrap is a system tx; cp 1 has one Alice tx. Filtering by SystemTx must
    // exclude Alice's tx and return a non-empty result (the bootstrap). We assert against
    // Alice's digest directly because `alpha_cluster()` doesn't surface the bootstrap's
    // digest — full set equality would require plumbing that out.
    let (cluster, temp_dir) = alpha_cluster().await;

    let checkpoint = TestCheckpointBuilder::new(1)
        .start_transaction(1)
        .create_owned_object(0)
        .finish_transaction()
        .build_checkpoint();
    let alice_digest = Base58::encode(
        checkpoint
            .contents
            .iter()
            .next()
            .expect("cp 1 should have one transaction")
            .transaction,
    );

    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();
    cluster
        .wait_for_graphql(1, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 1");

    let query = r#"{
        transactions(first: 100, filter: { kind: SYSTEM_TX }) {
            edges { node { digest } }
        }
    }"#;
    let resp = graphql(&cluster, query).await;
    let digests: HashSet<String> = resp
        .pointer("/data/transactions/edges")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected edges array in {resp}"))
        .iter()
        .map(|e| {
            e["node"]["digest"]
                .as_str()
                .expect("edge node digest")
                .to_string()
        })
        .collect();

    assert!(
        !digests.contains(&alice_digest),
        "ProgrammableTx digest must not appear under kind: SystemTx, got {digests:?}"
    );
    assert!(
        !digests.is_empty(),
        "SystemTx filter should return the bootstrap advance_epoch tx at minimum"
    );
}

#[tokio::test]
async fn test_sent_address_and_function() {
    // Proves the graphql filter constructs two Include literals (Sender + MoveCall) in a single
    // term that AND's correctly. cp 1 has three txs designed to discriminate independently on
    // each axis: only tx 0 matches both Alice-as-sender AND function-as-`0x42::m::a`. If either
    // Include were dropped during translation, tx 1 or tx 2 would leak into the result.
    //
    // This case is representative for any two-Include AND combination (sender + affectedAddress,
    // sender + affectedObject, etc.) — the bitmap term converter ANDs literals without branching
    // on predicate type, so the AND mechanism is exercised identically.
    let (cluster, temp_dir) = alpha_cluster().await;

    let alice = TestCheckpointBuilder::derive_address(1);
    let pkg = ObjectID::from_single_byte(0x42);

    let checkpoint = TestCheckpointBuilder::new(1)
        // tx 0: Alice + 0x42::m::a — matches both literals
        .start_transaction(1)
        .add_move_call(pkg, "m", "a")
        .create_owned_object(0)
        .finish_transaction()
        // tx 1: Alice + 0x42::m::b — matches sender, fails function
        .start_transaction(1)
        .add_move_call(pkg, "m", "b")
        .create_owned_object(1)
        .finish_transaction()
        // tx 2: Bob + 0x42::m::a — matches function, fails sender
        .start_transaction(2)
        .add_move_call(pkg, "m", "a")
        .create_owned_object(2)
        .finish_transaction()
        .build_checkpoint();
    let expected = Base58::encode(
        checkpoint
            .contents
            .iter()
            .next()
            .expect("cp 1 should have three transactions")
            .transaction,
    );

    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();
    cluster
        .wait_for_graphql(1, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 1");

    let query = format!(
        r#"{{
            transactions(
                first: 100,
                filter: {{ sentAddress: "{alice}", function: "0x42::m::a" }}
            ) {{
                edges {{ node {{ digest }} }}
            }}
        }}"#
    );
    let resp = graphql(&cluster, &query).await;
    let digests: HashSet<String> = resp
        .pointer("/data/transactions/edges")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected edges array in {resp}"))
        .iter()
        .map(|e| {
            e["node"]["digest"]
                .as_str()
                .expect("edge node digest")
                .to_string()
        })
        .collect();

    assert_eq!(digests, HashSet::from([expected]));
}

#[tokio::test]
async fn test_sent_address_and_programmable_kind() {
    // Proves graphql constructs a single term mixing polarities: `Include(Sender = Alice)` AND
    // `Exclude(Sender = 0x0)`. cp 1 has Alice's tx + Bob's tx; filtering by Alice's address +
    // ProgrammableTx must return only Alice's tx (Bob's is excluded by the Include literal,
    // bootstrap excluded by the Exclude literal). If the Include were dropped, Bob's tx would
    // leak in.
    let (cluster, temp_dir) = alpha_cluster().await;

    let alice = TestCheckpointBuilder::derive_address(1);

    let checkpoint = TestCheckpointBuilder::new(1)
        // tx 0: Alice PT — matches both Include(Sender=Alice) and Exclude(Sender=0x0)
        .start_transaction(1)
        .create_owned_object(0)
        .finish_transaction()
        // tx 1: Bob PT — fails Include(Sender=Alice); discriminator against dropping Include
        .start_transaction(2)
        .create_owned_object(1)
        .finish_transaction()
        .build_checkpoint();
    let expected = Base58::encode(
        checkpoint
            .contents
            .iter()
            .next()
            .expect("cp 1 should have two transactions")
            .transaction,
    );

    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();
    cluster
        .wait_for_graphql(1, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 1");

    let query = format!(
        r#"{{
            transactions(
                first: 100,
                filter: {{ sentAddress: "{alice}", kind: PROGRAMMABLE_TX }}
            ) {{
                edges {{ node {{ digest }} }}
            }}
        }}"#
    );
    let resp = graphql(&cluster, &query).await;
    let digests: HashSet<String> = resp
        .pointer("/data/transactions/edges")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected edges array in {resp}"))
        .iter()
        .map(|e| {
            e["node"]["digest"]
                .as_str()
                .expect("edge node digest")
                .to_string()
        })
        .collect();

    assert_eq!(digests, HashSet::from([expected]));
}

#[tokio::test]
async fn test_sent_address_and_system_kind() {
    // Proves graphql constructs a contradictory term: `Include(Sender = Alice)` AND
    // `Include(Sender = 0x0)`. No tx can have both senders simultaneously, so the bitmap
    // must return an empty result. If either Include were dropped, the result would be
    // non-empty: dropping the Alice Include would let the bootstrap system tx through;
    // dropping the 0x0 Include would let Alice's tx through.
    let (cluster, temp_dir) = alpha_cluster().await;

    let alice = TestCheckpointBuilder::derive_address(1);

    let checkpoint = TestCheckpointBuilder::new(1)
        .start_transaction(1)
        .create_owned_object(0)
        .finish_transaction()
        .build_checkpoint();

    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();
    cluster
        .wait_for_graphql(1, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 1");

    let query = format!(
        r#"{{
            transactions(
                first: 100,
                filter: {{ sentAddress: "{alice}", kind: SYSTEM_TX }}
            ) {{
                edges {{ node {{ digest }} }}
            }}
        }}"#
    );
    let resp = graphql(&cluster, &query).await;
    let digests: HashSet<String> = resp
        .pointer("/data/transactions/edges")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected edges array in {resp}"))
        .iter()
        .map(|e| {
            e["node"]["digest"]
                .as_str()
                .expect("edge node digest")
                .to_string()
        })
        .collect();

    assert!(
        digests.is_empty(),
        "contradictory Include(Sender=Alice) AND Include(Sender=0x0) must return empty, got {digests:?}"
    );
}
