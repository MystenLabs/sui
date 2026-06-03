// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ListObjectsByTypeRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ListOwnedObjectsRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::Owner;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_client::ConsistentServiceClient;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::owner::OwnerKind;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::base_types::ObjectRef;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::GasCoin;
use sui_types::object::Owner as TypesOwner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::Argument;
use sui_types::transaction::Command;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

/// 5 SUI gas budget, matches the alt-consistent-store tests.
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

const GAS_COIN_TYPE: &str = "0x0000000000000000000000000000000000000000000000000000000000000002::coin::Coin<\
     0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>";

async fn client(cluster: &LocalCluster) -> ConsistentServiceClient<Channel> {
    ConsistentServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

#[tokio::test]
async fn list_owned_objects_returns_funded_gas_coin() {
    let cluster = LocalCluster::new().await.unwrap();
    let (owner, _kp, gas) = cluster.funded_account(10_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let mut svc = client(&cluster).await;

    let response = svc
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(Owner {
                kind: Some(OwnerKind::Address as i32),
                address: Some(owner.to_string()),
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.has_previous_page, Some(false));
    assert_eq!(response.has_next_page, Some(false));
    let gas_id = gas.0.to_string();
    assert!(
        response
            .objects
            .iter()
            .any(|o| o.object_id.as_deref() == Some(&*gas_id)),
        "funded gas coin should appear in the owner's listing",
    );
}

#[tokio::test]
async fn list_owned_objects_filters_by_type() {
    let cluster = LocalCluster::new().await.unwrap();
    let (owner, _kp, gas) = cluster.funded_account(10_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    let mut svc = client(&cluster).await;

    let response = svc
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(Owner {
                kind: Some(OwnerKind::Address as i32),
                address: Some(owner.to_string()),
            }),
            object_type: Some(GAS_COIN_TYPE.to_string()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();

    let gas_id = gas.0.to_string();
    assert!(
        response
            .objects
            .iter()
            .any(|o| o.object_id.as_deref() == Some(&*gas_id)),
        "type-filtered listing should still return the gas coin",
    );
}

#[tokio::test]
async fn list_owned_objects_missing_owner_is_invalid_argument() {
    let cluster = LocalCluster::new().await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    let mut svc = client(&cluster).await;

    let err = svc
        .list_owned_objects(ListOwnedObjectsRequest::default())
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn list_objects_by_type_returns_funded_gas_coin() {
    let cluster = LocalCluster::new().await.unwrap();
    let (_owner, _kp, gas) = cluster.funded_account(10_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    let mut svc = client(&cluster).await;

    let response = svc
        .list_objects_by_type(ListObjectsByTypeRequest {
            object_type: Some(GAS_COIN_TYPE.to_string()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();

    let gas_id = gas.0.to_string();
    assert!(
        response
            .objects
            .iter()
            .any(|o| o.object_id.as_deref() == Some(&*gas_id)),
        "the gas coin should appear in the type-only listing",
    );
}

#[tokio::test]
async fn list_objects_by_type_missing_type_is_invalid_argument() {
    let cluster = LocalCluster::new().await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    let mut svc = client(&cluster).await;

    let err = svc
        .list_objects_by_type(ListObjectsByTypeRequest::default())
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

/// Ports `test_missing_address`: every shape of "the request
/// claims an Address/Object owner but doesn't supply a usable
/// address" surfaces as `InvalidArgument`. Covers
/// `kind=Address`/`address=None`,
/// `kind=Object`/`address=None`, and
/// `kind=Address`/`address=Some("")`.
#[tokio::test]
async fn list_owned_objects_missing_address_paths() {
    let cluster = LocalCluster::new().await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    let mut svc = client(&cluster).await;

    for (kind, address) in [
        (OwnerKind::Address as i32, None),
        (OwnerKind::Object as i32, None),
        (OwnerKind::Address as i32, Some(String::new())),
    ] {
        let err = svc
            .list_owned_objects(ListOwnedObjectsRequest {
                owner: Some(Owner {
                    kind: Some(kind),
                    address,
                }),
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }
}

/// Ports `test_unexpected_address`: `Shared` and `Immutable`
/// owner kinds don't carry an address; supplying one should
/// surface as `InvalidArgument` rather than silently being
/// ignored.
#[tokio::test]
async fn list_owned_objects_shared_immutable_with_address_rejected() {
    let cluster = LocalCluster::new().await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    let mut svc = client(&cluster).await;

    for kind in [OwnerKind::Shared, OwnerKind::Immutable] {
        let err = svc
            .list_owned_objects(ListOwnedObjectsRequest {
                owner: Some(Owner {
                    kind: Some(kind as i32),
                    address: Some("0x1".to_string()),
                }),
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }
}

/// Ports `test_shared_immutable_filters`: a `Shared` or
/// `Immutable` listing combined with a `0x2::coin` /
/// `0x2::coin::Coin` type filter surfaces the right rows
/// from `object_by_owner`.
#[tokio::test]
async fn list_owned_objects_shared_immutable_with_type_filter() {
    let cluster = LocalCluster::new().await.unwrap();
    let shared = share_coin(&cluster, 1).await;
    let frozen = freeze_coin(&cluster, 2).await;
    cluster.create_checkpoint().await.unwrap();
    let mut svc = client(&cluster).await;

    let response = svc
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(Owner {
                kind: Some(OwnerKind::Shared as i32),
                address: None,
            }),
            object_type: Some("0x2::coin".to_string()),
            page_size: Some(10),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(
        response
            .objects
            .iter()
            .any(|o| o.object_id.as_deref() == Some(&shared.0.to_string())),
        "shared coin should surface under (Shared, 0x2::coin)",
    );

    let response = svc
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(Owner {
                kind: Some(OwnerKind::Immutable as i32),
                address: None,
            }),
            object_type: Some("0x2::coin::Coin".to_string()),
            page_size: Some(10),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(
        response
            .objects
            .iter()
            .any(|o| o.object_id.as_deref() == Some(&frozen.0.to_string())),
        "frozen coin should surface under (Immutable, 0x2::coin::Coin)",
    );
}

/// Fund a fresh account, split a SUI coin off the gas, share
/// it. Returns the shared coin's ref. Mirrors `share_coin` in
/// the e2e suite.
async fn share_coin(cluster: &LocalCluster, amount: u64) -> ObjectRef {
    let (sender, kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET + amount)
        .await
        .expect("funded_account");

    let mut builder = ProgrammableTransactionBuilder::new();
    let amt = builder.pure(amount).unwrap();
    let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amt]));
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
        cluster.reference_gas_price().await,
    );
    let (fx, err) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .await
        .expect("execute_transaction");
    assert!(err.is_none(), "share_coin failed: {err:?}");
    assert!(fx.status().is_ok(), "share_coin tx status not ok");

    fx.created()
        .into_iter()
        .find_map(|(oref, o)| matches!(o, TypesOwner::Shared { .. }).then_some(oref))
        .expect("share_coin should yield a Shared coin")
}

/// Same shape as [`share_coin`] but freezes the result.
async fn freeze_coin(cluster: &LocalCluster, amount: u64) -> ObjectRef {
    let (sender, kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET + amount)
        .await
        .expect("funded_account");

    let mut builder = ProgrammableTransactionBuilder::new();
    let amt = builder.pure(amount).unwrap();
    let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amt]));
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
        cluster.reference_gas_price().await,
    );
    let (fx, err) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .await
        .expect("execute_transaction");
    assert!(err.is_none(), "freeze_coin failed: {err:?}");
    assert!(fx.status().is_ok(), "freeze_coin tx status not ok");

    fx.created()
        .into_iter()
        .find_map(|(oref, o)| matches!(o, TypesOwner::Immutable).then_some(oref))
        .expect("freeze_coin should yield an Immutable coin")
}
