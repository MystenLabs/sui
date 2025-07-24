// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, iter};

use move_core_types::{ident_str, u256::U256};
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::{
    consistent_service_client::ConsistentServiceClient, owner::OwnerKind, ListOwnedObjectsRequest,
    Owner,
};
use sui_indexer_alt_e2e_tests::{
    find_address_mutated, find_address_owned, find_immutable, find_shared, FullCluster,
};
use sui_types::{
    base_types::{FullObjectRef, ObjectRef, SuiAddress},
    crypto::{get_account_key_pair, Signature, Signer},
    effects::{TransactionEffects, TransactionEffectsAPI},
    gas_coin::GasCoin,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{Argument, Command, Transaction, TransactionData},
    TypeTag, SUI_FRAMEWORK_PACKAGE_ID,
};

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

#[tokio::test]
async fn test_address_owner() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, akp) = get_account_key_pair();
    let (b, bkp) = get_account_key_pair();
    let (c, _) = get_account_key_pair();

    // Helper to perform forward pagination over a list of owned objects.
    async fn list_owned_objects(
        cluster: &FullCluster,
        owner: SuiAddress,
        checkpoint: Option<u64>,
        after_token: Option<Vec<u8>>,
        page_size: Option<u32>,
    ) -> Result<(Vec<(String, u64, String)>, Option<Vec<u8>>), tonic::Status> {
        let mut client =
            ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
                .await
                .expect("Failed to connect to Consistent Store");

        let mut request = tonic::Request::new(ListOwnedObjectsRequest {
            owner: Some(Owner {
                kind: Some(OwnerKind::Address.into()),
                address: Some(owner.to_string()),
            }),
            page_size,
            after_token: after_token.map(Into::into),
            ..Default::default()
        });

        if let Some(checkpoint) = checkpoint {
            request
                .metadata_mut()
                .insert("x-sui-checkpoint", checkpoint.to_string().parse().unwrap());
        }

        let response = client.list_owned_objects(request).await?.into_inner();

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

    // In the first checkpoint, A owns coins, B owns bags, C owns nothing.
    let coins: BTreeMap<_, _> = (0..4)
        .map(|i| (i, create_coin(&mut cluster, a, i)))
        .collect();

    let bags: BTreeMap<_, _> = (0..4)
        .map(|i| {
            let bag = create_bag(&mut cluster, b, TypeTag::U64, i + 1);
            (bag.0, bag)
        })
        .collect();

    cluster.create_checkpoint().await;

    // Coins should be returned in decreasing balance order, i.e. 3, 2, 1, 0.
    assert_eq!(
        list_owned_objects(&cluster, a, None, None, Some(4))
            .await
            .unwrap(),
        (coins.values().rev().map(repr).collect(), None),
    );

    // Bags don't have a balance, so they are returned in order of ID.
    assert_eq!(
        list_owned_objects(&cluster, b, None, None, Some(4))
            .await
            .unwrap(),
        (bags.values().map(repr).collect(), None),
    );

    // C doesn't own anything yet.
    assert_eq!(
        list_owned_objects(&cluster, c, None, None, Some(4))
            .await
            .unwrap(),
        (vec![], None),
    );

    // Explicitly supplying the checkpoint also works.
    assert_eq!(
        list_owned_objects(&cluster, a, Some(1), None, Some(4))
            .await
            .unwrap(),
        (coins.values().rev().map(repr).collect(), None),
    );

    // Requesting a checkpoint that we don't have a snapshot for returns an error.
    assert_eq!(
        list_owned_objects(&cluster, a, Some(2), None, Some(4))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::OutOfRange,
    );

    // In the second checkpoint, A and B get funded, so they can sign transactions.
    let mut a_gas = find_address_owned(&cluster.request_gas(a, DEFAULT_GAS_BUDGET * 5).unwrap())
        .expect("Failed to find gas for A");
    let mut b_gas = find_address_owned(&cluster.request_gas(b, DEFAULT_GAS_BUDGET * 5).unwrap())
        .expect("Failed to find gas for B");
    cluster.create_checkpoint().await;

    // A now owns an extra coin, it has the biggest balance so it should appear first.
    assert_eq!(
        list_owned_objects(&cluster, a, None, None, Some(5))
            .await
            .unwrap(),
        (
            iter::once(&a_gas)
                .chain(coins.values().rev())
                .map(repr)
                .collect(),
            None
        ),
    );

    // B also owns a coin now, objects are grouped and sorted by type, so it should appear last
    // (after the bags).
    assert_eq!(
        list_owned_objects(&cluster, b, None, None, Some(5))
            .await
            .unwrap(),
        (
            bags.values().chain(iter::once(&b_gas)).map(repr).collect(),
            None
        ),
    );

    // C still owns nothing.
    assert_eq!(
        list_owned_objects(&cluster, c, None, None, Some(4))
            .await
            .unwrap(),
        (vec![], None),
    );

    // The old state of A is still accessible using an explicit checkpoint.
    assert_eq!(
        list_owned_objects(&cluster, a, Some(1), None, Some(4))
            .await
            .unwrap(),
        (coins.values().rev().map(repr).collect(), None),
    );

    // In the third checkpoint, A and B transfer all their assets to C.
    let mut objects = BTreeMap::new();
    for (i, coin) in coins {
        let fx = transfer_object(&mut cluster, a, &akp, a_gas, coin, c);
        objects.insert((!i, coin.0), find_address_mutated(&fx).unwrap());
        a_gas = fx.gas_object().0;
    }

    for bag in bags.into_values() {
        let fx = transfer_object(&mut cluster, b, &bkp, b_gas, bag, c);
        objects.insert((0, bag.0), find_address_mutated(&fx).unwrap());
        b_gas = fx.gas_object().0;
    }

    cluster.create_checkpoint().await;

    // A now only owns its gas coin,
    assert_eq!(
        list_owned_objects(&cluster, a, None, None, Some(5))
            .await
            .unwrap(),
        (vec![repr(&a_gas)], None),
    );

    // Same for B
    assert_eq!(
        list_owned_objects(&cluster, b, None, None, Some(5))
            .await
            .unwrap(),
        (vec![repr(&b_gas)], None),
    );

    // C owns A and B's original objects (but at new versions) -- we'll fetch them over multiple
    // pages.
    let mut after = None;
    let mut results = vec![];
    loop {
        let (page, next) = list_owned_objects(&cluster, c, None, after.clone(), Some(2))
            .await
            .unwrap();

        results.extend(page);
        after = next;
        if after.is_none() {
            break;
        }
    }

    assert_eq!(results, objects.values().map(repr).collect::<Vec<_>>());
}

