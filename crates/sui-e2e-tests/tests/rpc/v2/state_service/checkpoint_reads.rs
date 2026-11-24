// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::TryStreamExt;
use prost_types::FieldMask;
use rand::rngs::OsRng;
use std::collections::BTreeMap;
use std::time::Duration;
use sui_config::RpcConfig;
use sui_macros::sim_test;
use sui_rpc::Client;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::BatchGetBalancesRequest;
use sui_rpc::proto::sui::rpc::v2::ErrorReason;
use sui_rpc::proto::sui::rpc::v2::GetBalanceRequest;
use sui_rpc::proto::sui::rpc::v2::ListBalancesRequest;
use sui_rpc::proto::sui::rpc::v2::ListDynamicFieldsRequest;
use sui_rpc::proto::sui::rpc::v2::ListObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::ObjectFilter;
use sui_rpc::proto::sui::rpc::v2::ObjectLiteral;
use sui_rpc::proto::sui::rpc::v2::ObjectTerm;
use sui_rpc::proto::sui::rpc::v2::ObjectTypeFilter;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2::QueryOptions;
use sui_rpc::proto::sui::rpc::v2::object_literal;
use sui_rpc::proto::sui::rpc::v2::owner::OwnerKind;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::digests::ObjectDigest;
use sui_types::storage::RpcIndexes;
use test_cluster::FullNodeHandle;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;

use crate::rpc::v2::RpcStatusExt;
use crate::rpc::v2::assert_above_latest_available;
use crate::rpc::v2::assert_below_earliest_available;
use crate::rpc::v2::error_info;

const SUI_COIN_TYPE: &str =
    "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI";

fn assert_not_served_here_disabled(status: &tonic::Status) {
    let status = status.rpc_status();
    assert_eq!(status.code, tonic::Code::Unimplemented as i32, "{status:?}");
    let info = error_info(&status);
    assert_eq!(info.reason, ErrorReason::NotServedHere.as_str_name());
    assert_eq!(info.domain, ErrorReason::DOMAIN);
    assert_eq!(
        info.metadata,
        BTreeMap::from([("cause".to_owned(), "disabled_by_config".to_owned())]),
    );
}

async fn list_objects_err(client: &mut Client, request: ListObjectsRequest) -> tonic::Status {
    let mut stream = match client.state_client().list_objects(request).await {
        Err(status) => return status,
        Ok(response) => response.into_inner(),
    };
    stream
        .message()
        .await
        .expect_err("expected ListObjects refusal before any frame")
}

async fn spawn_snapshot_fullnode(
    cluster: &mut TestCluster,
    snapshots_to_retain: usize,
) -> FullNodeHandle {
    let mut config = cluster
        .fullnode_config_builder()
        .with_disable_pruning(true)
        .with_rpc_config(RpcConfig {
            enable_indexing: Some(true),
            ..Default::default()
        })
        .build(&mut OsRng, cluster.swarm.config());
    config
        .authority_store_pruning_config
        .rpc_store_num_checkpoint_snapshots_to_retain = snapshots_to_retain;
    cluster.start_fullnode_from_config(config).await
}

