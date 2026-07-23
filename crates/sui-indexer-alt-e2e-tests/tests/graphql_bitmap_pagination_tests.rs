// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use fastcrypto::encoding::Base58;
use fastcrypto::encoding::Base64;
use fastcrypto::encoding::Encoding;
use move_core_types::identifier::Identifier;
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
use sui_rpc_cursor::CursorToken;
use sui_rpc_cursor::Position;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::event::Event;
use sui_types::parse_sui_struct_tag;
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
/// ingestion path the indexer is reading from.
///
/// Checkpoints written after genesis MUST chain `network_total_transactions` (cp 0 contributes
/// tx_seq 0, so cp 1 starts from a total of 1). An unchained builder computes its own tx count as
/// the network total, colliding its tx_seqs with genesis: the bitmap rows stay per-sender-correct,
/// but `tx_seq_digest` rows are overwritten, so queries hydrate the wrong digests — e.g. a
/// `Sender = 0x0` (SystemTx) query returning a user transaction's digest.
async fn alpha_cluster() -> (OffchainCluster, TempDir) {
    telemetry_subscribers::init_for_testing();
    let (client_args, temp_dir) = local_ingestion_client_args();

    // Provide a stub genesis so `bootstrap()` skips waiting for cp 0 to contain a
    // `TransactionKind::Genesis(_)` transaction. The actual cp 0 written by the test still needs to
    // be a valid `advance_epoch` checkpoint that publishes the framework.
    //
    // Protocol version must be >= MIN_PROTOCOL_VERSION (1). The default
    // `mock::sui_system_state_inner_v2()` sets `protocol_version: 0`, which the
    // `kv_protocol_configs` pipeline rejects via `ProtocolConfig::get_for_version_if_supported`.
    const PROTOCOL_VERSION: u64 = 1;
    let system_state = SuiSystemState::V2(SuiSystemStateInnerV2 {
        protocol_version: PROTOCOL_VERSION,
        ..mock::sui_system_state_inner_v2()
    });
    let cluster = OffchainCluster::new(
        client_args,
        OffchainClusterConfig {
            experimental_query_apis: true,
            // Minimum set of PG pipelines. Graphql's watermark task polls the named pipelines.
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

async fn graphql(cluster: &OffchainCluster, query: &str, variables: Value) -> Value {
    sui_indexer_alt_e2e_tests::graphql::query(&cluster.graphql_url(), query, variables)
        .await
        .expect("graphql query failed")
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

    let query = r#"query($sender: SuiAddress, $first: Int, $after: String) {
        transactions(first: $first, filter: { sentAddress: $sender }, after: $after) {
            edges { node { digest } }
            pageInfo { endCursor hasNextPage }
        }
    }"#;

    loop {
        let resp = graphql(
            cluster,
            query,
            json!({ "sender": sender, "first": page_size, "after": after }),
        )
        .await;

        let edges = resp
            .pointer("/transactions/edges")
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
            .pointer("/transactions/pageInfo/hasNextPage")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let end_cursor = resp
            .pointer("/transactions/pageInfo/endCursor")
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

    let query = r#"query($sender: SuiAddress, $last: Int, $before: String) {
        transactions(last: $last, filter: { sentAddress: $sender }, before: $before) {
            edges { node { digest } }
            pageInfo { startCursor hasPreviousPage }
        }
    }"#;

    loop {
        let resp = graphql(
            cluster,
            query,
            json!({ "sender": sender, "last": page_size, "before": before }),
        )
        .await;

        let edges = resp
            .pointer("/transactions/edges")
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
            .pointer("/transactions/pageInfo/hasPreviousPage")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let start_cursor = resp
            .pointer("/transactions/pageInfo/startCursor")
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

/// One page of the sender-filtered transactions connection: each edge's
/// `(cursor, digest)`, in emission order (edges are ascending for both
/// `first` and `last` pages).
async fn transactions_page(
    cluster: &OffchainCluster,
    sender: &str,
    from_back: bool,
    after: Option<String>,
    before: Option<String>,
) -> Vec<(String, String)> {
    let query = r#"query($sender: SuiAddress, $first: Int, $last: Int, $after: String, $before: String) {
        transactions(first: $first, last: $last, filter: { sentAddress: $sender }, after: $after, before: $before) {
            edges { cursor node { digest } }
        }
    }"#;

    let (first, last) = if from_back {
        (None, Some(10))
    } else {
        (Some(10), None)
    };
    let resp = graphql(
        cluster,
        query,
        json!({ "sender": sender, "first": first, "last": last, "after": after, "before": before }),
    )
    .await;

    resp.pointer("/transactions/edges")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected edges array in {resp}"))
        .iter()
        .map(|e| {
            (
                e["cursor"].as_str().expect("edge cursor").to_string(),
                e["node"]["digest"]
                    .as_str()
                    .expect("edge node digest")
                    .to_string(),
            )
        })
        .collect()
}

/// Re-encode a transactions cursor with its checkpoint hint replaced,
/// mimicking position-only cursor sources (e.g. graphql's pg path) that
/// synthesize a hint instead of carrying a real one.
fn restamp_cursor_checkpoint(cursor: &str, checkpoint: u64) -> String {
    let bytes = Base64::decode(cursor).expect("cursor should be base64");
    let token = CursorToken::decode(&bytes).expect("cursor should decode");
    let position = match token.position {
        Position::Transactions { tx_seq, .. } => Position::Transactions { checkpoint, tx_seq },
        position => panic!("expected transactions cursor, got {position:?}"),
    };
    Base64::encode(
        CursorToken {
            kind: token.kind,
            position,
        }
        .encode(),
    )
}

fn test_event(package: ObjectID, module: &str, sender: SuiAddress, type_str: &str) -> Event {
    Event::new(
        &package,
        &Identifier::new(module).expect("valid module name"),
        sender,
        parse_sui_struct_tag(type_str).expect("valid struct tag"),
        vec![],
    )
}

async fn events_page(
    cluster: &OffchainCluster,
    filter: &Value,
    first: Option<usize>,
    after: Option<String>,
    last: Option<usize>,
    before: Option<String>,
) -> (Vec<(String, String, u64)>, Value) {
    let query = r#"query($filter: EventFilter, $first: Int, $after: String, $last: Int, $before: String) {
        events(filter: $filter, first: $first, after: $after, last: $last, before: $before) {
            edges { cursor node { sequenceNumber transaction { digest } } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
    }"#;

    let resp = graphql(
        cluster,
        query,
        json!({ "filter": filter, "first": first, "after": after, "last": last, "before": before }),
    )
    .await;

    let edges = resp
        .pointer("/events/edges")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected edges array in {resp}"));
    let page = edges
        .iter()
        .map(|e| {
            (
                e["cursor"].as_str().expect("edge cursor").to_string(),
                e["node"]["transaction"]["digest"]
                    .as_str()
                    .expect("edge node transaction digest")
                    .to_string(),
                e["node"]["sequenceNumber"]
                    .as_u64()
                    .expect("edge node sequenceNumber"),
            )
        })
        .collect();
    let page_info = resp
        .pointer("/events/pageInfo")
        .cloned()
        .unwrap_or_else(|| panic!("expected pageInfo in {resp}"));

    (page, page_info)
}

/// All `(digest, sequenceNumber)` positions matching `filter`, in one big forward page.
async fn all_event_positions(cluster: &OffchainCluster, filter: &Value) -> Vec<(String, u64)> {
    let (page, _) = events_page(cluster, filter, Some(50), None, None, None).await;
    page.into_iter().map(|(_, d, s)| (d, s)).collect()
}

/// Advance an `alpha_cluster` with regular checkpoints at cps 1-2, an epoch-advance at cp 3
/// (closing epoch 0 and starting epoch 1 at cp 4), and regular checkpoints at cps 4-5. Together
/// with the genesis fixture the epochs table reads: epoch 0 = cps [0, 3] (closed), epoch 1 = cps
/// [4, ..] (ongoing).
///
/// Every checkpoint carries exactly one transaction, so `network_total_transactions` chains as cp +
/// 1. The advance checkpoint's timestamp is bumped so its epoch-end cells supersede the genesis
/// fixture's (which also closed epoch 0, at cp 0, with the same mock system state).
async fn advance_multiple_epochs(cluster: &OffchainCluster, path: &Path) {
    for cp in 1..=2u64 {
        let checkpoint = TestCheckpointBuilder::new(cp)
            .with_network_total_transactions(cp)
            .start_transaction(1)
            .create_owned_object(cp)
            .finish_transaction()
            .build_checkpoint();
        write_checkpoint(path, checkpoint).await.unwrap();
    }

    let system_state = SuiSystemState::V2(SuiSystemStateInnerV2 {
        epoch: 1,
        protocol_version: 1,
        ..mock::sui_system_state_inner_v2()
    });
    let advance = TestCheckpointBuilder::new(3)
        .with_network_total_transactions(3)
        .with_timestamp_ms(3_000)
        .advance_epoch(AdvanceEpochConfig {
            output_objects: mock::system_state_output_objects(system_state),
            ..Default::default()
        });
    write_checkpoint(path, advance).await.unwrap();

    for cp in 4..=5u64 {
        let checkpoint = TestCheckpointBuilder::new(cp)
            .with_epoch(1)
            .with_network_total_transactions(cp)
            .start_transaction(1)
            .create_owned_object(cp)
            .finish_transaction()
            .build_checkpoint();
        write_checkpoint(path, checkpoint).await.unwrap();
    }

    cluster
        .wait_for_graphql(5, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 5");
}

/// One page of the checkpoints connection: each edge's `(cursor, sequenceNumber)` in emission
/// order (edges are ascending for both `first` and `last` pages), plus the raw `pageInfo`.
async fn checkpoints_page(
    cluster: &OffchainCluster,
    filter: &Value,
    first: Option<usize>,
    after: Option<String>,
    last: Option<usize>,
    before: Option<String>,
) -> (Vec<(String, u64)>, Value) {
    let query = r#"query($filter: CheckpointFilter, $first: Int, $after: String, $last: Int, $before: String) {
        checkpoints(filter: $filter, first: $first, after: $after, last: $last, before: $before) {
            edges { cursor node { sequenceNumber } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        }
    }"#;

    let resp = graphql(
        cluster,
        query,
        json!({ "filter": filter, "first": first, "after": after, "last": last, "before": before }),
    )
    .await;

    let edges = resp
        .pointer("/checkpoints/edges")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected edges array in {resp}"));
    let page = edges
        .iter()
        .map(|e| {
            (
                e["cursor"].as_str().expect("edge cursor").to_string(),
                e["node"]["sequenceNumber"]
                    .as_u64()
                    .expect("edge node sequenceNumber"),
            )
        })
        .collect();
    let page_info = resp
        .pointer("/checkpoints/pageInfo")
        .cloned()
        .unwrap_or_else(|| panic!("expected pageInfo in {resp}"));

    (page, page_info)
}

/// All checkpoint sequence numbers matching `filter`, in one big forward page.
async fn all_checkpoint_seqs(cluster: &OffchainCluster, filter: &Value) -> Vec<u64> {
    let (page, _) = checkpoints_page(cluster, filter, Some(50), None, None, None).await;
    page.into_iter().map(|(_, seq)| seq).collect()
}

#[tokio::test]
async fn opaque_cursor_round_trips_across_pages() {
    // Setup a checkpoint with 10 transactions. Sender 1 ("Alice") sends transactions at
    // tx_sequence_numbers [0, 1, 2, 4, 9]; sender 2 ("Bob") sends the rest. Filter by sentAddress =
    // Alice should yield exactly 5 transactions.
    //
    // Test paginates forward with `first: 2`, collects every edge, then paginates backward with
    // `last: 2` from the end and confirms the same set is recovered in the same order. The gaps
    // (between 2 and 4, between 4 and 9) exercise the bitmap scan's over-fetch behaviour and cursor
    // handoff across non-matching positions.
    let (cluster, temp_dir) = alpha_cluster().await;
    let alice_addr = TestCheckpointBuilder::derive_address(1);
    let alice_positions: HashSet<u64> = [0, 1, 2, 4, 9].into_iter().collect();
    let mut builder = TestCheckpointBuilder::new(1).with_network_total_transactions(1);
    for i in 0..10u64 {
        let sender_idx = if alice_positions.contains(&i) { 1 } else { 2 };
        builder = builder
            .start_transaction(sender_idx)
            .create_owned_object(i)
            .finish_transaction();
    }
    let checkpoint = builder.build_checkpoint();
    let expected: Vec<String> = [0, 1, 2, 4, 9]
        .iter()
        .map(|&i| Base58::encode(checkpoint.transactions[i as usize].transaction.digest()))
        .collect();
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

    assert_eq!(forward, expected);
    assert_eq!(backward, expected);
}

/// A transactions cursor's checkpoint is a hint: the grpc scan window is driven by the tx_seq
/// position clamp, so neutralized hints — `0` on `after`, `u64::MAX` on `before`, the values
/// position-only cursor sources (e.g. graphql's pg path) synthesize — must page identically to the
/// real checkpoints grpc mints.
#[tokio::test]
async fn cursor_checkpoint_hint_is_redundant() {
    let (cluster, temp_dir) = alpha_cluster().await;
    let alice = TestCheckpointBuilder::derive_address(1).to_string();

    // Six Alice transactions spread over two checkpoints.
    let mut total_txs = 1u64;
    for cp in [1u64, 2] {
        let mut builder = TestCheckpointBuilder::new(cp).with_network_total_transactions(total_txs);
        for i in 0..3u64 {
            builder = builder
                .start_transaction(1)
                .create_owned_object(cp * 10 + i)
                .finish_transaction();
        }
        total_txs += 3;
        write_checkpoint(temp_dir.path(), builder.build_checkpoint())
            .await
            .unwrap();
    }
    cluster
        .wait_for_graphql(2, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 2");

    fn digests(page: &[(String, String)]) -> Vec<String> {
        page.iter().map(|(_, digest)| digest.clone()).collect()
    }

    let all = transactions_page(&cluster, &alice, false, None, None).await;
    assert_eq!(all.len(), 6, "expected all Alice transactions, got {all:?}");
    let after_real = all[0].0.clone();
    let before_real = all[4].0.clone();
    let expected = digests(&all[1..4]);

    // Baseline: the between-page with the real checkpoint hints grpc minted.
    let real = transactions_page(
        &cluster,
        &alice,
        false,
        Some(after_real.clone()),
        Some(before_real.clone()),
    )
    .await;
    assert_eq!(digests(&real), expected);

    // Pg-minted cursors carry the `checkpoint: 0` placeholder. As a raw `before` hint it would
    // clamp the checkpoint window empty, so graphql's grpc edge rewrites it to the neutral upper
    // sentinel before forwarding: a client round-tripping a pg-minted cursor as `before` must see
    // the same page.
    let pg_minted = transactions_page(
        &cluster,
        &alice,
        false,
        Some(after_real.clone()),
        Some(restamp_cursor_checkpoint(&before_real, 0)),
    )
    .await;
    assert_eq!(digests(&pg_minted), expected);

    // Ordering-invariance: the same neutralized window paged from the back (`last`, a descending
    // scan). `after`/`before` bound coordinate sides, not scan-order sides, so the neutral values
    // hold in both orderings.
    let neutral_desc = transactions_page(
        &cluster,
        &alice,
        true,
        Some(restamp_cursor_checkpoint(&after_real, 0)),
        Some(restamp_cursor_checkpoint(&before_real, 0)),
    )
    .await;
    assert_eq!(digests(&neutral_desc), expected);

    // Only the `0` placeholder is repaired: a genuinely wrong (nonzero) hint still narrows at the
    // backend. `1` as the `before` hint clamps the checkpoint window to cp 1, dropping the cp 2
    // transaction the position bounds otherwise admit.
    let wrong = transactions_page(
        &cluster,
        &alice,
        false,
        Some(after_real),
        Some(restamp_cursor_checkpoint(&before_real, 1)),
    )
    .await;
    assert_eq!(digests(&wrong), digests(&all[1..3]));
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
        .with_network_total_transactions(1)
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

    let query = r#"query($address: SuiAddress) {
        transactions(first: 50, filter: { sentAddress: $address }) {
            edges { node { digest } }
        }
    }"#;
    let resp = graphql(&cluster, query, json!({ "address": alice })).await;
    let digests: HashSet<String> = resp
        .pointer("/transactions/edges")
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
        .with_network_total_transactions(1)
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
        transactions(first: 50, filter: { function: "0x42::m::a" }) {
            edges { node { digest } }
        }
    }"#;
    let resp = graphql(&cluster, query, json!({})).await;
    let digests: HashSet<String> = resp
        .pointer("/transactions/edges")
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
        .with_network_total_transactions(1)
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

    let query = r#"query($address: SuiAddress) {
        transactions(first: 50, filter: { affectedAddress: $address }) {
            edges { node { digest } }
        }
    }"#;
    let resp = graphql(&cluster, query, json!({ "address": bob })).await;
    let digests: HashSet<String> = resp
        .pointer("/transactions/edges")
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
        .with_network_total_transactions(1)
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
    let query = r#"query($object: SuiAddress) {
        transactions(first: 50, filter: { affectedObject: $object }) {
            edges { node { digest } }
        }
    }"#;
    let resp = graphql(&cluster, query, json!({ "object": target })).await;
    let digests: HashSet<String> = resp
        .pointer("/transactions/edges")
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
        .with_network_total_transactions(1)
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
        transactions(first: 50, filter: { kind: PROGRAMMABLE_TX }) {
            edges { node { digest } }
        }
    }"#;
    let resp = graphql(&cluster, query, json!({})).await;
    let digests: HashSet<String> = resp
        .pointer("/transactions/edges")
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
        .with_network_total_transactions(1)
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
        transactions(first: 50, filter: { kind: SYSTEM_TX }) {
            edges { node { digest } }
        }
    }"#;
    let resp = graphql(&cluster, query, json!({})).await;
    let digests: HashSet<String> = resp
        .pointer("/transactions/edges")
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
        .with_network_total_transactions(1)
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

    let query = r#"query($address: SuiAddress) {
        transactions(
            first: 50,
            filter: { sentAddress: $address, function: "0x42::m::a" }
        ) {
            edges { node { digest } }
        }
    }"#;
    let resp = graphql(&cluster, query, json!({ "address": alice })).await;
    let digests: HashSet<String> = resp
        .pointer("/transactions/edges")
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
        .with_network_total_transactions(1)
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

    let query = r#"query($address: SuiAddress) {
        transactions(
            first: 50,
            filter: { sentAddress: $address, kind: PROGRAMMABLE_TX }
        ) {
            edges { node { digest } }
        }
    }"#;
    let resp = graphql(&cluster, query, json!({ "address": alice })).await;
    let digests: HashSet<String> = resp
        .pointer("/transactions/edges")
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
        .with_network_total_transactions(1)
        .start_transaction(1)
        .create_owned_object(0)
        .finish_transaction()
        .build_checkpoint();

    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();
    cluster
        .wait_for_graphql(1, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 1");

    let query = r#"query($address: SuiAddress) {
        transactions(
            first: 50,
            filter: { sentAddress: $address, kind: SYSTEM_TX }
        ) {
            edges { node { digest } }
        }
    }"#;
    let resp = graphql(&cluster, query, json!({ "address": alice })).await;
    let digests: HashSet<String> = resp
        .pointer("/transactions/edges")
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

