// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proptest::collection::vec;
use proptest::prelude::*;

use proptest::strategy::ValueTree;
use transaction_fuzzer::account_universe::AccountCurrent;
use transaction_fuzzer::account_universe::AccountData;
use transaction_fuzzer::type_arg_fuzzer::generate_valid_and_invalid_type_factory_tags;
use transaction_fuzzer::type_arg_fuzzer::generate_valid_type_factory_tags;
use transaction_fuzzer::type_arg_fuzzer::pt_for_tags;
use transaction_fuzzer::type_arg_fuzzer::run_pt;
use transaction_fuzzer::type_arg_fuzzer::type_factory_pt_for_tags;
use transaction_fuzzer::{executor::Executor, type_arg_fuzzer::gen_type_tag};

fn _all_valid_type_tag_fuzzing(add_sub: isize) {
    let mut exec = Executor::new();
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut account = AccountCurrent::new(AccountData::new_random());
    let (package_id, _) = exec.publish("type_factory", &mut account);
    let strategy = vec(generate_valid_type_factory_tags(package_id.0), 1..10);
    for _ in 0..500 {
        let tys = strategy.new_tree(&mut runner).unwrap().current();
        let len = tys.len();
        let pt = type_factory_pt_for_tags(package_id.0, tys, (len as isize + add_sub) as usize);
        run_pt(&mut account, &mut exec, pt)
    }
}

#[test]
#[cfg_attr(msim, ignore)]
fn all_random_single_type_tag_fuzzing() {
    let mut exec = Executor::new();
    let strategy = vec(gen_type_tag(), 1..10);
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut account = AccountCurrent::new(AccountData::new_random());
    for _ in 0..1000 {
        let tys = strategy.new_tree(&mut runner).unwrap().current();
        run_pt(&mut account, &mut exec, pt_for_tags(tys))
    }
}

#[test]
#[cfg_attr(msim, ignore)]
fn all_valid_type_tag_fuzzing() {
    _all_valid_type_tag_fuzzing(0)
}

#[test]
#[cfg_attr(msim, ignore)]
fn all_valid_type_tag_incorrect_number_fuzzing() {
    _all_valid_type_tag_fuzzing(-1);
    _all_valid_type_tag_fuzzing(1);
}

#[test]
#[cfg_attr(msim, ignore)]
fn interesting_invalid_type_tags_fuzzing() {
    let mut exec = Executor::new();
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut account = AccountCurrent::new(AccountData::new_random());
    let (package_id, _) = exec.publish("type_factory", &mut account);
    let strategy = vec(
        generate_valid_and_invalid_type_factory_tags(package_id.0),
        1..10,
    );
    for _ in 0..500 {
        let tys = strategy.new_tree(&mut runner).unwrap().current();
        let len = tys.len();
        let pt = type_factory_pt_for_tags(package_id.0, tys, len);
        run_pt(&mut account, &mut exec, pt)
    }
}
