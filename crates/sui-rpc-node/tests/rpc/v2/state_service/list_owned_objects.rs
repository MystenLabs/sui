// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Ports
//! `sui-e2e-tests/tests/rpc/v2/state_service/list_owned_objects.rs`.
//! `test_indexing_with_tto` and `test_filter_by_type` publish
//! on-disk Move packages and drive PTBs through Simulacrum;
//! `test_reverse_sorted_coins_by_balance` issues a sequence of
//! gas grants and inspects the sort order
//! `object_by_owner` produces for `Coin<SUI>`.

use std::path::PathBuf;

use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoRequest;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoResponse;
use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::state_service_client::StateServiceClient;
use sui_sdk_types::TypeTag as SdkTypeTag;
use sui_types::Identifier;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::CallArg;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::TransactionData;
use sui_types::utils::to_sender_signed_transaction;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

async fn state_client(cluster: &LocalCluster) -> StateServiceClient<Channel> {
    StateServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

fn data_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("sui-e2e-tests")
        .join("tests")
        .join("rpc")
        .join("data")
        .join(name)
}

#[tokio::test]
async fn list_owned_objects_reports_funded_account_gas_coin() {
    let cluster = LocalCluster::new().await.unwrap();
    let (owner, _kp, gas) = cluster.funded_account(10_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let mut client = state_client(&cluster).await;

    let objects = client
        .list_owned_objects({
            let mut req = ListOwnedObjectsRequest::default();
            req.owner = Some(owner.to_string());
            req.read_mask = Some(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "object_type",
            ]));
            req
        })
        .await
        .unwrap()
        .into_inner()
        .objects;

    assert!(
        !objects.is_empty(),
        "freshly funded account should own at least its gas coin",
    );

    let gas_id = gas.0.to_string();
    let found = objects.iter().find(|o| o.object_id() == gas_id);
    assert!(
        found.is_some(),
        "the gas coin returned by funded_account should appear in list_owned_objects",
    );

    let found = found.unwrap();
    assert!(found.object_type.is_some());
    assert!(found.version.is_some());
    assert!(found.digest.is_some());
}

