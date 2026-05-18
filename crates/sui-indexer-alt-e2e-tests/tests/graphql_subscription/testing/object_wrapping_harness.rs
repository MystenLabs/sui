// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Helpers for interacting with the `object_wrapping` Move package in tests.

use std::path::PathBuf;

use move_core_types::ident_str;
use sui_indexer_alt_e2e_tests::move_helpers::execute_ptb;
use sui_indexer_alt_e2e_tests::move_helpers::publish_package;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::Argument;
use sui_types::transaction::ObjectArg;

pub async fn publish(cluster: &mut test_cluster::TestCluster) -> ObjectID {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["packages", "object_wrapping"]);
    publish_package(cluster, &path).await
}

/// Append a call to `wrapping::create(value)` onto an existing PTB and return the
/// `Item` argument so the caller can transfer it or feed it into a subsequent call.
pub fn add_create_call(
    ptb: &mut ProgrammableTransactionBuilder,
    package_id: ObjectID,
    value: u64,
) -> Argument {
    let value_arg = ptb.pure(value).unwrap();
    ptb.programmable_move_call(
        package_id,
        ident_str!("wrapping").to_owned(),
        ident_str!("create").to_owned(),
        vec![],
        vec![value_arg],
    )
}

pub async fn create_item(
    cluster: &mut test_cluster::TestCluster,
    package_id: ObjectID,
    value: u64,
) -> (String, ObjectRef) {
    let sender = cluster.wallet.active_address().unwrap();
    let mut ptb = ProgrammableTransactionBuilder::new();
    let item = add_create_call(&mut ptb, package_id, value);
    ptb.transfer_arg(sender, item);
    let (digest, effects) = execute_ptb(cluster, ptb).await;
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
    let (digest, effects) = execute_ptb(cluster, ptb).await;
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
    let (digest, effects) = execute_ptb(cluster, ptb).await;
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
    let (digest, effects) = execute_ptb(cluster, ptb).await;
    let unwrapped = effects
        .unwrapped()
        .into_iter()
        .next()
        .expect("Should have unwrapped an Item")
        .0;
    (digest, unwrapped)
}
