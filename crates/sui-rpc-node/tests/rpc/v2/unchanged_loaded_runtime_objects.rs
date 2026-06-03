// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Ports the TTO-based subset of
//! `sui-e2e-tests/tests/rpc/v2/unchanged_loaded_runtime_objects.rs`.
//!
//! Skipped:
//!
//! - `test_unchanged_loaded_runtime_objects` — depends on
//!   `stake_with_validator(&test_cluster)` (Simulacrum has no
//!   notion of multiple validators) and on a hard-coded
//!   `validator_set` object address that's specific to
//!   `TestClusterBuilder`'s setup.

use std::path::PathBuf;

use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_sdk_types::Digest;
use sui_types::Identifier;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::CallArg;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::TransactionData;
use sui_types::utils::to_sender_signed_transaction;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

fn tto_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("sui-e2e-tests")
        .join("tests")
        .join("rpc")
        .join("data")
        .join("tto")
}

async fn ledger_client(cluster: &LocalCluster) -> LedgerServiceClient<Channel> {
    LedgerServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

/// Publishes `tto`, transfers a coin to the parent object via
/// `M1::start`, and returns everything subsequent tests need.
struct TtoSetup {
    address: SuiAddress,
    keypair: AccountKeyPair,
    package_id: ObjectID,
    parent: ObjectRef,
    coin: ObjectRef,
    gas: ObjectRef,
    rgp: u64,
}

async fn setup_tto(cluster: &LocalCluster) -> TtoSetup {
    let (address, keypair, gas) = cluster.funded_account(50_000_000_000).await.unwrap();
    let coin_effects = cluster.request_gas(address, 50_000_000_000).await.unwrap();
    let coin = coin_effects
        .created()
        .into_iter()
        .find_map(|(oref, owner)| {
            matches!(owner, sui_types::object::Owner::AddressOwner(a) if a == address)
                .then_some(oref)
        })
        .expect("request_gas should produce a coin owned by the address");
    cluster.create_checkpoint().await.unwrap();
    let rgp = cluster.reference_gas_price().await;

    let (package_id, publish_effects) = cluster
        .publish_package(address, &keypair, gas, tto_path())
        .await
        .unwrap();
    cluster.create_checkpoint().await.unwrap();
    let gas = publish_effects.gas_object().unwrap().0;

    // Drive `M1::start` to TTO the coin under a new parent.
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            package_id,
            Identifier::new("M1").unwrap(),
            Identifier::new("start").unwrap(),
            vec![],
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(coin))],
        )
        .unwrap();
    let pt = builder.finish();
    let tx = to_sender_signed_transaction(
        TransactionData::new_programmable(address, vec![gas], pt, 50_000_000, rgp),
        &keypair,
    );
    let (start_effects, err) = cluster.execute_transaction(tx).await.unwrap();
    assert!(err.is_none(), "M1::start must succeed: {err:?}");
    cluster.create_checkpoint().await.unwrap();

    let parent = start_effects
        .created()
        .into_iter()
        .find_map(|(oref, owner)| {
            matches!(owner, sui_types::object::Owner::AddressOwner(_)).then_some(oref)
        })
        .expect("M1::start must create a parent object");
    let coin = updated_ref(&start_effects, coin.0).expect("M1::start mutates the coin");
    let gas = start_effects.gas_object().unwrap().0;

    TtoSetup {
        address,
        keypair,
        package_id,
        parent,
        coin,
        gas,
        rgp,
    }
}

