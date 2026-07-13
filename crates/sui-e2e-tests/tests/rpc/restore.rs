// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Restore/resume behavior of the embedded `sui-rpc-store` index backend.
//!
//! Each test drives a dedicated fullnode through a sequence of restarts,
//! toggling its index configuration between runs, and checks two things:
//!
//!  1. The startup decision the embedded store makes -- resume the
//!     existing on-disk indexes, or rebuild them -- observed in memory via
//!     [`sui_core::rpc_store_embed::EmbeddedRpcStore::bootstrap_action`]
//!     (surfaced through `SuiNode::embedded_rpc_store`), not over the RPC.
//!  2. That the index-backed RPCs answer correctly afterward: the
//!     `GetBalance` state API (live-object cohort, the `balance` index)
//!     and the `ListTransactions` ledger-history API (history cohort, the
//!     `transaction_bitmap` index).
//!
//! The unit under test is a fullnode spawned into the swarm separately
//! from the cluster's primary fullnode. The cluster's wallet executes
//! transactions against the primary (which stays up the whole time);
//! the dedicated node follows by state sync and is restarted with a
//! mutated `NodeConfig.rpc` between runs. Its `db_path` is stable across
//! restarts, so each restart sees the previous run's on-disk rpc-store and
//! exercises the real bootstrap path.
//!
//! Restart correctness note: the swarm node holds only a `Weak` reference
//! to the running `SuiNode`, so a stop releases the node's RocksDB locks
//! only if no strong handle outlives it. These helpers therefore never
//! retain a `SuiNodeHandle` across a restart -- reads fetch a transient
//! handle and drop it immediately.

use std::collections::HashSet;
use std::time::Duration;

use prost_types::FieldMask;
use rand::rngs::OsRng;
use sui_config::RpcConfig;
use sui_core::rpc_store_embed::Bootstrap;
use sui_macros::sim_test;
use sui_node::SuiNode;
use sui_rpc::Client;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetBalanceRequest;
use sui_rpc::proto::sui::rpc::v2::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2::QueryOptions;
use sui_rpc::proto::sui::rpc::v2::SenderFilter;
use sui_rpc::proto::sui::rpc::v2::TransactionFilter;
use sui_rpc::proto::sui::rpc::v2::TransactionLiteral;
use sui_rpc::proto::sui::rpc::v2::TransactionTerm;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2::transaction_literal;
use sui_test_transaction_builder::make_transfer_sui_transaction;
use sui_types::base_types::AuthorityName;
use sui_types::base_types::SuiAddress;
use sui_types::base_types::TransactionDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::transaction::TransactionDataAPI;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;

const SUI_COIN_TYPE: &str =
    "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI";

/// How long to wait for the dedicated fullnode to sync and index a target
/// checkpoint. A restore rebuilds the live cohort and backfills the
/// history cohort from genesis, so this is generous.
const WAIT_TIMEOUT: Duration = Duration::from_secs(60);

/// An rpc config that builds the embedded `sui-rpc-store` index backend,
/// which indexes both the live and ledger-history (bitmap) cohorts.
fn embedded_indexing_config() -> RpcConfig {
    RpcConfig {
        enable_indexing: Some(true),
        ..Default::default()
    }
}

/// An rpc config that builds no index at all: neither the legacy
/// `rpc-index` nor the embedded `sui-rpc-store`.
fn no_indexing_config() -> RpcConfig {
    RpcConfig {
        enable_indexing: Some(false),
        ..Default::default()
    }
}

/// Spawn a dedicated fullnode into the swarm with `rpc`, returning its
/// (stable) name and rpc url. The handle `spawn_new_node` returns is
/// dropped immediately so the node keeps no external strong reference and a
/// later [`restart_fullnode`] can release its DB locks on stop.
async fn spawn_fullnode(cluster: &mut TestCluster, rpc: RpcConfig) -> (AuthorityName, String) {
    let config = cluster
        .fullnode_config_builder()
        .with_rpc_config(rpc)
        .build(&mut OsRng, cluster.swarm.config());
    let name = config.protocol_public_key();
    let rpc_url = format!("http://{}", config.json_rpc_address);
    cluster.swarm.spawn_new_node(config).await;
    (name, rpc_url)
}

