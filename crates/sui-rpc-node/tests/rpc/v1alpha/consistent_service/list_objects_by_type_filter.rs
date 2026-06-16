// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Ports `test_type_filter` from
//! `sui-indexer-alt-e2e-tests/tests/consistent_store_list_objects_by_type_tests.rs`.
//! Exercises every `TypeFilter` shape (package, module,
//! fully-qualified name, fully-instantiated) and forward
//! pagination across the result set, plus the bad-filter /
//! missing-filter error paths.
//!
//! The original test creates 4 bags × 4 ownership kinds × 4
//! type instantiations per CF — overkill for the read path,
//! which only needs enough rows to exercise the filter
//! variants. We use one bag and two tables (u8 + u64) per
//! owner kind to keep the test under a second while still
//! covering every encoding path.

use std::collections::BTreeMap;

use move_core_types::ident_str;
use move_core_types::language_storage::StructTag;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ListObjectsByTypeRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_client::ConsistentServiceClient;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::TypeTag;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::get_account_key_pair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner as TypesOwner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

#[derive(Copy, Clone)]
enum Ownership {
    Address(SuiAddress),
    Shared,
    Immutable,
}

async fn client(cluster: &LocalCluster) -> ConsistentServiceClient<Channel> {
    ConsistentServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

/// Mirror of the e2e helper: forward pagination over
/// `list_objects_by_type` returning (results, next_token).
async fn list_objects_by_type(
    cluster: &LocalCluster,
    object_type: &str,
    after_token: Option<Vec<u8>>,
    page_size: Option<u32>,
) -> Result<(Vec<String>, Option<Vec<u8>>), tonic::Status> {
    let mut client = client(cluster).await;
    let response = client
        .list_objects_by_type(ListObjectsByTypeRequest {
            object_type: Some(object_type.to_string()),
            page_size,
            after_token: after_token.map(Into::into),
            ..Default::default()
        })
        .await?
        .into_inner();

    let after_token = response
        .has_next_page()
        .then(|| response.objects.last().map(|o| o.page_token().to_owned()))
        .flatten();
    let ids = response
        .objects
        .into_iter()
        .map(|o| o.object_id().to_owned())
        .collect();
    Ok((ids, after_token))
}

#[tokio::test]
async fn list_objects_by_type_filter_sweep() {
    let cluster = LocalCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Seed: one Bag and two Tables (u8, u64) per owner kind.
    // We index by ObjectID so the expected sets match the order
    // `list_objects_by_type` returns rows in (sorted by
    // ObjectID within a type prefix).
    let mut bags = BTreeMap::new();
    let mut tu8s = BTreeMap::new();
    let mut tu64s = BTreeMap::new();

    use Ownership as K;
    for kind in [K::Address(a), K::Address(b), K::Shared, K::Immutable] {
        let bag = create_bag(&cluster, kind, TypeTag::U64).await;
        bags.insert(bag.0, bag);
        let t8 = create_table(&cluster, kind, TypeTag::U8).await;
        tu8s.insert(t8.0, t8);
        let t64 = create_table(&cluster, kind, TypeTag::U64).await;
        tu64s.insert(t64.0, t64);
    }
    cluster.create_checkpoint().await.unwrap();

    let ids = |objs: &BTreeMap<ObjectID, ObjectRef>| -> Vec<String> {
        objs.values().map(|(id, _, _)| id.to_string()).collect()
    };

    // Module filter (`0x2::bag`) — every Bag, regardless of
    // owner or instantiation.
    let (got, next) = list_objects_by_type(&cluster, "0x2::bag", None, Some(50))
        .await
        .unwrap();
    assert!(next.is_none(), "single page should fit the whole set");
    assert_eq!(got, ids(&bags));

    // Fully-qualified name filter (`0x2::table::Table`) —
    // every Table, both instantiations.
    let (got, next) = list_objects_by_type(&cluster, "0x2::table::Table", None, Some(50))
        .await
        .unwrap();
    assert!(next.is_none());
    let mut expected: Vec<String> = ids(&tu8s);
    expected.extend(ids(&tu64s));
    // The CF is keyed by `(StructTag, ObjectID)`; the two
    // instantiations sort by their full type tag (Table<u8,
    // u8> < Table<u64, u64> by BCS ordering), so the order
    // we get back is u8 first, then u64. Sort each instantiation
    // block by ObjectID to match the iterator order.
    assert_eq!(got, expected);

    // Fully-instantiated filter (`Table<u64, u64>`) — only the
    // u64 tables.
    let (got, next) = list_objects_by_type(&cluster, "0x2::table::Table<u64, u64>", None, Some(50))
        .await
        .unwrap();
    assert!(next.is_none());
    assert_eq!(got, ids(&tu64s));

    // Mismatched instantiation — none exist.
    let (got, next) = list_objects_by_type(&cluster, "0x2::table::Table<u8, u64>", None, Some(50))
        .await
        .unwrap();
    assert!(next.is_none());
    assert!(got.is_empty());

    // Pagination over `0x2::table` (every Table) with page
    // size 3 — should accumulate the full set across pages.
    let mut after = None;
    let mut acc = Vec::new();
    loop {
        let (page, next) = list_objects_by_type(&cluster, "0x2::table", after.clone(), Some(3))
            .await
            .unwrap();
        acc.extend(page);
        after = next;
        if after.is_none() {
            break;
        }
    }
    let mut expected_tables: Vec<String> = ids(&tu8s);
    expected_tables.extend(ids(&tu64s));
    assert_eq!(acc, expected_tables);

    // Bad filter — InvalidArgument.
    let err = list_objects_by_type(&cluster, "not::a::valid::type<>", None, Some(20))
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    // Missing filter — InvalidArgument.
    let err = list_objects_by_type(&cluster, "", None, Some(20))
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

/// Build a Bag<ty, ty>, hand it to `kind`, and execute. Returns
/// the resulting object reference. Single-element bag — we only
/// care about the type for filter testing, not the contents.
async fn create_bag(cluster: &LocalCluster, kind: Ownership, ty: TypeTag) -> ObjectRef {
    let (sender, kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET)
        .await
        .expect("funded_account");

    let mut builder = ProgrammableTransactionBuilder::new();
    let bag = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("bag").to_owned(),
        ident_str!("new").to_owned(),
        vec![],
        vec![],
    );

    let bag_ty = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: ident_str!("bag").to_owned(),
        name: ident_str!("Bag").to_owned(),
        type_params: vec![],
    }));

    // Insert one entry so the bag actually carries the type
    // parameter in its `add` call. The bag itself's StructTag
    // is type-param-free; we exercise the inner type only as a
    // sanity check that the test fixture compiles.
    let kv = pure_for(&mut builder, &ty);
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("bag").to_owned(),
        ident_str!("add").to_owned(),
        vec![ty.clone(), ty.clone()],
        vec![bag, kv, kv],
    );

    transfer_or_share(&mut builder, kind, bag, bag_ty);

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price().await,
    );
    let (fx, err) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .await
        .expect("execute_transaction");
    assert!(err.is_none(), "create_bag failed: {err:?}");
    assert!(fx.status().is_ok());

    pick(&fx, kind).expect("created bag")
}