#[tokio::test]
async fn test_missing_address() {
    let mut cluster = FullCluster::new().await.unwrap();
    let mut client = ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
        .await
        .expect("Failed to connect to Consistent Store");

    cluster.create_checkpoint().await;

    // Test ADDRESS owner without address
    let response = client
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(Owner {
                kind: Some(OwnerKind::Address.into()),
                address: None,
            }),
            ..Default::default()
        })
        .await;

    assert_eq!(response.unwrap_err().code(), tonic::Code::InvalidArgument,);

    // Test OBJECT owner without address
    let response = client
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(Owner {
                kind: Some(OwnerKind::Object.into()),
                address: None,
            }),
            ..Default::default()
        })
        .await;

    assert_eq!(response.unwrap_err().code(), tonic::Code::InvalidArgument,);

    // Test ADDRESS owner with empty string address
    let response = client
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(Owner {
                kind: Some(OwnerKind::Address.into()),
                address: Some("".to_string()),
            }),
            ..Default::default()
        })
        .await;

    assert_eq!(response.unwrap_err().code(), tonic::Code::InvalidArgument,);
}

#[tokio::test]
async fn test_unexpected_address() {
    let mut cluster = FullCluster::new().await.unwrap();
    let mut client = ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
        .await
        .expect("Failed to connect to Consistent Store");

    cluster.create_checkpoint().await;

    // Test SHARED owner with address (should not have address)
    let response = client
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(Owner {
                kind: Some(OwnerKind::Shared.into()),
                address: Some("0x1".to_string()),
            }),
            ..Default::default()
        })
        .await;

    assert_eq!(response.unwrap_err().code(), tonic::Code::InvalidArgument,);

    // Test IMMUTABLE owner with address (should not have address)
    let response = client
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(Owner {
                kind: Some(OwnerKind::Immutable.into()),
                address: Some("0x2".to_string()),
            }),
            ..Default::default()
        })
        .await;

    assert_eq!(response.unwrap_err().code(), tonic::Code::InvalidArgument,);
}

