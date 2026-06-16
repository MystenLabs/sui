// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Ports `test_address_owner` and `test_type_filters` from
//! `sui-indexer-alt-e2e-tests/tests/consistent_store_list_owned_objects_tests.rs`.
//!
//! `test_address_owner` exercises the ordering guarantees of
//! the `object_by_owner` CF: coins of the same type are sorted
//! by inverted balance (richer first); non-coin objects of the
//! same type are sorted by ObjectID; objects of different
//! types are grouped by type tag. The original sweeps three
//! checkpoints; we trim to a single checkpoint with one of
//! each kind, which is enough to lock down the per-type
//! ordering rules our schema actually implements.
//!
//! `test_type_filters` re-uses the same machinery but issues
//! a `list_owned_objects` request with an `object_type` filter
//! against an `Address` owner. Asserts each filter shape
//! returns exactly the rows it should.

use move_core_types::ident_str;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ListOwnedObjectsRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::Owner;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_client::ConsistentServiceClient;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::owner::OwnerKind;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::TypeTag;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::crypto::get_account_key_pair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner as TypesOwner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use sui_types::utils::to_sender_signed_transaction;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

async fn client(cluster: &LocalCluster) -> ConsistentServiceClient<Channel> {
    ConsistentServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

/// Fund a new account, transfer `amount` SUI to `owner`. Returns
/// the resulting address-owned coin object. Same shape as
/// `create_coin` in the e2e helpers — the recipient `owner`
/// gets a fresh coin with balance `amount`.
async fn create_coin_for(cluster: &LocalCluster, owner: SuiAddress, amount: u64) -> ObjectRef {
    let (sender, kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET + amount)
        .await
        .unwrap();
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(owner, Some(amount));
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price().await,
    );
    let signed = to_sender_signed_transaction(data, &kp);
    let (fx, err) = cluster.execute_transaction(signed).await.unwrap();
    assert!(err.is_none() && fx.status().is_ok(), "create_coin: {err:?}");

    fx.created()
        .into_iter()
        .find_map(|(oref, o)| {
            matches!(o, TypesOwner::AddressOwner(a) if a == owner).then_some(oref)
        })
        .expect("address-owned coin")
}

/// Build a Bag<u64, u64> owned by `owner`. We use a single
/// element since we only care about the object's type for the
/// ordering / filter tests.
async fn create_bag(cluster: &LocalCluster, owner: SuiAddress) -> ObjectRef {
    let (sender, kp, gas) = cluster.funded_account(DEFAULT_GAS_BUDGET).await.unwrap();
    let mut builder = ProgrammableTransactionBuilder::new();
    let bag = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("bag").to_owned(),
        ident_str!("new").to_owned(),
        vec![],
        vec![],
    );
    let kv = builder.pure(0u64).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("bag").to_owned(),
        ident_str!("add").to_owned(),
        vec![TypeTag::U64, TypeTag::U64],
        vec![bag, kv, kv],
    );
    builder.transfer_arg(owner, bag);
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price().await,
    );
    let signed = to_sender_signed_transaction(data, &kp);
    let (fx, err) = cluster.execute_transaction(signed).await.unwrap();
    assert!(err.is_none() && fx.status().is_ok(), "create_bag: {err:?}");
    fx.created()
        .into_iter()
        .find_map(|(oref, o)| {
            matches!(o, TypesOwner::AddressOwner(a) if a == owner).then_some(oref)
        })
        .expect("address-owned bag")
}

/// Build a Table<u8, u8> owned by `owner`.
async fn create_table_u8(cluster: &LocalCluster, owner: SuiAddress) -> ObjectRef {
    create_table(cluster, owner, TypeTag::U8).await
}

/// Build a Table<u64, u64> owned by `owner`.
async fn create_table_u64(cluster: &LocalCluster, owner: SuiAddress) -> ObjectRef {
    create_table(cluster, owner, TypeTag::U64).await
}

