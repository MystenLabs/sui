// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proptest::collection::vec;
use proptest::prelude::*;

use proptest::strategy::ValueTree;
use transaction_fuzzer::account_universe::AccountCurrent;
use transaction_fuzzer::account_universe::AccountData;
use transaction_fuzzer::type_arg_fuzzer::run_type_tags;
use transaction_fuzzer::{executor::Executor, type_arg_fuzzer::gen_type_tag};

#[test]
#[cfg_attr(msim, ignore)]
fn all_random_single_type_tag_fuzzing() {
    let mut exec = Executor::new();
    let strategy = vec(gen_type_tag(), 1..10);
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let account = AccountCurrent::new(AccountData::new_random());
    for _ in 0..2000 {
        let tys = strategy.new_tree(&mut runner).unwrap().current();
        run_type_tags(&account, &mut exec, tys)
    }
}
