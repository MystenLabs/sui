// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use transaction_fuzzer::account_universe::AccountCurrent;
use transaction_fuzzer::account_universe::AccountData;
use transaction_fuzzer::executor::Executor;
use transaction_fuzzer::programmable_transaction_gen::gen_programmable_transaction;
use transaction_fuzzer::type_arg_fuzzer::run_pt;

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