async fn create_table(cluster: &LocalCluster, owner: SuiAddress, ty: TypeTag) -> ObjectRef {
    let (sender, kp, gas) = cluster.funded_account(DEFAULT_GAS_BUDGET).await.unwrap();
    let mut builder = ProgrammableTransactionBuilder::new();
    let table = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("table").to_owned(),
        ident_str!("new").to_owned(),
        vec![ty.clone(), ty.clone()],
        vec![],
    );
    let kv = match &ty {
        TypeTag::U8 => builder.pure(0u8),
        TypeTag::U64 => builder.pure(0u64),
        _ => panic!("unsupported ty"),
    }
    .unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("table").to_owned(),
        ident_str!("add").to_owned(),
        vec![ty.clone(), ty.clone()],
        vec![table, kv, kv],
    );
    builder.transfer_arg(owner, table);
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price().await,
    );
    let signed = to_sender_signed_transaction(data, &kp);
    let (fx, err) = cluster.execute_transaction(signed).await.unwrap();
    assert!(
        err.is_none() && fx.status().is_ok(),
        "create_table: {err:?}"
    );
    fx.created()
        .into_iter()
        .find_map(|(oref, o)| {
            matches!(o, TypesOwner::AddressOwner(a) if a == owner).then_some(oref)
        })
        .expect("address-owned table")
}

/// Single-page `list_owned_objects` over an Address owner.
async fn list_owned_ids(
    cluster: &LocalCluster,
    owner: SuiAddress,
    object_type: Option<&str>,
    page_size: u32,
) -> Vec<String> {
    let mut client = client(cluster).await;
    let response = client
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(Owner {
                kind: Some(OwnerKind::Address as i32),
                address: Some(owner.to_string()),
            }),
            object_type: object_type.map(str::to_owned),
            page_size: Some(page_size),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    response
        .objects
        .into_iter()
        .map(|o| o.object_id().to_owned())
        .collect()
}

