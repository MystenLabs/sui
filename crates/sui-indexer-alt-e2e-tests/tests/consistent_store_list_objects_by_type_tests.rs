// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use move_core_types::{ident_str, language_storage::StructTag, u256::U256};
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::{
    consistent_service_client::ConsistentServiceClient, ListObjectsByTypeRequest,
};
use sui_indexer_alt_e2e_tests::{find, FullCluster};
use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    crypto::get_account_key_pair,
    effects::TransactionEffectsAPI,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{Transaction, TransactionData},
    TypeTag, SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_PACKAGE_ID,
};

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

#[derive(Copy, Clone)]
enum OwnerKind {
    Address(SuiAddress),
    Shared,
    Immutable,
}

#[tokio::test]
async fn test_type_filter() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Helper to perform forward pagination over objects by type.
    async fn list_objects_by_type(
        cluster: &FullCluster,
        object_type: &str,
        after_token: Option<Vec<u8>>,
        page_size: Option<u32>,
    ) -> Result<(Vec<(String, u64, String)>, Option<Vec<u8>>), tonic::Status> {
        let mut client =
            ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
                .await
                .expect("Failed to connect to Consistent Store");

        let request = tonic::Request::new(ListObjectsByTypeRequest {
            object_type: Some(object_type.to_string()),
            page_size,
            after_token: after_token.map(Into::into),
            ..Default::default()
        });

        let response = client.list_objects_by_type(request).await?.into_inner();

        let after_token = response
            .has_next_page()
            .then(|| response.objects.last().map(|o| o.page_token().to_owned()))
            .flatten();

        let objects = response
            .objects
            .into_iter()
            .map(|o| (o.object_id().to_owned(), o.version(), o.digest().to_owned()))
            .collect();

        Ok((objects, after_token))
    }

    // Create bags and tables that are owned by addresses `a` and `b`, and as shared and immutable
    // objects. This includes (for each owner kind):
    //
    // - 4 `Bag`s mapping `u8` to `u8` in a variety of sizes.
    // - 4 `Bag`s mapping `u64` to `u64` in a variety of sizes.
    // - 4 `Table<u8, u8>`s in a variety of sizes.
    // - 4 `Table<u64, u64>`s in a variety of sizes.

    let mut bags = BTreeMap::new();
    let mut tu8s = BTreeMap::new();
    let mut tu64s = BTreeMap::new();

    // Create bags of different types owned by different addresses
    use OwnerKind as K;

    for kind in [K::Address(a), K::Address(b), K::Immutable, K::Shared] {
        for i in 0..4 {
            let bag = create_bag(&mut cluster, kind, TypeTag::U8, 4 * i);
            bags.insert(bag.0, bag);

            let bag = create_bag(&mut cluster, kind, TypeTag::U64, 4 * i + 1);
            bags.insert(bag.0, bag);

            let table = create_table(&mut cluster, kind, TypeTag::U8, 4 * i + 2);
            tu8s.insert(table.0, table);

            let table = create_table(&mut cluster, kind, TypeTag::U64, 4 * i + 3);
            tu64s.insert(table.0, table);
        }
    }

    cluster.create_checkpoint().await;

    // All bags, by module filter
    assert_eq!(
        list_objects_by_type(&cluster, "0x2::bag", None, Some(50))
            .await
            .unwrap(),
        (bags.values().map(repr).collect(), None)
    );

    // All tables, by fully-qualified name filter
    assert_eq!(
        list_objects_by_type(&cluster, "0x2::table::Table", None, Some(50))
            .await
            .unwrap(),
        (
            tu8s.values().chain(tu64s.values()).map(repr).collect(),
            None
        )
    );

    // All Table<u64, u64>s
    assert_eq!(
        list_objects_by_type(&cluster, "0x2::table::Table<u64, u64>", None, Some(50))
            .await
            .unwrap(),
        (tu64s.values().map(repr).collect(), None)
    );

    // Try to paginate Table<u8, u64>s -- none should exist.
    assert_eq!(
        list_objects_by_type(&cluster, "0x2::table::Table<u8, u64>", None, Some(50))
            .await
            .unwrap(),
        (vec![], None)
    );

    // Test pagination
    let mut after = None;
    let mut results = vec![];
    loop {
        let (page, next) = list_objects_by_type(&cluster, "0x2::table", after.clone(), Some(5))
            .await
            .unwrap();

        results.extend(page);
        after = next;
        if after.is_none() {
            break;
        }
    }

    // Pagination should return the same results as fetching all at once
    assert_eq!(
        results,
        tu8s.values()
            .chain(tu64s.values())
            .map(repr)
            .collect::<Vec<_>>()
    );

    // Pass in a bad type filter (malformed).
    let err = list_objects_by_type(&cluster, "not::a::valid::type<>", None, Some(20))
        .await
        .unwrap_err();

    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(
        err.message(),
        "Bad 'object_type' filter, expected: package[::module[::name[<type, ...>]]]"
    );

    // Missing type filter
    let err = list_objects_by_type(&cluster, "", None, Some(20))
        .await
        .unwrap_err();

    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), "Missing 'object_type' filter");
}