/// Stop the fullnode `name`, swap in `rpc` (the `db_path` is unchanged),
/// and restart it. The stop releases the previous run's RocksDB locks
/// because no strong `SuiNodeHandle` is held across the call.
async fn restart_fullnode(cluster: &TestCluster, name: &AuthorityName, rpc: RpcConfig) {
    let node = cluster.swarm.node(name).unwrap();
    node.stop();
    node.config().rpc = Some(rpc);
    if cfg!(msim) {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    node.start().await.unwrap();
}

/// Run `f` against the dedicated fullnode through a transient handle that
/// is dropped before returning (see the module note on restart safety).
fn with_node<T>(cluster: &TestCluster, name: &AuthorityName, f: impl FnOnce(&SuiNode) -> T) -> T {
    let handle = cluster.swarm.node(name).unwrap().get_node_handle().unwrap();
    handle.with(f)
}

fn has_embedded_store(cluster: &TestCluster, name: &AuthorityName) -> bool {
    with_node(cluster, name, |node| node.embedded_rpc_store().is_some())
}

fn bootstrap_action(cluster: &TestCluster, name: &AuthorityName) -> Option<Bootstrap> {
    with_node(cluster, name, |node| {
        node.embedded_rpc_store()
            .map(|store| store.bootstrap_action())
    })
}

fn live_committed(cluster: &TestCluster, name: &AuthorityName) -> Option<u64> {
    with_node(cluster, name, |node| {
        node.embedded_rpc_store()
            .and_then(|store| store.live_committed_checkpoint())
    })
}

fn history_committed(cluster: &TestCluster, name: &AuthorityName) -> Option<u64> {
    with_node(cluster, name, |node| {
        node.embedded_rpc_store()
            .and_then(|store| store.history_committed_checkpoint())
    })
}

/// The highest checkpoint the dedicated fullnode has executed (independent
/// of indexing).
fn node_highest_executed(cluster: &TestCluster, name: &AuthorityName) -> u64 {
    with_node(cluster, name, |node| {
        node.state()
            .get_checkpoint_store()
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
            .unwrap_or(0)
    })
}

/// Block until the dedicated fullnode has executed through `target` (by
/// state sync), independent of indexing.
async fn wait_for_executed(cluster: &TestCluster, name: &AuthorityName, target: u64) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    while node_highest_executed(cluster, name) < target {
        if tokio::time::Instant::now() >= deadline {
            panic!("timed out waiting for fullnode to execute checkpoint {target}");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Block until both cohorts of the dedicated fullnode's embedded store have
/// committed through `target`. Panics on timeout with the last-observed
/// watermarks.
async fn wait_for_indexed(cluster: &TestCluster, name: &AuthorityName, target: u64) {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let live = live_committed(cluster, name);
        let history = history_committed(cluster, name);
        if live.is_some_and(|c| c >= target) && history.is_some_and(|c| c >= target) {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "timed out waiting for embedded store to index checkpoint {target} \
                 (live={live:?}, history={history:?})"
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// A transfer executed against the cluster, with the facts a test asserts
/// on afterward.
struct Transfer {
    /// The transaction's sender (whichever account funded the gas).
    sender: SuiAddress,
    /// A fresh address with no prior coins, so its post-transfer SUI
    /// balance is exactly `amount`.
    receiver: SuiAddress,
    amount: u64,
    digest: TransactionDigest,
}

/// Transfer `amount` MIST of SUI to a fresh address through the cluster's
/// primary fullnode and wait for the transaction to land in an executed
/// checkpoint.
async fn transfer_to_fresh_address(cluster: &TestCluster, amount: u64) -> Transfer {
    let receiver = SuiAddress::random_for_testing_only();
    let txn = make_transfer_sui_transaction(&cluster.wallet, Some(receiver), Some(amount)).await;
    let executed = cluster.execute_transaction(txn).await;
    let transfer = Transfer {
        sender: executed.transaction.sender(),
        receiver,
        amount,
        digest: *executed.effects.transaction_digest(),
    };
    cluster.wait_for_tx_settlement(&[transfer.digest]).await;
    transfer
}

/// The chain tip as seen by the cluster's primary fullnode -- an upper
/// bound on the checkpoints the dedicated node must sync and index.
fn chain_tip(cluster: &TestCluster) -> u64 {
    cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .get_checkpoint_store()
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
            .unwrap_or(0)
    })
}

/// The total SUI balance the embedded store reports for `owner`.
async fn sui_balance(rpc_url: &str, owner: SuiAddress) -> u64 {
    let mut client = Client::new(rpc_url.to_owned()).unwrap();
    let mut request = GetBalanceRequest::default();
    request.owner = Some(owner.to_string());
    request.coin_type = Some(SUI_COIN_TYPE.to_string());
    client
        .state_client()
        .get_balance(request)
        .await
        .unwrap()
        .into_inner()
        .balance
        .unwrap()
        .balance
        .unwrap()
}

/// A `ListTransactions` filter matching a single sender.
fn sender_filter(sender: SuiAddress) -> TransactionFilter {
    let mut sender_filter = SenderFilter::default();
    sender_filter.address = Some(sender.to_string());
    let mut literal = TransactionLiteral::default();
    literal.predicate = Some(transaction_literal::Predicate::Sender(sender_filter));
    let mut term = TransactionTerm::default();
    term.literals = vec![literal];
    let mut filter = TransactionFilter::default();
    filter.terms = vec![term];
    filter
}

/// The set of transaction digests the ledger-history `ListTransactions` API
/// returns for `sender`, scanning the whole indexed range.
async fn list_transaction_digests_by_sender(rpc_url: &str, sender: SuiAddress) -> HashSet<String> {
    let mut client = LedgerServiceClient::connect(rpc_url.to_owned())
        .await
        .unwrap();
    let mut options = QueryOptions::default();
    options.limit = Some(500);
    let mut request = ListTransactionsRequest::default();
    request.read_mask = Some(FieldMask::from_paths(["digest"]));
    request.filter = Some(sender_filter(sender));
    request.options = Some(options);
    let mut stream = client
        .list_transactions(request)
        .await
        .unwrap()
        .into_inner();
    let mut digests = HashSet::new();
    while let Some(response) = stream.message().await.unwrap() {
        if let Some(digest) = response.transaction.and_then(|tx| tx.digest) {
            digests.insert(digest);
        }
    }
    digests
}

/// Assert both index surfaces reflect `transfer`: the `balance` index
/// reports the recipient's exact balance, and the `transaction_bitmap`
/// index returns the transfer under a sender filter.
async fn assert_transfer_indexed(rpc_url: &str, transfer: &Transfer) {
    assert_eq!(
        sui_balance(rpc_url, transfer.receiver).await,
        transfer.amount,
        "GetBalance should report the recipient's exact SUI balance",
    );
    let digests = list_transaction_digests_by_sender(rpc_url, transfer.sender).await;
    assert!(
        digests.contains(&transfer.digest.to_string()),
        "ListTransactions(sender={}) should include {}",
        transfer.sender,
        transfer.digest,
    );
}

/// A node that already has the embedded store, restarted with the same
/// config, resumes the on-disk indexes in place -- no restore.
#[sim_test]
async fn embedded_store_resumes_after_restart() {
    let mut cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .build()
        .await;
    let (name, rpc_url) = spawn_fullnode(&mut cluster, embedded_indexing_config()).await;

    // Index a transfer to the tip and confirm the APIs answer correctly.
    let transfer = transfer_to_fresh_address(&cluster, 12_345_000).await;
    let target = chain_tip(&cluster);
    wait_for_indexed(&cluster, &name, target).await;
    assert_transfer_indexed(&rpc_url, &transfer).await;

    // Restart with the same config: the on-disk indexes are in range, so
    // the store resumes rather than rebuilding.
    restart_fullnode(&cluster, &name, embedded_indexing_config()).await;
    assert_eq!(bootstrap_action(&cluster, &name), Some(Bootstrap::Resume));

    // The resumed store answers the same queries correctly.
    wait_for_indexed(&cluster, &name, target).await;
    assert_transfer_indexed(&rpc_url, &transfer).await;
}

/// Enabling the embedded store on a node that ran unindexed rebuilds the
/// indexes from the perpetual store: the live cohort bulk-loads to the tip
/// and the history cohort backfills from genesis (nothing was pruned).
#[sim_test]
async fn enabling_embedded_store_rebuilds_indexes() {
    let mut cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .build()
        .await;
    let (name, rpc_url) = spawn_fullnode(&mut cluster, no_indexing_config()).await;
    assert!(
        !has_embedded_store(&cluster, &name),
        "indexing is off, so the node should build no embedded store",
    );

    // Run a transfer while the node is unindexed.
    let transfer = transfer_to_fresh_address(&cluster, 7_000_000).await;
    let target = chain_tip(&cluster);

    // Turn on the embedded store and restart. With no prior rpc-store
    // database the live cohort has no watermark, so the store rebuilds it
    // (resuming any partial restore in place rather than clearing).
    restart_fullnode(&cluster, &name, embedded_indexing_config()).await;
    assert_eq!(
        bootstrap_action(&cluster, &name),
        Some(Bootstrap::Restore { clear: false }),
    );

    // After the rebuild + backfill the pre-enable transfer is visible
    // through both index cohorts.
    wait_for_indexed(&cluster, &name, target).await;
    assert_transfer_indexed(&rpc_url, &transfer).await;
}

/// Toggling indexing off, advancing the chain, then back on lets the store
/// resume from its frozen watermark (still within the available range) and
/// backfill the gap -- no rebuild.
#[sim_test]
async fn embedded_store_resumes_and_catches_up_after_indexing_gap() {
    let mut cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .build()
        .await;
    let (name, rpc_url) = spawn_fullnode(&mut cluster, embedded_indexing_config()).await;

    // Phase 1: index a first transfer to the tip.
    let transfer1 = transfer_to_fresh_address(&cluster, 4_000_000).await;
    wait_for_indexed(&cluster, &name, chain_tip(&cluster)).await;
    let frozen_watermark = live_committed(&cluster, &name).unwrap();

    // Phase 2: disable indexing and advance the chain past the watermark.
    // The node keeps executing checkpoints; the embedded watermark stays
    // frozen on disk.
    restart_fullnode(&cluster, &name, no_indexing_config()).await;
    assert!(!has_embedded_store(&cluster, &name));
    let transfer2 = transfer_to_fresh_address(&cluster, 9_000_000).await;
    for _ in 0..3 {
        transfer_to_fresh_address(&cluster, 1_000_000).await;
    }
    let executed_tip = chain_tip(&cluster);
    wait_for_executed(&cluster, &name, executed_tip).await;
    assert!(
        executed_tip > frozen_watermark,
        "chain ({executed_tip}) should advance past the frozen watermark ({frozen_watermark})",
    );

    // Phase 3: re-enable indexing. The frozen watermark is still within the
    // available range (nothing was pruned), so the store resumes and
    // backfills the gap rather than rebuilding.
    restart_fullnode(&cluster, &name, embedded_indexing_config()).await;
    assert_eq!(bootstrap_action(&cluster, &name), Some(Bootstrap::Resume));

    // After catching up, both the pre-gap and in-gap transfers are visible.
    wait_for_indexed(&cluster, &name, chain_tip(&cluster)).await;
    assert_transfer_indexed(&rpc_url, &transfer1).await;
    assert_transfer_indexed(&rpc_url, &transfer2).await;
}

/// When the available range advances past the embedded store's watermark
/// (its bulk-loaded indexes now reference pruned checkpoints), the bootstrap
/// wipes and rebuilds rather than resuming stale state.
#[sim_test]
async fn pruned_available_range_forces_rebuild() {
    let mut cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .build()
        .await;
    let (name, rpc_url) = spawn_fullnode(&mut cluster, embedded_indexing_config()).await;

    // Phase 1: index a transfer to the tip.
    let warmup = transfer_to_fresh_address(&cluster, 5_000_000).await;
    wait_for_indexed(&cluster, &name, chain_tip(&cluster)).await;
    let frozen_watermark = live_committed(&cluster, &name).unwrap();

    // Phase 2: disable indexing and advance the chain past the watermark.
    restart_fullnode(&cluster, &name, no_indexing_config()).await;
    for _ in 0..5 {
        transfer_to_fresh_address(&cluster, 1_000_000).await;
    }

    // Advance the available-range floor above the frozen index watermark by
    // bumping the checkpoint store's pruned watermark. Nothing is
    // physically deleted -- this only moves the floor the bootstrap
    // consults, simulating a perpetual-store prune that outran indexing.
    let prune_to = frozen_watermark + 2;
    wait_for_executed(&cluster, &name, prune_to + 1).await;
    with_node(&cluster, &name, |node| {
        let state = node.state();
        let checkpoint_store = state.get_checkpoint_store();
        let checkpoint = checkpoint_store
            .get_checkpoint_by_sequence_number(prune_to)
            .unwrap()
            .expect("checkpoint to prune to should exist");
        checkpoint_store
            .update_highest_pruned_checkpoint(&checkpoint)
            .unwrap();
    });

    // Phase 3: re-enable indexing. The live cohort's watermark now sits
    // below the available floor, so the store wipes and rebuilds.
    restart_fullnode(&cluster, &name, embedded_indexing_config()).await;
    assert_eq!(
        bootstrap_action(&cluster, &name),
        Some(Bootstrap::Restore { clear: true }),
    );

    // The rebuilt live cohort reflects full current state (the warmup
    // recipient still holds its coin), and a fresh transfer above the floor
    // is indexed end to end.
    let transfer = transfer_to_fresh_address(&cluster, 6_000_000).await;
    wait_for_indexed(&cluster, &name, chain_tip(&cluster)).await;
    assert_eq!(
        sui_balance(&rpc_url, warmup.receiver).await,
        warmup.amount,
        "rebuilt live cohort should reflect the pre-prune recipient's balance",
    );
    assert_transfer_indexed(&rpc_url, &transfer).await;
}