#[tokio::test]
async fn test_type_filters() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();

    // Helper to list owned objects with type filter
    async fn list_owned_objects(
        cluster: &FullCluster,
        owner: SuiAddress,
        object_type: &str,
        after_token: Option<Vec<u8>>,
        page_size: Option<u32>,
    ) -> Result<(Vec<(String, u64, String)>, Option<Vec<u8>>), tonic::Status> {
        let mut client =
            ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
                .await
                .expect("Failed to connect to Consistent Store");

        let response = client
            .list_owned_objects(ListOwnedObjectsRequest {
                owner: Some(Owner {
                    kind: Some(OwnerKind::Address.into()),
                    address: Some(owner.to_string()),
                }),
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

        let objects = response
            .objects
            .into_iter()
            .map(|o| (o.object_id().to_owned(), o.version(), o.digest().to_owned()))
            .collect();

        Ok((objects, after_token))
    }

    // Create different types of objects owned by A
    let coins: BTreeMap<_, _> = (0..4)
        .map(|i| (i, create_coin(&mut cluster, a, i)))
        .collect();

    let bags: BTreeMap<_, _> = (0..4)
        .flat_map(|i| {
            let bu64 = create_bag(&mut cluster, a, TypeTag::U64, i + 1);
            let bu8 = create_bag(&mut cluster, a, TypeTag::U8, i + 1);
            [(bu64.0, bu64), (bu8.0, bu8)]
        })
        .collect();

    let tu64s: BTreeMap<_, _> = (0..4)
        .map(|i| {
            let table = create_table(&mut cluster, a, TypeTag::U64, i + 1);
            (table.0, table)
        })
        .collect();

    let tu8s: BTreeMap<_, _> = (0..4)
        .map(|i| {
            let table = create_table(&mut cluster, a, TypeTag::U8, i + 1);
            (table.0, table)
        })
        .collect();

    cluster.create_checkpoint().await;

    // Fetch all the objects owned by A, filtering on package (all A's objects have a type from
    // `0x2`). Results are ordered by type, then by ID, unless they are coins, in which case they
    // are ordered by balance, and then by ID.
    assert_eq!(
        list_owned_objects(&cluster, a, "0x2", None, Some(20))
            .await
            .unwrap(),
        (
            bags.values()
                .chain(coins.values().rev())
                .chain(tu8s.values())
                .chain(tu64s.values())
                .map(repr)
                .collect(),
            None
        )
    );

    // Fetch all bags using a package and module filter
    assert_eq!(
        list_owned_objects(&cluster, a, "0x2::bag", None, Some(20))
            .await
            .unwrap(),
        (bags.values().map(repr).collect(), None)
    );

    // Fetch all tables using an unqualified type filter (no type parameters).
    assert_eq!(
        list_owned_objects(&cluster, a, "0x2::table::Table", None, Some(20))
            .await
            .unwrap(),
        (
            tu8s.values().chain(tu64s.values()).map(repr).collect(),
            None
        )
    );

    // Fetch all Table<u64, u64>s using a fully qualified type filter.
    assert_eq!(
        list_owned_objects(&cluster, a, "0x2::table::Table<u64, u64>", None, Some(20))
            .await
            .unwrap(),
        (tu64s.values().map(repr).collect(), None)
    );

    // Fetch a type that we don't have any of, like `0x2::clock::Clock`.
    assert_eq!(
        list_owned_objects(&cluster, a, "0x2::clock::Clock", None, Some(20))
            .await
            .unwrap(),
        (vec![], None)
    );

    // Pass in a bad type filter (malformed).
    let err = list_owned_objects(&cluster, a, "not::a::valid::type<>", None, Some(20))
        .await
        .unwrap_err();

    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(
        err.message(),
        "Bad 'object_type' filter, expected: package[::module[::name[<type, ...>]]]"
    );
}

#[tokio::test]
async fn test_shared_immutable_filters() {
    let mut cluster = FullCluster::new().await.unwrap();

    // Helper to list owned objects with type filter
    async fn list_owned_objects(
        cluster: &FullCluster,
        kind: OwnerKind,
        address: Option<SuiAddress>,
        object_type: &str,
        after_token: Option<Vec<u8>>,
        page_size: Option<u32>,
    ) -> Result<(Vec<(String, u64, String)>, Option<Vec<u8>>), tonic::Status> {
        let mut client =
            ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
                .await
                .expect("Failed to connect to Consistent Store");

        let response = client
            .list_owned_objects(ListOwnedObjectsRequest {
                owner: Some(Owner {
                    kind: Some(kind.into()),
                    address: address.map(|a| a.to_string()),
                }),
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

        let objects = response
            .objects
            .into_iter()
            .map(|o| (o.object_id().to_owned(), o.version(), o.digest().to_owned()))
            .collect();

        Ok((objects, after_token))
    }

    // Create coins with various different ownership kinds
    let shared: BTreeMap<_, _> = (0..4).map(|i| (i, share_coin(&mut cluster, i))).collect();
    let frozen: BTreeMap<_, _> = (0..4).map(|i| (i, freeze_coin(&mut cluster, i))).collect();

    cluster.create_checkpoint().await;

    // Fetching all the shared coins objects, filtering by package, and module name.
    assert_eq!(
        list_owned_objects(
            &cluster,
            OwnerKind::Shared,
            None,
            "0x2::coin",
            None,
            Some(20)
        )
        .await
        .unwrap(),
        (shared.values().rev().map(repr).collect(), None),
    );

    // Fetching all the frozen coins, filtering by package, module and type name.
    assert_eq!(
        list_owned_objects(
            &cluster,
            OwnerKind::Immutable,
            None,
            "0x2::coin::Coin",
            None,
            Some(20)
        )
        .await
        .unwrap(),
        (frozen.values().rev().map(repr).collect(), None),
    );
}

fn repr((i, v, d): &ObjectRef) -> (String, u64, String) {
    (
        i.to_canonical_string(/* with_prefix */ true),
        v.value(),
        d.base58_encode(),
    )
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
    find_address_owned(&fx).expect("Failed to find created coin")
}

fn share_coin(cluster: &mut FullCluster, amount: u64) -> ObjectRef {
    let (sender, kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET + amount)
        .expect("Failed to fund account");

    let mut builder = ProgrammableTransactionBuilder::new();
    let amount = builder.pure(amount).unwrap();
    let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount]));

    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("transfer").to_owned(),
        ident_str!("public_share_object").to_owned(),
        vec![GasCoin::type_().into()],
        vec![coin],
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

    assert!(fx.status().is_ok(), "share coin transaction failed");
    find_shared(&fx).expect("Failed to find shared coin")
}