fn repr((i, v, d): &ObjectRef) -> (String, u64, String) {
    (
        i.to_canonical_string(/* with_prefix */ true),
        v.value(),
        d.base58_encode(),
    )
}

/// Run a transaction on `cluster` signed by a fresh funded account that creates a `Bag` with
/// `size` many elements, owned by `kind`. The purpose of this is to create an object that isn't a
/// coin. `ty` can be any numeric Move type.
fn create_bag(cluster: &mut FullCluster, kind: OwnerKind, ty: TypeTag, size: u64) -> ObjectRef {
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

    let bag_ty = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: ident_str!("bag").to_owned(),
        name: ident_str!("Bag").to_owned(),
        type_params: vec![],
    }));

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

    match kind {
        OwnerKind::Address(addr) => {
            builder.transfer_arg(addr, bag);
        }

        OwnerKind::Shared => {
            builder.programmable_move_call(
                SUI_FRAMEWORK_PACKAGE_ID,
                ident_str!("transfer").to_owned(),
                ident_str!("public_share_object").to_owned(),
                vec![bag_ty],
                vec![bag],
            );
        }

        OwnerKind::Immutable => {
            builder.programmable_move_call(
                SUI_FRAMEWORK_PACKAGE_ID,
                ident_str!("transfer").to_owned(),
                ident_str!("public_freeze_object").to_owned(),
                vec![bag_ty],
                vec![bag],
            );
        }
    };

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

    match kind {
        OwnerKind::Address(_) => find::address_owned(&fx),
        OwnerKind::Immutable => find::immutable(&fx),
        OwnerKind::Shared => find::shared(&fx),
    }
    .expect("Failed to find created bag")
}

/// Run a transaction on `cluster` signed by a fresh funded account that creates a `Table<ty, ty>`
/// owned by `owner` with `size` many elements. The purpose of this is to create an object that
/// isn't a coin. `ty` can be any numeric Move type.
fn create_table(cluster: &mut FullCluster, kind: OwnerKind, ty: TypeTag, size: u64) -> ObjectRef {
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

    let table_ty = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: ident_str!("table").to_owned(),
        name: ident_str!("Table").to_owned(),
        type_params: vec![ty.clone(), ty.clone()],
    }));

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

    match kind {
        OwnerKind::Address(addr) => {
            builder.transfer_arg(addr, table);
        }

        OwnerKind::Shared => {
            builder.programmable_move_call(
                SUI_FRAMEWORK_PACKAGE_ID,
                ident_str!("transfer").to_owned(),
                ident_str!("public_share_object").to_owned(),
                vec![table_ty],
                vec![table],
            );
        }

        OwnerKind::Immutable => {
            builder.programmable_move_call(
                SUI_FRAMEWORK_PACKAGE_ID,
                ident_str!("transfer").to_owned(),
                ident_str!("public_freeze_object").to_owned(),
                vec![table_ty],
                vec![table],
            );
        }
    };

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

    match kind {
        OwnerKind::Address(_) => find::address_owned(&fx),
        OwnerKind::Immutable => find::immutable(&fx),
        OwnerKind::Shared => find::shared(&fx),
    }
    .expect("Failed to find created table")
}