/// After publishing a custom coin and minting it, the new
/// instance shows up under a type filter for that specific
/// instantiation, while a bare-tag (`0x2::coin::Coin`) filter
/// returns coins of every instantiation. Mirrors
/// `test_filter_by_type`.
#[tokio::test]
async fn filter_by_type() {
    let cluster = LocalCluster::new().await.unwrap();

    let sui_coin = "0x2::coin::Coin<0x2::sui::SUI>"
        .parse::<SdkTypeTag>()
        .unwrap()
        .to_string();

    // Start with one funded account holding one SUI coin.
    let (address, keypair, _gas) = cluster.funded_account(50_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let mut client = state_client(&cluster).await;

    let objects = client
        .list_owned_objects({
            let mut req = ListOwnedObjectsRequest::default();
            req.owner = Some(address.to_string());
            req.read_mask = Some(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "object_type",
            ]));
            req
        })
        .await
        .unwrap()
        .into_inner()
        .objects;
    assert!(!objects.is_empty());
    assert!(objects.iter().all(|o| o.object_type() == sui_coin));

    // The current gas coin's ObjectRef.
    let gas = objects[0].clone();
    let gas_ref: ObjectRef = (
        gas.object_id().parse().unwrap(),
        gas.version().into(),
        gas.digest().parse().unwrap(),
    );

    // Publish trusted_coin.
    let (package_id, publish_effects) = cluster
        .publish_package(address, &keypair, gas_ref, data_path("trusted_coin"))
        .await
        .unwrap();
    cluster.create_checkpoint().await.unwrap();

    let trusted_bare = format!("{}::trusted_coin::TRUSTED_COIN", package_id)
        .parse::<SdkTypeTag>()
        .unwrap()
        .to_string();
    let trusted_coin = format!("0x2::coin::Coin<{}>", trusted_bare)
        .parse::<SdkTypeTag>()
        .unwrap()
        .to_string();
    let trusted_struct = sui_types::parse_sui_struct_tag(&trusted_bare).unwrap();
    let treasury_cap_type =
        sui_types::coin::TreasuryCap::type_(trusted_struct).to_canonical_string(true);

    // After publishing the treasury cap exists, supply is 0.
    let GetCoinInfoResponse {
        coin_type,
        metadata,
        treasury,
        ..
    } = client
        .get_coin_info({
            let mut req = GetCoinInfoRequest::default();
            req.coin_type = Some(trusted_bare.clone());
            req
        })
        .await
        .unwrap()
        .into_inner();
    let metadata = metadata.unwrap();
    assert_eq!(coin_type.as_ref(), Some(&trusted_bare));
    assert_eq!(metadata.symbol.as_deref(), Some("TRUSTED"));
    assert_eq!(
        metadata.description.as_deref(),
        Some("Trusted Coin for test")
    );
    assert_eq!(metadata.name.as_deref(), Some("Trusted Coin"));
    assert_eq!(metadata.decimals, Some(2));
    assert_eq!(treasury.unwrap().total_supply, Some(0));

    let treasury_cap_objs = client
        .list_owned_objects({
            let mut req = ListOwnedObjectsRequest::default();
            req.owner = Some(address.to_string());
            req.object_type = Some(treasury_cap_type.clone());
            req.read_mask = Some(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "object_type",
            ]));
            req
        })
        .await
        .unwrap()
        .into_inner()
        .objects;
    assert_eq!(treasury_cap_objs.len(), 1);
    assert_eq!(treasury_cap_objs[0].object_type(), treasury_cap_type);

    let treasury_cap = treasury_cap_objs[0].clone();
    let treasury_cap_ref: ObjectRef = (
        treasury_cap.object_id().parse().unwrap(),
        treasury_cap.version().into(),
        treasury_cap.digest().parse().unwrap(),
    );
    let gas_ref = publish_effects.gas_object().unwrap().0;

    // Mint some coins.
    let rgp = cluster.reference_gas_price().await;
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            package_id,
            Identifier::new("trusted_coin").unwrap(),
            Identifier::new("mint").unwrap(),
            vec![],
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(treasury_cap_ref)),
                CallArg::from(100_000u64),
            ],
        )
        .unwrap();
    let pt = builder.finish();
    let tx = to_sender_signed_transaction(
        TransactionData::new_programmable(address, vec![gas_ref], pt, 50_000_000, rgp),
        &keypair,
    );
    let (_mint_effects, err) = cluster.execute_transaction(tx).await.unwrap();
    assert!(err.is_none(), "mint must succeed: {err:?}");
    cluster.create_checkpoint().await.unwrap();

    // Supply now reads 100_000.
    let GetCoinInfoResponse {
        coin_type,
        treasury,
        ..
    } = client
        .get_coin_info({
            let mut req = GetCoinInfoRequest::default();
            req.coin_type = Some(trusted_bare.clone());
            req
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(coin_type.as_ref(), Some(&trusted_bare));
    assert_eq!(treasury.unwrap().total_supply, Some(100_000));

    // Filter for the specific instantiation: exactly the minted
    // coin.
    let typed = client
        .list_owned_objects({
            let mut req = ListOwnedObjectsRequest::default();
            req.owner = Some(address.to_string());
            req.object_type = Some(trusted_coin.clone());
            req.read_mask = Some(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "object_type",
            ]));
            req
        })
        .await
        .unwrap()
        .into_inner()
        .objects;
    assert_eq!(typed.len(), 1);
    assert_eq!(typed[0].object_type(), trusted_coin);

    // Bare-tag filter `0x2::coin::Coin` should return every coin
    // — SUI coins (multiple, from gas splits across the publish +
    // mint flow) and the one trusted coin.
    let bare = client
        .list_owned_objects({
            let mut req = ListOwnedObjectsRequest::default();
            req.owner = Some(address.to_string());
            req.object_type = Some("0x2::coin::Coin".to_owned());
            req.read_mask = Some(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "object_type",
            ]));
            req
        })
        .await
        .unwrap()
        .into_inner()
        .objects;
    assert_eq!(
        bare.iter()
            .filter(|o| o.object_type() == trusted_coin)
            .count(),
        1,
        "exactly one trusted coin should appear under the bare-tag filter",
    );
    // The SUI coin count depends on how many splits Simulacrum
    // performed; assert at least the funded gas coin.
    assert!(bare.iter().any(|o| o.object_type() == sui_coin));
}

/// Multiple SUI coins sent to one address must be returned in
/// descending balance order under `list_owned_objects` (the
/// default sort for `object_by_owner` over a coin type).
#[tokio::test]
async fn reverse_sorted_coins_by_balance() {
    let cluster = LocalCluster::new().await.unwrap();
    let receiver = SuiAddress::random_for_testing_only();

    let amounts = [1u64, 2, 3, 4, 5];
    for amount in amounts {
        cluster.request_gas(receiver, amount).await.unwrap();
    }
    cluster.create_checkpoint().await.unwrap();

    let mut client = state_client(&cluster).await;
    let objects = client
        .list_owned_objects({
            let mut req = ListOwnedObjectsRequest::default();
            req.owner = Some(receiver.to_string());
            req.read_mask = Some(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "object_type",
                "balance",
            ]));
            req
        })
        .await
        .unwrap()
        .into_inner()
        .objects;
    assert_eq!(objects.len(), amounts.len());

    let balances: Vec<u64> = objects.iter().map(|o| o.balance.unwrap()).collect();
    let mut sorted = amounts;
    sorted.reverse();
    assert_eq!(balances.as_slice(), sorted.as_slice());
}