#[tokio::test]
async fn events_cursor_round_trips_across_pages() {
    // cp 1 has three transactions: Alice emits 3 events in tx 0 and 2 events in tx 2; Bob emits 1
    // event in tx 1. Filtering by sender = Alice must recover exactly Alice's five events in
    // (tx, event index) order, paginating forwards and backwards with page size 2 — exercising
    // resume from a mid-transaction cursor (the event-index coordinate) in both directions.
    let (cluster, temp_dir) = alpha_cluster().await;
    let alice = TestCheckpointBuilder::derive_address(1);
    let bob = TestCheckpointBuilder::derive_address(2);
    let pkg = ObjectID::from_single_byte(0x42);

    let ev = |sender| test_event(pkg, "m", sender, "0x42::m::T");
    let checkpoint = TestCheckpointBuilder::new(1)
        .with_network_total_transactions(1)
        .start_transaction(1)
        .with_events(vec![ev(alice), ev(alice), ev(alice)])
        .create_owned_object(0)
        .finish_transaction()
        .start_transaction(2)
        .with_events(vec![ev(bob)])
        .create_owned_object(1)
        .finish_transaction()
        .start_transaction(1)
        .with_events(vec![ev(alice), ev(alice)])
        .create_owned_object(2)
        .finish_transaction()
        .build_checkpoint();

    let digest = |i: usize| Base58::encode(checkpoint.transactions[i].transaction.digest());
    let expected: Vec<(String, u64)> = vec![
        (digest(0), 0),
        (digest(0), 1),
        (digest(0), 2),
        (digest(2), 0),
        (digest(2), 1),
    ];
    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();
    cluster
        .wait_for_graphql(1, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 1");

    let filter = json!({ "sender": alice.to_string() });

    // ----- Forward pagination -----
    let mut forward = Vec::new();
    let mut after: Option<String> = None;
    loop {
        let (page, page_info) =
            events_page(&cluster, &filter, Some(2), after.clone(), None, None).await;
        forward.extend(page.into_iter().map(|(_, d, s)| (d, s)));
        if !page_info["hasNextPage"].as_bool().unwrap_or(false) {
            break;
        }
        after = page_info["endCursor"].as_str().map(String::from);
        assert!(after.is_some(), "hasNextPage implies endCursor");
    }
    assert_eq!(forward, expected);

    // ----- Backward pagination -----
    // Pages arrive latest-first with edges ascending within each page; reverse the page order so
    // flattening yields ascending order.
    let mut pages = Vec::new();
    let mut before: Option<String> = None;
    loop {
        let (page, page_info) =
            events_page(&cluster, &filter, None, None, Some(2), before.clone()).await;
        let positions: Vec<_> = page.into_iter().map(|(_, d, s)| (d, s)).collect();
        if !positions.is_empty() {
            pages.push(positions);
        }
        if !page_info["hasPreviousPage"].as_bool().unwrap_or(false) {
            break;
        }
        before = page_info["startCursor"].as_str().map(String::from);
        assert!(before.is_some(), "hasPreviousPage implies startCursor");
    }
    let backward: Vec<(String, u64)> = pages.into_iter().rev().flatten().collect();
    assert_eq!(backward, expected);
}

/// Three single-event transactions emitting from 0x42::m, 0x42::n, and 0x43::m: the package-level
/// filter must select the two 0x42 events, and the module-level filter exactly one.
#[tokio::test]
async fn test_events_module() {
    let (cluster, temp_dir) = alpha_cluster().await;
    let alice = TestCheckpointBuilder::derive_address(1);

    let checkpoint = TestCheckpointBuilder::new(1)
        .with_network_total_transactions(1)
        .start_transaction(1)
        .with_events(vec![test_event(
            ObjectID::from_single_byte(0x42),
            "m",
            alice,
            "0x42::m::T",
        )])
        .create_owned_object(0)
        .finish_transaction()
        .start_transaction(1)
        .with_events(vec![test_event(
            ObjectID::from_single_byte(0x42),
            "n",
            alice,
            "0x42::n::T",
        )])
        .create_owned_object(1)
        .finish_transaction()
        .start_transaction(1)
        .with_events(vec![test_event(
            ObjectID::from_single_byte(0x43),
            "m",
            alice,
            "0x43::m::T",
        )])
        .create_owned_object(2)
        .finish_transaction()
        .build_checkpoint();

    let digest = |i: usize| Base58::encode(checkpoint.transactions[i].transaction.digest());
    let (in_m, in_n, other_pkg) = (digest(0), digest(1), digest(2));
    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();
    cluster
        .wait_for_graphql(1, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 1");

    let by_package = all_event_positions(&cluster, &json!({ "module": "0x42" })).await;
    assert_eq!(by_package, vec![(in_m.clone(), 0), (in_n, 0)]);

    let by_module = all_event_positions(&cluster, &json!({ "module": "0x42::m" })).await;
    assert_eq!(by_module, vec![(in_m, 0)]);

    let other = all_event_positions(&cluster, &json!({ "module": "0x43" })).await;
    assert_eq!(other, vec![(other_pkg, 0)]);
}

/// Test predicate matching at every specificity level.
#[tokio::test]
async fn test_events_type() {
    let (cluster, temp_dir) = alpha_cluster().await;
    let alice = TestCheckpointBuilder::derive_address(1);
    let pkg = ObjectID::from_single_byte(0x42);

    let types = [
        "0x42::m::T",
        "0x42::m::U",
        "0x42::m::T<0x2::sui::SUI>",
        "0x42::n::T",
        "0x43::m::T",
    ];
    let mut builder = TestCheckpointBuilder::new(1).with_network_total_transactions(1);
    for (i, type_str) in types.iter().enumerate() {
        builder = builder
            .start_transaction(1)
            .with_events(vec![test_event(pkg, "m", alice, type_str)])
            .create_owned_object(i as u64)
            .finish_transaction();
    }
    let checkpoint = builder.build_checkpoint();

    let digest = |i: usize| Base58::encode(checkpoint.transactions[i].transaction.digest());
    let digests: Vec<String> = (0..types.len()).map(digest).collect();
    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();
    cluster
        .wait_for_graphql(1, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 1");

    let positions = |idxs: &[usize]| -> Vec<(String, u64)> {
        idxs.iter().map(|&i| (digests[i].clone(), 0)).collect()
    };

    // Address level: everything whose type is defined at 0x42.
    let by_address = all_event_positions(&cluster, &json!({ "type": "0x42" })).await;
    assert_eq!(by_address, positions(&[0, 1, 2, 3]));

    // Module level.
    let by_module = all_event_positions(&cluster, &json!({ "type": "0x42::m" })).await;
    assert_eq!(by_module, positions(&[0, 1, 2]));

    // Name level matches any instantiation.
    let by_name = all_event_positions(&cluster, &json!({ "type": "0x42::m::T" })).await;
    assert_eq!(by_name, positions(&[0, 2]));

    // Exact generic instantiation.
    let by_instantiation =
        all_event_positions(&cluster, &json!({ "type": "0x42::m::T<0x2::sui::SUI>" })).await;
    assert_eq!(by_instantiation, positions(&[2]));
}

#[tokio::test]
async fn test_events_sender_joins_module_and_type() {
    // Proves `sender` combines with `module` / `type` as an AND within a single term. Alice and
    // Bob both emit the same event shape; Alice also emits from a second package. Joining sender
    // with either predicate must select only the intersection.
    let (cluster, temp_dir) = alpha_cluster().await;
    let alice = TestCheckpointBuilder::derive_address(1);
    let bob = TestCheckpointBuilder::derive_address(2);

    let checkpoint = TestCheckpointBuilder::new(1)
        .with_network_total_transactions(1)
        .start_transaction(1)
        .with_events(vec![test_event(
            ObjectID::from_single_byte(0x42),
            "m",
            alice,
            "0x42::m::T",
        )])
        .create_owned_object(0)
        .finish_transaction()
        .start_transaction(2)
        .with_events(vec![test_event(
            ObjectID::from_single_byte(0x42),
            "m",
            bob,
            "0x42::m::T",
        )])
        .create_owned_object(1)
        .finish_transaction()
        .start_transaction(1)
        .with_events(vec![test_event(
            ObjectID::from_single_byte(0x43),
            "n",
            alice,
            "0x43::n::U",
        )])
        .create_owned_object(2)
        .finish_transaction()
        .build_checkpoint();

    let digest = |i: usize| Base58::encode(checkpoint.transactions[i].transaction.digest());
    let alice_in_m = digest(0);
    write_checkpoint(temp_dir.path(), checkpoint).await.unwrap();
    cluster
        .wait_for_graphql(1, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 1");

    let sender_and_module = all_event_positions(
        &cluster,
        &json!({ "sender": alice.to_string(), "module": "0x42::m" }),
    )
    .await;
    assert_eq!(sender_and_module, vec![(alice_in_m.clone(), 0)]);

    let sender_and_type = all_event_positions(
        &cluster,
        &json!({ "sender": alice.to_string(), "type": "0x42::m::T" }),
    )
    .await;
    assert_eq!(sender_and_type, vec![(alice_in_m, 0)]);

    // Contradictory join: Bob never emitted from 0x43.
    let empty = all_event_positions(
        &cluster,
        &json!({ "sender": bob.to_string(), "type": "0x43::n::U" }),
    )
    .await;
    assert!(empty.is_empty(), "expected no matches, got {empty:?}");
}

#[tokio::test]
async fn test_events_module_and_type_rejected() {
    // Parity with the Postgres path: combining `module` and `type` is rejected as a feature
    // unavailable error rather than served as a (supported) bitmap conjunction.
    let (cluster, _temp_dir) = alpha_cluster().await;
    cluster
        .wait_for_graphql(0, Duration::from_secs(30))
        .await
        .expect("graphql did not reach checkpoint 0");

    let query = r#"query($filter: EventFilter) {
        events(first: 10, filter: $filter) {
            edges { node { sequenceNumber } }
        }
    }"#;
    let err = sui_indexer_alt_e2e_tests::graphql::query(
        &cluster.graphql_url(),
        query,
        json!({ "filter": { "module": "0x42::m", "type": "0x42::m::T" } }),
    )
    .await
    .expect_err("module + type filter must be rejected");
    assert!(
        err.to_string().contains("not supported"),
        "unexpected error: {err:#}"
    );
}

#[tokio::test]
async fn checkpoints_cursor_round_trips_across_pages() {
    let (cluster, temp_dir) = alpha_cluster().await;
    advance_multiple_epochs(&cluster, temp_dir.path()).await;
    let filter = json!(null);
    let expected: Vec<u64> = (0..=5).collect();

    // ----- Forward pagination -----
    let mut forward = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..10 {
        let (page, page_info) =
            checkpoints_page(&cluster, &filter, Some(4), after.clone(), None, None).await;
        forward.extend(page.into_iter().map(|(_, seq)| seq));
        if !page_info["hasNextPage"].as_bool().unwrap_or(false) {
            break;
        }
        after = page_info["endCursor"].as_str().map(String::from);
        assert!(after.is_some(), "hasNextPage implies endCursor");
    }
    assert_eq!(forward, expected);

    // ----- Backward pagination -----
    //
    // Pages arrive latest-first with edges ascending within each page; reverse the page order so
    // flattening yields ascending order.
    let mut pages = Vec::new();
    let mut before: Option<String> = None;
    for _ in 0..10 {
        let (page, page_info) =
            checkpoints_page(&cluster, &filter, None, None, Some(4), before.clone()).await;
        let seqs: Vec<u64> = page.into_iter().map(|(_, seq)| seq).collect();
        if !seqs.is_empty() {
            pages.push(seqs);
        }
        if !page_info["hasPreviousPage"].as_bool().unwrap_or(false) {
            break;
        }
        before = page_info["startCursor"].as_str().map(String::from);
        assert!(before.is_some(), "hasPreviousPage implies startCursor");
    }
    let backward: Vec<u64> = pages.into_iter().rev().flatten().collect();
    assert_eq!(backward, expected);
}

#[tokio::test]
async fn test_checkpoints_checkpoint_bounds() {
    let (cluster, temp_dir) = alpha_cluster().await;
    advance_multiple_epochs(&cluster, temp_dir.path()).await;

    let window = all_checkpoint_seqs(
        &cluster,
        &json!({ "afterCheckpoint": 1, "beforeCheckpoint": 4 }),
    )
    .await;
    assert_eq!(window, vec![2, 3]);

    let at = all_checkpoint_seqs(&cluster, &json!({ "atCheckpoint": 2 })).await;
    assert_eq!(at, vec![2]);

    // A window that collapses to nothing.
    let empty = all_checkpoint_seqs(
        &cluster,
        &json!({ "afterCheckpoint": 2, "beforeCheckpoint": 3 }),
    )
    .await;
    assert!(empty.is_empty(), "expected no matches, got {empty:?}");
}

#[tokio::test]
async fn test_checkpoints_at_epoch() {
    // `atEpoch` resolves through a `GetEpoch` point-read into a checkpoint range on the request.
    let (cluster, temp_dir) = alpha_cluster().await;
    advance_multiple_epochs(&cluster, temp_dir.path()).await;

    // Closed epoch: cps [0, 3].
    let closed = all_checkpoint_seqs(&cluster, &json!({ "atEpoch": 0 })).await;
    assert_eq!(closed, vec![0, 1, 2, 3]);

    // Ongoing epoch: no last checkpoint yet; upper bound comes from the scope.
    let ongoing = all_checkpoint_seqs(&cluster, &json!({ "atEpoch": 1 })).await;
    assert_eq!(ongoing, vec![4, 5]);

    // Nonexistent epoch.
    let missing = all_checkpoint_seqs(&cluster, &json!({ "atEpoch": 9 })).await;
    assert!(missing.is_empty(), "expected no matches, got {missing:?}");

    // `atEpoch` composes with checkpoint bounds.
    let bounded = all_checkpoint_seqs(
        &cluster,
        &json!({ "atEpoch": 0, "afterCheckpoint": 0, "beforeCheckpoint": 3 }),
    )
    .await;
    assert_eq!(bounded, vec![1, 2]);
}