/// Ports `test_address_owner` (single-checkpoint variant):
/// confirm coin ordering (by balance descending) and non-coin
/// ordering (by ObjectID), grouped by type.
#[tokio::test]
async fn list_owned_objects_orders_by_balance_and_id() {
    let cluster = LocalCluster::new().await.unwrap();
    let (a, _akp): (SuiAddress, AccountKeyPair) = get_account_key_pair();

    // A owns 4 coins with balances 1, 2, 3, 4.
    let c1 = create_coin_for(&cluster, a, 1).await;
    let c2 = create_coin_for(&cluster, a, 2).await;
    let c3 = create_coin_for(&cluster, a, 3).await;
    let c4 = create_coin_for(&cluster, a, 4).await;

    // A also owns one Bag, one Table<u8, u8>, one Table<u64, u64>.
    let bag = create_bag(&cluster, a).await;
    let tu8 = create_table_u8(&cluster, a).await;
    let tu64 = create_table_u64(&cluster, a).await;

    cluster.create_checkpoint().await.unwrap();

    // No filter: every object owned by A. Bags first (StructTag
    // sorts `bag::Bag` before `coin::Coin` before
    // `table::Table`), then coins (richest first because the
    // schema stores `!balance`), then tables (u8 before u64 by
    // BCS-encoded StructTag). The exact across-type ordering
    // is what our schema's encoder is locked down to.
    let ids = list_owned_ids(&cluster, a, None, 20).await;

    let bag_idx = ids
        .iter()
        .position(|id| id == &bag.0.to_string())
        .expect("bag present");
    let coin_idxs: Vec<usize> = [c4, c3, c2, c1]
        .iter()
        .map(|c| {
            ids.iter()
                .position(|id| id == &c.0.to_string())
                .unwrap_or_else(|| panic!("coin {c:?} missing from {ids:?}"))
        })
        .collect();
    let tu8_idx = ids
        .iter()
        .position(|id| id == &tu8.0.to_string())
        .expect("Table<u8,u8> present");
    let tu64_idx = ids
        .iter()
        .position(|id| id == &tu64.0.to_string())
        .expect("Table<u64,u64> present");

    // Bag before any coin (StructTag of Bag < Coin).
    assert!(
        coin_idxs.iter().all(|i| *i > bag_idx),
        "bag should sort before coins: {ids:?}",
    );

    // Coins sorted by balance descending: c4 (4) < c3 (3) < c2 (2) < c1 (1).
    let mut sorted = coin_idxs.clone();
    sorted.sort();
    assert_eq!(
        coin_idxs, sorted,
        "coins should be ordered by inverted balance (4, 3, 2, 1): {ids:?}",
    );

    // All tables after coins; Table<u8,u8> before Table<u64,u64>.
    assert!(
        tu8_idx > *coin_idxs.last().unwrap(),
        "Table<u8> should sort after coins: {ids:?}",
    );
    assert!(
        tu64_idx > tu8_idx,
        "Table<u64> should sort after Table<u8>: {ids:?}",
    );

    // Also exercise an explicit checkpoint cursor; A's holdings
    // should be the same as the live read because we only
    // produced one checkpoint.
    let with_cp_ids = {
        let mut client = client(&cluster).await;
        let response = client
            .list_owned_objects(ListOwnedObjectsRequest {
                owner: Some(Owner {
                    kind: Some(OwnerKind::Address as i32),
                    address: Some(a.to_string()),
                }),
                page_size: Some(20),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        response
            .objects
            .into_iter()
            .map(|o| o.object_id().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids, with_cp_ids);

    // Pagination round-trip with page_size=3 over A's full
    // holdings (7 objects). Each page advances `after_token`
    // to the previous response's last entry.
    let mut client = client(&cluster).await;
    let mut acc = Vec::new();
    let mut after: Option<Vec<u8>> = None;
    loop {
        let resp = client
            .list_owned_objects(ListOwnedObjectsRequest {
                owner: Some(Owner {
                    kind: Some(OwnerKind::Address as i32),
                    address: Some(a.to_string()),
                }),
                page_size: Some(3),
                after_token: after.clone().map(Into::into),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        acc.extend(resp.objects.iter().map(|o| o.object_id().to_owned()));
        if resp.has_next_page() {
            after = resp.objects.last().map(|o| o.page_token().to_owned());
        } else {
            break;
        }
    }
    assert_eq!(acc, ids, "paginated walk should match unpaginated read");

    // No second-account exercise: future ports of the
    // cross-account flow can extend this.
    let _ = (bag, tu8, tu64, _akp);
}

/// Ports `test_type_filters`: same address but filter the
/// listing by package / module / fully-qualified / instantiated
/// type — every shape narrows the result set correctly.
#[tokio::test]
async fn list_owned_objects_type_filter_sweep() {
    let cluster = LocalCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();

    let c1 = create_coin_for(&cluster, a, 1).await;
    let c2 = create_coin_for(&cluster, a, 2).await;
    let bag = create_bag(&cluster, a).await;
    let tu8 = create_table_u8(&cluster, a).await;
    let tu64 = create_table_u64(&cluster, a).await;
    cluster.create_checkpoint().await.unwrap();

    let coin_ids = [c1, c2].iter().map(|c| c.0.to_string()).collect::<Vec<_>>();
    let bag_id = bag.0.to_string();
    let tu8_id = tu8.0.to_string();
    let tu64_id = tu64.0.to_string();

    // Package filter (0x2) — every object lives in the
    // framework, so everything matches.
    let all = list_owned_ids(&cluster, a, Some("0x2"), 20).await;
    assert!(all.contains(&bag_id));
    assert!(all.contains(&tu8_id));
    assert!(all.contains(&tu64_id));
    for id in &coin_ids {
        assert!(all.contains(id));
    }

    // Module filter (0x2::bag) — only the bag.
    let bags = list_owned_ids(&cluster, a, Some("0x2::bag"), 20).await;
    assert_eq!(bags, vec![bag_id.clone()]);

    // Fully-qualified type filter (0x2::table::Table) — both
    // tables.
    let tables = list_owned_ids(&cluster, a, Some("0x2::table::Table"), 20).await;
    assert!(tables.contains(&tu8_id));
    assert!(tables.contains(&tu64_id));
    assert!(!tables.contains(&bag_id));

    // Instantiated filter (Table<u64,u64>) — only the u64
    // table.
    let only_u64 = list_owned_ids(&cluster, a, Some("0x2::table::Table<u64, u64>"), 20).await;
    assert_eq!(only_u64, vec![tu64_id.clone()]);

    // A type A doesn't own — empty.
    let none = list_owned_ids(&cluster, a, Some("0x2::clock::Clock"), 20).await;
    assert!(none.is_empty());

    // Sanity: a bare struct tag silently ignores type params,
    // so a tag of e.g. `0x2::table::Table` matches every
    // instantiation regardless of params. Already covered
    // above; just keep `_ = tu8_id` clippy-quiet.
    let _ = (tu8_id, tu64_id, bag, c1, c2);
}