/// Publish `data/tto`, run `M1::start` to transfer-to-object a
/// coin under a parent, then `M1::receive` to move it to `0x0`.
/// Mirrors `test_indexing_with_tto`.
#[tokio::test]
async fn indexing_with_tto() {
    let cluster = LocalCluster::new().await.unwrap();
    let (address, keypair, _gas) = cluster.funded_account(50_000_000_000).await.unwrap();
    let coin = cluster.request_gas(address, 50_000_000_000).await.unwrap();
    let coin_ref = coin
        .created()
        .into_iter()
        .find_map(|(oref, owner)| {
            matches!(owner, sui_types::object::Owner::AddressOwner(a) if a == address)
                .then_some(oref)
        })
        .expect("request_gas should create a coin owned by the address");
    cluster.create_checkpoint().await.unwrap();

    let mut client = state_client(&cluster).await;

    let initial_objects = client
        .list_owned_objects({
            let mut req = ListOwnedObjectsRequest::default();
            req.owner = Some(address.to_string());
            req.read_mask = Some(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "object_type",
            ]));
            req
        })
        .await
        .unwrap()
        .into_inner()
        .objects;
    let gas_ref = pick_gas_owned_by(address, &initial_objects, coin_ref.0);

    // Publish tto package.
    let (package_id, publish_effects) = cluster
        .publish_package(address, &keypair, gas_ref, data_path("tto"))
        .await
        .unwrap();
    cluster.create_checkpoint().await.unwrap();
    let gas_ref = publish_effects.gas_object().unwrap().0;
    let coin_ref = updated_ref(&publish_effects, coin_ref.0).unwrap_or(coin_ref);

    // `start(coin)` creates a parent and TTO-transfers the coin
    // under it.
    let rgp = cluster.reference_gas_price().await;
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            package_id,
            Identifier::new("M1").unwrap(),
            Identifier::new("start").unwrap(),
            vec![],
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_ref))],
        )
        .unwrap();
    let pt = builder.finish();
    let tx = to_sender_signed_transaction(
        TransactionData::new_programmable(address, vec![gas_ref], pt, 50_000_000, rgp),
        &keypair,
    );
    let (start_effects, err) = cluster.execute_transaction(tx).await.unwrap();
    assert!(err.is_none(), "M1::start must succeed: {err:?}");
    cluster.create_checkpoint().await.unwrap();

    let parent_ref = start_effects
        .created()
        .into_iter()
        .find_map(|(oref, owner)| {
            matches!(owner, sui_types::object::Owner::AddressOwner(_)).then_some(oref)
        })
        .expect("M1::start should create the parent object");
    let coin_ref = updated_ref(&start_effects, coin_ref.0)
        .expect("M1::start should mutate the coin we passed in");
    let gas_ref = start_effects.gas_object().unwrap().0;

    // Parent owns exactly 1 coin (the one we just TTO'd).
    assert_eq!(
        list_owned_count(&mut client, parent_ref.0.to_string()).await,
        1,
    );
    // 0x0 owns nothing.
    assert_eq!(list_owned_count(&mut client, "0x0".to_owned()).await, 0);

    // `receive(parent, coin)` moves the coin to 0x0.
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            package_id,
            Identifier::new("M1").unwrap(),
            Identifier::new("receive").unwrap(),
            vec![],
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(parent_ref)),
                CallArg::Object(ObjectArg::Receiving(coin_ref)),
            ],
        )
        .unwrap();
    let pt = builder.finish();
    let tx = to_sender_signed_transaction(
        TransactionData::new_programmable(address, vec![gas_ref], pt, 50_000_000, rgp),
        &keypair,
    );
    let (_, err) = cluster.execute_transaction(tx).await.unwrap();
    assert!(err.is_none(), "M1::receive must succeed: {err:?}");
    cluster.create_checkpoint().await.unwrap();

    assert_eq!(
        list_owned_count(&mut client, parent_ref.0.to_string()).await,
        0,
    );
    assert_eq!(list_owned_count(&mut client, "0x0".to_owned()).await, 1);
}

fn pick_gas_owned_by(
    address: SuiAddress,
    objects: &[sui_rpc::proto::sui::rpc::v2::Object],
    not: ObjectID,
) -> ObjectRef {
    let obj = objects
        .iter()
        .find(|o| o.object_id() != not.to_string())
        .unwrap_or_else(|| panic!("{address} needs a second coin to pay gas with"));
    (
        obj.object_id().parse().unwrap(),
        obj.version().into(),
        obj.digest().parse().unwrap(),
    )
}

fn updated_ref(effects: &TransactionEffects, id: ObjectID) -> Option<ObjectRef> {
    effects
        .mutated()
        .into_iter()
        .find(|((oid, _, _), _)| oid == &id)
        .map(|(oref, _)| oref)
}

async fn list_owned_count(client: &mut StateServiceClient<Channel>, owner: String) -> usize {
    let mut req = ListOwnedObjectsRequest::default();
    req.owner = Some(owner);
    client
        .list_owned_objects(req)
        .await
        .unwrap()
        .into_inner()
        .objects
        .len()
}
