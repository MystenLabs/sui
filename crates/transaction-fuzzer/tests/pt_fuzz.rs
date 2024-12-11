// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use transaction_fuzzer::account_universe::AccountCurrent;
use transaction_fuzzer::account_universe::AccountData;
use transaction_fuzzer::executor::Executor;
use transaction_fuzzer::programmable_transaction_gen::{
    gen_many_input_match, gen_programmable_transaction, MAX_ITERATIONS_INPUT_MATCH,
};
use transaction_fuzzer::type_arg_fuzzer::{run_pt, run_pt_effects};

use sui_types::base_types::ObjectRef;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::execution_status::{ExecutionFailureStatus, ExecutionStatus};
use sui_types::object::Owner;
use sui_types::transaction::{CallArg, ObjectArg, ProgrammableTransaction};
use sui_types::{MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID};

#[test]
#[cfg_attr(msim, ignore)]
fn invalid_pt_fuzz() {
    let mut exec = Executor::new();
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut account = AccountCurrent::new(AccountData::new_random());
    let strategy = gen_programmable_transaction();
    for _ in 0..50 {
        let pt = strategy.new_tree(&mut runner).unwrap().current();
        run_pt(&mut account, &mut exec, pt)
    }
}

fn publish_coin_factory(
    exec: &mut Executor,
    account: &mut AccountCurrent,
) -> (ObjectRef, ObjectRef) {
    let effects = exec.publish(
        "coin_factory",
        vec![MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID],
        account,
    );
    let package = effects
        .created()
        .into_iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .unwrap();
    let cap = effects
        .created()
        .into_iter()
        .find(|(obj_ref, _)| {
            if let Some(stag) = exec
                .rt
                .block_on(exec.state.get_object(&obj_ref.0))
                .unwrap()
                .data
                .struct_tag()
            {
                stag.name.as_str().eq("TreasuryCap")
            } else {
                false
            }
        })
        .unwrap();

    (package.0, cap.0)
}

/// This function runs programmable transaction block and checks if it executed successfully. It
/// also updates the treasury cap of a coin used for testing which gets updated between transaction
/// blocks if coins get minted. We need this to work around limitations of the proptest framework
/// where no external mutable state can be used to the `prop_compose` macro.
pub fn run_pt_success(
    account: &mut AccountCurrent,
    exec: &mut Executor,
    mut pt: ProgrammableTransaction,
    cap: ObjectRef,
) -> ObjectRef {
    for i in 0..pt.inputs.len() {
        if let CallArg::Object(ObjectArg::ImmOrOwnedObject(obj_ref)) = pt.inputs[i] {
            if obj_ref.0 == cap.0 {
                pt.inputs[i] = CallArg::Object(ObjectArg::ImmOrOwnedObject(cap));
            }
        }
    }

    let effects = run_pt_effects(account, exec, pt).unwrap();
    let status = effects.status();
    // at this point we know that the whole block executed successfully and we also know that we are
    // not properly dropping all the used values (in particular empty vectors of coins generated as
    // input - doing so for as many getter functions as we have increases complexity of the code to
    // the point that it fails verification)
    assert!(
        matches!(
            status,
            ExecutionStatus::Failure {
                error: ExecutionFailureStatus::UnusedValueWithoutDrop { .. },
                command: _,
            }
        ),
        "{:?}",
        status
    );
    let new_cap = effects
        .mutated()
        .into_iter()
        .find(|(obj_ref, _)| {
            if let Some(stag) = exec
                .rt
                .block_on(exec.state.get_object(&obj_ref.0))
                .unwrap()
                .data
                .struct_tag()
            {
                stag.name.as_str().eq("TreasuryCap")
            } else {
                false
            }
        })
        .unwrap();

    new_cap.0
}

#[test]
#[cfg_attr(msim, ignore)]
fn pt_fuzz_input_match() {
    let mut exec = Executor::new();
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut account = AccountCurrent::new(AccountData::new_random());
    let (package, cap) = publish_coin_factory(&mut exec, &mut account);

    let strategy = gen_many_input_match(account.initial_data.account.address, package.0, cap);
    let mut new_cap = cap;
    for _ in 0..MAX_ITERATIONS_INPUT_MATCH {
        let pt = strategy.new_tree(&mut runner).unwrap().current();
        new_cap = run_pt_success(&mut account, &mut exec, pt, new_cap);
    }
}
