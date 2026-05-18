// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Helpers for interacting with the `emit_test_event` Move package in tests.

use std::path::PathBuf;

use move_core_types::ident_str;
use sui_indexer_alt_e2e_tests::move_helpers::execute_ptb;
use sui_indexer_alt_e2e_tests::move_helpers::publish_package;
use sui_types::base_types::ObjectID;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;

pub async fn publish(cluster: &mut test_cluster::TestCluster) -> ObjectID {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["packages", "event", "emit_test_event"]);
    publish_package(cluster, &path).await
}

/// Append a call to `emit_test_event::emit_test_event()` onto an existing PTB. Use this
/// to compose with other move calls in the same transaction.
pub fn add_emit_call(ptb: &mut ProgrammableTransactionBuilder, package_id: ObjectID) {
    ptb.programmable_move_call(
        package_id,
        ident_str!("emit_test_event").to_owned(),
        ident_str!("emit_test_event").to_owned(),
        vec![],
        vec![],
    );
}

/// Append a call to `emit_test_event::emit_with_value(value)` onto an existing PTB.
/// Each call with a distinct `value` produces a distinct `TestEvent`, useful for tests
/// that need to identify events individually.
pub fn add_emit_with_value_call(
    ptb: &mut ProgrammableTransactionBuilder,
    package_id: ObjectID,
    value: u64,
) {
    let value_arg = ptb.pure(value).unwrap();
    ptb.programmable_move_call(
        package_id,
        ident_str!("emit_test_event").to_owned(),
        ident_str!("emit_with_value").to_owned(),
        vec![],
        vec![value_arg],
    );
}

pub async fn emit(cluster: &mut test_cluster::TestCluster, package_id: ObjectID) -> String {
    let mut ptb = ProgrammableTransactionBuilder::new();
    add_emit_call(&mut ptb, package_id);
    execute_ptb(cluster, ptb).await.0
}

pub async fn emit_with_value(
    cluster: &mut test_cluster::TestCluster,
    package_id: ObjectID,
    value: u64,
) -> String {
    let mut ptb = ProgrammableTransactionBuilder::new();
    add_emit_with_value_call(&mut ptb, package_id, value);
    execute_ptb(cluster, ptb).await.0
}

/// Emit a `TestEvent` and create a `TestObject` in the same transaction. Used by tests
/// that need an event subscription yield to also reference object changes via
/// `event { transaction { effects { objectChanges } } }`.
pub async fn emit_and_create(
    cluster: &mut test_cluster::TestCluster,
    package_id: ObjectID,
    value: u64,
) -> String {
    let sender = cluster.wallet.active_address().unwrap();
    let mut ptb = ProgrammableTransactionBuilder::new();
    let value_arg = ptb.pure(value).unwrap();
    let test_object = ptb.programmable_move_call(
        package_id,
        ident_str!("emit_test_event").to_owned(),
        ident_str!("emit_and_create").to_owned(),
        vec![],
        vec![value_arg],
    );
    ptb.transfer_arg(sender, test_object);
    execute_ptb(cluster, ptb).await.0
}