/// Calling `M1::receive(parent, coin)` twice in the same PTB
/// fails the transaction; the resulting effects carry no
/// `unchanged_loaded_runtime_objects` (the receiving input is
/// invalidated by the first call before the second runs).
#[tokio::test]
async fn tto_receive_twice() {
    let cluster = LocalCluster::new().await.unwrap();
    let s = setup_tto(&cluster).await;

    let mut builder = ProgrammableTransactionBuilder::new();
    for _ in 0..2 {
        builder
            .move_call(
                s.package_id,
                Identifier::new("M1").unwrap(),
                Identifier::new("receive").unwrap(),
                vec![],
                vec![
                    CallArg::Object(ObjectArg::ImmOrOwnedObject(s.parent)),
                    CallArg::Object(ObjectArg::Receiving(s.coin)),
                ],
            )
            .unwrap();
    }
    let pt = builder.finish();
    let tx = to_sender_signed_transaction(
        TransactionData::new_programmable(s.address, vec![s.gas], pt, 50_000_000, s.rgp),
        &s.keypair,
    );
    let digest: Digest = (*tx.digest()).into();
    let (_, err) = cluster.execute_transaction(tx).await.unwrap();
    assert!(err.is_some(), "double-receive should fail");
    cluster.create_checkpoint().await.unwrap();

    let mut client = ledger_client(&cluster).await;
    let txn = client
        .get_transaction(
            GetTransactionRequest::new(&digest).with_read_mask(FieldMask::from_paths(["*"])),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();

    assert_eq!(txn.effects().unchanged_loaded_runtime_objects().len(), 0);
    let coin_id_str = s.coin.0.to_string();
    assert!(
        !txn.effects()
            .changed_objects()
            .iter()
            .any(|o| o.object_id() == coin_id_str),
        "the receiving object should NOT appear in changed_objects on a double-receive failure",
    );
}

/// Single-call `M1::receive` succeeds: the coin moves under
/// `@tto` (the package address), parent ends up empty, and the
/// successful effects carry no `unchanged_loaded_runtime_objects`
/// (the coin is changed). Then a second receive of the same
/// (now-stale) coin reference fails — also empty unchanged set.
#[tokio::test]
async fn tto_success() {
    let cluster = LocalCluster::new().await.unwrap();
    let s = setup_tto(&cluster).await;

    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            s.package_id,
            Identifier::new("M1").unwrap(),
            Identifier::new("receive").unwrap(),
            vec![],
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(s.parent)),
                CallArg::Object(ObjectArg::Receiving(s.coin)),
            ],
        )
        .unwrap();
    let pt = builder.finish();
    let tx = to_sender_signed_transaction(
        TransactionData::new_programmable(s.address, vec![s.gas], pt, 50_000_000, s.rgp),
        &s.keypair,
    );
    let digest: Digest = (*tx.digest()).into();
    let (success_effects, err) = cluster.execute_transaction(tx).await.unwrap();
    assert!(err.is_none(), "M1::receive must succeed: {err:?}");
    cluster.create_checkpoint().await.unwrap();

    let mut client = ledger_client(&cluster).await;
    let txn = client
        .get_transaction(
            GetTransactionRequest::new(&digest).with_read_mask(FieldMask::from_paths(["*"])),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();
    assert!(txn.effects().unchanged_loaded_runtime_objects().is_empty());
    let coin_id_str = s.coin.0.to_string();
    assert!(
        txn.effects()
            .changed_objects()
            .iter()
            .any(|o| o.object_id() == coin_id_str),
        "successful receive should mutate the coin",
    );

    // Re-fetch the parent and the coin against the live store —
    // the coin moved to `@tto` so a second `receive(parent, coin)`
    // will now fail.
    let gas = success_effects.gas_object().unwrap().0;
    let parent_ref = fresh_ref(&mut client, s.parent.0).await;
    let coin_ref = fresh_ref(&mut client, s.coin.0).await;

    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            s.package_id,
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
        TransactionData::new_programmable(s.address, vec![gas], pt, 50_000_000, s.rgp),
        &s.keypair,
    );
    let digest: Digest = (*tx.digest()).into();
    let (_, err) = cluster.execute_transaction(tx).await.unwrap();
    assert!(err.is_some(), "second receive should fail — the coin already moved");
    cluster.create_checkpoint().await.unwrap();

    let txn = client
        .get_transaction(
            GetTransactionRequest::new(&digest).with_read_mask(FieldMask::from_paths(["*"])),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();
    assert!(txn.effects().unchanged_loaded_runtime_objects().is_empty());
}

/// `M1::dont_receive` loads the Receiving<Coin<SUI>> input but
/// never consumes it. The expected behavior is that the
/// effects' `unchanged_loaded_runtime_objects` are empty (the
/// adapter only records `unchanged_loaded_runtime_objects` for
/// objects that were genuinely *loaded* through the standard
/// resolver, not for unused Receiving inputs), and that the
/// receiving object doesn't appear in `changed_objects`.
#[tokio::test]
async fn receive_input() {
    let cluster = LocalCluster::new().await.unwrap();
    let s = setup_tto(&cluster).await;

    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            s.package_id,
            Identifier::new("M1").unwrap(),
            Identifier::new("dont_receive").unwrap(),
            vec![],
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(s.parent)),
                CallArg::Object(ObjectArg::Receiving(s.coin)),
            ],
        )
        .unwrap();
    let pt = builder.finish();
    let tx = to_sender_signed_transaction(
        TransactionData::new_programmable(s.address, vec![s.gas], pt, 50_000_000, s.rgp),
        &s.keypair,
    );
    let digest: Digest = (*tx.digest()).into();
    let (_, err) = cluster.execute_transaction(tx).await.unwrap();
    assert!(err.is_none(), "M1::dont_receive must succeed: {err:?}");
    cluster.create_checkpoint().await.unwrap();

    let mut client = ledger_client(&cluster).await;
    let txn = client
        .get_transaction(
            GetTransactionRequest::new(&digest).with_read_mask(FieldMask::from_paths(["*"])),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();
    assert!(txn.effects().unchanged_loaded_runtime_objects().is_empty());
    let coin_id_str = s.coin.0.to_string();
    assert!(
        !txn.effects()
            .changed_objects()
            .iter()
            .any(|o| o.object_id() == coin_id_str),
        "dont_receive should leave the receiving input untouched",
    );
}

fn updated_ref(effects: &TransactionEffects, id: ObjectID) -> Option<ObjectRef> {
    effects
        .mutated()
        .into_iter()
        .find(|((oid, _, _), _)| oid == &id)
        .map(|(oref, _)| oref)
}

async fn fresh_ref(client: &mut LedgerServiceClient<Channel>, id: ObjectID) -> ObjectRef {
    let obj = client
        .get_object(
            GetObjectRequest::default()
                .with_object_id(id.to_string())
                .with_read_mask(FieldMask::from_paths(["object_id", "version", "digest"])),
        )
        .await
        .unwrap()
        .into_inner()
        .object
        .unwrap();
    (
        obj.object_id().parse().unwrap(),
        obj.version().into(),
        obj.digest().parse().unwrap(),
    )
}