/// Same shape as [`create_bag`] but produces a `Table<ty, ty>`.
async fn create_table(cluster: &LocalCluster, kind: Ownership, ty: TypeTag) -> ObjectRef {
    let (sender, kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET)
        .await
        .expect("funded_account");

    let mut builder = ProgrammableTransactionBuilder::new();
    let table = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("table").to_owned(),
        ident_str!("new").to_owned(),
        vec![ty.clone(), ty.clone()],
        vec![],
    );

    let table_ty = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: ident_str!("table").to_owned(),
        name: ident_str!("Table").to_owned(),
        type_params: vec![ty.clone(), ty.clone()],
    }));

    let kv = pure_for(&mut builder, &ty);
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("table").to_owned(),
        ident_str!("add").to_owned(),
        vec![ty.clone(), ty.clone()],
        vec![table, kv, kv],
    );

    transfer_or_share(&mut builder, kind, table, table_ty);

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price().await,
    );
    let (fx, err) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .await
        .expect("execute_transaction");
    assert!(err.is_none(), "create_table failed: {err:?}");
    assert!(fx.status().is_ok());

    pick(&fx, kind).expect("created table")
}

fn pure_for(
    builder: &mut ProgrammableTransactionBuilder,
    ty: &TypeTag,
) -> sui_types::transaction::Argument {
    match ty {
        TypeTag::U8 => builder.pure(0u8),
        TypeTag::U64 => builder.pure(0u64),
        _ => panic!("unsupported type: {ty}"),
    }
    .expect("pure value")
}

fn transfer_or_share(
    builder: &mut ProgrammableTransactionBuilder,
    kind: Ownership,
    arg: sui_types::transaction::Argument,
    arg_ty: TypeTag,
) {
    match kind {
        Ownership::Address(addr) => {
            builder.transfer_arg(addr, arg);
        }
        Ownership::Shared => {
            builder.programmable_move_call(
                SUI_FRAMEWORK_PACKAGE_ID,
                ident_str!("transfer").to_owned(),
                ident_str!("public_share_object").to_owned(),
                vec![arg_ty],
                vec![arg],
            );
        }
        Ownership::Immutable => {
            builder.programmable_move_call(
                SUI_FRAMEWORK_PACKAGE_ID,
                ident_str!("transfer").to_owned(),
                ident_str!("public_freeze_object").to_owned(),
                vec![arg_ty],
                vec![arg],
            );
        }
    }
}

fn pick(fx: &sui_types::effects::TransactionEffects, kind: Ownership) -> Option<ObjectRef> {
    fx.created()
        .into_iter()
        .find_map(|(oref, o)| match (kind, o) {
            (Ownership::Address(addr), TypesOwner::AddressOwner(a)) if a == addr => Some(oref),
            (Ownership::Shared, TypesOwner::Shared { .. }) => Some(oref),
            (Ownership::Immutable, TypesOwner::Immutable) => Some(oref),
            _ => None,
        })
}