fn freeze_coin(cluster: &mut FullCluster, amount: u64) -> ObjectRef {
    let (sender, kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET + amount)
        .expect("Failed to fund account");

    let mut builder = ProgrammableTransactionBuilder::new();
    let amount = builder.pure(amount).unwrap();
    let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount]));

    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("transfer").to_owned(),
        ident_str!("public_freeze_object").to_owned(),
        vec![GasCoin::type_().into()],
        vec![coin],
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

    assert!(fx.status().is_ok(), "share coin transaction failed");
    find_immutable(&fx).expect("Failed to find frozen coin")
}

/// Run a transaction on `cluster` signed by a fresh funded account that creates a `Bag`
/// owned by `owner` with `size` many elements. The purpose of this is to create an object that
/// isn't a coin. `ty` can be any numeric Move type.
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
/// owned by `owner` with `size` many elements. The purpose of this is to create an object that
/// isn't a coin. `ty` can be any numeric Move type.
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

fn transfer_object(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    sign: &dyn Signer<Signature>,
    gas: ObjectRef,
    object: ObjectRef,
    recipient: SuiAddress,
) -> TransactionEffects {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .transfer_object(recipient, FullObjectRef::from_fastpath_ref(object))
        .unwrap();

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![sign]))
        .expect("Failed to execute transaction");

    assert!(fx.status().is_ok(), "transafer object transaction failed");
    fx
}
