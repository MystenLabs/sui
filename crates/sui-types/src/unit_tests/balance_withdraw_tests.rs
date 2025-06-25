// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    accumulator_root::derive_balance_account_object_id,
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
    let account_id = derive_balance_account_object_id(
        sender,
        WithdrawTypeParam::Balance(GAS::type_tag().into()),
    )
    .unwrap();
    assert_eq!(withdraws, BTreeMap::from([(account_id, arg.reservation)]));
}

#[test]
fn test_withdraw_entire_balance() {
    let arg = BalanceWithdrawArg::new_with_entire_balance(TypeInput::from(GAS::type_tag()));
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.balance_withdraw(arg.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.has_balance_withdraws());
    let withdraws = tx.process_balance_withdraws().unwrap();
    let account_id = derive_balance_account_object_id(
        sender,
        WithdrawTypeParam::Balance(GAS::type_tag().into()),
    )
    .unwrap();
    assert_eq!(withdraws, BTreeMap::from([(account_id, arg.reservation)]));
}

#[test]
fn test_multiple_withdraws() {
    let arg1 = BalanceWithdrawArg::new_with_amount(100, TypeInput::from(GAS::type_tag()));
    let arg2 = BalanceWithdrawArg::new_with_entire_balance(TypeInput::Bool);
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.balance_withdraw(arg1.clone()).unwrap();
    ptb.balance_withdraw(arg2.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.has_balance_withdraws());
    let withdraws = tx.process_balance_withdraws().unwrap();
    let account_id1 = derive_balance_account_object_id(
        sender,
        WithdrawTypeParam::Balance(GAS::type_tag().into()),
    )
    .unwrap();
    let account_id2 =
        derive_balance_account_object_id(sender, WithdrawTypeParam::Balance(TypeInput::Bool))
            .unwrap();
    assert_eq!(
        withdraws,
        BTreeMap::from([
            (account_id1, arg1.reservation),
            (account_id2, arg2.reservation)
        ])
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
fn test_withdraw_entire_balance_multiple_times() {
    let arg1 = BalanceWithdrawArg::new_with_entire_balance(TypeInput::from(GAS::type_tag()));
    let arg2 = BalanceWithdrawArg::new_with_entire_balance(TypeInput::from(GAS::type_tag()));
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.balance_withdraw(arg1.clone()).unwrap();
    ptb.balance_withdraw(arg2.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.process_balance_withdraws().is_err());
}

#[test]
fn test_withdraw_amount_and_entire_balance() {
    let arg1 = BalanceWithdrawArg::new_with_amount(100, TypeInput::from(GAS::type_tag()));
    let arg2 = BalanceWithdrawArg::new_with_entire_balance(TypeInput::from(GAS::type_tag()));
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.balance_withdraw(arg1.clone()).unwrap();
    ptb.balance_withdraw(arg2.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.process_balance_withdraws().is_err());
}

#[test]
fn test_withdraw_entire_balance_multiple_times_different_types() {
    let arg1 = BalanceWithdrawArg::new_with_entire_balance(TypeInput::from(GAS::type_tag()));
    let arg2 = BalanceWithdrawArg::new_with_entire_balance(TypeInput::Bool);
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.balance_withdraw(arg1.clone()).unwrap();
    ptb.balance_withdraw(arg2.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    let withdraws = tx.process_balance_withdraws().unwrap();
    let account_id1 = derive_balance_account_object_id(
        sender,
        WithdrawTypeParam::Balance(GAS::type_tag().into()),
    )
    .unwrap();
    let account_id2 =
        derive_balance_account_object_id(sender, WithdrawTypeParam::Balance(TypeInput::Bool))
            .unwrap();
    assert_eq!(
        withdraws,
        BTreeMap::from([
            (account_id1, arg1.reservation),
            (account_id2, arg2.reservation)
        ])
    );
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