async fn wait_for_snapshot_range(
    handle: &FullNodeHandle,
    predicate: impl Fn(u64, u64) -> bool,
) -> (u64, u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let range = handle.sui_node.with(|node| {
            node.embedded_rpc_store()
                .expect("fullnode runs rpc indexes")
                .reader()
                .db()
                .snapshot_range()
        });
        if let Some(range) = &range {
            let (lo, hi) = (*range.start(), *range.end());
            if predicate(lo, hi) {
                return (lo, hi);
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for fullnode snapshot range (last observed: {range:?})"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// When `rpc-store-num-checkpoint-snapshots-to-retain` is set to 0, every checkpoint-pinned
/// `StateService` read refuses with `UNIMPLEMENTED` / `NOT_SERVED_HERE` (`disabled_by_config`),
/// while unpinned reads still work.
#[sim_test]
async fn test_window_zero_refuses_pinned_reads() {
    let mut cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let snapshot_fn = spawn_snapshot_fullnode(&mut cluster, 0).await;
    // Wait for checkpoint 0 to be indexed so the unpinned read below sees genesis balances while
    // the snapshot window remains disabled (`None`).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let indexed = snapshot_fn.sui_node.with(|node| {
            let reader = node
                .embedded_rpc_store()
                .expect("fullnode runs rpc indexes")
                .reader();
            RpcIndexes::get_highest_indexed_checkpoint_seq_number(&reader).unwrap()
        });
        if indexed.is_some() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for window-0 fullnode to index checkpoint 0"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let mut client = Client::new(snapshot_fn.rpc_url.clone()).unwrap();
    let owner = cluster.get_address_0();

    let mut get_bal = GetBalanceRequest::default();
    get_bal.owner = Some(owner.to_string());
    get_bal.coin_type = Some(SUI_COIN_TYPE.to_owned());
    get_bal.at_checkpoint = Some(0);
    assert_not_served_here_disabled(
        &client
            .state_client()
            .get_balance(get_bal.clone())
            .await
            .unwrap_err(),
    );

    let mut batch_bal = BatchGetBalancesRequest::default();
    batch_bal.requests = vec![get_bal];
    assert_not_served_here_disabled(
        &client
            .state_client()
            .batch_get_balances(batch_bal)
            .await
            .unwrap_err(),
    );

    let mut list_bal = ListBalancesRequest::default();
    list_bal.owner = Some(owner.to_string());
    list_bal.options = Some(QueryOptions::default());
    assert_not_served_here_disabled(
        &client
            .state_client()
            .list_balances(list_bal)
            .await
            .unwrap_err(),
    );

    let mut list_owned = ListOwnedObjectsRequest::default();
    list_owned.owner = Some(owner.to_string());
    list_owned.options = Some(QueryOptions::default());
    assert_not_served_here_disabled(
        &client
            .state_client()
            .list_owned_objects(list_owned)
            .await
            .unwrap_err(),
    );

    let mut list_df = ListDynamicFieldsRequest::default();
    list_df.parent = Some(ObjectID::from_single_byte(5).to_string());
    list_df.at_checkpoint = Some(0);
    assert_not_served_here_disabled(
        &client
            .state_client()
            .list_dynamic_fields(list_df)
            .await
            .unwrap_err(),
    );

    assert_not_served_here_disabled(
        &list_objects_err(&mut client, ListObjectsRequest::default()).await,
    );

    // Unpinned current-state read still succeeds on a window-0 fullnode.
    let mut unpinned = GetBalanceRequest::default();
    unpinned.owner = Some(owner.to_string());
    unpinned.coin_type = Some(SUI_COIN_TYPE.to_owned());
    let balance = client
        .state_client()
        .get_balance(unpinned)
        .await
        .unwrap()
        .into_inner()
        .balance
        .unwrap();
    assert!(balance.balance() > 0);
}

/// A cursor-paged listing started without `at_checkpoint` pins to the first page's checkpoint and
/// keeps reading that exact checkpoint across later pages even after transactions mutate state in
/// newer checkpoints.
#[sim_test]
async fn test_paging_across_checkpoints_stays_pinned_while_state_changes() {
    // Default fullnode config retains 15,000 snapshots (~1h), so consistent pagination works out
    // of the box without extra snapshot configuration.
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .build()
        .await;
    let snapshot_fn = &cluster.fullnode_handle;
    let mut client = Client::new(cluster.rpc_url().to_owned()).unwrap();

    let sender = cluster.get_address_0();
    let receiver = cluster.get_address_1();

    // Wait for the default fullnode to retain at least one snapshot before recording `initial_hi`.
    let (_, initial_hi) = wait_for_snapshot_range(snapshot_fn, |_, _| true).await;

    // Record sender's exact balance and owned objects as of `initial_hi`.
    let mut bal_req = GetBalanceRequest::default();
    bal_req.owner = Some(sender.to_string());
    bal_req.coin_type = Some(SUI_COIN_TYPE.to_owned());
    bal_req.at_checkpoint = Some(initial_hi);
    let balance_before = client
        .state_client()
        .get_balance(bal_req)
        .await
        .unwrap()
        .into_inner()
        .balance
        .unwrap()
        .balance();

    // Start cursor-paging sender's owned objects with limit = 1 (pins to newest servable checkpoint).
    let mut opts1 = QueryOptions::default();
    opts1.limit = Some(1);
    let mut req1 = ListOwnedObjectsRequest::default();
    req1.owner = Some(sender.to_string());
    req1.read_mask = Some(FieldMask::from_str("object_id,version,balance,digest"));
    req1.options = Some(opts1);
    let page1 = client
        .state_client()
        .list_owned_objects(req1)
        .await
        .unwrap()
        .into_inner();
    assert_eq!(page1.objects.len(), 1);
    assert_eq!(page1.cursors.len(), 1);
    assert_eq!(
        page1.end.as_ref().unwrap().reason(),
        QueryEndReason::ItemLimit
    );

    let mut baseline_req = ListOwnedObjectsRequest::default();
    baseline_req.owner = Some(sender.to_string());
    baseline_req.read_mask = Some(FieldMask::from_str("object_id,version,balance,digest"));
    baseline_req.at_checkpoint = Some(initial_hi);
    baseline_req.options = Some(QueryOptions::default());
    let expected_objects: Vec<(String, u64, u64, String)> = client
        .state_client()
        .list_owned_objects(baseline_req)
        .await
        .unwrap()
        .into_inner()
        .objects
        .into_iter()
        .map(|o| {
            (
                o.object_id().to_owned(),
                o.version(),
                o.balance(),
                o.digest().to_owned(),
            )
        })
        .collect();
    assert!(
        expected_objects.len() >= 2,
        "sender should own multiple gas coins at genesis"
    );
    assert!(
        expected_objects
            .iter()
            .all(|(_, _, _, digest)| !digest.is_empty()),
        "read_mask includes digest so every loaded object must carry a digest"
    );

    // Execute a transfer paying gas with the last coin in `expected_objects` so the mutated coin
    // appears on a cursor-resumed page fetched after the transfer (not on `page1`).
    let transfer_amount = 1_000_000;
    let (last_id, last_version, _, last_digest) = expected_objects.last().unwrap();
    let gas_ref = (
        ObjectID::from_str(last_id).unwrap(),
        SequenceNumber::from_u64(*last_version),
        ObjectDigest::from_str(last_digest).unwrap(),
    );
    let gas_price = cluster.get_reference_gas_price().await;
    let tx_data = TestTransactionBuilder::new(sender, gas_ref, gas_price)
        .transfer_sui(Some(transfer_amount), receiver)
        .build();
    let txn = cluster.wallet.sign_transaction(&tx_data).await;
    let executed = super::super::execute_transaction(&mut client, &txn).await;
    let tx_checkpoint = executed.checkpoint();

    // Wait for the fullnode to commit and snapshot `tx_checkpoint`.
    wait_for_snapshot_range(snapshot_fn, |_, hi| hi >= tx_checkpoint).await;

    // Finish paging sender's owned objects from page 1's cursor: every remaining page must still
    // see the pre-transfer checkpoint state.
    let mut paged: Vec<(String, u64, u64, String)> = page1
        .objects
        .iter()
        .map(|o| {
            (
                o.object_id().to_owned(),
                o.version(),
                o.balance(),
                o.digest().to_owned(),
            )
        })
        .collect();
    let mut after = page1.cursors[0].clone();
    loop {
        let mut opts = QueryOptions::default();
        opts.limit = Some(1);
        opts.after = Some(after);
        let mut req = ListOwnedObjectsRequest::default();
        req.owner = Some(sender.to_string());
        req.read_mask = Some(FieldMask::from_str("object_id,version,balance,digest"));
        req.options = Some(opts);
        let page = client
            .state_client()
            .list_owned_objects(req)
            .await
            .unwrap()
            .into_inner();
        for o in &page.objects {
            paged.push((
                o.object_id().to_owned(),
                o.version(),
                o.balance(),
                o.digest().to_owned(),
            ));
        }
        match page.end.as_ref().unwrap().reason() {
            QueryEndReason::ItemLimit => {
                after = page.cursors.last().unwrap().clone();
            }
            QueryEndReason::CheckpointBound => break,
            other => panic!("unexpected end reason: {other:?}"),
        }
    }
    assert_eq!(paged, expected_objects);

    // Pinned legacy `page_token` paging stays at `initial_hi` even when follow-up requests omit
    // `at_checkpoint`.
    let mut legacy_paged: Vec<(String, u64, u64, String)> = Vec::new();
    let mut page_token = None;
    loop {
        let mut req = ListOwnedObjectsRequest::default();
        req.owner = Some(sender.to_string());
        req.read_mask = Some(FieldMask::from_str("object_id,version,balance,digest"));
        req.page_size = Some(1);
        if page_token.is_none() {
            req.at_checkpoint = Some(initial_hi);
        }
        req.page_token = page_token.take();
        let page = client
            .state_client()
            .list_owned_objects(req)
            .await
            .unwrap()
            .into_inner();
        assert!(page.cursors.is_empty());
        assert!(page.end.is_none());
        for o in &page.objects {
            legacy_paged.push((
                o.object_id().to_owned(),
                o.version(),
                o.balance(),
                o.digest().to_owned(),
            ));
        }
        match page.next_page_token {
            Some(token) => page_token = Some(token),
            None => break,
        }
    }
    assert_eq!(legacy_paged, expected_objects);

    // BatchGetBalances reads pre-transfer and post-transfer balances in one batch.
    let mut before_item = GetBalanceRequest::default();
    before_item.owner = Some(sender.to_string());
    before_item.coin_type = Some(SUI_COIN_TYPE.to_owned());
    before_item.at_checkpoint = Some(initial_hi);
    let mut after_item = GetBalanceRequest::default();
    after_item.owner = Some(sender.to_string());
    after_item.coin_type = Some(SUI_COIN_TYPE.to_owned());
    after_item.at_checkpoint = Some(tx_checkpoint);
    let mut zero_item = GetBalanceRequest::default();
    zero_item.owner = Some(SuiAddress::random_for_testing_only().to_string());
    zero_item.coin_type = Some(SUI_COIN_TYPE.to_owned());
    zero_item.at_checkpoint = Some(tx_checkpoint);

    let mut batch_req = BatchGetBalancesRequest::default();
    batch_req.requests = vec![before_item, after_item, zero_item];
    let batch_resp = client
        .state_client()
        .batch_get_balances(batch_req)
        .await
        .unwrap()
        .into_inner();
    assert_eq!(batch_resp.balances.len(), 3);
    assert_eq!(batch_resp.balances[0].balance(), balance_before);
    assert!(batch_resp.balances[1].balance() < balance_before - transfer_amount);
    assert_eq!(batch_resp.balances[2].balance(), 0);

    // Server-streaming ListObjects with a filter lists shared objects / coins cleanly.
    let mut tf = ObjectTypeFilter::default();
    tf.object_type = Some("0x3::sui_system::SuiSystemState".to_owned());
    let mut lit = ObjectLiteral::default();
    lit.predicate = Some(object_literal::Predicate::ObjectType(tf));
    let mut term = ObjectTerm::default();
    term.literals = vec![lit];
    let mut filter = ObjectFilter::default();
    filter.terms = vec![term];

    let mut list_objects_req = ListObjectsRequest::default();
    list_objects_req.filter = Some(filter);
    list_objects_req.at_checkpoint = Some(tx_checkpoint);
    let frames: Vec<_> = client
        .state_client()
        .list_objects(list_objects_req)
        .await
        .unwrap()
        .into_inner()
        .try_collect()
        .await
        .unwrap();
    assert_eq!(frames.len(), 2); // 1 SuiSystemState object + 1 empty terminal frame
    assert!(frames[0].object.is_some());
    assert!(frames[0].cursor.is_some());
    assert!(frames[0].end.is_none());
    assert!(frames[1].object.is_none());
    assert!(frames[1].cursor.is_none());
    assert_eq!(
        frames[1].end.as_ref().unwrap().reason(),
        QueryEndReason::CheckpointBound
    );

    // OwnerKind::Shared lists shared objects including 0x5 (SuiSystemState).
    let mut shared_req = ListOwnedObjectsRequest::default();
    shared_req.owner_kind = Some(OwnerKind::Shared as i32);
    shared_req.at_checkpoint = Some(tx_checkpoint);
    shared_req.options = Some(QueryOptions::default());
    let shared_resp = client
        .state_client()
        .list_owned_objects(shared_req)
        .await
        .unwrap()
        .into_inner();
    assert!(
        shared_resp
            .objects
            .iter()
            .any(|o| o.object_id() == ObjectID::from_single_byte(5).to_canonical_string(true))
    );
}

/// `at_checkpoint` below the retained snapshot window returns `BELOW_EARLIEST_AVAILABLE` and above
/// the highest retained snapshot returns `ABOVE_LATEST_AVAILABLE`.
#[sim_test]
async fn test_at_checkpoint_below_and_above_window() {
    let mut cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let snapshot_fn = spawn_snapshot_fullnode(&mut cluster, 2).await;
    let mut client = Client::new(snapshot_fn.rpc_url.clone()).unwrap();
    let owner = cluster.get_address_0();

    // Wait until at least one snapshot exists, then mint a cursor without `at_checkpoint` so it
    // atomically pins to the latest snapshot without racing catch-up eviction.
    wait_for_snapshot_range(&snapshot_fn, |_, _| true).await;
    let mut early_opts = QueryOptions::default();
    early_opts.limit = Some(1);
    let mut early_req = ListOwnedObjectsRequest::default();
    early_req.owner = Some(owner.to_string());
    early_req.options = Some(early_opts);
    let evicted_cursor = client
        .state_client()
        .list_owned_objects(early_req)
        .await
        .unwrap()
        .into_inner()
        .cursors
        .into_iter()
        .next()
        .expect("genesis owner has at least one object");

    // Any `hi` observed after `early_req` succeeded is an upper bound on the cursor's checkpoint;
    // wait until it (and checkpoints 0..=1) have been evicted from the 2-checkpoint window.
    let (_, early_upper_bound) = wait_for_snapshot_range(&snapshot_fn, |_, _| true).await;
    wait_for_snapshot_range(&snapshot_fn, |lo, hi| {
        lo > early_upper_bound && lo >= 2 && hi >= 3
    })
    .await;
    // Halt the default fullnode and all validators so `snapshot_fn` has no remaining peer to sync
    // newer checkpoints from, then wait for its own state to settle so `lo..=hi` is stationary.
    let default_fn = cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.state().name);
    cluster.stop_node(&default_fn);
    cluster.stop_all_validators().await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let (lo, hi) = loop {
        let (synced, executed, range) = snapshot_fn.sui_node.with(|node| {
            let state = node.state();
            let cs = state.get_checkpoint_store();
            let range = node
                .embedded_rpc_store()
                .expect("fullnode runs rpc indexes")
                .reader()
                .db()
                .snapshot_range();
            (
                cs.get_highest_synced_checkpoint_seq_number().unwrap(),
                cs.get_highest_executed_checkpoint_seq_number().unwrap(),
                range,
            )
        });
        if let (Some(exec), Some(range)) = (executed, range)
            && synced == Some(exec)
            && *range.end() == exec
        {
            break (*range.start(), *range.end());
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for halted snapshot fullnode to settle"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert!(lo >= 2);
    assert_eq!(hi - lo + 1, 2);

    // Evicted cursor returns BELOW_EARLIEST_AVAILABLE (floor = lo), and mismatched at_checkpoint
    // returns INVALID_ARGUMENT.
    let mut evicted_opts = QueryOptions::default();
    evicted_opts.after = Some(evicted_cursor.clone());
    let mut evicted_req = ListOwnedObjectsRequest::default();
    evicted_req.owner = Some(owner.to_string());
    evicted_req.options = Some(evicted_opts);
    assert_below_earliest_available(
        &client
            .state_client()
            .list_owned_objects(evicted_req)
            .await
            .unwrap_err(),
        lo,
    );

    let mut mismatched_opts = QueryOptions::default();
    mismatched_opts.after = Some(evicted_cursor);
    let mut mismatched_req = ListOwnedObjectsRequest::default();
    mismatched_req.owner = Some(owner.to_string());
    mismatched_req.at_checkpoint = Some(hi);
    mismatched_req.options = Some(mismatched_opts);
    assert_eq!(
        client
            .state_client()
            .list_owned_objects(mismatched_req)
            .await
            .unwrap_err()
            .code(),
        tonic::Code::InvalidArgument
    );
    // Below window -> BELOW_EARLIEST_AVAILABLE (floor = lo).
    let mut below_bal = GetBalanceRequest::default();
    below_bal.owner = Some(owner.to_string());
    below_bal.coin_type = Some(SUI_COIN_TYPE.to_owned());
    below_bal.at_checkpoint = Some(lo - 1);
    assert_below_earliest_available(
        &client
            .state_client()
            .get_balance(below_bal)
            .await
            .unwrap_err(),
        lo,
    );

    let mut below_obj = ListObjectsRequest::default();
    below_obj.at_checkpoint = Some(0);
    assert_below_earliest_available(&list_objects_err(&mut client, below_obj).await, lo);

    // Exact upper boundary `hi + 1` -> ABOVE_LATEST_AVAILABLE (bound = "at_checkpoint").
    let above_cp = hi + 1;
    let mut above_bal = GetBalanceRequest::default();
    above_bal.owner = Some(owner.to_string());
    above_bal.coin_type = Some(SUI_COIN_TYPE.to_owned());
    above_bal.at_checkpoint = Some(above_cp);
    assert_above_latest_available(
        &client
            .state_client()
            .get_balance(above_bal)
            .await
            .unwrap_err(),
        "at_checkpoint",
        above_cp,
    );

    let mut above_owned = ListOwnedObjectsRequest::default();
    above_owned.owner = Some(owner.to_string());
    above_owned.at_checkpoint = Some(above_cp);
    assert_above_latest_available(
        &client
            .state_client()
            .list_owned_objects(above_owned)
            .await
            .unwrap_err(),
        "at_checkpoint",
        above_cp,
    );

    // Endpoints at the exact retained window edges (`lo` and `hi`) succeed.
    for cp in [lo, hi] {
        let mut ok_bal = GetBalanceRequest::default();
        ok_bal.owner = Some(owner.to_string());
        ok_bal.coin_type = Some(SUI_COIN_TYPE.to_owned());
        ok_bal.at_checkpoint = Some(cp);
        client
            .state_client()
            .get_balance(ok_bal)
            .await
            .expect("retained window edge should succeed");
    }
}
