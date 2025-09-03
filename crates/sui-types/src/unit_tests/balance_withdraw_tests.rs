// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    accumulator_root::AccumulatorValue,
    base_types::{random_object_ref, SuiAddress},
    gas_coin::GAS,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{BalanceWithdrawArg, TransactionData, TransactionDataAPI, WithdrawTypeParam},
    type_input::TypeInput,
};

#[test]
fn test_withdraw_max_amount() {
    let arg = BalanceWithdrawArg::new_with_amount(100, TypeInput::from(GAS::type_tag()));
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.balance_withdraw(arg.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.has_balance_withdraws());
    let withdraws = tx.process_balance_withdraws().unwrap();
    let account_id = AccumulatorValue::get_field_id(
        sender,
        &WithdrawTypeParam::Balance(GAS::type_tag().into())
            .get_type_tag()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(withdraws, BTreeMap::from([(account_id, 100)]));
}

#[test]
fn test_multiple_withdraws_same_account() {
    let arg1 = BalanceWithdrawArg::new_with_amount(100, TypeInput::from(GAS::type_tag()));
    let arg2 = BalanceWithdrawArg::new_with_amount(200, TypeInput::from(GAS::type_tag()));
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.balance_withdraw(arg1.clone()).unwrap();
    ptb.balance_withdraw(arg2.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.has_balance_withdraws());
    let withdraws = tx.process_balance_withdraws().unwrap();
    let account_id = AccumulatorValue::get_field_id(
        sender,
        &WithdrawTypeParam::Balance(GAS::type_tag().into())
            .get_type_tag()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(withdraws, BTreeMap::from([(account_id, 300)]));
}

#[test]
fn test_multiple_withdraws_different_accounts() {
    let arg1 = BalanceWithdrawArg::new_with_amount(100, TypeInput::from(GAS::type_tag()));
    let arg2 = BalanceWithdrawArg::new_with_amount(200, TypeInput::Bool);
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.balance_withdraw(arg1.clone()).unwrap();
    ptb.balance_withdraw(arg2.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.has_balance_withdraws());
    let withdraws = tx.process_balance_withdraws().unwrap();
    let account_id1 = AccumulatorValue::get_field_id(
        sender,
        &WithdrawTypeParam::Balance(GAS::type_tag().into())
            .get_type_tag()
            .unwrap(),
    )
    .unwrap();
    let account_id2 = AccumulatorValue::get_field_id(
        sender,
        &WithdrawTypeParam::Balance(TypeInput::Bool)
            .get_type_tag()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        withdraws,
        BTreeMap::from([(account_id1, 100), (account_id2, 200),])
    );
}

#[test]
fn test_withdraw_zero_amount() {
    let arg = BalanceWithdrawArg::new_with_amount(0, TypeInput::from(GAS::type_tag()));
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.balance_withdraw(arg.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.process_balance_withdraws().is_err());
}

#[test]
fn test_withdraw_too_many_withdraws() {
    let mut ptb = ProgrammableTransactionBuilder::new();
    for _ in 0..11 {
        ptb.balance_withdraw(BalanceWithdrawArg::new_with_amount(
            100,
            TypeInput::from(GAS::type_tag()),
        ))
        .unwrap();
    }
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.process_balance_withdraws().is_err());
}
