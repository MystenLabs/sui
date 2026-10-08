// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_macros::sim_test;
use sui_types::{
    SUI_CLOCK_OBJECT_ID,
    base_types::{ObjectID, SequenceNumber},
    clock::Clock,
    effects::{TransactionEffects, TransactionEffectsAPI, UnchangedConsensusKind},
    object::Object,
    transaction::{CallArg, TransactionData},
    transaction_executor::TransactionChecks,
};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder};

fn move_test_code_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code_clock");
    path
}

async fn setup() -> (TestEnv, ObjectID) {
    let mut test_env = TestEnvBuilder::new().build().await;
    let package_id = test_env.setup_test_package(move_test_code_path()).await;
    (test_env, package_id)
}

fn clock_test_call(
    test_env: &TestEnv,
    package_id: ObjectID,
    function: &'static str,
    args: Vec<CallArg>,
) -> TransactionData {
    let (sender, gas) = test_env.get_sender_and_gas(0);
    test_env
        .tx_builder_with_gas(sender, gas)
        .move_call(package_id, "clock_test", function, args)
        .build()
}

fn clock_read_only_root_version(effects: &TransactionEffects) -> Option<SequenceNumber> {
    effects
        .unchanged_consensus_objects()
        .iter()
        .find_map(|(id, kind)| match kind {
            UnchangedConsensusKind::ReadOnlyRoot((version, _)) if *id == SUI_CLOCK_OBJECT_ID => {
                Some(*version)
            }
            _ => None,
        })
}

fn clock_timestamp_ms(clock: &Object) -> u64 {
    let clock: Clock = clock.data.try_as_move().unwrap().to_rust().unwrap();
    clock.timestamp_ms()
}

/// A transaction that passes the Clock explicitly sees the same timestamp through
/// `clock::now_ms` as through the Clock input.
#[sim_test]
async fn test_now_ms_matches_clock_input() {
    let (mut test_env, package_id) = setup().await;
    let tx = clock_test_call(
        &test_env,
        package_id,
        "assert_matches_clock",
        vec![CallArg::CLOCK_IMM],
    );
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
}

/// A transaction without any Clock input reads the Clock written by its commit's prologue. The
/// read is recorded in effects, and the fullnode reproduces it from effects.
#[sim_test]
async fn test_now_ms_without_clock_input() {
    let (mut test_env, package_id) = setup().await;
    let tx = clock_test_call(&test_env, package_id, "emit_now_ms", vec![]);
    let (digest, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    let clock_version = clock_read_only_root_version(&effects)
        .expect("implicit Clock read must be recorded in effects");

    test_env.cluster.wait_for_tx_settlement(&[digest]).await;
    let (events, clock) = test_env.cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let events = state.get_transaction_events(&digest).unwrap();
        let clock = state
            .get_object_cache_reader()
            .get_object_by_key(&SUI_CLOCK_OBJECT_ID, clock_version)
            .unwrap();
        (events, clock)
    });
    let timestamp_ms = clock_timestamp_ms(&clock);
    assert!(timestamp_ms > 0);
    let [event] = events.data.as_slice() else {
        panic!("expected exactly one event, got {:?}", events.data);
    };
    let emitted_timestamp_ms: u64 = bcs::from_bytes(&event.contents).unwrap();
    assert_eq!(emitted_timestamp_ms, timestamp_ms);
}

/// An aborted transaction still records its implicit Clock read, so the fullnode can reproduce
/// the failed execution from effects.
#[sim_test]
async fn test_now_ms_read_recorded_on_abort() {
    let (mut test_env, package_id) = setup().await;
    let tx = clock_test_call(&test_env, package_id, "read_then_abort", vec![]);
    let (digest, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_err(), "{:?}", effects.status());
    assert!(
        clock_read_only_root_version(&effects).is_some(),
        "implicit Clock read must be recorded in effects: {:?}",
        effects.unchanged_consensus_objects()
    );
    test_env.cluster.wait_for_tx_settlement(&[digest]).await;
}

/// Simulation has no consensus assignment, so it reads the fullnode's latest Clock and reports
/// that version in its effects.
#[sim_test]
async fn test_now_ms_in_simulation() {
    let (test_env, package_id) = setup().await;
    let tx = clock_test_call(&test_env, package_id, "emit_now_ms", vec![]);
    let (result, clock) = test_env.cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let result = state
            .simulate_transaction(tx, TransactionChecks::Enabled, false)
            .unwrap();
        let clock_version = clock_read_only_root_version(&result.effects)
            .expect("implicit Clock read must be recorded in simulated effects");
        let clock = state
            .get_object_cache_reader()
            .get_object_by_key(&SUI_CLOCK_OBJECT_ID, clock_version)
            .unwrap();
        (result, clock)
    });
    assert!(
        result.effects.status().is_ok(),
        "{:?}",
        result.effects.status()
    );
    let events = result.events.expect("simulation must return events");
    let [event] = events.data.as_slice() else {
        panic!("expected exactly one event, got {:?}", events.data);
    };
    let emitted_timestamp_ms: u64 = bcs::from_bytes(&event.contents).unwrap();
    assert_eq!(emitted_timestamp_ms, clock_timestamp_ms(&clock));
}
