// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proptest::prelude::*;
use sui_types::utils::to_sender_signed_transaction;

use proptest::strategy::ValueTree;
use transaction_fuzzer::account_universe::AccountCurrent;
use transaction_fuzzer::account_universe::AccountData;

use transaction_fuzzer::{
    executor::{assert_is_acceptable_result, Executor},
    transaction_data_gen::transaction_data_gen,
};

#[test]
#[cfg_attr(msim, ignore)]
fn all_random_transaction_data() {
    let mut exec = Executor::new();
    let account = AccountCurrent::new(AccountData::new_random());
    let strategy = transaction_data_gen(account.initial_data.account.address);
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    for _ in 0..1000 {
        let tx_data = strategy.new_tree(&mut runner).unwrap().current();
        let signed_txn = to_sender_signed_transaction(tx_data, &account.initial_data.account.key);
        let result = exec.execute_transaction(signed_txn);
        assert_is_acceptable_result(&result);
    }
}
