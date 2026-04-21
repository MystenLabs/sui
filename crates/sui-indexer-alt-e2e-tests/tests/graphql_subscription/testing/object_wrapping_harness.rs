// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Helpers for interacting with the `object_wrapping` Move package in tests.

use std::collections::BTreeSet;
use std::path::PathBuf;

use fastcrypto::encoding::Base58;
use fastcrypto::encoding::Encoding;
use move_core_types::ident_str;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::TransactionData;

pub async fn publish(cluster: &mut test_cluster::TestCluster) -> ObjectID {
    let sender = cluster.wallet.active_address().unwrap();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["packages", "object_wrapping"]);
    let gas = gas_for(cluster).await;
    let rgp = cluster.wallet.get_reference_gas_price().await.unwrap();
    let tx = cluster
        .wallet
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas, rgp)
                .publish_async(path)
                .await
                .build(),
        )
        .await;
    let resp = cluster.wallet.execute_transaction_must_succeed(tx).await;
    resp.get_new_package_obj().unwrap().0
}

pub async fn create_item(
    cluster: &mut test_cluster::TestCluster,
    package_id: ObjectID,
    value: u64,
) -> (String, ObjectRef) {
    let sender = cluster.wallet.active_address().unwrap();
    let mut ptb = ProgrammableTransactionBuilder::new();
    let value_arg = ptb.pure(value).unwrap();
    let item = ptb.programmable_move_call(
        package_id,
        ident_str!("wrapping").to_owned(),
        ident_str!("create").to_owned(),
        vec![],
        vec![value_arg],
    );
    ptb.transfer_arg(sender, item);
    let (digest, effects) = execute(cluster, ptb).await;
    let created = effects
        .created()
        .into_iter()
        .find(|(_, owner)| owner.get_address_owner_address().is_ok())
        .expect("Should have created an owned object")
        .0;
    (digest, created)
}

pub async fn update_item(
    cluster: &mut test_cluster::TestCluster,
    package_id: ObjectID,
    item_ref: ObjectRef,
    value: u64,
) -> (String, ObjectRef) {
    let mut ptb = ProgrammableTransactionBuilder::new();
    let item_arg = ptb.obj(ObjectArg::ImmOrOwnedObject(item_ref)).unwrap();
    let value_arg = ptb.pure(value).unwrap();
    ptb.programmable_move_call(
        package_id,
        ident_str!("wrapping").to_owned(),
        ident_str!("update").to_owned(),
        vec![],
        vec![item_arg, value_arg],
    );
    let (digest, effects) = execute(cluster, ptb).await;
    let mutated = effects
        .mutated()
        .into_iter()
        .find(|(r, _)| r.0 == item_ref.0)
        .expect("Item should be mutated")
        .0;
    (digest, mutated)
}

pub async fn wrap_item(
    cluster: &mut test_cluster::TestCluster,
    package_id: ObjectID,
    item_ref: ObjectRef,
) -> (String, ObjectRef) {
    let sender = cluster.wallet.active_address().unwrap();
    let mut ptb = ProgrammableTransactionBuilder::new();
    let item_arg = ptb.obj(ObjectArg::ImmOrOwnedObject(item_ref)).unwrap();
    let wrapper = ptb.programmable_move_call(
        package_id,
        ident_str!("wrapping").to_owned(),
        ident_str!("wrap").to_owned(),
        vec![],
        vec![item_arg],
    );
    ptb.transfer_arg(sender, wrapper);
    let (digest, effects) = execute(cluster, ptb).await;
    let created = effects
        .created()
        .into_iter()
        .find(|(_, owner)| owner.get_address_owner_address().is_ok())
        .expect("Should have created a Wrapper")
        .0;
    (digest, created)
}

pub async fn unwrap_wrapper(
    cluster: &mut test_cluster::TestCluster,
    package_id: ObjectID,
    wrapper_ref: ObjectRef,
) -> (String, ObjectRef) {
    let sender = cluster.wallet.active_address().unwrap();
    let mut ptb = ProgrammableTransactionBuilder::new();
    let wrapper_arg = ptb.obj(ObjectArg::ImmOrOwnedObject(wrapper_ref)).unwrap();
    let item = ptb.programmable_move_call(
        package_id,
        ident_str!("wrapping").to_owned(),
        ident_str!("unwrap").to_owned(),
        vec![],
        vec![wrapper_arg],
    );
    ptb.transfer_arg(sender, item);
    let (digest, effects) = execute(cluster, ptb).await;
    let unwrapped = effects
        .unwrapped()
        .into_iter()
        .next()
        .expect("Should have unwrapped an Item")
        .0;
    (digest, unwrapped)
}

async fn execute(
    cluster: &mut test_cluster::TestCluster,
    ptb: ProgrammableTransactionBuilder,
) -> (String, sui_types::effects::TransactionEffects) {
    let sender = cluster.wallet.active_address().unwrap();
    let gas = gas_for(cluster).await;
    let rgp = cluster.wallet.get_reference_gas_price().await.unwrap();
    let tx_data =
        TransactionData::new_programmable(sender, vec![gas], ptb.finish(), 5_000_000_000, rgp);
    let tx = cluster.wallet.sign_transaction(&tx_data).await;
    let resp = cluster.wallet.execute_transaction_must_succeed(tx).await;
    (
        Base58::encode(*resp.effects.transaction_digest()),
        resp.effects,
    )
}

async fn gas_for(cluster: &mut test_cluster::TestCluster) -> ObjectRef {
    let sender = cluster.wallet.active_address().unwrap();
    cluster
        .wallet
        .gas_for_owner_budget(sender, 5_000_000_000, BTreeSet::new())
        .await
        .unwrap()
        .1
        .compute_object_reference()
}
